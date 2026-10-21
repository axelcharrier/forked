//! Custom-painted controls shared by the views.

use std::hash::{Hash, Hasher};

use egui::{
    Align2, Color32, CursorIcon, FontId, Id, Painter, Pos2, Rect, Response, Sense, Stroke, Ui,
    pos2, vec2,
};

use super::{icons, theme};
use crate::git::ChangeKind;

pub const ROW_H: f32 = 24.0;
const INDENT: f32 = 14.0;

/// Leading glyph of a [`Row`].
pub enum Lead<'a> {
    None,
    Icon(&'a str, Color32),
    Status(ChangeKind),
}

/// A full-width list row as in Fork's sidebar and file lists.
pub struct Row<'a> {
    pub depth: usize,
    /// `Some(open)` draws a disclosure caret.
    pub disclosure: Option<bool>,
    pub lead: Lead<'a>,
    pub label: &'a str,
    pub detail: Option<&'a str>,
    pub trailing: Option<String>,
    pub selected: bool,
    pub strong: bool,
}

impl<'a> Row<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            depth: 0,
            disclosure: None,
            lead: Lead::None,
            label,
            detail: None,
            trailing: None,
            selected: false,
            strong: false,
        }
    }

    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn disclosure(mut self, open: bool) -> Self {
        self.disclosure = Some(open);
        self
    }

    pub fn icon(mut self, icon: &'a str, color: Color32) -> Self {
        self.lead = Lead::Icon(icon, color);
        self
    }

    pub fn status(mut self, kind: ChangeKind) -> Self {
        self.lead = Lead::Status(kind);
        self
    }

    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail).filter(|d| !d.is_empty());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<String>) -> Self {
        self.trailing = Some(trailing.into()).filter(|t| !t.is_empty());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let p = theme::of(ui);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let painter = ui.painter_at(rect);
        let bg = rect.shrink2(vec2(6.0, 1.0));
        if self.selected {
            painter.rect_filled(bg, 5.0, p.accent);
        } else if resp.hovered() {
            painter.rect_filled(bg, 5.0, p.hover);
        }
        let text = if self.selected { p.on_accent } else { p.text };
        let weak = if self.selected { p.on_accent.gamma_multiply(0.8) } else { p.weak };
        let cy = rect.center().y;
        let mut x = rect.left() + 12.0 + self.depth as f32 * INDENT;

        if let Some(open) = self.disclosure {
            let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
            painter.text(pos2(x + 5.0, cy), Align2::CENTER_CENTER, caret, FontId::proportional(10.0), weak);
        }
        x += 14.0;
        match self.lead {
            Lead::None => {}
            Lead::Icon(icon, color) => {
                let color = if self.selected { p.on_accent } else { color };
                painter.text(pos2(x + 7.0, cy), Align2::CENTER_CENTER, icon, FontId::proportional(15.0), color);
                x += 21.0;
            }
            Lead::Status(kind) => {
                status_badge(&painter, pos2(x + 7.0, cy), kind, self.selected);
                x += 21.0;
            }
        }

        let mut right = rect.right() - 12.0;
        if let Some(t) = &self.trailing {
            let r = pill(&painter, right, cy, t, weak, if self.selected { Color32::TRANSPARENT } else { p.hover });
            right = r.left() - 6.0;
        }
        let label_font = FontId::proportional(if self.strong { 13.5 } else { 13.0 });
        let clip = painter.with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), pos2(right, rect.bottom())));
        let label_rect = clip.text(pos2(x, cy), Align2::LEFT_CENTER, self.label, label_font, text);
        if self.strong {
            // Faux bold: the system font is loaded as a single weight.
            clip.text(pos2(x + 0.45, cy), Align2::LEFT_CENTER, self.label, FontId::proportional(13.5), text);
        }
        if let Some(d) = self.detail {
            clip.text(
                pos2(label_rect.right() + 6.0, cy),
                Align2::LEFT_CENTER,
                d,
                FontId::proportional(11.5),
                weak,
            );
        }
        resp
    }
}

/// Small rounded pill right-aligned at `right`; returns its rect.
pub fn pill(painter: &Painter, right: f32, cy: f32, text: &str, fg: Color32, bg: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), fg);
    let size = vec2(galley.size().x + 10.0, 16.0);
    let rect = Rect::from_min_size(pos2(right - size.x, cy - size.y / 2.0), size);
    painter.rect_filled(rect, 8.0, bg);
    painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
    rect
}

/// Colored square with the status letter (M, A, D, …).
pub fn status_badge(painter: &Painter, center: Pos2, kind: ChangeKind, on_accent: bool) {
    let color = theme::kind_color(kind);
    let rect = Rect::from_center_size(center, vec2(15.0, 15.0));
    if on_accent {
        painter.rect_filled(rect, 3.5, Color32::WHITE);
    } else {
        painter.rect_filled(rect, 3.5, color);
    }
    let letter = match kind {
        ChangeKind::Untracked => "+",
        k => k.letter(),
    };
    painter.text(
        center + vec2(0.0, 0.5),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional(10.5),
        if on_accent { color } else { Color32::WHITE },
    );
}

/// Uppercase section title (BRANCHES, REMOTES…) that toggles its section.
pub fn section(ui: &mut Ui, title: &str, count: Option<usize>, default_open: bool) -> (bool, Response) {
    let id = Id::unique(("section", title));
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, default_open));
    let p = theme::of(ui);
    ui.add_space(6.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
    let painter = ui.painter_at(rect);
    let cy = rect.center().y;
    let label = match count {
        Some(n) => format!("{}  {n}", title.to_uppercase()),
        None => title.to_uppercase(),
    };
    painter.text(pos2(rect.left() + 14.0, cy), Align2::LEFT_CENTER, label, FontId::proportional(10.5), p.weak);
    if resp.hovered() {
        let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
        painter.text(pos2(rect.right() - 16.0, cy), Align2::CENTER_CENTER, caret, FontId::proportional(10.0), p.weak);
    }
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    (open, resp.on_hover_cursor(CursorIcon::PointingHand))
}

/// Remembered open/closed state for tree folders.
pub fn folder_open(ui: &Ui, id: Id, default: bool) -> bool {
    ui.data_mut(|d| *d.get_persisted_mut_or(id, default))
}

pub fn toggle_folder(ui: &Ui, id: Id) {
    ui.data_mut(|d| {
        let v = d.get_persisted_mut_or(id, true);
        *v = !*v;
    });
}

/// Fork-style toolbar button: icon above a small label, with an optional count.
pub fn tool_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool, count: usize) -> Response {
    let p = theme::of(ui);
    let width = (label.len() as f32 * 6.5 + 16.0).max(50.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), sense);
    let painter = ui.painter_at(rect);
    if enabled && resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), 7.0, p.hover);
    }
    let alpha = if enabled { 1.0 } else { 0.35 };
    let color = p.text.gamma_multiply(alpha);
    painter.text(
        pos2(rect.center().x, rect.top() + 15.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(19.0),
        color,
    );
    painter.text(
        pos2(rect.center().x, rect.bottom() - 9.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        p.weak.gamma_multiply(alpha),
    );
    if count > 0 && enabled {
        let text = count.to_string();
        let center = pos2(rect.center().x + 13.0, rect.top() + 7.0);
        let w = (text.len() as f32 * 6.0 + 6.0).max(14.0);
        let badge = Rect::from_center_size(center, vec2(w, 14.0));
        painter.rect_filled(badge, 7.0, p.accent);
        painter.text(badge.center(), Align2::CENTER_CENTER, text, FontId::proportional(9.5), Color32::WHITE);
    }
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Small flat icon button (list/tree toggle, close…).
pub fn icon_button(ui: &mut Ui, icon: &str, active: bool, hint: &str) -> Response {
    let p = theme::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::click());
    let painter = ui.painter_at(rect);
    if active {
        painter.rect_filled(rect, 4.0, p.hover);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, p.hover.gamma_multiply(0.6));
    }
    let color = if active { p.text } else { p.weak };
    painter.text(rect.center(), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), color);
    resp.on_hover_text(hint)
}

/// Accent-filled primary button.
pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool, width: f32) -> Response {
    let p = theme::of(ui);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 28.0), sense);
    let fill = if !enabled {
        if p.dark { Color32::from_gray(58) } else { Color32::from_gray(215) }
    } else if resp.is_pointer_button_down_on() {
        p.accent.gamma_multiply(0.85)
    } else {
        p.accent
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let fg = if enabled { p.on_accent } else { p.weak };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), fg);
    resp
}

fn hash_color(key: &str) -> Color32 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.to_lowercase().hash(&mut h);
    theme::lane_color(h.finish() as usize)
}

pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace().filter_map(|w| w.chars().next());
    let first = words.next().unwrap_or('?');
    match words.next_back() {
        Some(last) => format!("{first}{last}"),
        None => first.to_string(),
    }
    .to_uppercase()
}

/// Round avatar with initials, colored from the email.
pub fn avatar(painter: &Painter, center: Pos2, radius: f32, name: &str, email: &str) {
    painter.circle_filled(center, radius, hash_color(email));
    painter.text(
        center,
        Align2::CENTER_CENTER,
        initials(name),
        FontId::proportional(radius * 0.9),
        Color32::WHITE,
    );
}

/// Thin bordered frame around a block (used for headers).
pub fn bar_frame(ui: &Ui) -> egui::Frame {
    let p = theme::of(ui);
    egui::Frame::new()
        .fill(p.bar)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .stroke(Stroke::NONE)
}

pub fn separator_line(ui: &Ui, rect: Rect) {
    let p = theme::of(ui);
    ui.painter()
        .hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
}
