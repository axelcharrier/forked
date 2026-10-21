use egui::{
    CentralPanel, FontId, Frame, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers, Panel,
    RichText, ScrollArea, TextEdit, Ui,
};

use super::diff::{self, DiffMode};
use super::file_tree::{self, ListMode, Target};
use super::{Action, GitOp, icons, menu_item, theme, widgets};
use crate::git::{ChangeKind, DiffSide, FileChange};
use crate::tab::RepoTab;

pub fn show(ui: &mut Ui, tab: &mut RepoTab, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    Panel::left("changes_lists")
        .resizable(true)
        .default_size(400.0)
        .min_size(280.0)
        .frame(Frame::new().fill(p.content))
        .show(ui, |ui| {
            Panel::bottom("commit_box")
                .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(10)))
                .show(ui, |ui| commit_box(ui, tab, actions));
            let half = ui.available_height() / 2.0;
            Panel::top("unstaged_list")
                .resizable(true)
                .default_size(half)
                .min_size(90.0)
                .frame(Frame::new().fill(p.content))
                .show(ui, |ui| {
                    if let Some(mode) = pane(ui, tab, DiffSide::Unstaged, actions) {
                        tab.list_mode = mode;
                    }
                });
            CentralPanel::default()
                .frame(Frame::new().fill(p.content))
                .show(ui, |ui| {
                    if let Some(mode) = pane(ui, tab, DiffSide::Staged, actions) {
                        tab.list_mode = mode;
                    }
                });
        });
    CentralPanel::default()
        .frame(Frame::new().fill(p.content))
        .show(ui, |ui| diff_area(ui, tab, actions));
}

fn diff_area(ui: &mut Ui, tab: &mut RepoTab, actions: &mut Vec<Action>) {
    let Some(diff) = &tab.change_diff else {
        ui.centered_and_justified(|ui| {
            if tab.status.total() == 0 {
                ui.weak("Aucune modification locale")
            } else {
                ui.weak("Sélectionnez un fichier")
            }
        });
        return;
    };
    let Some((side, path)) = tab.selected_change.clone() else {
        return;
    };
    let file = tab.status.list(side).iter().find(|f| f.path == path).cloned();
    let kind = file.as_ref().map(|f| f.kind);
    let partial = kind.is_some_and(ChangeKind::supports_partial);
    let mode = match (side, kind) {
        (_, Some(ChangeKind::Conflicted)) => DiffMode::ReadOnly,
        (DiffSide::Unstaged, _) => DiffMode::Unstaged { partial },
        (DiffSide::Staged, _) => DiffMode::Staged { partial },
    };
    let selected = tab.selected_lines.len();
    diff::header(ui, diff, |ui| {
        let Some(file) = &file else { return };
        if file.kind == ChangeKind::Conflicted {
            for (label, op) in conflict_ops(&file.path) {
                if ui.button(label).clicked() {
                    actions.push(Action::Git(op));
                }
            }
            return;
        }
        match side {
            DiffSide::Unstaged => {
                if ui.button("Annuler…").clicked() {
                    actions.push(discard_action(file));
                }
                if ui.button("Indexer le fichier").clicked() {
                    actions.push(Action::Git(stage_op(file)));
                }
            }
            DiffSide::Staged => {
                if ui.button("Désindexer le fichier").clicked() {
                    actions.push(Action::Git(unstage_op(tab, file)));
                }
            }
        }
        if mode.partial() {
            if selected > 0 {
                ui.weak(format!("{selected} ligne(s) sélectionnée(s)"));
            } else {
                ui.weak("Cliquez des lignes pour les traiter séparément");
            }
        }
    });
    diff::show(
        ui,
        diff,
        mode,
        &mut tab.selected_lines,
        &mut tab.line_anchor,
        actions,
    );
}

pub fn stage_op(file: &FileChange) -> GitOp {
    GitOp::new(
        format!("Indexer {}", file.path),
        ["add", "-A", "--", &file.path],
    )
    .quiet()
}

fn unstage_paths(tab: &RepoTab, label: String, paths: Vec<String>) -> GitOp {
    let mut args: Vec<String> = if tab.head.oid.is_none() {
        // No HEAD to reset to before the first commit.
        vec!["rm".into(), "-r".into(), "--cached".into(), "-q".into(), "--".into()]
    } else {
        vec!["reset".into(), "-q".into(), "--".into()]
    };
    args.extend(paths);
    GitOp::new(label, args).quiet()
}

pub fn unstage_op(tab: &RepoTab, file: &FileChange) -> GitOp {
    let mut paths = vec![file.path.clone()];
    paths.extend(file.old_path.clone());
    unstage_paths(tab, format!("Désindexer {}", file.path), paths)
}

/// Reverts tracked files and deletes untracked ones.
fn discard_op(label: String, files: &[&FileChange]) -> GitOp {
    let (untracked, tracked): (Vec<&&FileChange>, Vec<&&FileChange>) =
        files.iter().partition(|f| f.kind == ChangeKind::Untracked);
    let mut op: Option<GitOp> = None;
    if !tracked.is_empty() {
        let mut args = vec!["checkout".to_owned(), "--".into()];
        args.extend(tracked.iter().map(|f| f.path.clone()));
        op = Some(GitOp::new(label.clone(), args));
    }
    if !untracked.is_empty() {
        let mut args = vec!["clean".to_owned(), "-f".into(), "--".into()];
        args.extend(untracked.iter().map(|f| f.path.clone()));
        op = Some(match op {
            Some(op) => op.then(args),
            None => GitOp::new(label, args),
        });
    }
    op.expect("at least one file")
}

fn discard_action(file: &FileChange) -> Action {
    Action::Confirm {
        title: "Annuler les modifications".into(),
        message: format!("Les modifications de « {} » seront définitivement perdues.", file.path),
        op: discard_op(format!("Annuler {}", file.path), &[file]),
    }
}

fn conflict_ops(path: &str) -> Vec<(&'static str, GitOp)> {
    vec![
        (
            "Marquer résolu",
            GitOp::new("Marquer résolu", ["add", "--", path]),
        ),
        (
            "Prendre « theirs »",
            GitOp::new("Résoudre (theirs)", ["checkout", "--theirs", "--", path]).then(["add", "--", path]),
        ),
        (
            "Prendre « ours »",
            GitOp::new("Résoudre (ours)", ["checkout", "--ours", "--", path]).then(["add", "--", path]),
        ),
    ]
}

/// One of the two file panes. Returns a new list mode if the user switched it.
fn pane(ui: &mut Ui, tab: &RepoTab, side: DiffSide, actions: &mut Vec<Action>) -> Option<ListMode> {
    let p = theme::of(ui);
    let files = tab.status.list(side);
    let mut new_mode = None;
    let bar = widgets::bar_frame(ui).show(ui, |ui| {
        ui.set_min_height(24.0);
        ui.horizontal(|ui| {
            let title = match side {
                DiffSide::Unstaged => "Non indexés",
                DiffSide::Staged => "Indexés",
            };
            ui.label(RichText::new(title).strong().color(p.text));
            ui.label(RichText::new(files.len().to_string()).color(p.weak));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !files.is_empty();
                match side {
                    DiffSide::Unstaged => {
                        if ui.add_enabled(enabled, egui::Button::new("Tout indexer")).clicked() {
                            actions.push(Action::Git(GitOp::new("Tout indexer", ["add", "-A"]).quiet()));
                        }
                    }
                    DiffSide::Staged => {
                        if ui.add_enabled(enabled, egui::Button::new("Tout désindexer")).clicked() {
                            let op = if tab.head.oid.is_none() {
                                GitOp::new("Tout désindexer", ["rm", "-r", "-q", "--cached", "."])
                            } else {
                                GitOp::new("Tout désindexer", ["reset", "-q"])
                            };
                            actions.push(Action::Git(op.quiet()));
                        }
                    }
                }
                ui.add_space(4.0);
                if widgets::icon_button(ui, icons::LIST, tab.list_mode == ListMode::Flat, "Liste").clicked() {
                    new_mode = Some(ListMode::Flat);
                }
                if widgets::icon_button(ui, icons::TREE, tab.list_mode == ListMode::Tree, "Arborescence").clicked() {
                    new_mode = Some(ListMode::Tree);
                }
            });
        });
    });
    widgets::separator_line(ui, bar.response.rect);

    let selected = tab
        .selected_change
        .as_ref()
        .filter(|(s, _)| *s == side)
        .and_then(|(_, path)| files.iter().position(|f| &f.path == path));
    let event = ScrollArea::vertical()
        .id_salt(("files", side == DiffSide::Staged))
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(4.0);
            if files.is_empty() {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| ui.weak("Aucun fichier"));
                return None;
            }
            file_tree::show(
                ui,
                Id::unique(("changes_tree", side == DiffSide::Staged)),
                files,
                tab.list_mode,
                selected,
                &mut |ui, target| menu(ui, tab, side, target, actions),
            )
        })
        .inner;
    match event {
        Some(file_tree::Event::Select(i)) => {
            actions.push(Action::SelectChange(side, files[i].path.clone()));
        }
        Some(file_tree::Event::Activate(i)) => {
            let f = &files[i];
            if f.kind != ChangeKind::Conflicted {
                actions.push(Action::Git(match side {
                    DiffSide::Unstaged => stage_op(f),
                    DiffSide::Staged => unstage_op(tab, f),
                }));
            }
        }
        None => {}
    }
    new_mode
}

fn menu(ui: &mut Ui, tab: &RepoTab, side: DiffSide, target: Target<'_>, actions: &mut Vec<Action>) {
    match target {
        Target::File(f) => file_menu(ui, tab, side, f, actions),
        Target::Folder { path, files } => {
            let dir = path.trim_end_matches('/');
            match side {
                DiffSide::Unstaged => {
                    if menu_item(ui, "Indexer le dossier") {
                        actions.push(Action::Git(
                            GitOp::new(format!("Indexer {dir}"), ["add", "-A", "--", dir]).quiet(),
                        ));
                    }
                    let discardable: Vec<&FileChange> = files
                        .iter()
                        .copied()
                        .filter(|f| f.kind != ChangeKind::Conflicted)
                        .collect();
                    if !discardable.is_empty() && menu_item(ui, "Annuler les modifications du dossier…") {
                        actions.push(Action::Confirm {
                            title: "Annuler les modifications".into(),
                            message: format!(
                                "Les modifications de {} fichier(s) dans « {dir} » seront définitivement perdues.",
                                discardable.len()
                            ),
                            op: discard_op(format!("Annuler {dir}"), &discardable),
                        });
                    }
                }
                DiffSide::Staged => {
                    if menu_item(ui, "Désindexer le dossier") {
                        actions.push(Action::Git(unstage_paths(
                            tab,
                            format!("Désindexer {dir}"),
                            vec![dir.to_owned()],
                        )));
                    }
                }
            }
            ui.separator();
            if menu_item(ui, crate::platform::REVEAL_LABEL) {
                actions.push(Action::Reveal(tab.path().join(dir)));
            }
            if menu_item(ui, "Copier le chemin") {
                actions.push(Action::Copy(dir.to_owned()));
            }
        }
    }
}

fn file_menu(ui: &mut Ui, tab: &RepoTab, side: DiffSide, f: &FileChange, actions: &mut Vec<Action>) {
    let path = f.path.clone();
    match (side, f.kind) {
        (_, ChangeKind::Conflicted) => {
            for (label, op) in conflict_ops(&path) {
                if menu_item(ui, label) {
                    actions.push(Action::Git(op));
                }
            }
        }
        (DiffSide::Unstaged, kind) => {
            if menu_item(ui, "Indexer") {
                actions.push(Action::Git(stage_op(f)));
            }
            if menu_item(ui, "Annuler les modifications…") {
                actions.push(discard_action(f));
            }
            if kind == ChangeKind::Untracked && menu_item(ui, "Ignorer (.gitignore)") {
                actions.push(Action::Ignore(path.clone()));
            }
        }
        (DiffSide::Staged, _) => {
            if menu_item(ui, "Désindexer") {
                actions.push(Action::Git(unstage_op(tab, f)));
            }
        }
    }
    ui.separator();
    let abs = tab.path().join(&path);
    if abs.exists() {
        if menu_item(ui, "Ouvrir le fichier") {
            actions.push(Action::Open(abs.clone()));
        }
        if menu_item(ui, crate::platform::REVEAL_LABEL) {
            actions.push(Action::Reveal(abs));
        }
    }
    if menu_item(ui, "Copier le chemin") {
        actions.push(Action::Copy(path));
    }
}

fn commit_box(ui: &mut Ui, tab: &mut RepoTab, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let subject = ui.add(
        TextEdit::singleline(&mut tab.commit_subject)
            .id_salt("commit_subject")
            .hint_text("Titre du commit")
            .font(FontId::proportional(13.5))
            .margin(Margin::symmetric(8, 6))
            .desired_width(f32::INFINITY),
    );
    let len = tab.commit_subject.chars().count();
    if len > 0 {
        let color = if len > 72 { theme::kind_color(ChangeKind::Modified) } else { p.weak };
        ui.painter().text(
            subject.rect.right_center() - egui::vec2(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            len.to_string(),
            FontId::proportional(11.0),
            color,
        );
    }
    if subject.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        tab.focus_body = true;
    }
    ui.add_space(4.0);
    let body = ui.add(
        TextEdit::multiline(&mut tab.commit_body)
            .id_salt("commit_body")
            .hint_text("Description (facultative)")
            .margin(Margin::symmetric(8, 6))
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    if tab.focus_body {
        body.request_focus();
        tab.focus_body = false;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let was_amend = tab.amend;
        ui.checkbox(&mut tab.amend, "Amend");
        if tab.amend
            && !was_amend
            && tab.message_is_empty()
            && let Some(m) = tab.repo.head_message()
        {
            tab.set_message(&m);
        }
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if tab.amend {
                "Modifier le dernier commit".to_owned()
            } else {
                match tab.status.staged.len() {
                    0 => "Commit".to_owned(),
                    1 => "Commit 1 fichier".to_owned(),
                    n => format!("Commit {n} fichiers"),
                }
            };
            let width = ui.available_width().min(220.0);
            let resp = widgets::primary_button(ui, &label, tab.can_commit(), width);
            let resp = if tab.commit_subject.trim().is_empty() {
                resp.on_hover_text("Saisissez un titre de commit")
            } else if tab.status.staged.is_empty() && !tab.amend {
                resp.on_hover_text("Indexez au moins un fichier")
            } else {
                resp.on_hover_text(crate::platform::shortcut("↩"))
            };
            if resp.clicked() {
                actions.push(Action::Commit);
            }
        });
    });
    let shortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
    if ui.input_mut(|i| i.consume_shortcut(&shortcut)) && tab.can_commit() {
        actions.push(Action::Commit);
    }
}
