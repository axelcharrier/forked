use egui::{Align, Button, Color32, Frame, Layout, Margin, RichText, Ui};

use super::sidebar::{pull_dialog, push_dialog};
use super::widgets::tool_button;
use super::{Action, Dialog, GitOp, icons, theme};
use crate::git::repo::RepoState;
use crate::tab::RepoTab;

pub fn show(ui: &mut Ui, tab: &RepoTab, busy: &[String], actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add_space(6.0);
        let has_remote = !tab.remotes.is_empty();
        let branch = tab.current_branch();
        let (ahead, behind) = branch.map_or((0, 0), |b| (b.ahead, b.behind));
        if tool_button(ui, icons::FETCH, "Fetch", has_remote, 0)
            .on_hover_text("Récupérer tous les remotes")
            .clicked()
        {
            actions.push(Action::Git(GitOp::new("Fetch", ["fetch", "--all", "--prune"])));
        }
        if tool_button(ui, icons::PULL, "Pull", has_remote && branch.is_some(), behind)
            .on_hover_text("Récupérer et intégrer la branche distante")
            .clicked()
        {
            actions.push(Action::Dialog(pull_dialog(tab)));
        }
        if tool_button(ui, icons::PUSH, "Push", has_remote && branch.is_some(), ahead)
            .on_hover_text("Envoyer la branche courante")
            .clicked()
            && let Some(b) = branch
        {
            actions.push(Action::Dialog(push_dialog(tab, b)));
        }
        ui.add_space(10.0);
        if tool_button(ui, icons::STASH, "Stash", tab.status.total() > 0, 0)
            .on_hover_text("Mettre de côté les modifications locales")
            .clicked()
        {
            actions.push(Action::Dialog(Dialog::Stash {
                message: String::new(),
                untracked: true,
            }));
        }
        if tool_button(ui, icons::POP, "Pop", !tab.stashes.is_empty(), 0)
            .on_hover_text("Réappliquer le dernier stash")
            .clicked()
        {
            actions.push(Action::Git(GitOp::new("Pop du stash", ["stash", "pop"])));
        }
        ui.add_space(10.0);
        if tool_button(ui, icons::BRANCH, "Branche", tab.head.oid.is_some(), 0)
            .on_hover_text("Créer une branche depuis HEAD")
            .clicked()
        {
            actions.push(Action::Dialog(Dialog::NewBranch {
                start: "HEAD".into(),
                name: String::new(),
                checkout: true,
            }));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(6.0);
            if tool_button(ui, icons::TERMINAL, "Terminal", true, 0).clicked() {
                actions.push(Action::OpenTerminal);
            }
            if tool_button(ui, icons::FOLDER_OPEN, crate::platform::FILE_MANAGER, true, 0).clicked() {
                actions.push(Action::Open(tab.path().clone()));
            }
            if tool_button(ui, icons::REFRESH, "Actualiser", true, 0)
                .on_hover_text(crate::platform::shortcut("R"))
                .clicked()
            {
                actions.push(Action::Refresh);
            }
            ui.add_space(12.0);
            branch_chip(ui, tab);
            if !busy.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(busy.join(" · ")).color(p.weak).size(12.0));
                ui.spinner();
            }
        });
    });

    if tab.head.state != RepoState::Clean {
        state_banner(ui, tab, actions);
    }
}

/// Current branch, like Fork's title area.
fn branch_chip(ui: &mut Ui, tab: &RepoTab) {
    let p = theme::of(ui);
    let (icon, text) = match (&tab.head.branch, tab.head.detached) {
        (Some(b), _) => (icons::BRANCH, b.clone()),
        (None, true) => (
            icons::COMMIT,
            format!(
                "HEAD détaché · {}",
                tab.head.oid.map(|o| o.to_string()[..7].to_owned()).unwrap_or_default()
            ),
        ),
        (None, false) => (icons::BRANCH, "—".into()),
    };
    let label = RichText::new(format!("{icon}  {text}")).strong().color(p.text);
    Frame::new()
        .fill(p.hover)
        .corner_radius(6.0)
        .inner_margin(Margin::symmetric(10, 5))
        .show(ui, |ui| ui.label(label));
}

fn state_banner(ui: &mut Ui, tab: &RepoTab, actions: &mut Vec<Action>) {
    let (text, cmd) = match tab.head.state {
        RepoState::Merging => ("Fusion en cours", "merge"),
        RepoState::Rebasing => ("Rebase en cours", "rebase"),
        RepoState::CherryPicking => ("Cherry-pick en cours", "cherry-pick"),
        RepoState::Reverting => ("Revert en cours", "revert"),
        RepoState::Bisecting => ("Bisect en cours", "bisect"),
        RepoState::Clean => return,
    };
    let conflicts = tab
        .status
        .unstaged
        .iter()
        .filter(|f| f.kind == crate::git::ChangeKind::Conflicted)
        .count();
    ui.add_space(4.0);
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(230, 160, 40, 45))
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let detail = if conflicts > 0 {
                    format!("{}  {text} — {conflicts} conflit(s) à résoudre", icons::WARNING)
                } else {
                    format!("{}  {text} — aucun conflit restant", icons::WARNING)
                };
                ui.label(RichText::new(detail).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if cmd == "bisect" {
                        if ui.button("Terminer le bisect").clicked() {
                            actions.push(Action::Git(GitOp::new("Bisect reset", ["bisect", "reset"])));
                        }
                        return;
                    }
                    if ui.button("Abandonner").clicked() {
                        actions.push(Action::Confirm {
                            title: format!("Abandonner : {text}"),
                            message: "Le dépôt reviendra à son état d'avant l'opération.".into(),
                            op: GitOp::new(format!("{cmd} --abort"), [cmd, "--abort"]),
                        });
                    }
                    if cmd == "rebase" && ui.button("Passer ce commit").clicked() {
                        actions.push(Action::Git(GitOp::new("rebase --skip", ["rebase", "--skip"])));
                    }
                    if ui.add_enabled(conflicts == 0, Button::new("Continuer")).clicked() {
                        actions.push(Action::Git(GitOp::new(
                            format!("{cmd} --continue"),
                            [cmd, "--continue"],
                        )));
                    }
                });
            });
        });
}
