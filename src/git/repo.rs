use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{
    BranchType, Commit, Delta, Diff, DiffOptions, Patch, Repository, Sort, Status, StatusOptions,
};

use super::*;

pub struct GitRepo {
    pub path: PathBuf,
    repo: Repository,
}

#[derive(Clone, Debug, Default)]
pub struct HeadInfo {
    /// Branch name, or `None` when detached.
    pub branch: Option<String>,
    pub oid: Option<Oid>,
    pub detached: bool,
    pub state: RepoState,
}

/// An operation git has left half-done, waiting for the user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepoState {
    #[default]
    Clean,
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
    Bisecting,
}

impl GitRepo {
    /// Opens the repository containing `path` (it may be a sub-folder).
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("{} n'est pas un dépôt Git", path.display()))?;
        let workdir = repo
            .workdir()
            .map(|p| p.to_path_buf())
            .with_context(|| "les dépôts « bare » ne sont pas pris en charge")?;
        let workdir = workdir.canonicalize().unwrap_or(workdir);
        Ok(Self {
            path: workdir,
            repo,
        })
    }

    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn git_dir(&self) -> &Path {
        self.repo.path()
    }

    pub fn head(&self) -> HeadInfo {
        let state = match self.repo.state() {
            git2::RepositoryState::Clean => RepoState::Clean,
            git2::RepositoryState::Merge => RepoState::Merging,
            git2::RepositoryState::Revert | git2::RepositoryState::RevertSequence => {
                RepoState::Reverting
            }
            git2::RepositoryState::CherryPick | git2::RepositoryState::CherryPickSequence => {
                RepoState::CherryPicking
            }
            git2::RepositoryState::Bisect => RepoState::Bisecting,
            _ => RepoState::Rebasing,
        };
        let detached = self.repo.head_detached().unwrap_or(false);
        match self.repo.head() {
            Ok(h) => HeadInfo {
                branch: if detached {
                    None
                } else {
                    h.shorthand().ok().map(str::to_owned)
                },
                oid: h.target(),
                detached,
                state,
            },
            Err(_) => {
                // Unborn branch: read the symbolic target directly.
                let branch = self
                    .repo
                    .find_reference("HEAD")
                    .ok()
                    .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_owned))
                    .map(|t| t.trim_start_matches("refs/heads/").to_owned());
                HeadInfo {
                    branch,
                    oid: None,
                    detached: false,
                    state,
                }
            }
        }
    }

    pub fn branches(&self) -> Result<(Vec<BranchInfo>, Vec<BranchInfo>)> {
        let mut local = Vec::new();
        let mut remote = Vec::new();
        for b in self.repo.branches(None)? {
            let (branch, kind) = b?;
            let Some(name) = branch.name()?.map(str::to_owned) else {
                continue;
            };
            if kind == BranchType::Remote && name.ends_with("/HEAD") {
                continue;
            }
            let target = branch.get().target();
            let mut info = BranchInfo {
                name,
                is_head: branch.is_head(),
                upstream: None,
                ahead: 0,
                behind: 0,
                target,
            };
            if kind == BranchType::Local {
                if let Ok(up) = branch.upstream() {
                    info.upstream = up.name().ok().flatten().map(str::to_owned);
                    if let (Some(a), Some(b)) = (target, up.get().target())
                        && let Ok((ahead, behind)) = self.repo.graph_ahead_behind(a, b) {
                            info.ahead = ahead;
                            info.behind = behind;
                        }
                }
                local.push(info);
            } else {
                remote.push(info);
            }
        }
        local.sort_by(|a, b| a.name.cmp(&b.name));
        remote.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((local, remote))
    }

    pub fn remotes(&self) -> Vec<String> {
        self.repo
            .remotes()
            .map(|r| r.iter().flatten().flatten().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    pub fn remote_url(&self, name: &str) -> Option<String> {
        self.repo
            .find_remote(name)
            .ok()
            .and_then(|r| r.url().ok().map(str::to_owned))
    }

    pub fn tags(&self) -> Result<Vec<TagInfo>> {
        let mut tags = Vec::new();
        self.repo.tag_foreach(|oid, name| {
            let name = String::from_utf8_lossy(name)
                .trim_start_matches("refs/tags/")
                .to_owned();
            let target = self
                .repo
                .find_object(oid, None)
                .and_then(|o| o.peel_to_commit())
                .map(|c| c.id())
                .ok();
            tags.push(TagInfo { name, target });
            true
        })?;
        tags.sort_by(|a, b| b.name.cmp(&a.name));
        Ok(tags)
    }

    pub fn stashes(&mut self) -> Result<Vec<StashInfo>> {
        let mut out = Vec::new();
        self.repo.stash_foreach(|index, message, id| {
            out.push(StashInfo {
                index,
                message: message.to_owned(),
                id: *id,
            });
            true
        })?;
        Ok(out)
    }

    fn ref_labels(&self, head: &HeadInfo) -> HashMap<Oid, Vec<RefLabel>> {
        let mut map: HashMap<Oid, Vec<RefLabel>> = HashMap::new();
        if head.detached
            && let Some(oid) = head.oid {
                map.entry(oid).or_default().push(RefLabel {
                    name: "HEAD".into(),
                    kind: RefKind::Head,
                });
            }
        let Ok(refs) = self.repo.references() else {
            return map;
        };
        for r in refs.flatten() {
            let Ok(full) = r.name() else { continue };
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                let kind = if head.branch.as_deref() == Some(n) {
                    RefKind::Head
                } else {
                    RefKind::LocalBranch
                };
                (kind, n)
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                if n.ends_with("/HEAD") {
                    continue;
                }
                (RefKind::RemoteBranch, n)
            } else if let Some(n) = full.strip_prefix("refs/tags/") {
                (RefKind::Tag, n)
            } else {
                continue;
            };
            let Ok(commit) = r.peel_to_commit() else {
                continue;
            };
            map.entry(commit.id()).or_default().push(RefLabel {
                name: name.to_owned(),
                kind,
            });
        }
        for labels in map.values_mut() {
            labels.sort_by_key(|l| match l.kind {
                RefKind::Head => 0,
                RefKind::LocalBranch => 1,
                RefKind::RemoteBranch => 2,
                RefKind::Tag => 3,
            });
        }
        map
    }

    fn info(c: &Commit<'_>, refs: Vec<RefLabel>) -> CommitInfo {
        let author = c.author();
        let committer = c.committer();
        let id = c.id();
        CommitInfo {
            id,
            short_id: id.to_string()[..7].to_owned(),
            summary: lossy(c.summary_bytes().unwrap_or_default()),
            message: lossy(c.message_bytes()).trim_end().to_owned(),
            author: lossy(author.name_bytes()),
            email: lossy(author.email_bytes()),
            time: author.when().seconds(),
            committer: lossy(committer.name_bytes()),
            commit_time: committer.when().seconds(),
            parents: c.parent_ids().collect(),
            refs,
        }
    }

    pub fn commit_info(&self, oid: Oid) -> Result<CommitInfo> {
        Ok(Self::info(&self.repo.find_commit(oid)?, Vec::new()))
    }

    /// Commits reachable from HEAD and all refs, newest first in topological order.
    pub fn log(&self, head: &HeadInfo, limit: usize) -> Result<Vec<CommitInfo>> {
        let mut walk = self.repo.revwalk()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        if head.oid.is_some() {
            walk.push_head()?;
        }
        walk.push_glob("refs/heads")?;
        walk.push_glob("refs/remotes")?;
        walk.push_glob("refs/tags")?;
        let mut labels = self.ref_labels(head);
        let mut out = Vec::new();
        for oid in walk.take(limit) {
            let oid = oid?;
            let c = self.repo.find_commit(oid)?;
            out.push(Self::info(&c, labels.remove(&oid).unwrap_or_default()));
        }
        Ok(out)
    }

    pub fn status(&self) -> Result<WorkingStatus> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .renames_head_to_index(true)
            .include_ignored(false);
        let statuses = self.repo.statuses(Some(&mut opts))?;
        let mut ws = WorkingStatus::default();
        for e in statuses.iter() {
            let s = e.status();
            let Ok(path) = e.path().map(str::to_owned) else {
                continue;
            };
            if s.contains(Status::CONFLICTED) {
                ws.unstaged.push(FileChange {
                    path,
                    old_path: None,
                    kind: ChangeKind::Conflicted,
                });
                continue;
            }
            let index_kind = if s.contains(Status::INDEX_NEW) {
                Some(ChangeKind::Added)
            } else if s.contains(Status::INDEX_MODIFIED) {
                Some(ChangeKind::Modified)
            } else if s.contains(Status::INDEX_DELETED) {
                Some(ChangeKind::Deleted)
            } else if s.contains(Status::INDEX_RENAMED) {
                Some(ChangeKind::Renamed)
            } else if s.contains(Status::INDEX_TYPECHANGE) {
                Some(ChangeKind::Typechange)
            } else {
                None
            };
            if let Some(kind) = index_kind {
                let old_path = e
                    .head_to_index()
                    .and_then(|d| d.old_file().path().map(|p| p.to_string_lossy().into_owned()))
                    .filter(|p| *p != path);
                ws.staged.push(FileChange {
                    path: path.clone(),
                    old_path,
                    kind,
                });
            }
            let wt_kind = if s.contains(Status::WT_NEW) {
                Some(ChangeKind::Untracked)
            } else if s.contains(Status::WT_MODIFIED) {
                Some(ChangeKind::Modified)
            } else if s.contains(Status::WT_DELETED) {
                Some(ChangeKind::Deleted)
            } else if s.contains(Status::WT_RENAMED) {
                Some(ChangeKind::Renamed)
            } else if s.contains(Status::WT_TYPECHANGE) {
                Some(ChangeKind::Typechange)
            } else {
                None
            };
            if let Some(kind) = wt_kind {
                ws.unstaged.push(FileChange {
                    path,
                    old_path: None,
                    kind,
                });
            }
        }
        ws.staged.sort_by(|a, b| a.path.cmp(&b.path));
        ws.unstaged.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(ws)
    }

    fn diff_opts(paths: &[&str], context: u32) -> DiffOptions {
        let mut opts = DiffOptions::new();
        opts.context_lines(context);
        for p in paths {
            opts.pathspec(*p);
        }
        if !paths.is_empty() {
            opts.disable_pathspec_match(true);
        }
        opts
    }

    pub fn working_diff(&self, path: &str, side: DiffSide, context: u32) -> Result<FileDiff> {
        let mut opts = Self::diff_opts(&[path], context);
        let diff = match side {
            DiffSide::Unstaged => {
                opts.include_untracked(true)
                    .show_untracked_content(true)
                    .recurse_untracked_dirs(true);
                self.repo.diff_index_to_workdir(None, Some(&mut opts))?
            }
            DiffSide::Staged => {
                let head = self.repo.head().ok().and_then(|h| h.peel_to_tree().ok());
                self.repo
                    .diff_tree_to_index(head.as_ref(), None, Some(&mut opts))?
            }
        };
        Ok(Self::collect(&diff)?.into_iter().next().unwrap_or(FileDiff {
            path: path.to_owned(),
            ..Default::default()
        }))
    }

    fn commit_diff(&self, oid: Oid, paths: &[&str], context: u32) -> Result<Diff<'_>> {
        let commit = self.repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let parent_tree = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
        let mut opts = Self::diff_opts(paths, context);
        let mut diff =
            self.repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;
        diff.find_similar(None)?;
        Ok(diff)
    }

    /// Files changed by `oid` relative to its first parent.
    pub fn commit_files(&self, oid: Oid) -> Result<Vec<FileChange>> {
        let diff = self.commit_diff(oid, &[], 0)?;
        Ok(diff
            .deltas()
            .map(|d| {
                let path = d
                    .new_file()
                    .path()
                    .or(d.old_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let old_path = d
                    .old_file()
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .filter(|p| *p != path);
                FileChange {
                    path,
                    old_path,
                    kind: delta_kind(d.status()),
                }
            })
            .collect())
    }

    pub fn commit_file_diff(&self, oid: Oid, file: &FileChange, context: u32) -> Result<FileDiff> {
        // For renames, diff both paths so libgit2 can pair them up again.
        let mut paths = vec![file.path.as_str()];
        if let Some(old) = &file.old_path {
            paths.push(old);
        }
        let diff = self.commit_diff(oid, &paths, context)?;
        Ok(Self::collect(&diff)?.into_iter().next().unwrap_or(FileDiff {
            path: file.path.clone(),
            ..Default::default()
        }))
    }

    fn collect(diff: &Diff<'_>) -> Result<Vec<FileDiff>> {
        let mut files = Vec::new();
        for idx in 0..diff.deltas().len() {
            let delta = diff.get_delta(idx).expect("delta index in range");
            let path = delta
                .new_file()
                .path()
                .or(delta.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let old_path = delta
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().into_owned())
                .filter(|p| *p != path);
            let mut fd = FileDiff {
                path,
                old_path,
                binary: delta.flags().is_binary(),
                hunks: Vec::new(),
            };
            let Some(patch) = Patch::from_diff(diff, idx)? else {
                fd.binary = true;
                files.push(fd);
                continue;
            };
            for h in 0..patch.num_hunks() {
                let (hunk, n_lines) = patch.hunk(h)?;
                let mut lines = Vec::with_capacity(n_lines);
                for l in 0..n_lines {
                    let line = patch.line_in_hunk(h, l)?;
                    let origin = match line.origin() {
                        '+' | '-' | ' ' => line.origin(),
                        '=' | '>' | '<' => '\\',
                        _ => continue,
                    };
                    let mut content = String::from_utf8_lossy(line.content()).into_owned();
                    if origin == '\\' {
                        content = "No newline at end of file".into();
                    } else if content.ends_with('\n') {
                        content.pop();
                        if content.ends_with('\r') {
                            content.pop();
                        }
                    }
                    lines.push(DiffLine {
                        origin,
                        old_no: line.old_lineno(),
                        new_no: line.new_lineno(),
                        content,
                    });
                }
                fd.hunks.push(Hunk {
                    header: String::from_utf8_lossy(hunk.header()).trim_end().to_owned(),
                    old_start: hunk.old_start(),
                    new_start: hunk.new_start(),
                    lines,
                });
            }
            files.push(fd);
        }
        Ok(files)
    }

    pub fn head_message(&self) -> Option<String> {
        self.repo
            .head()
            .ok()?
            .peel_to_commit()
            .ok()
            .map(|c| lossy(c.message_bytes()).trim_end().to_owned())
    }

    /// The message git prepared for a merge/cherry-pick/revert in progress.
    pub fn prepared_message(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.git_dir().join("MERGE_MSG")).ok()?;
        let msg: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        Some(msg.join("\n").trim().to_owned()).filter(|m| !m.is_empty())
    }
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn delta_kind(d: Delta) -> ChangeKind {
    match d {
        Delta::Added | Delta::Copied => ChangeKind::Added,
        Delta::Deleted => ChangeKind::Deleted,
        Delta::Renamed => ChangeKind::Renamed,
        Delta::Typechange => ChangeKind::Typechange,
        Delta::Untracked => ChangeKind::Untracked,
        Delta::Conflicted => ChangeKind::Conflicted,
        _ => ChangeKind::Modified,
    }
}
