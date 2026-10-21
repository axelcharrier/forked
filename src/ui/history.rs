use std::collections::BTreeSet;

use egui::epaint::CubicBezierShape;
use egui::{
    Align2, CentralPanel, Color32, FontId, Frame, Id, Key, Layout, Margin, Painter, Panel, Pos2,
    Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, pos2, vec2,
};

use super::diff::{self, DiffMode};
use super::file_tree::{self, ListMode, Target};
use super::{Action, Dialog, GitOp, checkout, icons, menu_item, theme, widgets};
use crate::git::graph::GraphRow;
use crate::git::{CommitInfo, RefKind, RefLabel};
use crate::tab::RepoTab;

const ROW_H: f32 = 26.0;
const LANE_W: f32 = 14.0;
const MAX_LANES: usize = 24;

pub fn show(ui: &mut Ui, tab: &mut RepoTab, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let height = ui.available_height();
    Panel::bottom("commit_detail")
        .resizable(true)
        .default_size(height * 0.45)
        .min_size(140.0)
        .frame(Frame::new().fill(p.content))
        .show(ui, |ui| detail(ui, tab, actions));
    CentralPanel::default()
        .frame(Frame::new().fill(p.content))
        .show(ui, |ui| list(ui, tab, actions));
}

fn list(ui: &mut Ui, tab: &mut RepoTab, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let bar = widgets::bar_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::SEARCH).color(p.weak));
            ui.add(
                TextEdit::singleline(&mut tab.search)
                    .id_salt("history_search")
                    .hint_text("Rechercher un message, un auteur, un SHA")
                    .frame(egui::Frame::NONE)
                    .desired_width(340.0),
            );
            if !tab.search.is_empty() && widgets::icon_button(ui, icons::CLOSE, false, "Effacer").clicked() {
                tab.search.clear();
            }
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{} commits", tab.commits.len())).color(p.weak).size(12.0));
            });
        });
    });
    widgets::separator_line(ui, bar.response.rect);

    let needle = tab.search.trim().to_lowercase();
    let visible: Option<Vec<usize>> = (!needle.is_empty()).then(|| {
        tab.commits
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.message.to_lowercase().contains(&needle)
                    || c.author.to_lowercase().contains(&needle)
                    || c.id.to_string().starts_with(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    });
    let row_count = visible.as_ref().map_or(tab.commits.len(), Vec::len);
    let row_index = |i: usize| visible.as_ref().map_or(i, |v| v[i]);

    keyboard_navigation(ui, tab, &visible, row_count, actions);
    column_header(ui);

    ui.spacing_mut().item_spacing.y = 0.0;
    let mut scroll = ScrollArea::vertical().auto_shrink(false).id_salt("history");
    if let Some(row) = tab.scroll_to.take() {
        let row = visible
            .as_ref()
            .and_then(|v| v.iter().position(|&i| i == row))
            .unwrap_or(row);
        scroll = scroll.vertical_scroll_offset((row as f32 * ROW_H - 120.0).max(0.0));
    }
    let tab_ref: &RepoTab = tab;
    let out = scroll.show_rows(ui, ROW_H, row_count, |ui, range| {
        for i in range.clone() {
            row(ui, tab_ref, row_index(i), visible.is_none(), actions);
        }
        range
    });
    tab.visible_rows = out.inner;
}

struct Columns {
    author: f32,
    date: f32,
    sha: f32,
}

fn columns(rect: Rect) -> Columns {
    let sha = rect.right() - 72.0;
    let date = sha - 128.0;
    let author = date - 170.0;
    Columns { author, date, sha }
}

fn column_header(ui: &mut Ui) {
    let p = theme::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let c = columns(rect);
    let font = FontId::proportional(11.0);
    let cy = rect.center().y;
    for (x, label) in [
        (rect.left() + 12.0, "Description"),
        (c.author, "Auteur"),
        (c.date, "Date"),
        (c.sha, "Commit"),
    ] {
        painter.text(pos2(x, cy), Align2::LEFT_CENTER, label, font.clone(), p.weak);
    }
    widgets::separator_line(ui, rect);
}

fn keyboard_navigation(
    ui: &Ui,
    tab: &mut RepoTab,
    visible: &Option<Vec<usize>>,
    row_count: usize,
    actions: &mut Vec<Action>,
) {
    if row_count == 0 || ui.ctx().memory(|m| m.focused().is_some()) {
        return;
    }
    let (up, down) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
    if !up && !down {
        return;
    }
    let current = tab
        .selected_commit
        .and_then(|oid| tab.index_of.get(&oid).copied())
        .and_then(|idx| match visible {
            Some(v) => v.iter().position(|&i| i == idx),
            None => Some(idx),
        });
    let next = match (current, up) {
        (None, _) => 0,
        (Some(c), true) => c.saturating_sub(1),
        (Some(c), false) => (c + 1).min(row_count - 1),
    };
    let idx = visible.as_ref().map_or(next, |v| v[next]);
    // Scroll only when the row leaves the visible window.
    let inner = tab.visible_rows.start + 1..tab.visible_rows.end.saturating_sub(2);
    if !inner.contains(&next) {
        tab.scroll_to = Some(idx);
    }
    actions.push(Action::SelectCommit(tab.commits[idx].id));
}

fn row(ui: &mut Ui, tab: &RepoTab, idx: usize, with_graph: bool, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let c = &tab.commits[idx];
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let selected = tab.selected_commit == Some(c.id);
    let painter = ui.painter_at(rect);
    let row_bg = if selected {
        painter.rect_filled(rect, 0.0, p.accent);
        p.accent
    } else {
        if resp.hovered() {
            painter.rect_filled(rect, 0.0, p.hover);
        }
        p.content
    };
    let text = if selected { p.on_accent } else { p.text };
    let weak = if selected { p.on_accent.gamma_multiply(0.8) } else { p.weak };
    let cy = rect.center().y;
    let cols = columns(rect);

    let mut x = rect.left() + 4.0;
    if with_graph {
        let g = &tab.graph[idx];
        draw_graph(&painter, rect, g, c.parents.len() > 1, row_bg);
        x += 12.0 + LANE_W * g.width.min(MAX_LANES) as f32;
    } else {
        x += 8.0;
    }

    let summary_end = cols.author - 12.0;
    let badges = merged_refs(&c.refs);
    let mut badge_rects = Vec::new();
    for label in &badges {
        if x > summary_end - 80.0 {
            break;
        }
        let right = badge(&painter, x, cy, label, p.dark);
        badge_rects.push(Rect::from_x_y_ranges(x..=right, rect.y_range()));
        x = right + 5.0;
    }
    let font = FontId::proportional(13.0);
    let clip = painter.with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), pos2(summary_end, rect.bottom())));
    clip.text(pos2(x + 2.0, cy), Align2::LEFT_CENTER, &c.summary, font.clone(), text);

    widgets::avatar(&painter, pos2(cols.author + 8.0, cy), 8.0, &c.author, &c.email);
    let author_clip = painter.with_clip_rect(Rect::from_min_max(
        pos2(cols.author + 20.0, rect.top()),
        pos2(cols.date - 10.0, rect.bottom()),
    ));
    author_clip.text(pos2(cols.author + 22.0, cy), Align2::LEFT_CENTER, &c.author, FontId::proportional(12.5), weak);
    painter.text(pos2(cols.date, cy), Align2::LEFT_CENTER, super::format_date(c.time), FontId::proportional(12.5), weak);
    painter.text(pos2(cols.sha, cy), Align2::LEFT_CENTER, &c.short_id, FontId::monospace(11.5), weak);

    if resp.clicked() {
        actions.push(Action::SelectCommit(c.id));
    }
    if resp.double_clicked() {
        let pointer = resp.interact_pointer_pos();
        let hit = badge_rects
            .iter()
            .position(|r| pointer.is_some_and(|p| r.contains(p)));
        if let Some(d) = checkout_target(&badges, hit).and_then(|(name, remote)| checkout::dialog(tab, name, remote)) {
            actions.push(Action::Dialog(d));
        }
    }
    resp.context_menu(|ui| commit_menu(ui, tab, c, actions));
}

/// The branch a double click checks out: the badge under the pointer, else
/// the row's first local branch, else its first remote branch.
fn checkout_target(badges: &[Badge], hit: Option<usize>) -> Option<(&str, bool)> {
    if let Some(i) = hit {
        return pick(&badges[i]);
    }
    badges
        .iter()
        .find(|b| b.kind == RefKind::LocalBranch)
        .or_else(|| badges.iter().find(|b| b.kind == RefKind::RemoteBranch))
        .and_then(pick)
}

fn pick(b: &Badge) -> Option<(&str, bool)> {
    match b.kind {
        RefKind::Head | RefKind::LocalBranch => Some((b.name.as_str(), false)),
        RefKind::RemoteBranch => Some((b.name.as_str(), true)),
        RefKind::Tag => None,
    }
}

fn lane_x(rect: Rect, lane: usize) -> f32 {
    rect.left() + 14.0 + lane as f32 * LANE_W
}

fn curve(painter: &Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    if (from.x - to.x).abs() < 0.5 {
        painter.line_segment([from, to], stroke);
        return;
    }
    let mid = (from.y + to.y) / 2.0;
    painter.add(CubicBezierShape::from_points_stroke(
        [from, pos2(from.x, mid), pos2(to.x, mid), to],
        false,
        Color32::TRANSPARENT,
        stroke,
    ));
}

fn draw_graph(painter: &Painter, rect: Rect, g: &GraphRow, merge: bool, bg: Color32) {
    let (top, bottom, cy) = (rect.top(), rect.bottom(), rect.center().y);
    let stroke = |lane: usize| Stroke::new(2.0, theme::lane_color(lane));
    let visible = |lane: usize| lane < MAX_LANES;
    let node_x = lane_x(rect, g.lane.min(MAX_LANES - 1));

    for &l in g.pass_through.iter().filter(|l| visible(**l)) {
        let x = lane_x(rect, l);
        painter.line_segment([pos2(x, top), pos2(x, bottom)], stroke(l));
    }
    for &l in g.incoming.iter().filter(|l| visible(**l)) {
        curve(painter, pos2(lane_x(rect, l), top), pos2(node_x, cy), stroke(l));
    }
    for &l in g.outgoing.iter().filter(|l| visible(**l)) {
        // An edge to another lane takes that lane's color: it joins that branch.
        curve(painter, pos2(node_x, cy), pos2(lane_x(rect, l), bottom), stroke(l));
    }
    let center = pos2(node_x, cy);
    let color = theme::lane_color(g.lane);
    if merge {
        painter.circle_filled(center, 4.5, color);
        painter.circle_filled(center, 2.0, bg);
    } else {
        painter.circle_filled(center, 5.5, bg);
        painter.circle_filled(center, 4.5, color);
    }
}

/// A ref label as displayed: a local branch and its same-named remote merge into one.
struct Badge {
    name: String,
    kind: RefKind,
    on_remote: bool,
}

fn merged_refs(refs: &[RefLabel]) -> Vec<Badge> {
    let mut out: Vec<Badge> = Vec::new();
    for r in refs {
        if r.kind == RefKind::RemoteBranch {
            let short = r.name.split_once('/').map_or(r.name.as_str(), |(_, b)| b);
            if let Some(local) = out
                .iter_mut()
                .find(|b| matches!(b.kind, RefKind::Head | RefKind::LocalBranch) && b.name == short)
            {
                local.on_remote = true;
                continue;
            }
        }
        out.push(Badge {
            name: r.name.clone(),
            kind: r.kind,
            on_remote: false,
        });
    }
    out
}

fn badge(painter: &Painter, x: f32, cy: f32, b: &Badge, dark: bool) -> f32 {
    let color = theme::ref_color(b.kind);
    let icon = match b.kind {
        RefKind::Head => icons::CHECK,
        RefKind::LocalBranch => icons::BRANCH,
        RefKind::RemoteBranch => icons::CLOUD,
        RefKind::Tag => icons::TAG,
    };
    let mut text = format!("{icon} {}", b.name);
    if b.on_remote {
        text.push_str(&format!("  {}", icons::CLOUD));
    }
    let fg = if dark { color.lerp_to_gamma(Color32::WHITE, 0.45) } else { color.lerp_to_gamma(Color32::BLACK, 0.25) };
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.5), fg);
    let rect = Rect::from_min_size(pos2(x, cy - 9.0), vec2(galley.size().x + 12.0, 18.0));
    painter.rect_filled(rect, 5.0, color.gamma_multiply(if dark { 0.28 } else { 0.16 }));
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    painter.galley(pos2(rect.left() + 6.0, cy - galley.size().y / 2.0), galley, fg);
    rect.right()
}

pub fn commit_menu(ui: &mut Ui, tab: &RepoTab, c: &CommitInfo, actions: &mut Vec<Action>) {
    let sha = c.id.to_string();
    let short = c.short_id.clone();
    let current = tab.head.branch.clone().unwrap_or_else(|| "HEAD".into());
    let merge_flags: Vec<String> = if c.parents.len() > 1 {
        vec!["-m".into(), "1".into()]
    } else {
        Vec::new()
    };

    if menu_item(ui, format!("Checkout de {short}…")) {
        actions.push(Action::Confirm {
            title: "Checkout d'un commit".into(),
            message: format!("HEAD sera détaché sur {short}. Créez une branche pour y garder vos commits."),
            op: GitOp::new(format!("Checkout {short}"), ["checkout", &sha]),
        });
    }
    if menu_item(ui, "Nouvelle branche ici…") {
        actions.push(Action::Dialog(Dialog::NewBranch {
            start: sha.clone(),
            name: String::new(),
            checkout: true,
        }));
    }
    if menu_item(ui, "Nouveau tag ici…") {
        actions.push(Action::Dialog(Dialog::NewTag {
            target: sha.clone(),
            name: String::new(),
            message: String::new(),
            push: false,
        }));
    }
    ui.separator();
    if menu_item(ui, format!("Cherry-pick sur {current}")) {
        let mut args = vec!["cherry-pick".to_string()];
        args.extend(merge_flags.clone());
        args.push(sha.clone());
        actions.push(Action::Git(GitOp::new(format!("Cherry-pick {short}"), args)));
    }
    if menu_item(ui, "Revert du commit") {
        let mut args = vec!["revert".to_string(), "--no-edit".into()];
        args.extend(merge_flags);
        args.push(sha.clone());
        actions.push(Action::Git(GitOp::new(format!("Revert {short}"), args)));
    }
    if menu_item(ui, format!("Fusionner dans {current}…")) {
        actions.push(Action::Dialog(Dialog::Merge {
            source: sha.clone(),
            mode: super::MergeMode::Default,
        }));
    }
    if menu_item(ui, format!("Rebaser {current} sur {short}…")) {
        actions.push(Action::Confirm {
            title: "Rebase".into(),
            message: format!("Rejouer les commits de {current} au-dessus de {short} ?"),
            op: GitOp::new(format!("Rebase sur {short}"), ["rebase", &sha]),
        });
    }
    if menu_item(ui, format!("Réinitialiser {current} ici…")) {
        actions.push(Action::Dialog(Dialog::Reset {
            target: sha.clone(),
            summary: format!("{short} {}", c.summary),
            mode: super::ResetMode::Mixed,
        }));
    }
    ui.separator();
    if menu_item(ui, "Copier le SHA") {
        actions.push(Action::Copy(sha));
    }
    if menu_item(ui, "Copier le message") {
        actions.push(Action::Copy(c.message.clone()));
    }
}

fn detail(ui: &mut Ui, tab: &mut RepoTab, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    let Some(d) = &tab.detail else {
        ui.centered_and_justified(|ui| ui.weak("Sélectionnez un commit"));
        return;
    };
    let mut new_mode = None;
    Panel::left("detail_files")
        .resizable(true)
        .default_size(380.0)
        .min_size(240.0)
        .frame(Frame::new().fill(p.content).inner_margin(Margin::symmetric(0, 8)))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("detail_info")
                .auto_shrink(false)
                .show(ui, |ui| {
                    let c = &d.info;
                    Frame::new().inner_margin(Margin::symmetric(14, 2)).show(ui, |ui| {
                        commit_card(ui, c, actions);
                    });
                    ui.add_space(8.0);
                    let bar = widgets::bar_frame(ui).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} fichier(s)", d.files.len())).strong().color(p.text));
                            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
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
                    ui.add_space(4.0);
                    let root = tab.path().clone();
                    let event = file_tree::show(
                        ui,
                        Id::unique(("commit_tree", c.id.to_string())),
                        &d.files,
                        tab.list_mode,
                        d.selected_file,
                        &mut |ui, target| {
                            let path = match target {
                                Target::File(f) => f.path.clone(),
                                Target::Folder { path, .. } => path.trim_end_matches('/').to_owned(),
                            };
                            if menu_item(ui, "Copier le chemin") {
                                actions.push(Action::Copy(path.clone()));
                            }
                            let abs = root.join(&path);
                            if abs.exists() {
                                if menu_item(ui, "Ouvrir") {
                                    actions.push(Action::Open(abs.clone()));
                                }
                                if menu_item(ui, crate::platform::REVEAL_LABEL) {
                                    actions.push(Action::Reveal(abs));
                                }
                            }
                        },
                    );
                    if let Some(file_tree::Event::Select(i) | file_tree::Event::Activate(i)) = event {
                        actions.push(Action::SelectCommitFile(i));
                    }
                });
        });
    CentralPanel::default()
        .frame(Frame::new().fill(p.content))
        .show(ui, |ui| {
            let Some(diff) = &d.diff else {
                ui.centered_and_justified(|ui| ui.weak("Sélectionnez un fichier"));
                return;
            };
            diff::header(ui, diff, |_| {});
            let mut none = BTreeSet::new();
            let mut anchor = None;
            diff::show(ui, diff, DiffMode::ReadOnly, &mut none, &mut anchor, actions);
        });
    if let Some(mode) = new_mode {
        tab.list_mode = mode;
    }
}

fn commit_card(ui: &mut Ui, c: &CommitInfo, actions: &mut Vec<Action>) {
    let p = theme::of(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
        widgets::avatar(ui.painter(), rect.center(), 17.0, &c.author, &c.email);
        ui.vertical(|ui| {
            ui.label(RichText::new(&c.author).strong().color(p.text));
            ui.label(RichText::new(&c.email).color(p.weak).size(12.0));
        });
    });
    ui.add_space(6.0);
    ui.label(RichText::new(&c.summary).size(14.5).strong().color(p.text));
    let body = c.message.lines().skip(1).collect::<Vec<_>>().join("\n");
    if !body.trim().is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(body.trim()).color(p.text));
    }
    ui.add_space(8.0);
    egui::Grid::new("commit_meta")
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            let key = |ui: &mut Ui, k: &str| ui.label(RichText::new(k).color(p.weak).size(12.0));
            key(ui, "Date");
            ui.label(super::format_date(c.time));
            ui.end_row();
            if c.committer != c.author || c.commit_time != c.time {
                key(ui, "Commiteur");
                ui.label(format!("{} · {}", c.committer, super::format_date(c.commit_time)));
                ui.end_row();
            }
            key(ui, "SHA");
            if ui
                .link(RichText::new(c.id.to_string()).monospace().size(11.0))
                .on_hover_text("Copier")
                .clicked()
            {
                actions.push(Action::Copy(c.id.to_string()));
            }
            ui.end_row();
            if !c.parents.is_empty() {
                key(ui, if c.parents.len() > 1 { "Parents" } else { "Parent" });
                ui.horizontal(|ui| {
                    for parent in &c.parents {
                        let s = parent.to_string();
                        if ui.link(RichText::new(&s[..7]).monospace().size(11.5)).clicked() {
                            actions.push(Action::RevealCommit(*parent));
                        }
                    }
                });
                ui.end_row();
            }
        });
}
