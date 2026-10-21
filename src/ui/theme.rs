//! Colors and fonts, modelled on Fork's macOS look (light and dark).

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Margin, Stroke, Theme,
    Visuals, vec2,
};

use crate::git::{ChangeKind, RefKind};

pub struct Palette {
    pub content: Color32,
    pub sidebar: Color32,
    pub bar: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub hover: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub field: Color32,
    pub gutter: Color32,
    pub diff_added: Color32,
    pub diff_removed: Color32,
    pub diff_hunk: Color32,
    pub diff_selected: Color32,
    pub diff_conflict: Color32,
    pub dark: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const DARK: Palette = Palette {
    content: rgb(30, 30, 31),
    sidebar: rgb(38, 38, 40),
    bar: rgb(44, 44, 46),
    border: rgb(20, 20, 21),
    text: rgb(229, 229, 231),
    weak: rgb(142, 142, 147),
    hover: Color32::from_rgba_premultiplied(14, 14, 14, 14),
    accent: rgb(10, 100, 220),
    on_accent: Color32::WHITE,
    field: rgb(24, 24, 25),
    gutter: rgb(36, 36, 38),
    diff_added: Color32::from_rgba_premultiplied(8, 32, 14, 36),
    diff_removed: Color32::from_rgba_premultiplied(40, 11, 10, 40),
    diff_hunk: Color32::from_rgba_premultiplied(2, 22, 44, 44),
    diff_selected: Color32::from_rgba_premultiplied(5, 50, 110, 110),
    diff_conflict: Color32::from_rgba_premultiplied(70, 20, 18, 70),
    dark: true,
};

const LIGHT: Palette = Palette {
    content: rgb(255, 255, 255),
    sidebar: rgb(240, 240, 242),
    bar: rgb(232, 232, 235),
    border: rgb(212, 212, 216),
    text: rgb(29, 29, 31),
    weak: rgb(128, 128, 134),
    hover: Color32::from_rgba_premultiplied(0, 0, 0, 12),
    accent: rgb(0, 110, 230),
    on_accent: Color32::WHITE,
    field: rgb(255, 255, 255),
    gutter: rgb(246, 246, 248),
    diff_added: rgb(226, 247, 231),
    diff_removed: rgb(253, 232, 232),
    diff_hunk: rgb(236, 243, 255),
    diff_selected: Color32::from_rgba_premultiplied(0, 50, 104, 104),
    diff_conflict: rgb(255, 205, 205),
    dark: false,
};

pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

pub fn of(ui: &egui::Ui) -> &'static Palette {
    palette(ui.visuals().dark_mode)
}

pub fn kind_color(kind: ChangeKind) -> Color32 {
    match kind {
        ChangeKind::Added | ChangeKind::Untracked => rgb(52, 180, 90),
        ChangeKind::Modified | ChangeKind::Typechange => rgb(230, 150, 20),
        ChangeKind::Deleted => rgb(235, 75, 65),
        ChangeKind::Renamed => rgb(40, 130, 240),
        ChangeKind::Conflicted => rgb(220, 50, 50),
    }
}

pub fn ref_color(kind: RefKind) -> Color32 {
    match kind {
        RefKind::Head => rgb(52, 170, 85),
        RefKind::LocalBranch => rgb(40, 125, 235),
        RefKind::RemoteBranch => rgb(140, 95, 220),
        RefKind::Tag => rgb(215, 150, 20),
    }
}

pub const LANE_COLORS: [Color32; 8] = [
    rgb(40, 125, 235),
    rgb(240, 135, 40),
    rgb(52, 180, 90),
    rgb(225, 75, 135),
    rgb(150, 105, 235),
    rgb(30, 185, 190),
    rgb(225, 175, 30),
    rgb(225, 85, 70),
];

pub fn lane_color(lane: usize) -> Color32 {
    LANE_COLORS[lane % LANE_COLORS.len()]
}

fn load_system_font(path: &str) -> Option<FontData> {
    std::fs::read(path).ok().map(FontData::from_owned)
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut proportional = Vec::new();
    let mut monospace = Vec::new();
    // macOS system fonts, read at runtime (they cannot be redistributed).
    if let Some(sf) = load_system_font("/System/Library/Fonts/SFNS.ttf") {
        fonts.font_data.insert("sf".into(), sf.into());
        proportional.push("sf".to_owned());
    }
    if let Some(mono) = load_system_font("/System/Library/Fonts/SFNSMono.ttf") {
        fonts.font_data.insert("sfmono".into(), mono.into());
        monospace.push("sfmono".to_owned());
    }
    let mut phosphor = FontData::from_static(include_bytes!("../../assets/fonts/Phosphor.ttf"));
    phosphor.tweak.y_offset_factor = 0.08;
    fonts.font_data.insert("phosphor".into(), phosphor.into());

    for (family, mut first) in [
        (FontFamily::Proportional, proportional),
        (FontFamily::Monospace, monospace),
    ] {
        first.push("phosphor".to_owned());
        let list = fonts.families.entry(family).or_default();
        for (i, name) in first.into_iter().enumerate() {
            list.insert(i, name);
        }
    }
    fonts
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.content;
    v.window_fill = p.content;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.faint_bg_color = p.gutter;
    v.code_bg_color = p.gutter;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, p.on_accent);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.override_text_color = None;
    v.weak_text_color = Some(p.weak);

    let radius = CornerRadius::same(6);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, if p.dark { rgb(56, 56, 58) } else { p.border });
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.corner_radius = radius;
    let button = if p.dark { rgb(58, 58, 60) } else { rgb(255, 255, 255) };
    let button_hover = if p.dark { rgb(70, 70, 73) } else { rgb(245, 245, 247) };
    for (w, fill) in [
        (&mut v.widgets.inactive, button),
        (&mut v.widgets.hovered, button_hover),
        (&mut v.widgets.active, button_hover),
        (&mut v.widgets.open, button_hover),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = radius;
        w.fg_stroke = Stroke::new(1.0, p.text);
        w.bg_stroke = Stroke::new(
            1.0,
            if p.dark { rgb(72, 72, 75) } else { rgb(205, 205, 210) },
        );
        w.expansion = 0.0;
    }
    v
}

pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = vec2(6.0, 4.0);
        style.spacing.button_padding = vec2(10.0, 4.0);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.interact_size.y = 22.0;
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(13.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(13.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(11.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(12.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(17.0),
        );
    });
    // macOS default; egui's 0.3s makes double clicks easy to miss.
    ctx.options_mut(|o| o.input_options.max_double_click_delay = 0.5);
}
