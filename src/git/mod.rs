pub mod cli;
pub mod graph;
pub mod patch;
pub mod repo;

pub use git2::Oid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Clone, Debug)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

#[derive(Clone, Debug)]
pub struct CommitInfo {
    pub id: Oid,
    pub short_id: String,
    pub summary: String,
    pub message: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub committer: String,
    pub commit_time: i64,
    pub parents: Vec<Oid>,
    pub refs: Vec<RefLabel>,
}

#[derive(Clone, Debug)]
pub struct BranchInfo {
    /// Short name, e.g. `main` or `origin/main`.
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub target: Option<Oid>,
}

#[derive(Clone, Debug)]
pub struct TagInfo {
    pub name: String,
    pub target: Option<Oid>,
}

#[derive(Clone, Debug)]
pub struct StashInfo {
    pub index: usize,
    pub message: String,
    pub id: Oid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Typechange,
    Untracked,
    Conflicted,
}

impl ChangeKind {
    pub fn letter(self) -> &'static str {
        match self {
            ChangeKind::Added => "A",
            ChangeKind::Modified => "M",
            ChangeKind::Deleted => "D",
            ChangeKind::Renamed => "R",
            ChangeKind::Typechange => "T",
            ChangeKind::Untracked => "?",
            ChangeKind::Conflicted => "C",
        }
    }

    /// Partial (hunk/line) staging only makes sense when both sides of the diff exist.
    pub fn supports_partial(self) -> bool {
        matches!(self, ChangeKind::Modified | ChangeKind::Renamed)
    }
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
}

#[derive(Clone, Debug, Default)]
pub struct WorkingStatus {
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
}

impl WorkingStatus {
    pub fn total(&self) -> usize {
        let mut paths: Vec<&str> = self
            .staged
            .iter()
            .chain(&self.unstaged)
            .map(|f| f.path.as_str())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths.len()
    }

    pub fn list(&self, side: DiffSide) -> &[FileChange] {
        match side {
            DiffSide::Unstaged => &self.unstaged,
            DiffSide::Staged => &self.staged,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    /// One of ' ', '+', '-', or '\\' for "no newline at end of file".
    pub origin: char,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    /// Text without the line ending.
    pub content: String,
    /// The line ends with `\r\n`; kept so partial-staging patches still apply.
    pub crlf: bool,
}

#[derive(Clone, Debug)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, Default)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    pub fn stats(&self) -> (usize, usize) {
        let mut add = 0;
        let mut del = 0;
        for l in self.hunks.iter().flat_map(|h| &h.lines) {
            match l.origin {
                '+' => add += 1,
                '-' => del += 1,
                _ => {}
            }
        }
        (add, del)
    }
}

/// Which diff of a working-copy file we are looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffSide {
    /// Index → working tree.
    Unstaged,
    /// HEAD → index.
    Staged,
}
