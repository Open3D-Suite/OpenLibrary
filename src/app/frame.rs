// SPDX-License-Identifier: GPL-3.0-or-later
//! The frame of the window: the header (tabs, search, and the buttons of the whole app) and the status bar.

use super::OpenLibraryApp;
use crate::foldertree::TreeAction;
use crate::kit;
use crate::maxfile::Engine;
use crate::texts::{Text, fill};
use crate::ui_settings::{Page, SettingsWindow};
use eframe::egui::{self, Vec2};

pub(super) fn engine_name(engine: Engine, other: &'static str) -> &'static str {
    match engine {
        Engine::Corona => "Corona",
        Engine::VRay => "V-Ray",
        Engine::FStorm => "FStorm",
        Engine::Other => other,
    }
}

impl OpenLibraryApp {
    pub(super) fn header(&mut self, ui: &mut egui::Ui) {
        let p = kit::pal(ui);
        let frame = egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(14, 7)).stroke(egui::Stroke::NONE);
        egui::Panel::top("header").frame(frame).show(ui, |ui| {
            let whole = ui.available_rect_before_wrap();
            let width = (whole.width() * 0.34).clamp(220.0, 520.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let mut open = None;
                for (i, name) in self.tab_names.iter().enumerate() {
                    if kit::tab(ui, name, i == self.tab).clicked() {
                        open = Some(i);
                    }
                }
                if let Some(tab) = open {
                    self.open_tab(tab);
                }
                let tabs_end = ui.cursor().left();
                // in the middle of the window, unless the tabs reach it
                let left = (whole.center().x - width / 2.0).max(tabs_end + 16.0);
                let room = (whole.right() - 120.0 - left).max(120.0);
                ui.add_space((left - tabs_end).max(0.0));
                self.search_box(ui, width.min(room));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.app_buttons(ui));
            });
        });
    }

    fn search_box(&mut self, ui: &mut egui::Ui, width: f32) {
        let hint = self.t(Text::Search);
        let margin = egui::Margin { left: 28, right: 24, top: 6, bottom: 6 };
        let response = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(hint).desired_width(width - 52.0).margin(margin));
        let square = egui::Rect::from_center_size(egui::pos2(response.rect.left() + 15.0, response.rect.center().y), Vec2::splat(15.0));
        kit::paint_icon(ui.painter(), square, "search", kit::pal(ui).muted);
        let mut changed = response.changed();
        if !self.search.is_empty() {
            let square = egui::Rect::from_center_size(egui::pos2(response.rect.right() - 13.0, response.rect.center().y), Vec2::splat(18.0));
            let clear = ui.interact(square, ui.scope_id().with("clear-search"), egui::Sense::click());
            kit::paint_icon(ui.painter(), square.shrink(4.0), "close", kit::pal(ui).muted);
            if clear.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.search.clear();
                changed = true;
            }
        }
        if changed {
            self.models.dirty = true;
        }
    }

    fn app_buttons(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 6.0;
        if kit::icon_button(ui, "settings", self.t(Text::Settings), false).clicked() {
            self.settings_window = Some(SettingsWindow::new(&self.settings, &self.max, self.lang, Page::Tabs, false));
        }
        let dark = ui.visuals().dark_mode;
        let (icon, tip) = if dark { ("sun", Text::ThemeLight) } else { ("moon", Text::ThemeDark) };
        if kit::icon_button(ui, icon, self.t(tip), false).clicked() {
            let theme = if dark { "light" } else { "dark" };
            kit::set_theme(ui.ctx(), Some(theme));
            self.set("theme", theme);
        }
        let refresh = kit::icon_button(ui, "refresh", self.t(Text::RefreshHint), false);
        if refresh.clicked() {
            self.read_again(true);
        }
        refresh.context_menu(|ui| {
            if ui.button(self.t(Text::RefreshQuick)).clicked() {
                self.read_again(true);
                ui.close();
            }
            if ui.button(self.t(Text::RefreshAll)).clicked() {
                self.read_again(false);
                ui.close();
            }
        });
    }

    /// Looks for changes in the folders of the tab, or reads them all again.
    pub(super) fn read_again(&mut self, incremental: bool) {
        match &mut self.surfaces {
            Some(panel) => panel.rescan(incremental, &self.data_dir),
            None => self.scan(incremental),
        }
    }

    /// What was chosen in the menu of the folder tree.
    pub(super) fn tree_action(&mut self, action: Option<TreeAction>) {
        match action {
            None => {}
            Some(TreeAction::Rescan) => self.read_again(true),
            Some(TreeAction::HideNames) => {
                self.ask_text = self.settings.hidden().0.join(", ");
                self.ask = Some(super::Ask::HideNames);
            }
            Some(TreeAction::NewFolder(parent)) => {
                self.ask_text.clear();
                self.ask = Some(super::Ask::NewFolder(parent));
            }
        }
    }

    pub(super) fn set_on_top(&self, ctx: &egui::Context) {
        let level = if self.flag("always_on_top", false) { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }

    /// The text of the status bar.
    fn status(&self) -> String {
        if let Some(work) = self.work_text() {
            return work;
        }
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        if let Some(panel) = &self.surfaces {
            return panel.status();
        }
        match (self.models.scanning(), self.models.progress) {
            (true, Some((folders, models))) => fill(self.t(Text::ScanProgress), &[("folders", &folders), ("models", &models)]),
            (true, None) => self.t(Text::Scanning).to_string(),
            _ => String::new(),
        }
    }

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = kit::pal(ui);
        let frame = egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(12, 3));
        egui::Panel::bottom("status").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(28.0);
                ui.label(egui::RichText::new(format!("v{}", crate::VERSION)).font(kit::mono(11.5)).color(p.accent));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.status_buttons(ui);
                    self.max_state(ui);
                    kit::separator(ui);
                    self.thumb_slider(ui);
                    if let Some((done, total)) = self.work_progress() {
                        if ui.small_button(self.t(Text::Cancel)).clicked() {
                            self.cancel_work();
                        }
                        ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).desired_width(120.0).desired_height(8.0));
                    } else if self.busy() {
                        ui.spinner();
                    }
                    let status = self.status();
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), egui::Sense::hover());
                    let at = egui::pos2(rect.left(), rect.center().y);
                    let font = egui::FontId::proportional(12.5);
                    kit::elided(ui.painter(), at, egui::Align2::LEFT_CENTER, &status, font, p.muted, rect.width());
                    if !status.is_empty() {
                        response.on_hover_text(status);
                    }
                });
            });
        });
    }

    fn status_buttons(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 4.0;
        let info = self.flag("show_info_bar", true);
        if kit::icon_button(ui, "info", self.t(Text::InfoHint), info).clicked() {
            self.set("show_info_bar", !info);
        }
        let pin = self.flag("always_on_top", false);
        if kit::icon_button(ui, "pin", self.t(Text::PinHint), pin).clicked() {
            self.set("always_on_top", !pin);
            self.set_on_top(ui.ctx());
        }
    }

    /// The 3ds Max the models go to.
    fn max_state(&self, ui: &mut egui::Ui) {
        let p = kit::pal(ui);
        let maxes = self.max.maxes();
        let (text, color, tip) = match self.max.target() {
            None => (self.t(Text::MaxNone).to_string(), p.faint, self.t(Text::MaxNoneHint).to_string()),
            Some(info) => {
                let file = if info.file.is_empty() { String::new() } else { format!("  {}", info.file) };
                let text = format!("3ds Max {}{file} · {}", info.year, engine_name(info.engine, self.t(Text::EngineOther)));
                let mut tip = format!("{text}\n\n{}", self.t(Text::MaxHint));
                if maxes.len() > 1 {
                    for max in &maxes {
                        tip.push_str(&format!("\n· 3ds Max {} {}", max.year, max.file));
                    }
                }
                (text, p.ok, tip)
            }
        };
        let font = egui::FontId::proportional(12.5);
        let wide = ui.painter().layout_no_wrap(text.clone(), font.clone(), p.text).size().x.min(300.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(wide + 24.0, 22.0), egui::Sense::hover());
        let at = egui::pos2(rect.right() - 4.0, rect.center().y);
        let used = kit::elided(ui.painter(), at, egui::Align2::RIGHT_CENTER, &text, font, p.text, 300.0);
        ui.painter().circle_filled(egui::pos2(used.left() - 9.0, rect.center().y), 4.0, color);
        response.on_hover_text(tip);
    }

    /// The size of the thumbnails.
    fn thumb_slider(&mut self, ui: &mut egui::Ui) {
        let mut size = self.settings.data.get("thumb_size").and_then(serde_json::Value::as_f64).unwrap_or(160.0);
        ui.spacing_mut().slider_width = 110.0;
        let response = ui.add(egui::Slider::new(&mut size, 96.0..=400.0).show_value(false)).on_hover_text(self.t(Text::ThumbSize));
        if response.changed() {
            self.set("thumb_size", size.round() as i64);
        }
        kit::icon(ui, "preview", 16.0, kit::pal(ui).muted);
    }
}
