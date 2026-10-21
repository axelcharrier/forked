pub mod changes;
pub mod checkout;
pub mod dialogs;
pub mod diff;
pub mod file_tree;
pub mod history;
pub mod icons;
pub mod sidebar;
pub mod theme;
pub mod toolbar;
pub mod widgets;

use std::path::PathBuf;

use crate::git::patch::PatchMode;
use crate::git::{DiffSide, Oid};

#[derive(Clone, Debug)]
pub struct Step {
    pub args: Vec<String>,
    pub stdin: Option<String>,
}

/// What to do once an operation has succeeded.
#[derive(Clone, Debug, PartialEq)]
pub enum After {
    Nothing,
    ClearCommitMessage,
    OpenRepo(PathBuf),
}

/// One or more git commands run in sequence on a background thread.
#[derive(Clone, Debug)]
pub struct GitOp {
    pub label: String,
    pub steps: Vec<Step>,
    /// Defaults to the repository of the tab that started it.
    pub cwd: Option<PathBuf>,
    pub after: After,
    /// No notification on success (for frequent operations like staging).
    pub quiet: bool,
}

impl GitOp {
    pub fn new<I, S>(label: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            label: label.into(),
            steps: vec![Step {
                args: args.into_iter().map(Into::into).collect(),
                stdin: None,
            }],
            cwd: None,
            after: After::Nothing,
            quiet: false,
        }
    }

    pub fn then<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.steps.push(Step {
            args: args.into_iter().map(Into::into).collect(),
            stdin: None,
        });
        self
    }

    /// Feeds `input` to the last step's stdin.
    pub fn stdin(mut self, input: String) -> Self {
        if let Some(s) = self.steps.last_mut() {
            s.stdin = Some(input);
        }
        self
    }

    pub fn after(mut self, after: After) -> Self {
        self.after = after;
        self
    }

    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    pub fn cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Changes,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeMode {
    Default,
    NoFastForward,
    FastForwardOnly,
    Squash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

impl ResetMode {
    pub fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

pub enum Dialog {
    Checkout {
        /// Local branch that will be current afterwards.
        target: String,
        note: Option<String>,
        args: Vec<String>,
        has_changes: bool,
        changes: checkout::LocalChanges,
    },
    NewBranch {
        start: String,
        name: String,
        checkout: bool,
    },
    RenameBranch {
        old: String,
        name: String,
    },
    NewTag {
        target: String,
        name: String,
        message: String,
        push: bool,
    },
    Stash {
        message: String,
        untracked: bool,
    },
    Push {
        remote: String,
        branch: String,
        remote_branch: String,
        set_upstream: bool,
        force: bool,
        tags: bool,
    },
    Pull {
        remote: String,
        branch: String,
        rebase: bool,
    },
    Merge {
        source: String,
        mode: MergeMode,
    },
    Reset {
        target: String,
        summary: String,
        mode: ResetMode,
    },
    AddRemote {
        name: String,
        url: String,
    },
    Clone {
        url: String,
        parent: String,
        name: String,
    },
    Confirm {
        title: String,
        message: String,
        op: GitOp,
    },
}

pub enum Action {
    Git(GitOp),
    Confirm {
        title: String,
        message: String,
        op: GitOp,
    },
    Dialog(Dialog),
    Refresh,
    SetView(View),
    SelectCommit(Oid),
    /// Select a commit and scroll the history to it.
    RevealCommit(Oid),
    /// Show a commit outside the history list (e.g. a stash).
    ShowCommit(Oid),
    SelectCommitFile(usize),
    SelectChange(DiffSide, String),
    Patch {
        mode: PatchMode,
        hunk: usize,
        lines: Option<Vec<usize>>,
    },
    Commit,
    Ignore(String),
    Copy(String),
    Open(PathBuf),
    Reveal(PathBuf),
    OpenTerminal,
    OpenRepo,
    OpenRepoAt(PathBuf),
    InitRepo,
    CloneRepo,
    NewTab,
    SwitchTab(usize),
    CloseTab(usize),
}

pub fn format_date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// `feature/login` → ("feature/", "login"), used for tree display.
pub fn split_last(name: &str) -> (&str, &str) {
    match name.rfind('/') {
        Some(i) => (&name[..=i], &name[i + 1..]),
        None => ("", name),
    }
}

/// A context-menu entry that closes the menu when clicked.
pub fn menu_item(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> bool {
    let clicked = ui.button(label).clicked();
    if clicked {
        ui.close();
    }
    clicked
}
