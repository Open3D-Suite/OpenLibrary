// SPDX-License-Identifier: GPL-3.0-or-later
//! The look of the Open3D programs: warm greys, square corners, labels and buttons in capital monospaced letters,
//! and the amber of OpenLibrary, in the dark and in the light theme. Colours, fonts, icons and the few widgets the
//! windows share.

use crate::foldericons::lines_of;
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub surface_hi: Color32,
    pub border: Color32,
    pub border_hi: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_hi: Color32,
    pub accent_text: Color32,
    pub ok: Color32,
    pub warn: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const LIGHT: Palette = Palette {
    bg: rgb(0xF2_F1EE),
    sidebar: rgb(0xED_ECE8),
    surface: rgb(0xF7_F6F3),
    surface_hi: rgb(0xE6_E4DF),
    border: rgb(0xD7_D5CF),
    border_hi: rgb(0xB4_B1A9),
    text: rgb(0x23_2327),
    muted: rgb(0x5D_5C61),
    faint: rgb(0x99_978F),
    accent: rgb(0xD4_8A00),
    accent_hi: rgb(0xE9_9B0C),
    accent_text: rgb(0x1A_1306),
    ok: rgb(0x1E_8F57),
    warn: rgb(0xB8_8700),
};

const DARK: Palette = Palette {
    bg: rgb(0x17_1719),
    sidebar: rgb(0x1C_1C1F),
    surface: rgb(0x20_2023),
    surface_hi: rgb(0x29_292D),
    border: rgb(0x32_3236),
    border_hi: rgb(0x48_484D),
    text: rgb(0xEC_EBE7),
    muted: rgb(0x9C_9A95),
    faint: rgb(0x67_665F),
    accent: rgb(0xF0_A93B),
    accent_hi: rgb(0xFF_BE5C),
    accent_text: rgb(0x1A_1306),
    ok: rgb(0x4C_C38A),
    warn: rgb(0xF2_C230),
};

pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

/// The colours of the theme shown.
pub fn pal(ui: &Ui) -> &'static Palette {
    palette(ui.visuals().dark_mode)
}

const STRONG: &str = "strong";
const MONO_BOLD: &str = "mono-bold";

/// Bolder text, for titles and names.
pub fn strong(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(STRONG.into()))
}

/// The letters of buttons and labels.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_BOLD.into()))
}

/// The fonts of Windows, with those of egui behind them for what they lack.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("SystemRoot").map_or_else(|| std::path::PathBuf::from("C:/Windows"), Into::into).join("Fonts");
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let families: [(FontFamily, &[&str]); 4] = [
        (FontFamily::Proportional, &["segoeui.ttf"]),
        (FontFamily::Name(STRONG.into()), &["seguisb.ttf", "segoeuib.ttf"]),
        (FontFamily::Monospace, &["consola.ttf"]),
        (FontFamily::Name(MONO_BOLD.into()), &["consolab.ttf", "consola.ttf"]),
    ];
    for (family, files) in families {
        let mut list = Vec::new();
        if let Some((name, data)) = files.iter().find_map(|f| Some((*f, std::fs::read(dir.join(f)).ok()?))) {
            fonts.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_owned(data)));
            list.push(name.to_string());
        }
        list.extend(fallback.iter().cloned());
        fonts.families.insert(family, list);
    }
    fonts
}

fn visuals(dark: bool) -> egui::Visuals {
    let p = palette(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border_hi);
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.surface_hi;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.selection.bg_fill = p.accent.gamma_multiply(if dark { 0.32 } else { 0.38 });
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.weak_text_color = Some(p.muted);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.slider_trailing_fill = true;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill, weak, border) in [
        (&mut w.inactive, p.surface_hi, p.surface, p.border),
        (&mut w.hovered, p.surface_hi, p.surface_hi, p.border_hi),
        (&mut w.active, p.border_hi, p.surface_hi, p.text),
        (&mut w.open, p.surface_hi, p.surface_hi, p.border_hi),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = weak;
        state.bg_stroke = Stroke::new(1.0, border);
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.expansion = 0.0;
    }
    for state in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        state.corner_radius = CornerRadius::ZERO;
    }
    v
}

/// Fonts, colours and spacing for both themes.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    for (theme, dark) in [(egui::Theme::Dark, true), (egui::Theme::Light, false)] {
        ctx.set_visuals_of(theme, visuals(dark));
    }
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::{Body, Button, Heading, Monospace, Small};
        style.text_styles = [
            (Small, FontId::proportional(11.5)),
            (Body, FontId::proportional(13.5)),
            (Button, FontId::proportional(13.0)),
            (Monospace, FontId::monospace(12.0)),
            (Heading, strong(18.0)),
        ]
        .into();
        style.spacing.button_padding = Vec2::new(9.0, 5.0);
        style.spacing.interact_size.y = 24.0;
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.scroll.bar_width = 9.0;
    });
}

/// The theme chosen: "dark", "light", or that of Windows.
pub fn set_theme(ctx: &egui::Context, theme: Option<&str>) {
    let (preference, system) = match theme {
        Some("dark") => (egui::ThemePreference::Dark, egui::SystemTheme::Dark),
        Some("light") => (egui::ThemePreference::Light, egui::SystemTheme::Light),
        _ => (egui::ThemePreference::System, egui::SystemTheme::SystemDefault),
    };
    ctx.set_theme(preference);
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(system)); // the title bar
}

// ----------------------------------------------------------------------------------------------------
// Icons
// ----------------------------------------------------------------------------------------------------

/// The icons of the buttons: SVG paths in a box of 24.
const ICONS: [(&str, &str); 23] = [
    ("sun", "M8 12A4 4 0 1 1 16 12A4 4 0 1 1 8 12M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4L7 17M17 7l1.4-1.4"),
    ("moon", "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"),
    ("refresh", "M20 11a8 8 0 0 0-14.6-4.5L4 8M4 3.5V8h4.5M4 13a8 8 0 0 0 14.6 4.5L20 16M20 20.5V16h-4.5"),
    ("search", "M4 10.5A6.5 6.5 0 1 1 17 10.5A6.5 6.5 0 1 1 4 10.5M15.5 15.5L21 21"),
    ("catalog", "M4 8h16v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zM3 4h18v4H3zM12 11v5M9.5 14l2.5 2.5 2.5-2.5"),
    (
        "preview",
        "M4 7.5h3l1.6-2.5h6.8L17 7.5h3a1 1 0 0 1 1 1V18a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V8.5a1 1 0 0 1 1-1zM8.4 13A3.6 3.6 0 1 1 15.6 13A3.6 3.6 0 1 1 8.4 13",
    ),
    ("relink", "M10 14a4 4 0 0 0 5.66 0l3.17-3.17a4 4 0 0 0-5.66-5.66L11.6 6.74M14 10a4 4 0 0 0-5.66 0l-3.17 3.17a4 4 0 0 0 5.66 5.66l1.57-1.57"),
    ("info", "M3.5 12A8.5 8.5 0 1 1 20.5 12A8.5 8.5 0 1 1 3.5 12M12 11v5.2M11.6 7.8A.4 .4 0 1 1 12.4 7.8A.4 .4 0 1 1 11.6 7.8"),
    ("pin", "M9 3.5h6l-1 5 3.5 3.5v1.5h-11V12L10 8.5zM12 13.5V21"),
    ("folder", "M3 6.5A1.5 1.5 0 0 1 4.5 5h4.3l2 2.2h8.7A1.5 1.5 0 0 1 21 8.7v9.8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z"),
    ("max", "M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9"),
    ("image", "M3.5 4.5h17v15h-17zM7.4 10A1.6 1.6 0 1 1 10.6 10A1.6 1.6 0 1 1 7.4 10M4 17l4.5-4 3.5 3 3-2.5 5 4"),
    ("file", "M6 3h8l4 4v14H6zM14 3v4h4"),
    ("settings", "M4 7h9M17 7h3M4 17h3M11 17h9M13 7A2 2 0 1 1 17 7A2 2 0 1 1 13 7M7 17A2 2 0 1 1 11 17A2 2 0 1 1 7 17"),
    ("check", "M5 12.5l4.5 4.5L19 7.5"),
    ("close", "M6 6l12 12M18 6L6 18"),
    ("plus", "M12 5v14M5 12h14"),
    ("minus", "M5 12h14"),
    ("up", "M6 14l6-6 6 6"),
    ("dropdown", "M6 10l6 6 6-6"),
    ("down", "M7 10l5 5 5-5"),
    ("right", "M10 7l5 5-5 5"),
    ("warning", "M12 4L21 20H3zM12 10v5M12 17.2v.4"),
];

static DRAWN: LazyLock<HashMap<&'static str, Vec<Vec<Pos2>>>> = LazyLock::new(|| ICONS.iter().map(|(n, p)| (*n, lines_of(p))).collect());

/// Draws an icon in a square.
pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: &str, color: Color32) {
    let scale = rect.width() / 24.0;
    for line in DRAWN.get(icon).into_iter().flatten() {
        let points = line.iter().map(|p| rect.min + p.to_vec2() * scale).collect();
        painter.add(egui::Shape::line(points, Stroke::new((1.8 * scale).max(1.1), color)));
    }
}

/// An icon by itself.
pub fn icon(ui: &mut Ui, name: &str, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect, name, color);
    response
}

// ----------------------------------------------------------------------------------------------------
// Widgets
// ----------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A border and no fill.
    Action,
    /// In the colour of the app.
    Accent,
    /// A strong border, filled under the mouse.
    Primary,
}

/// A button with capital monospaced letters, and an icon if it has one.
pub fn button(ui: &mut Ui, kind: Kind, icon: Option<&str>, text: &str) -> Response {
    sized_button(ui, kind, icon, text, 0.0)
}

/// The same, at least this wide.
pub fn sized_button(ui: &mut Ui, kind: Kind, icon: Option<&str>, text: &str, width: f32) -> Response {
    let p = pal(ui);
    let enabled = ui.is_enabled();
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), mono_bold(11.5), Color32::PLACEHOLDER);
    let side = if icon.is_some() { 22.0 } else { 0.0 };
    let size = Vec2::new((galley.size().x + side + 22.0).max(width), 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let hot = response.hovered() && enabled;
        let (fill, border, ink) = match (kind, hot) {
            (Kind::Action, false) => (Color32::TRANSPARENT, p.border_hi, p.text),
            (Kind::Action, true) => (p.surface_hi, p.text, p.text),
            (Kind::Accent, false) => (p.accent, p.accent, p.accent_text),
            (Kind::Accent, true) => (p.accent_hi, p.accent_hi, p.accent_text),
            (Kind::Primary, false) => (Color32::TRANSPARENT, p.text, p.text),
            (Kind::Primary, true) => (p.text, p.text, p.bg),
        };
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.4) };
        let painter = ui.painter();
        painter.rect(rect, 0.0, fade(fill), Stroke::new(1.0, fade(border)), egui::StrokeKind::Inside);
        let left = rect.center().x - (galley.size().x + side) / 2.0;
        if let Some(icon) = icon {
            let square = Rect::from_center_size(egui::pos2(left + 8.0, rect.center().y), Vec2::splat(16.0));
            paint_icon(painter, square, icon, fade(ink));
        }
        painter.galley_with_override_text_color(egui::pos2(left + side, rect.center().y - galley.size().y / 2.0 + 1.5), galley, fade(ink));
    }
    let label = text.to_string();
    response.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, enabled, label.clone()));
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// A button that is only an icon; `on` when it is pressed in.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str, on: bool) -> Response {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if response.hovered() || on {
        ui.painter().rect_filled(rect, 0.0, p.surface_hi);
    }
    let ink = if on { p.accent } else { p.text };
    paint_icon(ui.painter(), rect.shrink(5.0), icon, ink);
    let label = tip.to_string();
    response.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, true, label.clone()));
    response.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small switch, for filters.
pub fn chip(ui: &mut Ui, text: &str, on: bool) -> Response {
    let p = pal(ui);
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), mono(11.0), Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(galley.size().x + 20.0, 24.0), Sense::click());
    let (fill, border, ink) = match (on, response.hovered()) {
        (true, _) => (p.text, p.text, p.bg),
        (false, true) => (p.surface_hi, p.border_hi, p.text),
        (false, false) => (Color32::TRANSPARENT, p.border, p.muted),
    };
    ui.painter().rect(rect, 0.0, fill, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0 + Vec2::new(0.0, 1.5), galley, ink);
    let label = text.to_string();
    response.widget_info(move || egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, label.clone()));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A part of a form, with its title and a frame around it.
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).font(strong(14.0)).color(p.text));
    let frame = egui::Frame::new().fill(p.sidebar).stroke(Stroke::new(1.0, p.border)).inner_margin(egui::Margin::same(12));
    let out = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(8.0);
    out
}

/// A row of a form: what it is, and its control where those of the others are.
pub fn field<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let labels = (ui.available_width() * 0.42).min(190.0);
    ui.horizontal(|ui| {
        ui.set_height(28.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(labels, 28.0), Sense::hover());
        let font = FontId::proportional(13.5);
        elided(ui.painter(), rect.left_center(), egui::Align2::LEFT_CENTER, label, font, pal(ui).muted, labels);
        add(ui)
    })
    .inner
}

/// Draws in a part of the window, and nowhere else.
pub fn inside<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(rect.expand(1.0).intersect(ui.clip_rect()));
        add(ui)
    })
    .inner
}

/// A tab of the header: capital letters, and a line below the one shown.
pub fn tab(ui: &mut Ui, text: &str, on: bool) -> Response {
    let p = pal(ui);
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), mono(12.5), Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(galley.size().x + 24.0, 34.0), Sense::click());
    let ink = if on || response.hovered() { p.text } else { p.muted };
    ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, ink);
    if on {
        ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(2.0, p.accent));
    }
    let label = text.to_string();
    response.widget_info(move || egui::WidgetInfo::selected(egui::Role::Tab, true, on, label.clone()));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The small title of a section.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text.to_uppercase()).font(mono(10.5)).color(pal(ui).faint).extra_letter_spacing(1.5));
}

/// A thin upright line between groups.
pub fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 18.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, pal(ui).border_hi));
}

/// A drop-down; `true` when another value was chosen.
pub fn combo<T: PartialEq + Copy>(ui: &mut Ui, id: &str, options: &[(T, &str)], current: &mut T) -> bool {
    let shown = options.iter().find(|(v, _)| v == current).map_or("", |(_, t)| *t);
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(0.0).show_ui(ui, |ui| {
        for (value, text) in options {
            changed |= ui.selectable_value(current, *value, *text).changed();
        }
    });
    changed
}

/// Draws text in one line, cut with "…" at a width; where it was.
pub fn elided(painter: &egui::Painter, at: Pos2, align: egui::Align2, text: &str, font: FontId, color: Color32, width: f32) -> Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping { max_width: width.max(8.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galley = painter.layout_job(job);
    let rect = align.anchor_size(at, galley.size());
    painter.galley(rect.min, galley, color);
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_drawn_inside_its_box() {
        for (name, _) in ICONS {
            let lines = &DRAWN[name];
            assert!(!lines.is_empty(), "{name}");
            for p in lines.iter().flatten() {
                assert!((-0.5..=24.5).contains(&p.x) && (-0.5..=24.5).contains(&p.y), "{name} {p:?}");
            }
        }
    }

    #[test]
    fn the_two_themes_are_those_of_open3d() {
        assert_eq!((palette(true).accent, palette(false).bg), (Color32::from_rgb(0xF0, 0xA9, 0x3B), Color32::from_rgb(0xF2, 0xF1, 0xEE)));
    }
}
