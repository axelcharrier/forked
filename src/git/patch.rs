//! Builds patches for hunk and line level staging, fed to `git apply`.

use std::collections::BTreeSet;

use super::FileDiff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchMode {
    /// Apply part of the unstaged diff to the index.
    Stage,
    /// Remove part of the staged diff from the index.
    Unstage,
    /// Revert part of the unstaged diff in the working tree.
    Discard,
}

impl PatchMode {
    pub fn apply_args(self) -> Vec<String> {
        let args: &[&str] = match self {
            PatchMode::Stage => &["apply", "--cached", "--recount", "--whitespace=nowarn", "-"],
            PatchMode::Unstage => &[
                "apply",
                "--cached",
                "--reverse",
                "--recount",
                "--whitespace=nowarn",
                "-",
            ],
            PatchMode::Discard => &["apply", "--reverse", "--recount", "--whitespace=nowarn", "-"],
        };
        args.iter().map(|s| s.to_string()).collect()
    }

    fn reverse(self) -> bool {
        self != PatchMode::Stage
    }
}

/// Patch for hunk `hunk` of `diff`, keeping only `lines` (indices in the hunk)
/// when given. Returns `None` if nothing would change.
pub fn build(
    diff: &FileDiff,
    hunk: usize,
    lines: Option<&BTreeSet<usize>>,
    mode: PatchMode,
) -> Option<String> {
    let h = diff.hunks.get(hunk)?;
    let mut body = String::new();
    let (mut old_count, mut new_count) = (0u32, 0u32);
    let mut changes = 0;
    // Whether the previous line was written, so a "\ No newline" marker
    // only follows a line that is part of the patch.
    let mut last_written = false;

    for (i, l) in h.lines.iter().enumerate() {
        let selected = lines.is_none_or(|s| s.contains(&i));
        // An unselected change must not be applied. When applying forward the
        // patch's old side is the base, so an unselected '-' line still exists
        // there (context) and an unselected '+' line does not (dropped).
        // Applying in reverse swaps the two.
        let origin = match (l.origin, selected, mode.reverse()) {
            ('\\', ..) => {
                if last_written {
                    body.push_str("\\ No newline at end of file\n");
                }
                continue;
            }
            (' ', ..) => ' ',
            (o, true, _) => o,
            ('-', false, false) | ('+', false, true) => ' ',
            _ => {
                last_written = false;
                continue;
            }
        };
        match origin {
            ' ' => {
                old_count += 1;
                new_count += 1;
            }
            '-' => {
                old_count += 1;
                changes += 1;
            }
            _ => {
                new_count += 1;
                changes += 1;
            }
        }
        body.push(origin);
        body.push_str(&l.content);
        if l.crlf {
            body.push('\r');
        }
        body.push('\n');
        last_written = true;
    }
    if changes == 0 {
        return None;
    }
    let old_path = diff.old_path.as_deref().unwrap_or(&diff.path);
    let new_path = &diff.path;
    Some(format!(
        "diff --git a/{old_path} b/{new_path}\n--- a/{old_path}\n+++ b/{new_path}\n@@ -{},{old_count} +{},{new_count} @@\n{body}",
        h.old_start, h.new_start,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{DiffLine, Hunk};

    fn line(origin: char, content: &str) -> DiffLine {
        DiffLine {
            origin,
            old_no: None,
            new_no: None,
            content: content.into(),
            crlf: false,
        }
    }

    fn sample() -> FileDiff {
        FileDiff {
            path: "a.txt".into(),
            old_path: None,
            binary: false,
            hunks: vec![Hunk {
                header: String::new(),
                old_start: 1,
                new_start: 1,
                lines: vec![
                    line(' ', "keep"),
                    line('-', "old1"),
                    line('-', "old2"),
                    line('+', "new1"),
                    line('+', "new2"),
                ],
            }],
        }
    }

    #[test]
    fn whole_hunk() {
        let p = build(&sample(), 0, None, PatchMode::Stage).unwrap();
        assert!(p.contains("@@ -1,3 +1,3 @@\n keep\n-old1\n-old2\n+new1\n+new2\n"));
        assert!(p.starts_with("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n"));
    }

    #[test]
    fn stage_selected_lines() {
        let sel = BTreeSet::from([1, 3]);
        let p = build(&sample(), 0, Some(&sel), PatchMode::Stage).unwrap();
        assert!(p.contains("@@ -1,3 +1,3 @@\n keep\n-old1\n old2\n+new1\n"));
        assert!(!p.contains("new2"));
    }

    #[test]
    fn unstage_selected_lines_is_built_for_reverse() {
        let sel = BTreeSet::from([1]);
        let p = build(&sample(), 0, Some(&sel), PatchMode::Unstage).unwrap();
        // Unselected '+' lines exist in the index, so they become context.
        assert!(p.contains("@@ -1,4 +1,3 @@\n keep\n-old1\n new1\n new2\n"));
        assert!(!p.contains("old2"));
    }

    /// End-to-end against a real repository and `git apply`.
    #[test]
    fn partial_stage_and_unstage_with_git() {
        use crate::git::repo::GitRepo;
        use crate::git::{DiffSide, cli};

        let dir = std::env::temp_dir().join(format!("forked-patch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            cli::run(&dir, &args, None).unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(dir.join("f.txt"), "a\nb\nc\nd\ne\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "init"]);
        std::fs::write(dir.join("f.txt"), "a\nB\nc\nD\ne\n").unwrap();

        let repo = GitRepo::open(&dir).unwrap();
        let diff = repo.working_diff("f.txt", DiffSide::Unstaged, 3).unwrap();
        let hunk = &diff.hunks[0];
        // Select only the change on line 2 ("-b" and "+B").
        let sel: BTreeSet<usize> = hunk
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.content == "b" || l.content == "B")
            .map(|(i, _)| i)
            .collect();
        let p = build(&diff, 0, Some(&sel), PatchMode::Stage).unwrap();
        cli::run(&dir, &PatchMode::Stage.apply_args(), Some(&p)).unwrap();
        let staged = git(&["diff", "--cached", "--unified=0"]);
        assert!(staged.contains("-b\n+B"), "{staged}");
        assert!(!staged.contains("+D"), "{staged}");

        // Now unstage that same line again.
        let staged_diff = repo.working_diff("f.txt", DiffSide::Staged, 3).unwrap();
        let p = build(&staged_diff, 0, None, PatchMode::Unstage).unwrap();
        cli::run(&dir, &PatchMode::Unstage.apply_args(), Some(&p)).unwrap();
        assert!(git(&["diff", "--cached"]).is_empty());
        // The working tree still has both changes.
        assert!(git(&["diff"]).contains("+D"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// CRLF files must keep their `\r` or `git apply` rejects the patch.
    #[test]
    fn partial_stage_of_a_crlf_file() {
        use crate::git::repo::GitRepo;
        use crate::git::{DiffSide, cli};

        let dir = std::env::temp_dir().join(format!("forked-crlf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            cli::run(&dir, &args, None).unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "core.autocrlf", "false"]);
        std::fs::write(dir.join("f.txt"), "a\r\nb\r\nc\r\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "init"]);
        std::fs::write(dir.join("f.txt"), "a\r\nB\r\nc\r\nd\r\n").unwrap();

        let repo = GitRepo::open(&dir).unwrap();
        let diff = repo.working_diff("f.txt", DiffSide::Unstaged, 3).unwrap();
        let sel: BTreeSet<usize> = diff.hunks[0]
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.content == "b" || l.content == "B")
            .map(|(i, _)| i)
            .collect();
        let p = build(&diff, 0, Some(&sel), PatchMode::Stage).unwrap();
        cli::run(&dir, &PatchMode::Stage.apply_args(), Some(&p)).unwrap();
        let staged = git(&["show", ":f.txt"]);
        assert_eq!(staged, "a\r\nB\r\nc");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_context_selected_yields_nothing() {
        let sel = BTreeSet::from([0]);
        assert!(build(&sample(), 0, Some(&sel), PatchMode::Stage).is_none());
    }
}
