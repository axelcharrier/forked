//! Fork's "Checkout — switch to another branch" flow.

use super::{Dialog, GitOp};
use crate::tab::RepoTab;

/// What to do with uncommitted changes when switching branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalChanges {
    Keep,
    Stash,
    Discard,
}

/// Dialog for checking out `name`, a local branch or a remote one
/// (`origin/feature`). `None` when it is already the current branch.
pub fn dialog(tab: &RepoTab, name: &str, remote: bool) -> Option<Dialog> {
    let (target, args, note) = if remote {
        let branch = tab
            .remotes
            .iter()
            .find_map(|r| name.strip_prefix(&format!("{r}/")))
            .unwrap_or(name)
            .to_owned();
        if tab.local.iter().any(|l| l.name == branch) {
            let note = format!("La branche locale existante sera utilisée (pas {name}).");
            (branch.clone(), vec!["checkout".into(), branch], Some(note))
        } else {
            let note = format!("Crée la branche locale « {branch} » qui suit {name}.");
            (branch, vec!["checkout".into(), "--track".into(), name.to_owned()], Some(note))
        }
    } else {
        (name.to_owned(), vec!["checkout".into(), name.to_owned()], None)
    };
    if tab.head.branch.as_deref() == Some(target.as_str()) {
        return None;
    }
    Some(Dialog::Checkout {
        target,
        note,
        args,
        has_changes: tab.status.total() > 0,
        changes: LocalChanges::Keep,
    })
}

pub fn op(target: &str, args: &[String], changes: LocalChanges, has_changes: bool) -> GitOp {
    let label = format!("Checkout {target}");
    match (changes, has_changes) {
        (LocalChanges::Stash, true) => GitOp::new(
            label,
            [
                "stash",
                "push",
                "--include-untracked",
                "-m",
                &format!("Forked : avant checkout de {target}"),
            ],
        )
        .then(args.iter().cloned())
        .then(["stash", "pop"]),
        (LocalChanges::Discard, true) => {
            let mut forced = args.to_vec();
            forced.insert(1, "--force".into());
            GitOp::new(label, forced)
        }
        _ => GitOp::new(label, args.iter().cloned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Vec<String> {
        vec!["checkout".into(), "develop".into()]
    }

    #[test]
    fn stash_wraps_checkout_only_when_there_are_changes() {
        let op = op("develop", &args(), LocalChanges::Stash, true);
        let steps: Vec<_> = op.steps.iter().map(|s| s.args[..2].join(" ")).collect();
        assert_eq!(steps, ["stash push", "checkout develop", "stash pop"]);
        // Without changes `stash push` creates nothing and `pop` would apply an older stash.
        let op = super::op("develop", &args(), LocalChanges::Stash, false);
        assert_eq!(op.steps.len(), 1);
    }

    #[test]
    fn discard_forces_the_checkout() {
        let op = op("develop", &args(), LocalChanges::Discard, true);
        assert_eq!(op.steps[0].args, ["checkout", "--force", "develop"]);
    }
}
