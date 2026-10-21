use std::collections::BTreeSet;

use egui::{
    Align2, CursorIcon, FontId, Id, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};

use super::{Action, theme, widgets};
use crate::git::FileDiff;
use crate::git::patch::PatchMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    ReadOnly,
    Unstaged { partial: bool },
    Staged { partial: bool },
}

impl DiffMode {
    pub fn partial(self) -> bool {
        matches!(
            self,
            DiffMode::Unstaged { partial: true } | DiffMode::Staged { partial: true }
        )
    }
}

const ROW_H: f32 = 19.0;
const TAB: &str = "    ";

/// File name bar above a diff; `extra` adds right-aligned controls.
pub fn header(ui: &mut Ui, diff: &FileDiff, extra: impl FnOnce(&mut Ui)) {
    let p = theme::of(ui);
    let (add, del) = diff.stats();
    let resp = widgets::bar_frame(ui).show(ui, |ui| {
        ui.set_min_height(24.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(super::icons::FILE).color(p.weak));
            let (dir, name) = super::split_last(&diff.path);
            ui.label(RichText::new(name).strong().color(p.text));
            if !dir.is_empty() {
                ui.label(RichText::new(dir.trim_end_matches('/')).color(p.weak).size(12.0));
            }
            if let Some(old) = &diff.old_path {
                ui.label(RichText::new(format!("← {old}")).color(p.weak).size(12.0));
            }
            ui.label(RichText::new(format!("+{add}")).color(theme::kind_color(crate::git::ChangeKind::Added)));
            ui.label(RichText::new(format!("−{del}")).color(theme::kind_color(crate::git::ChangeKind::Deleted)));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), extra);
        });
    });
    widgets::separator_line(ui, resp.response.rect);
}

pub fn show(
    ui: &mut Ui,
    diff: &FileDiff,
    mode: DiffMode,
    selection: &mut BTreeSet<(usize, usize)>,
    anchor: &mut Option<(usize, usize)>,
    actions: &mut Vec<Action>,
) {
    let p = theme::of(ui);
    if diff.binary {
        ui.centered_and_justified(|ui| ui.weak("Fichier binaire"));
        return;
    }
    if diff.hunks.is_empty() {
        ui.centered_and_justified(|ui| ui.weak("Aucune différence de contenu (mode ou fichier vide)"));
        return;
    }

    let rows: Vec<(usize, Option<usize>)> = diff
        .hunks
        .iter()
        .enumerate()
        .flat_map(|(h, hunk)| {
            std::iter::once((h, None)).chain((0..hunk.lines.len()).map(move |l| (h, Some(l))))
        })
        .collect();

    let font = FontId::monospace(12.0);
    let char_w = ui
        .painter()
        .layout_no_wrap("M".into(), font.clone(), p.text)
        .size()
        .x;
    let max_no = diff
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter_map(|l| l.new_no.max(l.old_no))
        .max()
        .unwrap_or(1);
    let digits = max_no.to_string().len().max(3) as f32;
    let no_w = char_w * (digits + 1.5);
    let gutter = no_w * 2.0;
    let longest = diff
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .map(|l| l.content.chars().count() + l.content.matches('\t').count() * 3)
        .max()
        .unwrap_or(0);
    let text_x = gutter + char_w * 2.5;
    let content_w = (text_x + char_w * (longest as f32 + 2.0)).max(ui.available_width());
    let shift = ui.input(|i| i.modifiers.shift);
    let added = theme::kind_color(crate::git::ChangeKind::Added);
    let removed = theme::kind_color(crate::git::ChangeKind::Deleted);
    let mut clicked: Option<(usize, usize)> = None;

    ui.spacing_mut().item_spacing.y = 0.0;
    ScrollArea::both()
        .auto_shrink(false)
        .id_salt(("diff", &diff.path))
        .show_rows(ui, ROW_H, rows.len(), |ui, range| {
            let visible_right = ui.clip_rect().right();
            for &(h, line) in &rows[range] {
                let hunk = &diff.hunks[h];
                let Some(l) = line else {
                    let (rect, _) = ui.allocate_exact_size(vec2(content_w, ROW_H + 6.0), Sense::hover());
                    let band = rect.shrink2(vec2(0.0, 2.0));
                    ui.painter().rect_filled(band, 0.0, p.diff_hunk);
                    ui.painter().text(
                        pos2(rect.left() + 10.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        &hunk.header,
                        font.clone(),
                        p.weak,
                    );
                    let in_hunk: Vec<usize> = selection
                        .iter()
                        .filter(|(sh, _)| *sh == h)
                        .map(|(_, sl)| *sl)
                        .collect();
                    let mut x = visible_right - 8.0;
                    let mut button = |ui: &Ui, label: &str, mode: PatchMode, lines: bool| {
                        if hunk_button(ui, &mut x, rect.center().y, label, Id::unique(("hunk", label, h))) {
                            actions.push(Action::Patch {
                                mode,
                                hunk: h,
                                lines: lines.then(|| in_hunk.clone()),
                            });
                        }
                    };
                    // Laid out right to left.
                    match mode {
                        DiffMode::Unstaged { partial: true } if in_hunk.is_empty() => {
                            button(ui, "Annuler le bloc", PatchMode::Discard, false);
                            button(ui, "Indexer le bloc", PatchMode::Stage, false);
                        }
                        DiffMode::Unstaged { partial: true } => {
                            button(ui, "Annuler les lignes", PatchMode::Discard, true);
                            button(ui, "Indexer les lignes", PatchMode::Stage, true);
                        }
                        DiffMode::Staged { partial: true } if in_hunk.is_empty() => {
                            button(ui, "Désindexer le bloc", PatchMode::Unstage, false);
                        }
                        DiffMode::Staged { partial: true } => {
                            button(ui, "Désindexer les lignes", PatchMode::Unstage, true);
                        }
                        _ => {}
                    }
                    continue;
                };

                let dl = &hunk.lines[l];
                let selectable = mode.partial() && matches!(dl.origin, '+' | '-');
                let sense = if selectable { Sense::click() } else { Sense::hover() };
                let (rect, resp) = ui.allocate_exact_size(vec2(content_w, ROW_H), sense);
                let painter = ui.painter();
                let bg = match dl.origin {
                    '+' => Some(p.diff_added),
                    '-' => Some(p.diff_removed),
                    '!' => Some(p.diff_conflict),
                    _ => None,
                };
                let gutter_rect = Rect::from_min_size(rect.min, vec2(gutter, ROW_H));
                painter.rect_filled(gutter_rect, 0.0, p.gutter);
                if let Some(bg) = bg {
                    painter.rect_filled(rect, 0.0, bg);
                }
                let is_selected = selection.contains(&(h, l));
                if is_selected {
                    painter.rect_filled(rect, 0.0, p.diff_selected);
                    painter.rect_filled(
                        Rect::from_min_size(rect.min, vec2(3.0, ROW_H)),
                        0.0,
                        p.accent,
                    );
                } else if selectable && resp.hovered() {
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, p.accent.gamma_multiply(0.6)), StrokeKind::Inside);
                }
                let cy = rect.center().y;
                let x0 = rect.left();
                let no = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
                painter.text(pos2(x0 + no_w - char_w * 0.7, cy), Align2::RIGHT_CENTER, no(dl.old_no), font.clone(), p.weak);
                painter.text(pos2(x0 + gutter - char_w * 0.7, cy), Align2::RIGHT_CENTER, no(dl.new_no), font.clone(), p.weak);
                let (marker, marker_color) = match dl.origin {
                    '+' => ("+", added),
                    '-' => ("−", removed),
                    _ => ("", p.weak),
                };
                painter.text(pos2(x0 + gutter + char_w * 0.6, cy), Align2::LEFT_CENTER, marker, font.clone(), marker_color);
                let (content, color) = if dl.origin == '\\' {
                    (format!("\\ {}", dl.content), p.weak)
                } else {
                    (dl.content.replace('\t', TAB), p.text)
                };
                painter.text(pos2(x0 + text_x, cy), Align2::LEFT_CENTER, content, font.clone(), color);
                if selectable {
                    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                    if resp.clicked() {
                        clicked = Some((h, l));
                    }
                }
            }
        });

    if let Some((h, l)) = clicked {
        match *anchor {
            Some((ah, al)) if shift && ah == h => {
                for i in al.min(l)..=al.max(l) {
                    if matches!(diff.hunks[h].lines[i].origin, '+' | '-') {
                        selection.insert((h, i));
                    }
                }
            }
            _ => {
                if !selection.remove(&(h, l)) {
                    selection.insert((h, l));
                }
                *anchor = Some((h, l));
            }
        }
    }
}

/// A compact button painted inside a virtualized row, laid out right to left.
fn hunk_button(ui: &Ui, right: &mut f32, cy: f32, text: &str, id: Id) -> bool {
    let p = theme::of(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(11.5), p.text);
    let size = vec2(galley.size().x + 14.0, 18.0);
    let rect = Rect::from_min_size(pos2(*right - size.x, cy - size.y / 2.0), size);
    let resp = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    let visuals = ui.style().interact(&resp);
    ui.painter().rect(rect, 5.0, visuals.bg_fill, visuals.bg_stroke, StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.text);
    *right = rect.left() - 6.0;
    resp.clicked()
}
