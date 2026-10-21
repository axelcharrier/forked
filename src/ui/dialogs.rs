use std::path::PathBuf;

use egui::{
    Align2, Color32, ComboBox, Context, FontId, Id, Key, Modal, RichText, Sense, TextEdit, Ui, vec2,
};

use super::checkout::{self, LocalChanges};
use super::{After, Dialog, GitOp, MergeMode, ResetMode, icons, theme, widgets};
use crate::tab::RepoTab;

pub enum Outcome {
    Open,
    Close,
    Run(GitOp),
}

pub fn show(ctx: &Context, dialog: &mut Dialog, tab: Option<&RepoTab>) -> Outcome {
    let mut outcome = Outcome::Open;
    let resp = Modal::new(Id::unique("forked_dialog")).show(ctx, |ui| {
        ui.set_width(440.0);
        outcome = body(ui, dialog, tab);
    });
    if matches!(outcome, Outcome::Open) && resp.should_close() {
        Outcome::Close
    } else {
        outcome
    }
}

/// Cancel / confirm row. Enter confirms when `enabled`.
fn buttons(ui: &mut Ui, primary: &str, enabled: bool, op: impl FnOnce() -> GitOp) -> Outcome {
    ui.add_space(10.0);
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let width = primary.chars().count() as f32 * 7.5 + 32.0;
            let confirm = widgets::primary_button(ui, primary, enabled, width);
            if ui.button("Annuler").clicked() {
                outcome = Outcome::Close;
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
            if enabled && (confirm.clicked() || enter) {
                outcome = Outcome::Run(op());
            }
        });
    });
    outcome
}

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str, focus: bool) {
    ui.label(label);
    let resp = ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    if focus && !resp.has_focus() && ui.ctx().memory(|m| m.focused().is_none()) {
        resp.request_focus();
    }
    ui.add_space(4.0);
}

fn combo(ui: &mut Ui, id: &str, value: &mut String, options: &[String]) {
    ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(value, o.clone(), o);
            }
        });
}

fn valid_ref(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && !name.contains(char::is_whitespace)
        && !name.contains("..")
        && !name.starts_with('-')
        && !name.ends_with('/')
        && !name.ends_with(".lock")
        && !name.contains(['~', '^', ':', '?', '*', '[', '\\'])
}

fn body(ui: &mut Ui, dialog: &mut Dialog, tab: Option<&RepoTab>) -> Outcome {
    let remotes = tab.map(|t| t.remotes.clone()).unwrap_or_default();
    match dialog {
        Dialog::Checkout {
            target,
            note,
            args,
            has_changes,
            changes,
        } => {
            let p = theme::of(ui);
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 30.0, p.accent);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    icons::BRANCH,
                    FontId::proportional(30.0),
                    Color32::WHITE,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Checkout").strong().size(15.0).color(p.text));
                    ui.label(RichText::new("Basculer sur une autre branche").color(p.weak));
                    ui.add_space(12.0);
                    egui::Grid::new("checkout_grid")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Basculer vers :");
                            ui.label(RichText::new(format!("{}  {target}", icons::BRANCH)).strong().color(p.text));
                            ui.end_row();
                            ui.label("Modifications locales :");
                            if *has_changes {
                                ui.vertical(|ui| {
                                    ui.radio_value(changes, LocalChanges::Keep, "Ne rien changer");
                                    ui.radio_value(changes, LocalChanges::Stash, "Stash puis réappliquer");
                                    ui.radio_value(changes, LocalChanges::Discard, "Annuler les modifications");
                                });
                            } else {
                                ui.label(RichText::new("aucune").color(p.weak));
                            }
                            ui.end_row();
                        });
                    if let Some(note) = note {
                        ui.add_space(6.0);
                        ui.label(RichText::new(note.as_str()).color(p.weak).size(12.0));
                    }
                    if *has_changes && *changes == LocalChanges::Discard {
                        ui.add_space(6.0);
                        ui.colored_label(
                            Color32::from_rgb(0xe0, 0x5a, 0x4a),
                            "Les modifications des fichiers suivis seront définitivement perdues.",
                        );
                    }
                });
            });
            let (target, args, changes, has_changes) = (target.clone(), args.clone(), *changes, *has_changes);
            buttons(ui, "Checkout", true, || checkout::op(&target, &args, changes, has_changes))
        }
        Dialog::NewBranch {
            start,
            name,
            checkout,
        } => {
            ui.heading("Nouvelle branche");
            let from = if start.len() == 40 { &start[..7] } else { start.as_str() };
            ui.weak(format!("À partir de {from}"));
            ui.add_space(6.0);
            field(ui, "Nom", name, "feature/ma-branche", true);
            ui.checkbox(checkout, "Checkout après création");
            let n = name.trim().to_owned();
            let (start, checkout) = (start.clone(), *checkout);
            buttons(ui, "Créer", valid_ref(&n), || {
                if checkout {
                    GitOp::new(format!("Créer {n}"), ["checkout", "-b", &n, &start])
                } else {
                    GitOp::new(format!("Créer {n}"), ["branch", &n, &start])
                }
            })
        }
        Dialog::RenameBranch { old, name } => {
            ui.heading(format!("Renommer {old}"));
            ui.add_space(6.0);
            field(ui, "Nouveau nom", name, "", true);
            let n = name.trim().to_owned();
            let old = old.clone();
            buttons(ui, "Renommer", valid_ref(&n) && n != old, || {
                GitOp::new(format!("Renommer {old}"), ["branch", "-m", &old, &n])
            })
        }
        Dialog::NewTag {
            target,
            name,
            message,
            push,
        } => {
            ui.heading("Nouveau tag");
            ui.weak(format!("Sur {}", &target[..target.len().min(7)]));
            ui.add_space(6.0);
            field(ui, "Nom", name, "v1.0.0", true);
            ui.label("Message (facultatif — crée un tag annoté)");
            ui.add(TextEdit::multiline(message).desired_rows(3).desired_width(f32::INFINITY));
            let remote = tab.and_then(RepoTab::default_remote);
            if let Some(r) = &remote {
                ui.checkbox(push, format!("Pousser vers {r}"));
            }
            let n = name.trim().to_owned();
            let (target, msg, push) = (target.clone(), message.trim().to_owned(), *push);
            buttons(ui, "Créer", valid_ref(&n), || {
                let mut op = if msg.is_empty() {
                    GitOp::new(format!("Tag {n}"), ["tag", &n, &target])
                } else {
                    GitOp::new(format!("Tag {n}"), ["tag", "-a", &n, "-m", &msg, &target])
                };
                if let (true, Some(r)) = (push, remote) {
                    op = op.then(["push", &r, &format!("refs/tags/{n}")]);
                }
                op
            })
        }
        Dialog::Stash { message, untracked } => {
            ui.heading("Stash");
            ui.add_space(6.0);
            field(ui, "Message (facultatif)", message, "WIP", true);
            ui.checkbox(untracked, "Inclure les fichiers non suivis");
            let (msg, untracked) = (message.trim().to_owned(), *untracked);
            buttons(ui, "Stash", true, || {
                let mut args = vec!["stash".to_owned(), "push".into()];
                if untracked {
                    args.push("--include-untracked".into());
                }
                if !msg.is_empty() {
                    args.extend(["-m".into(), msg.clone()]);
                }
                GitOp::new("Stash", args)
            })
        }
        Dialog::Push {
            remote,
            branch,
            remote_branch,
            set_upstream,
            force,
            tags,
        } => {
            ui.heading("Push");
            ui.add_space(6.0);
            let locals: Vec<String> = tab
                .map(|t| t.local.iter().map(|b| b.name.clone()).collect())
                .unwrap_or_default();
            ui.label("Branche locale");
            combo(ui, "push_branch", branch, &locals);
            ui.add_space(4.0);
            ui.label("Remote");
            combo(ui, "push_remote", remote, &remotes);
            ui.add_space(4.0);
            field(ui, "Branche distante", remote_branch, "", false);
            ui.checkbox(set_upstream, "Suivre la branche distante (--set-upstream)");
            ui.checkbox(tags, "Pousser aussi les tags");
            ui.checkbox(force, "Forcer (--force-with-lease)");
            if *force {
                ui.colored_label(
                    Color32::from_rgb(0xe0, 0x5a, 0x4a),
                    "Les commits distants absents en local seront écrasés.",
                );
            }
            let (r, b, rb) = (remote.clone(), branch.clone(), remote_branch.trim().to_owned());
            let (up, force, tags) = (*set_upstream, *force, *tags);
            buttons(ui, "Push", !r.is_empty() && !b.is_empty() && valid_ref(&rb), || {
                let mut args = vec!["push".to_owned()];
                if up {
                    args.push("--set-upstream".into());
                }
                if force {
                    args.push("--force-with-lease".into());
                }
                if tags {
                    args.push("--tags".into());
                }
                args.push(r.clone());
                args.push(format!("refs/heads/{b}:refs/heads/{rb}"));
                GitOp::new(format!("Push {b}"), args)
            })
        }
        Dialog::Pull {
            remote,
            branch,
            rebase,
        } => {
            ui.heading("Pull");
            ui.add_space(6.0);
            ui.label("Remote");
            combo(ui, "pull_remote", remote, &remotes);
            ui.add_space(4.0);
            let prefix = format!("{remote}/");
            let branches: Vec<String> = tab
                .map(|t| {
                    t.remote_branches
                        .iter()
                        .filter_map(|b| b.name.strip_prefix(&prefix).map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            ui.label("Branche distante");
            combo(ui, "pull_branch", branch, &branches);
            ui.add_space(4.0);
            ui.checkbox(rebase, "Rebaser au lieu de fusionner (--rebase)");
            let (r, b, rebase) = (remote.clone(), branch.clone(), *rebase);
            buttons(ui, "Pull", !r.is_empty() && !b.is_empty(), || {
                let mode = if rebase { "--rebase" } else { "--no-rebase" };
                GitOp::new(format!("Pull {r}/{b}"), ["pull", mode, &r, &b])
            })
        }
        Dialog::Merge { source, mode } => {
            let current = tab
                .and_then(|t| t.head.branch.clone())
                .unwrap_or_else(|| "HEAD".into());
            let src = if source.len() == 40 { source[..7].to_owned() } else { source.clone() };
            ui.heading("Fusion");
            ui.label(format!("Fusionner {src} dans {current}"));
            ui.add_space(6.0);
            ui.radio_value(mode, MergeMode::Default, "Par défaut (fast-forward si possible)");
            ui.radio_value(mode, MergeMode::NoFastForward, "Toujours créer un commit de fusion (--no-ff)");
            ui.radio_value(mode, MergeMode::FastForwardOnly, "Fast-forward uniquement (--ff-only)");
            ui.radio_value(mode, MergeMode::Squash, "Squash (--squash, sans commit)");
            let (source, mode) = (source.clone(), *mode);
            buttons(ui, "Fusionner", true, || {
                let mut args = vec!["merge".to_owned()];
                match mode {
                    MergeMode::Default => {}
                    MergeMode::NoFastForward => args.push("--no-ff".into()),
                    MergeMode::FastForwardOnly => args.push("--ff-only".into()),
                    MergeMode::Squash => args.push("--squash".into()),
                }
                args.push(source.clone());
                GitOp::new(format!("Fusion de {src}"), args)
            })
        }
        Dialog::Reset {
            target,
            summary,
            mode,
        } => {
            let current = tab
                .and_then(|t| t.head.branch.clone())
                .unwrap_or_else(|| "HEAD".into());
            ui.heading(format!("Réinitialiser {current}"));
            ui.label(format!("Vers {summary}"));
            ui.add_space(6.0);
            ui.radio_value(mode, ResetMode::Soft, "Soft — garder les modifications indexées");
            ui.radio_value(mode, ResetMode::Mixed, "Mixed — garder les modifications non indexées");
            ui.radio_value(mode, ResetMode::Hard, "Hard — tout supprimer");
            if *mode == ResetMode::Hard {
                ui.colored_label(
                    Color32::from_rgb(0xe0, 0x5a, 0x4a),
                    "Toutes les modifications locales seront définitivement perdues.",
                );
            }
            let (target, mode) = (target.clone(), *mode);
            buttons(ui, "Réinitialiser", true, || {
                GitOp::new(format!("Reset {}", mode.flag()), ["reset", mode.flag(), &target])
            })
        }
        Dialog::AddRemote { name, url } => {
            ui.heading("Ajouter un remote");
            ui.add_space(6.0);
            field(ui, "Nom", name, "origin", name.is_empty());
            field(ui, "URL", url, "git@github.com:moi/projet.git", !name.is_empty());
            let (n, u) = (name.trim().to_owned(), url.trim().to_owned());
            buttons(ui, "Ajouter", valid_ref(&n) && !u.is_empty(), || {
                GitOp::new(format!("Ajouter {n}"), ["remote", "add", &n, &u])
            })
        }
        Dialog::Clone { url, parent, name } => {
            ui.heading("Cloner un dépôt");
            ui.add_space(6.0);
            field(ui, "URL", url, "https://github.com/moi/projet.git", true);
            ui.label("Dossier parent");
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(parent).desired_width(ui.available_width() - 90.0));
                if ui.button("Choisir…").clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        *parent = dir.display().to_string();
                    }
            });
            ui.add_space(4.0);
            let derived = url
                .trim()
                .trim_end_matches('/')
                .rsplit(['/', ':'])
                .next()
                .unwrap_or("")
                .trim_end_matches(".git")
                .to_owned();
            field(ui, "Nom du dossier", name, &derived, false);
            let folder = if name.trim().is_empty() { derived } else { name.trim().to_owned() };
            let (u, p) = (url.trim().to_owned(), PathBuf::from(parent.trim()));
            let target = p.join(&folder);
            if target.exists() && !folder.is_empty() {
                ui.colored_label(Color32::from_rgb(0xe0, 0x5a, 0x4a), "Ce dossier existe déjà.");
            }
            let ok = !u.is_empty() && !folder.is_empty() && p.is_dir() && !target.exists();
            buttons(ui, "Cloner", ok, || {
                GitOp::new(format!("Clone {folder}"), ["clone", "--progress", &u, &folder])
                    .cwd(p.clone())
                    .after(After::OpenRepo(target.clone()))
            })
        }
        Dialog::Confirm { title, message, op } => {
            ui.heading(title.as_str());
            ui.add_space(6.0);
            ui.label(message.as_str());
            let op = op.clone();
            buttons(ui, "Confirmer", true, || op)
        }
    }
}
