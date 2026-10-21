use std::collections::BTreeMap;

use egui::{Id, ScrollArea, Ui};

use super::widgets::{self, Row};
use super::{Action, Dialog, GitOp, MergeMode, View, checkout, icons, menu_item, split_last, theme};
use crate::git::BranchInfo;
use crate::tab::RepoTab;

/// Folder tree of slash-separated names (`feature/login` → feature ▸ login).
#[derive(Default)]
struct Tree<'a> {
    folders: BTreeMap<&'a str, Tree<'a>>,
    leaves: Vec<usize>,
}

impl<'a> Tree<'a> {
    fn build(names: impl Iterator<Item = (usize, &'a str)>) -> Self {
        let mut root = Tree::default();
        for (i, name) in names {
            let mut node = &mut root;
            let mut parts: Vec<&str> = name.split('/').collect();
            parts.pop();
            for p in parts {
                node = node.folders.entry(p).or_default();
            }
            node.leaves.push(i);
        }
        root
    }

    fn show(&self, ui: &mut Ui, id: Id, depth: usize, leaf: &mut dyn FnMut(&mut Ui, usize, usize)) {
        let p = theme::of(ui);
        for (name, child) in &self.folders {
            let child_id = id.with(name);
            let open = widgets::folder_open(ui, child_id, false);
            let resp = Row::new(name)
                .depth(depth)
                .disclosure(open)
                .icon(icons::FOLDER_SIMPLE, p.weak)
                .show(ui);
            if resp.clicked() {
                widgets::toggle_folder(ui, child_id);
            }
            if open {
                child.show(ui, child_id, depth + 1, leaf);
            }
        }
        for &i in &self.leaves {
            leaf(ui, i, depth);
        }
    }
}

pub fn show(ui: &mut Ui, tab: &RepoTab, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(8.0);
        let changes = tab.status.total();
        let resp = Row::new("Modifications locales")
            .icon(icons::CHANGES, theme::kind_color(crate::git::ChangeKind::Modified))
            .trailing(if changes > 0 { changes.to_string() } else { String::new() })
            .selected(tab.view == View::Changes)
            .show(ui);
        if resp.clicked() {
            actions.push(Action::SetView(View::Changes));
        }
        let resp = Row::new("Tous les commits")
            .icon(icons::HISTORY, p.accent)
            .selected(tab.view == View::History)
            .show(ui);
        if resp.clicked() {
            actions.push(Action::SetView(View::History));
        }

        let (open, _) = widgets::section(ui, "Branches", Some(tab.local.len()), true);
        if open {
            Tree::build(tab.local.iter().enumerate().map(|(i, b)| (i, b.name.as_str()))).show(
                ui,
                Id::unique("local_tree"),
                0,
                &mut |ui, i, depth| local_branch(ui, tab, &tab.local[i], depth, actions),
            );
        }

        let (open, header) = widgets::section(ui, "Remotes", Some(tab.remotes.len()), true);
        header.context_menu(|ui| {
            if menu_item(ui, "Ajouter un remote…") {
                actions.push(Action::Dialog(Dialog::AddRemote {
                    name: if tab.remotes.is_empty() { "origin".into() } else { String::new() },
                    url: String::new(),
                }));
            }
        });
        if open {
            for remote in &tab.remotes {
                remote_rows(ui, tab, remote, actions);
            }
        }

        let (open, _) = widgets::section(ui, "Tags", Some(tab.tags.len()), false);
        if open {
            for t in &tab.tags {
                let resp = Row::new(&t.name).icon(icons::TAG, theme::ref_color(crate::git::RefKind::Tag)).show(ui);
                if resp.clicked()
                    && let Some(oid) = t.target
                {
                    actions.push(Action::RevealCommit(oid));
                }
                resp.context_menu(|ui| tag_menu(ui, tab, &t.name, actions));
            }
        }

        let (open, _) = widgets::section(ui, "Stashes", Some(tab.stashes.len()), true);
        if open {
            for s in &tab.stashes {
                let name = format!("stash@{{{}}}", s.index);
                let resp = Row::new(&s.message)
                    .icon(icons::STASH, p.weak)
                    .selected(tab.selected_commit == Some(s.id) && tab.view == View::History)
                    .show(ui)
                    .on_hover_text(&name);
                if resp.clicked() {
                    actions.push(Action::ShowCommit(s.id));
                }
                resp.context_menu(|ui| {
                    if menu_item(ui, "Appliquer") {
                        actions.push(Action::Git(GitOp::new("Appliquer le stash", ["stash", "apply", &name])));
                    }
                    if menu_item(ui, "Appliquer et supprimer (pop)") {
                        actions.push(Action::Git(GitOp::new("Pop du stash", ["stash", "pop", &name])));
                    }
                    ui.separator();
                    if menu_item(ui, "Supprimer…") {
                        actions.push(Action::Confirm {
                            title: "Supprimer le stash".into(),
                            message: format!("Supprimer définitivement « {} » ?", s.message),
                            op: GitOp::new("Supprimer le stash", ["stash", "drop", &name]),
                        });
                    }
                });
            }
        }
        ui.add_space(12.0);
    });
}

fn remote_rows(ui: &mut Ui, tab: &RepoTab, remote: &str, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let id = Id::unique(("remote", remote));
    let open = widgets::folder_open(ui, id, remote == "origin");
    let resp = Row::new(remote)
        .disclosure(open)
        .icon(icons::CLOUD, p.weak)
        .show(ui)
        .on_hover_text(tab.repo.remote_url(remote).unwrap_or_default());
    if resp.clicked() {
        widgets::toggle_folder(ui, id);
    }
    resp.context_menu(|ui| {
        if menu_item(ui, format!("Fetch {remote}")) {
            actions.push(Action::Git(GitOp::new(format!("Fetch {remote}"), ["fetch", "--prune", remote])));
        }
        if let Some(url) = tab.repo.remote_url(remote)
            && menu_item(ui, "Copier l'URL")
        {
            actions.push(Action::Copy(url));
        }
        ui.separator();
        if menu_item(ui, "Supprimer le remote…") {
            actions.push(Action::Confirm {
                title: "Supprimer le remote".into(),
                message: format!("Supprimer « {remote} » et ses branches de suivi ?"),
                op: GitOp::new(format!("Supprimer {remote}"), ["remote", "remove", remote]),
            });
        }
    });
    if !open {
        return;
    }
    let prefix = format!("{remote}/");
    let branches: Vec<(usize, &str)> = tab
        .remote_branches
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.name.strip_prefix(&prefix).map(|n| (i, n)))
        .collect();
    Tree::build(branches.into_iter()).show(ui, id, 1, &mut |ui, i, depth| {
        remote_branch(ui, tab, remote, &tab.remote_branches[i], depth, actions)
    });
}

fn local_branch(ui: &mut Ui, tab: &RepoTab, b: &BranchInfo, depth: usize, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let (_, short) = split_last(&b.name);
    let mut sync = String::new();
    if b.ahead > 0 {
        sync += &format!("↑{}", b.ahead);
    }
    if b.behind > 0 {
        if !sync.is_empty() {
            sync.push(' ');
        }
        sync += &format!("↓{}", b.behind);
    }
    let (icon, color) = if b.is_head {
        (icons::CHECK, theme::ref_color(crate::git::RefKind::Head))
    } else {
        (icons::BRANCH, p.weak)
    };
    let hover = match &b.upstream {
        Some(up) => format!("{} → {up}\nDouble-clic : checkout", b.name),
        None => format!("{} (pas de branche distante)\nDouble-clic : checkout", b.name),
    };
    let resp = Row::new(short)
        .depth(depth)
        .icon(icon, color)
        .strong(b.is_head)
        .trailing(sync)
        .show(ui)
        .on_hover_text(hover);
    if resp.double_clicked() {
        if let Some(d) = checkout::dialog(tab, &b.name, false) {
            actions.push(Action::Dialog(d));
        }
    } else if resp.clicked()
        && let Some(oid) = b.target
    {
        actions.push(Action::RevealCommit(oid));
    }
    resp.context_menu(|ui| local_branch_menu(ui, tab, b, actions));
}

fn local_branch_menu(ui: &mut Ui, tab: &RepoTab, b: &BranchInfo, actions: &mut Vec<Action>) {
    let name = b.name.clone();
    let current = tab.head.branch.clone().unwrap_or_else(|| "HEAD".into());
    if !b.is_head {
        if menu_item(ui, format!("Checkout {name}…"))
            && let Some(d) = checkout::dialog(tab, &name, false)
        {
            actions.push(Action::Dialog(d));
        }
        if menu_item(ui, format!("Fusionner {name} dans {current}…")) {
            actions.push(Action::Dialog(Dialog::Merge {
                source: name.clone(),
                mode: MergeMode::Default,
            }));
        }
        if menu_item(ui, format!("Rebaser {current} sur {name}…")) {
            actions.push(Action::Confirm {
                title: "Rebase".into(),
                message: format!("Rejouer les commits de {current} au-dessus de {name} ?"),
                op: GitOp::new(format!("Rebase sur {name}"), ["rebase", &name]),
            });
        }
        ui.separator();
    }
    if !tab.remotes.is_empty() && menu_item(ui, format!("Push {name}…")) {
        actions.push(Action::Dialog(push_dialog(tab, b)));
    }
    if b.is_head && b.upstream.is_some() && menu_item(ui, "Pull…") {
        actions.push(Action::Dialog(pull_dialog(tab)));
    }
    if menu_item(ui, "Nouvelle branche à partir d'ici…") {
        actions.push(Action::Dialog(Dialog::NewBranch {
            start: name.clone(),
            name: String::new(),
            checkout: true,
        }));
    }
    if menu_item(ui, "Renommer…") {
        actions.push(Action::Dialog(Dialog::RenameBranch {
            old: name.clone(),
            name: name.clone(),
        }));
    }
    if b.upstream.is_some() && menu_item(ui, "Ne plus suivre la branche distante") {
        actions.push(Action::Git(GitOp::new(
            "Supprimer le suivi",
            ["branch", "--unset-upstream", &name],
        )));
    }
    if !b.is_head {
        ui.separator();
        if menu_item(ui, "Supprimer…") {
            actions.push(Action::Confirm {
                title: "Supprimer la branche".into(),
                message: format!("Supprimer la branche locale « {name} » ? (refusé si elle n'est pas fusionnée)"),
                op: GitOp::new(format!("Supprimer {name}"), ["branch", "-d", &name]),
            });
        }
        if menu_item(ui, "Supprimer (forcer)…") {
            actions.push(Action::Confirm {
                title: "Supprimer la branche".into(),
                message: format!("Supprimer « {name} » même si elle contient des commits non fusionnés ?"),
                op: GitOp::new(format!("Supprimer {name}"), ["branch", "-D", &name]),
            });
        }
    }
    ui.separator();
    if menu_item(ui, "Copier le nom") {
        actions.push(Action::Copy(name));
    }
}

fn remote_branch(
    ui: &mut Ui,
    tab: &RepoTab,
    remote: &str,
    b: &BranchInfo,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let p = theme::of(ui);
    let (_, short) = split_last(&b.name);
    let resp = Row::new(short)
        .depth(depth)
        .icon(icons::BRANCH, p.weak)
        .show(ui)
        .on_hover_text(format!("{}\nDouble-clic : checkout", b.name));
    let name = b.name.clone();
    let branch = name[remote.len() + 1..].to_owned();
    if resp.double_clicked() {
        if let Some(d) = checkout::dialog(tab, &name, true) {
            actions.push(Action::Dialog(d));
        }
    } else if resp.clicked()
        && let Some(oid) = b.target
    {
        actions.push(Action::RevealCommit(oid));
    }
    let current = tab.head.branch.clone().unwrap_or_else(|| "HEAD".into());
    resp.context_menu(|ui| {
        if menu_item(ui, format!("Checkout {branch}…"))
            && let Some(d) = checkout::dialog(tab, &name, true)
        {
            actions.push(Action::Dialog(d));
        }
        if menu_item(ui, format!("Fusionner {name} dans {current}…")) {
            actions.push(Action::Dialog(Dialog::Merge {
                source: name.clone(),
                mode: MergeMode::Default,
            }));
        }
        if menu_item(ui, format!("Rebaser {current} sur {name}…")) {
            actions.push(Action::Confirm {
                title: "Rebase".into(),
                message: format!("Rejouer les commits de {current} au-dessus de {name} ?"),
                op: GitOp::new(format!("Rebase sur {name}"), ["rebase", &name]),
            });
        }
        if menu_item(ui, "Nouvelle branche à partir d'ici…") {
            actions.push(Action::Dialog(Dialog::NewBranch {
                start: name.clone(),
                name: branch.clone(),
                checkout: true,
            }));
        }
        ui.separator();
        if menu_item(ui, format!("Supprimer {branch} sur {remote}…")) {
            actions.push(Action::Confirm {
                title: "Supprimer la branche distante".into(),
                message: format!("Supprimer « {branch} » sur le serveur « {remote} » ?"),
                op: GitOp::new(format!("Supprimer {name}"), ["push", remote, "--delete", &branch]),
            });
        }
        if menu_item(ui, "Copier le nom") {
            actions.push(Action::Copy(name.clone()));
        }
    });
}

fn tag_menu(ui: &mut Ui, tab: &RepoTab, name: &str, actions: &mut Vec<Action>) {
    let refname = format!("refs/tags/{name}");
    if menu_item(ui, format!("Checkout {name}…")) {
        actions.push(Action::Confirm {
            title: "Checkout d'un tag".into(),
            message: format!("HEAD sera détaché sur le tag « {name} »."),
            op: GitOp::new(format!("Checkout {name}"), ["checkout", &refname]),
        });
    }
    if let Some(remote) = tab.default_remote() {
        if menu_item(ui, format!("Push vers {remote}")) {
            actions.push(Action::Git(GitOp::new(format!("Push {name}"), ["push", &remote, &refname])));
        }
        if menu_item(ui, format!("Supprimer sur {remote}…")) {
            actions.push(Action::Confirm {
                title: "Supprimer le tag distant".into(),
                message: format!("Supprimer le tag « {name} » sur « {remote} » ?"),
                op: GitOp::new(format!("Supprimer {name}"), ["push", &remote, "--delete", &refname]),
            });
        }
    }
    if menu_item(ui, "Supprimer…") {
        actions.push(Action::Confirm {
            title: "Supprimer le tag".into(),
            message: format!("Supprimer le tag local « {name} » ?"),
            op: GitOp::new(format!("Supprimer {name}"), ["tag", "-d", name]),
        });
    }
    ui.separator();
    if menu_item(ui, "Copier le nom") {
        actions.push(Action::Copy(name.to_owned()));
    }
}

pub fn push_dialog(tab: &RepoTab, b: &BranchInfo) -> Dialog {
    let upstream = b.upstream.as_deref();
    let remote = upstream
        .and_then(|u| tab.remotes.iter().find(|r| u.starts_with(&format!("{r}/"))))
        .cloned()
        .or_else(|| tab.default_remote())
        .unwrap_or_default();
    let remote_branch = upstream
        .and_then(|u| u.strip_prefix(&format!("{remote}/")))
        .unwrap_or(&b.name)
        .to_owned();
    Dialog::Push {
        remote,
        branch: b.name.clone(),
        remote_branch,
        set_upstream: upstream.is_none(),
        force: false,
        tags: false,
    }
}

pub fn pull_dialog(tab: &RepoTab) -> Dialog {
    let remote = tab.default_remote().unwrap_or_default();
    let branch = tab
        .current_branch()
        .and_then(|b| {
            b.upstream
                .as_deref()
                .and_then(|u| u.strip_prefix(&format!("{remote}/")))
                .map(str::to_owned)
                .or_else(|| Some(b.name.clone()))
        })
        .unwrap_or_default();
    Dialog::Pull {
        remote,
        branch,
        rebase: false,
    }
}
