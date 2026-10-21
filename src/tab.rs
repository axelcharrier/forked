use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::git::patch::{self, PatchMode};
use crate::git::repo::{GitRepo, HeadInfo, RepoState};
use crate::git::{
    BranchInfo, ChangeKind, CommitInfo, DiffSide, FileChange, FileDiff, Oid, StashInfo, TagInfo,
    WorkingStatus, graph,
};
use crate::ui::file_tree::ListMode;
use crate::ui::{After, GitOp, View};

const LOG_LIMIT: usize = 10_000;
const CONTEXT_LINES: u32 = 3;

pub struct CommitDetail {
    pub info: CommitInfo,
    pub files: Vec<FileChange>,
    pub selected_file: Option<usize>,
    pub diff: Option<FileDiff>,
}

pub struct RepoTab {
    pub repo: GitRepo,
    pub name: String,
    pub head: HeadInfo,
    pub local: Vec<BranchInfo>,
    pub remote_branches: Vec<BranchInfo>,
    pub remotes: Vec<String>,
    pub tags: Vec<TagInfo>,
    pub stashes: Vec<StashInfo>,
    pub status: WorkingStatus,
    pub commits: Vec<CommitInfo>,
    pub graph: Vec<graph::GraphRow>,
    pub index_of: HashMap<Oid, usize>,
    pub error: Option<String>,

    pub view: View,
    pub search: String,
    pub selected_commit: Option<Oid>,
    pub detail: Option<CommitDetail>,
    pub scroll_to: Option<usize>,
    /// Rows of the history list drawn last frame.
    pub visible_rows: std::ops::Range<usize>,

    pub selected_change: Option<(DiffSide, String)>,
    pub change_diff: Option<FileDiff>,
    pub selected_lines: BTreeSet<(usize, usize)>,
    pub line_anchor: Option<(usize, usize)>,
    pub commit_subject: String,
    pub commit_body: String,
    pub amend: bool,
    /// Move keyboard focus to the description field on the next frame.
    pub focus_body: bool,
    pub list_mode: ListMode,
}

impl RepoTab {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = GitRepo::open(path)?;
        let mut tab = Self {
            name: repo.name(),
            repo,
            head: HeadInfo::default(),
            local: Vec::new(),
            remote_branches: Vec::new(),
            remotes: Vec::new(),
            tags: Vec::new(),
            stashes: Vec::new(),
            status: WorkingStatus::default(),
            commits: Vec::new(),
            graph: Vec::new(),
            index_of: HashMap::new(),
            error: None,
            view: View::History,
            search: String::new(),
            selected_commit: None,
            detail: None,
            scroll_to: None,
            visible_rows: 0..0,
            selected_change: None,
            change_diff: None,
            selected_lines: BTreeSet::new(),
            line_anchor: None,
            commit_subject: String::new(),
            commit_body: String::new(),
            amend: false,
            focus_body: false,
            list_mode: ListMode::Tree,
        };
        tab.refresh();
        if tab.status.total() > 0 {
            tab.view = View::Changes;
        }
        Ok(tab)
    }

    pub fn path(&self) -> &PathBuf {
        &self.repo.path
    }

    pub fn refresh(&mut self) {
        self.error = self.try_refresh().err().map(|e| e.to_string());
    }

    fn try_refresh(&mut self) -> Result<()> {
        self.head = self.repo.head();
        (self.local, self.remote_branches) = self.repo.branches()?;
        self.remotes = self.repo.remotes();
        self.tags = self.repo.tags()?;
        self.stashes = self.repo.stashes()?;
        self.status = self.repo.status()?;
        self.commits = self.repo.log(&self.head, LOG_LIMIT)?;
        self.graph = graph::layout(&self.commits);
        self.index_of = self
            .commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();

        if self.head.state != RepoState::Clean
            && self.message_is_empty()
            && let Some(msg) = self.repo.prepared_message()
        {
            self.set_message(&msg);
        }

        // Keep the selected working-copy file if it still has changes.
        if let Some((side, path)) = self.selected_change.clone() {
            let still_there = self.status.list(side).iter().any(|f| f.path == path);
            if still_there {
                self.load_change_diff();
            } else {
                // A fully staged file moves to the other list: follow it.
                let other = match side {
                    DiffSide::Unstaged => DiffSide::Staged,
                    DiffSide::Staged => DiffSide::Unstaged,
                };
                if self.status.list(other).iter().any(|f| f.path == path) {
                    self.select_change(other, path);
                } else {
                    self.selected_change = None;
                    self.change_diff = None;
                }
            }
        }
        Ok(())
    }

    pub fn current_branch(&self) -> Option<&BranchInfo> {
        self.local.iter().find(|b| b.is_head)
    }

    /// The remote to use by default: the upstream's, else `origin`, else the first one.
    pub fn default_remote(&self) -> Option<String> {
        if let Some(up) = self.current_branch().and_then(|b| b.upstream.as_deref())
            && let Some(r) = self.remotes.iter().find(|r| up.starts_with(&format!("{r}/"))) {
                return Some(r.clone());
            }
        self.remotes
            .iter()
            .find(|r| *r == "origin")
            .or(self.remotes.first())
            .cloned()
    }

    pub fn select_commit(&mut self, oid: Oid) {
        self.selected_commit = Some(oid);
        let info = match self.index_of.get(&oid) {
            Some(&i) => Ok(self.commits[i].clone()),
            None => self.repo.commit_info(oid),
        };
        let detail = info.and_then(|info| {
            let files = self.repo.commit_files(oid)?;
            Ok(CommitDetail {
                info,
                files,
                selected_file: None,
                diff: None,
            })
        });
        match detail {
            Ok(d) => {
                self.detail = Some(d);
                if self.detail.as_ref().is_some_and(|d| !d.files.is_empty()) {
                    self.select_commit_file(0);
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn select_commit_file(&mut self, index: usize) {
        let Some(d) = &mut self.detail else { return };
        let Some(file) = d.files.get(index) else {
            return;
        };
        d.selected_file = Some(index);
        match self.repo.commit_file_diff(d.info.id, file, CONTEXT_LINES) {
            Ok(diff) => d.diff = Some(diff),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn select_change(&mut self, side: DiffSide, path: String) {
        if self.selected_change.as_ref() != Some(&(side, path.clone())) {
            self.selected_lines.clear();
            self.line_anchor = None;
        }
        self.selected_change = Some((side, path));
        self.load_change_diff();
    }

    pub fn selected_change_kind(&self) -> Option<ChangeKind> {
        let (side, path) = self.selected_change.as_ref()?;
        self.status
            .list(*side)
            .iter()
            .find(|f| &f.path == path)
            .map(|f| f.kind)
    }

    fn load_change_diff(&mut self) {
        let Some((side, path)) = self.selected_change.clone() else {
            return;
        };
        if self.selected_change_kind() == Some(ChangeKind::Conflicted) {
            self.change_diff = Some(conflict_view(&self.repo.path, &path));
            return;
        }
        match self.repo.working_diff(&path, side, CONTEXT_LINES) {
            Ok(diff) => {
                // Drop line selections that no longer exist.
                self.selected_lines.retain(|(h, l)| {
                    diff.hunks.get(*h).is_some_and(|hunk| *l < hunk.lines.len())
                });
                self.change_diff = Some(diff);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn patch_op(&self, mode: PatchMode, hunk: usize, lines: Option<&[usize]>) -> Option<GitOp> {
        let diff = self.change_diff.as_ref()?;
        let lines: Option<BTreeSet<usize>> = lines.map(|l| l.iter().copied().collect());
        let patch = patch::build(diff, hunk, lines.as_ref(), mode)?;
        let what = if lines.is_some() { "les lignes" } else { "le bloc" };
        let label = match mode {
            PatchMode::Stage => format!("Indexer {what}"),
            PatchMode::Unstage => format!("Désindexer {what}"),
            PatchMode::Discard => format!("Annuler {what}"),
        };
        Some(GitOp::new(label, mode.apply_args()).stdin(patch))
    }

    pub fn message_is_empty(&self) -> bool {
        self.commit_subject.trim().is_empty() && self.commit_body.trim().is_empty()
    }

    /// Splits a full commit message into the subject and description fields.
    pub fn set_message(&mut self, message: &str) {
        let (subject, body) = message.split_once('\n').unwrap_or((message, ""));
        self.commit_subject = subject.trim().to_owned();
        self.commit_body = body.trim().to_owned();
    }

    pub fn clear_message(&mut self) {
        self.commit_subject.clear();
        self.commit_body.clear();
        self.amend = false;
    }

    fn message(&self) -> String {
        let subject = self.commit_subject.trim();
        let body = self.commit_body.trim();
        if body.is_empty() {
            format!("{subject}\n")
        } else {
            format!("{subject}\n\n{body}\n")
        }
    }

    pub fn can_commit(&self) -> bool {
        !self.commit_subject.trim().is_empty()
            && (!self.status.staged.is_empty() || self.amend || self.head.state != RepoState::Clean)
    }

    pub fn commit_op(&self) -> Option<GitOp> {
        if !self.can_commit() {
            return None;
        }
        let mut args = vec!["commit", "-F", "-"];
        if self.amend {
            args.push("--amend");
        }
        Some(
            GitOp::new("Commit", args)
                .stdin(self.message())
                .after(After::ClearCommitMessage),
        )
    }
}

/// A conflicted file has no regular diff: show it whole, with markers.
fn conflict_view(root: &Path, path: &str) -> FileDiff {
    let text = std::fs::read_to_string(root.join(path)).unwrap_or_default();
    let lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| crate::git::DiffLine {
            origin: if l.starts_with("<<<<<<<") || l.starts_with(">>>>>>>") || l.starts_with("=======")
            {
                '!'
            } else {
                ' '
            },
            old_no: None,
            new_no: Some(i as u32 + 1),
            content: l.to_owned(),
        })
        .collect();
    FileDiff {
        path: path.to_owned(),
        old_path: None,
        binary: false,
        hunks: vec![crate::git::Hunk {
            header: "Conflit — résolvez dans votre éditeur ou choisissez « mine »/« theirs »".into(),
            old_start: 1,
            new_start: 1,
            lines,
        }],
    }
}
