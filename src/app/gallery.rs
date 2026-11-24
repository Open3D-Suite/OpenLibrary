// SPDX-License-Identifier: GPL-3.0-or-later
//! The tab of models: the folders, the title with its actions, the filters, and the cards.

use super::frame::engine_name;
use super::{Job, OpenLibraryApp, Shown};
use crate::kit::{self, Kind};
use crate::library::Asset;
use crate::max_session::MergeKind;
use crate::maxfile::Engine;
use crate::texts::{Text, fill};
use crate::ui_catalog::CatalogWindow;
use eframe::egui::{self, Color32, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Below a card, when names are shown.
const FOOTER: f32 = 44.0;

pub(super) fn known(engine: Engine) -> bool {
    engine != Engine::Other
}

/// How the cards are laid out.
#[derive(Clone, Copy)]
pub(crate) struct Grid {
    pub side: f32,
    pub cell: Vec2,
    pub margin: f32,
    pub columns: usize,
    cards: bool,
}

impl Grid {
    /// The thumbnails grow until they fill the row: the slider gives the smallest size.
    pub fn new(width: f32, thumb: f32, cards: bool) -> Self {
        let margin = if cards { 6.0 } else { 3.0 };
        let columns = ((width / (thumb + 2.0 * margin)).floor() as usize).max(1);
        let side = (width / columns as f32 - 2.0 * margin).floor().max(thumb.min(width - 2.0 * margin));
        let footer = if cards { FOOTER } else { 0.0 };
        Self { side, cell: Vec2::new(side + 2.0 * margin, side + footer + 2.0 * margin), margin, columns, cards }
    }
}

impl OpenLibraryApp {
    // ---- what is shown ------------------------------------------------------------------------------------

    /// The renderer of the scene of the 3ds Max the models go to.
    pub(super) fn scene_engine(&self) -> Engine {
        self.max.target().map_or(Engine::Other, |t| t.engine)
    }

    /// The renderers of a model, as far as they are known without asking the disk; `None` for one not read yet.
    pub(super) fn engines_of(&self, asset: &Asset) -> Vec<Option<Engine>> {
        let mut out: Vec<Option<Engine>> = asset.versions.keys().map(|e| Some(*e)).collect();
        out.extend(asset.auto_files.iter().map(|f| self.engines.peek(f)));
        out.sort();
        out.dedup();
        out
    }

    /// Whether a filter depends on what the background reads.
    pub(super) fn filters_need_files(&self) -> bool {
        !matches!(self.option("filter_engine"), "" | "all") || self.flag("olib_filter_warn", false)
    }

    fn filtered(&self) -> bool {
        self.filters_need_files() || self.flag("olib_filter_nopreview", false)
    }

    pub(super) fn refresh_shown(&mut self) {
        if !std::mem::take(&mut self.models.dirty) {
            return;
        }
        let Some(index) = &self.models.index else {
            self.models.shown.clear();
            return;
        };
        let engine = match self.option("filter_engine") {
            "scene" => Some(self.scene_engine()).filter(|e| known(*e)),
            key => Engine::from_key(key).filter(|e| known(*e)),
        };
        let (warn, bare) = (self.flag("olib_filter_warn", false), self.flag("olib_filter_nopreview", false));
        let folder = self.models.side.folder.as_deref();
        let text = self.search.trim();
        let recursive = self.flag("include_subfolders", true) || !text.is_empty() || folder.is_none();
        let mut found = index.find(text, folder, recursive);
        if found.is_empty() && !recursive {
            found = index.find(text, folder, true); // a folder that only has subfolders: what is in them
        }
        let any = crate::favorites::any(&self.settings);
        let wanted = |asset: &Asset| {
            engine.is_none_or(|e| self.engines_of(asset).contains(&Some(e)))
                && (!bare || asset.preview.is_none())
                && (!warn || asset.all_files().iter().any(|f| self.checks.warns(f)))
        };
        let roots = &index.roots;
        let favorite = |key: &str| any && crate::favorites::is_favorite(&self.settings, &crate::favorites::model_key(roots, key));
        let mut shown: Vec<Shown> = found
            .into_iter()
            .filter(|r| !self.models.hidden.contains(&r.folder) && wanted(&r.asset))
            .map(|r| Shown { asset: r.asset.clone(), key: r.key.clone(), favorite: favorite(&r.key) })
            .collect();
        let count = if any { index.rows().iter().filter(|r| !self.models.hidden.contains(&r.folder) && favorite(&r.key)).count() } else { 0 };
        if self.option("sort") == "recent" {
            shown.sort_by(|a, b| b.asset.mtime.total_cmp(&a.asset.mtime));
        }
        if self.models.side.favorites {
            shown.retain(|s| s.favorite);
        } else if self.tab_view().favorites_first {
            shown.sort_by_key(|s| !s.favorite); // stable: the order of before within each part
        }
        self.models.favorite_count = count;
        self.models.shown = shown;
        if !self.models.selected.is_empty() {
            let keys: std::collections::HashSet<&str> = self.models.shown.iter().map(|s| s.key.as_str()).collect();
            self.models.selected.retain(|k| keys.contains(k.as_str()));
        }
    }

    // ---- the tab ------------------------------------------------------------------------------------------

    pub(super) fn models_ui(&mut self, ui: &mut egui::Ui) {
        let p = kit::pal(ui);
        let side = egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin { left: 12, right: 8, top: 12, bottom: 8 });
        egui::Panel::left("tree").frame(side).resizable(true).default_size(230.0).min_size(170.0).show(ui, |ui| self.sidebar(ui));
        if self.flag("show_info_bar", true) {
            let frame = egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::same(14));
            egui::Panel::right("inspector").frame(frame).resizable(true).default_size(310.0).min_size(250.0).show(ui, |ui| self.inspector(ui));
        }
        let main = egui::Frame::new().fill(p.bg).inner_margin(egui::Margin { left: 18, right: 10, top: 12, bottom: 4 });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            self.title_row(ui);
            ui.add_space(4.0);
            self.filter_row(ui);
            ui.add_space(6.0);
            self.gallery(ui);
        });
        self.drag(ui.ctx());
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        kit::caption(ui, self.t(Text::Folders));
        ui.add_space(2.0);
        let (view, counts) = (self.tab_view(), (self.models.favorite_count, self.models.all_count));
        let mut subfolders = self.flag("include_subfolders", true);
        egui::Panel::bottom("subfolders").frame(egui::Frame::NONE).show_separator_line(false).show(ui, |ui| {
            ui.add_space(4.0);
            if ui.checkbox(&mut subfolders, self.t(Text::Subfolders)).changed() {
                self.set("include_subfolders", subfolders);
                self.models.dirty = true;
            }
        });
        let changed = self.models.side.show(ui, "models", &mut self.settings, self.lang, view, counts);
        if changed.settings {
            self.save_settings();
            self.rebuild_tree();
        }
        if changed.shown {
            self.models.dirty = true;
            self.loader.clear_queue();
        }
        self.tree_action(changed.action);
    }

    fn title_row(&mut self, ui: &mut egui::Ui) {
        let p = kit::pal(ui);
        let text = self.search.trim();
        let title = if !text.is_empty() {
            fill(self.t(Text::ResultsOf), &[("text", &text)])
        } else if self.models.side.favorites {
            self.t(Text::Favorites).to_string()
        } else {
            let folder = self.models.side.folder.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned());
            folder.or_else(|| self.tab_names.get(self.tab).cloned()).unwrap_or_default()
        };
        let count = fill(self.t(Text::Models), &[("count", &self.models.shown.len())]);
        ui.horizontal(|ui| {
            ui.set_height(32.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.relink_button(ui);
                self.preview_button(ui);
                if kit::button(ui, Kind::Accent, Some("catalog"), self.t(Text::CatalogGo)).on_hover_text(self.t(Text::CatalogTip)).clicked() {
                    self.open_catalogue();
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let room = (ui.available_width() - 120.0).max(60.0);
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(room, 30.0), Sense::hover());
                    let at = egui::pos2(rect.left(), rect.center().y);
                    let used = kit::elided(ui.painter(), at, egui::Align2::LEFT_CENTER, &title, kit::strong(20.0), p.text, room);
                    let at = egui::pos2(used.right() + 10.0, rect.center().y + 3.0);
                    ui.painter().text(at, egui::Align2::LEFT_CENTER, count, egui::FontId::proportional(13.0), p.muted);
                });
            });
        });
    }

    fn open_catalogue(&mut self) {
        let config = self.settings.data.get("catalog").and_then(Value::as_object);
        let tools = (self.lang, self.wake.clone());
        let models = &self.models;
        self.catalog = Some(CatalogWindow::new(config, models.index.as_ref(), &models.hidden, models.side.folder.as_deref(), &self.data_dir, tools));
    }

    fn preview_button(&mut self, ui: &mut egui::Ui) {
        let button = kit::button(ui, Kind::Action, Some("preview"), self.t(Text::Previews)).on_hover_text(self.t(Text::PreviewTip));
        egui::Popup::menu(&button).show(|ui| {
            ui.set_min_width(320.0);
            let selected = self.models.selected_assets();
            let one = (selected.len() == 1).then(|| selected[0].clone());
            for (text, from_view) in [(Text::MenuPreviewSelection, false), (Text::MenuPreviewView, true)] {
                if ui.add_enabled(one.is_some(), egui::Button::new(self.t(text))).clicked()
                    && let Some(asset) = &one
                {
                    self.live_preview(asset, from_view);
                }
            }
            ui.separator();
            let label = fill(self.t(Text::PreviewOfSelected), &[("count", &selected.len())]);
            if ui.add_enabled(!selected.is_empty(), egui::Button::new(label)).clicked() {
                self.ask_previews(&selected);
            }
            let shown = self.models.shown.iter().map(|s| &s.asset);
            let without: Vec<Arc<Asset>> = shown.filter(|a| a.preview.is_none() && a.any_file().is_some()).cloned().collect();
            let label = fill(self.t(Text::PreviewOfMissing), &[("count", &without.len())]);
            if ui.add_enabled(!without.is_empty(), egui::Button::new(label)).clicked() {
                self.ask_previews(&without);
            }
        });
    }

    fn relink_button(&mut self, ui: &mut egui::Ui) {
        let button = kit::button(ui, Kind::Action, Some("relink"), self.t(Text::Relink)).on_hover_text(self.t(Text::RelinkTip));
        egui::Popup::menu(&button).show(|ui| {
            ui.set_min_width(320.0);
            let selected = self.models.selected_assets();
            if ui.button(self.t(Text::RelinkScene)).clicked() {
                self.relink_scene(&selected);
            }
            ui.separator();
            let files = |list: &[Arc<Asset>]| list.iter().map(|a| a.all_files().len()).sum::<usize>();
            let label = fill(self.t(Text::RelinkSelected), &[("count", &files(&selected))]);
            if ui.add_enabled(!selected.is_empty(), egui::Button::new(label)).clicked() {
                self.ask_relink(&selected);
            }
            let all: Vec<Arc<Asset>> = self.models.shown.iter().map(|s| s.asset.clone()).collect();
            let label = fill(self.t(Text::RelinkAll), &[("count", &files(&all))]);
            if ui.add_enabled(!all.is_empty(), egui::Button::new(label)).clicked() {
                self.ask_relink(&all);
            }
        });
    }

    fn filter_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.set_height(28.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            let current = match self.option("filter_engine") {
                "" => "all".to_string(),
                key => key.to_string(),
            };
            let chips = [
                ("all", self.t(Text::FilterAll), Text::FilterAllTip),
                ("corona", "Corona", Text::FilterEngineTip),
                ("vray", "V-Ray", Text::FilterEngineTip),
                ("fstorm", "FStorm", Text::FilterEngineTip),
                ("scene", self.t(Text::FilterScene), Text::FilterSceneTip),
            ];
            for (key, text, tip) in chips {
                if kit::chip(ui, text, current == key).on_hover_text(self.t(tip)).clicked() {
                    self.set("filter_engine", key);
                    self.models.dirty = true;
                }
            }
            kit::separator(ui);
            for (key, text, tip) in [
                ("olib_filter_warn", Text::FilterWarn, Text::FilterWarnTip),
                ("olib_filter_nopreview", Text::FilterNoPreview, Text::FilterNoPreviewTip),
            ] {
                let on = self.flag(key, false);
                if kit::chip(ui, self.t(text), on).on_hover_text(self.t(tip)).clicked() {
                    self.set(key, !on);
                    self.models.dirty = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let names = self.option("view_mode") == "cards";
                if kit::chip(ui, self.t(Text::ShowNames), names).on_hover_text(self.t(Text::ShowNamesTip)).clicked() {
                    self.set("view_mode", if names { "gallery" } else { "cards" });
                }
                let mut recent = self.option("sort") == "recent";
                let options = [(false, self.t(Text::SortName)), (true, self.t(Text::SortRecent))];
                if kit::combo(ui, "sort", &options, &mut recent) {
                    self.set("sort", if recent { "recent" } else { "name" });
                    self.models.dirty = true;
                }
            });
        });
    }

    // ---- the cards ----------------------------------------------------------------------------------------

    fn empty_text(&self) -> String {
        if self.settings.library_paths(self.tab).is_empty() {
            return self.t(Text::NoFolders).to_string();
        }
        if self.models.index.is_none() {
            return self.t(Text::Scanning).to_string();
        }
        let text = self.search.trim();
        if !text.is_empty() {
            return fill(self.t(Text::NothingMatches), &[("text", &text)]);
        }
        self.t(if self.models.side.favorites {
            Text::NoFavorites
        } else if self.filtered() {
            Text::NothingPasses
        } else if self.models.scanning() {
            Text::Scanning
        } else {
            Text::NoModels
        })
        .to_string()
    }

    fn gallery(&mut self, ui: &mut egui::Ui) {
        if self.models.shown.is_empty() {
            let text = self.empty_text();
            ui.centered_and_justified(|ui| ui.weak(text));
            return;
        }
        let thumb = self.settings.data.get("thumb_size").and_then(Value::as_f64).unwrap_or(160.0).clamp(80.0, 400.0) as f32;
        let bar = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin + ui.spacing().scroll.bar_outer_margin;
        let grid = Grid::new(ui.available_width() - bar, thumb, self.option("view_mode") == "cards");
        let rows = self.models.shown.len().div_ceil(grid.columns);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut area = egui::ScrollArea::vertical().id_salt(("gallery", self.tab)).auto_shrink(false);
        if let Some(at) = self.models.reveal.take() {
            let top = (at / grid.columns) as f32 * grid.cell.y;
            area = area.vertical_scroll_offset((top - ui.available_height() / 2.0 + grid.cell.y / 2.0).max(0.0));
        }
        let mut seen = 0..0;
        area.show_rows(ui, grid.cell.y, rows, |ui, range| {
            seen = range.start * grid.columns..range.end * grid.columns;
            for row in range {
                ui.horizontal(|ui| {
                    for at in row * grid.columns..((row + 1) * grid.columns).min(self.models.shown.len()) {
                        self.card(ui, at, grid);
                    }
                });
            }
            // where there are no cards: a click selects nothing
            let rest = ui.available_rect_before_wrap();
            if rest.is_positive() && ui.interact(rest, ui.scope_id().with("empty"), Sense::click()).clicked() {
                self.models.selected.clear();
            }
        });
        self.prefetch(seen);
    }

    /// Asks for the thumbnails of the next screens, when those of this one are here.
    fn prefetch(&mut self, seen: std::ops::Range<usize>) {
        if self.loader.waiting() > 4 {
            return;
        }
        let count = seen.len();
        for item in self.models.shown.iter().skip(seen.end).take(count * 2) {
            if !self.textures.contains_key(&item.key)
                && let Some(src) = item.asset.thumb_source()
            {
                self.loader.prefetch(&item.key, src);
            }
        }
    }

    fn select(&mut self, at: usize, modifiers: egui::Modifiers) {
        let Some(key) = self.models.shown.get(at).map(|s| s.key.clone()) else { return };
        let models = &mut self.models;
        if modifiers.shift {
            let range = models.anchor.min(at)..=models.anchor.max(at);
            models.selected = models.shown.get(range).unwrap_or_default().iter().map(|s| s.key.clone()).collect();
            return;
        }
        models.anchor = at;
        if !modifiers.command {
            models.selected.clear();
        }
        if !models.selected.remove(&key) {
            models.selected.insert(key);
        }
    }

    /// The arrows go from card to card.
    pub(super) fn arrow_keys(&mut self, ctx: &egui::Context) {
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        let by: isize = if pressed(egui::Key::ArrowRight) || pressed(egui::Key::ArrowDown) {
            1
        } else if pressed(egui::Key::ArrowLeft) || pressed(egui::Key::ArrowUp) {
            -1
        } else {
            return;
        };
        if self.models.shown.is_empty() {
            return;
        }
        let at = self.models.anchor.saturating_add_signed(by).min(self.models.shown.len() - 1);
        self.select(at, egui::Modifiers::NONE);
        self.models.reveal = Some(at);
    }

    /// What is asked of the disk for a card on screen, once, in the background.
    fn ask_files(&mut self, asset: &Asset) {
        for file in asset.all_files() {
            if self.asked.contains(file) {
                continue;
            }
            self.asked.insert(file.to_path_buf());
            if asset.auto_files.iter().any(|f| f == file) {
                let _ = self.jobs.send(Job::Engine(file.to_path_buf()));
            }
            let roots = self.all_roots();
            let _ = self.jobs.send(Job::Check(file.to_path_buf(), roots));
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, at: usize, grid: Grid) {
        let (cell, response) = ui.allocate_exact_size(grid.cell, Sense::click_and_drag());
        let Some(item) = self.models.shown.get(at).cloned() else { return };
        if !ui.is_rect_visible(cell) {
            return;
        }
        let rect = cell.shrink(grid.margin);
        let selected = self.models.selected.contains(&item.key);
        let hovered = response.hovered();
        self.ask_files(&item.asset);
        let image = Rect::from_min_size(rect.min, Vec2::splat(grid.side));
        self.paint_image(ui, &item, rect, image, (hovered, selected), grid.cards);
        self.paint_engines(ui, &item.asset, image);
        let warns = item.asset.all_files().iter().any(|f| self.checks.warns(f));
        if warns {
            let size = crate::badges::size(grid.side) * 0.82;
            let square = Rect::from_min_size(egui::pos2(image.right() - size - 6.0, image.top() + 6.0), Vec2::splat(size));
            crate::badges::warning(ui.painter(), square);
        }
        if grid.cards {
            self.paint_footer(ui, &item, rect, selected);
        }
        let heart = if grid.cards {
            Rect::from_min_size(egui::pos2(rect.right() - 30.0, rect.bottom() - FOOTER + 2.0), Vec2::splat(24.0))
        } else {
            Rect::from_min_size(image.right_bottom() - Vec2::splat(30.0), Vec2::splat(24.0))
        };
        if (item.favorite || hovered || selected) && self::heart(ui, &response, heart, item.favorite, !grid.cards) {
            self.toggle_favorites(std::slice::from_ref(&item.key));
            return;
        }
        let p = kit::pal(ui);
        let border = match (selected, hovered) {
            (true, _) => Stroke::new(2.5, p.accent),
            (false, true) => Stroke::new(1.2, p.border_hi),
            _ => Stroke::new(1.0, p.border),
        };
        ui.painter().rect_stroke(rect, 0.0, border, StrokeKind::Inside);
        self.card_clicks(ui, at, &item, selected, &response);
    }

    fn card_clicks(&mut self, ui: &egui::Ui, at: usize, item: &Shown, selected: bool, response: &egui::Response) {
        if response.drag_started_by(egui::PointerButton::Primary) {
            if !selected {
                self.select(at, egui::Modifiers::NONE);
            }
            self.dragging = true;
        }
        if response.secondary_clicked() && !selected {
            self.select(at, egui::Modifiers::NONE);
        }
        if response.double_clicked() {
            self.models.selected = std::collections::HashSet::from([item.key.clone()]);
            self.quick_look();
        } else if response.clicked() {
            self.select(at, ui.input(|i| i.modifiers));
        }
        response.context_menu(|ui| self.card_menu(ui, item));
        if response.hovered() && !self.dragging {
            response.clone().on_hover_ui(|ui| self.tooltip(ui, &item.asset));
        }
    }

    fn tooltip(&self, ui: &mut egui::Ui, asset: &Asset) {
        ui.set_max_width(460.0);
        ui.strong(&asset.name);
        ui.weak(asset.folder.display().to_string());
        for file in asset.all_files() {
            let engine = asset.versions.iter().find(|(_, f)| f.as_path() == file).map(|(e, _)| *e).or_else(|| self.engines.peek(file));
            let label = engine.map_or(self.t(Text::EnginePending), |e| engine_name(e, self.t(Text::EngineOther)));
            ui.label(format!("{label}: {}", file_name(file)));
        }
        if asset.preview.is_none() {
            ui.weak(self.t(Text::NoPreview));
        }
        let names: Vec<String> = asset.all_files().iter().filter_map(|f| self.checks.peek(f)).flat_map(|c| c.missing).collect();
        if !names.is_empty() {
            ui.colored_label(kit::pal(ui).warn, fill(self.t(Text::TipMissing), &[("count", &names.len())]));
            for name in names.iter().take(6) {
                ui.weak(format!("· {name}"));
            }
        }
    }

    fn paint_image(&mut self, ui: &egui::Ui, item: &Shown, rect: Rect, image: Rect, (hovered, selected): (bool, bool), cards: bool) {
        let p = kit::pal(ui);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, if hovered || selected { p.surface_hi } else { p.surface });
        let mut waiting = false;
        match self.textures.get(&item.key) {
            Some(Some(texture)) => {
                painter.rect_filled(image, 0.0, self.edges.get(&item.key).copied().unwrap_or(p.surface));
                picture(&painter, image, texture);
                if !cards && (hovered || selected) {
                    name_over(&painter, image, &item.asset.name);
                }
                return;
            }
            Some(None) => {}
            None => match item.asset.thumb_source() {
                Some(src) => {
                    self.loader.request(&item.key, src);
                    waiting = true;
                }
                None => {
                    self.textures.insert(item.key.clone(), None);
                }
            },
        }
        let text = self.t(if waiting { Text::Loading } else { Text::NoPreview });
        if cards {
            painter.text(image.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.5), p.faint);
        } else {
            let galley = painter.layout(item.asset.name.clone(), kit::strong(13.0), p.muted, image.width() - 20.0);
            painter.galley(image.center() - galley.size() / 2.0, galley, p.muted);
            let at = image.center_bottom() - Vec2::new(0.0, 14.0);
            painter.text(at, egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.0), p.faint);
        }
    }

    /// The renderers, at the top left; the version that is not of the scene's renderer, faded.
    fn paint_engines(&self, ui: &egui::Ui, asset: &Asset, image: Rect) {
        let size = crate::badges::size(image.width());
        let scene = self.scene_engine();
        let mut x = image.left() + 7.0;
        for engine in self.engines_of(asset) {
            let other = known(scene) && engine.is_some_and(|e| known(e) && e != scene);
            crate::badges::engine(ui.painter(), Rect::from_min_size(egui::pos2(x, image.top() + 7.0), Vec2::splat(size)), engine, other);
            x += size + 4.0;
        }
    }

    fn paint_footer(&self, ui: &egui::Ui, item: &Shown, rect: Rect, selected: bool) {
        let p = kit::pal(ui);
        let foot = Rect::from_min_max(egui::pos2(rect.left() + 10.0, rect.bottom() - FOOTER), rect.max - Vec2::new(10.0, 4.0));
        let painter = ui.painter_at(rect);
        let color = if selected { p.accent } else { p.text };
        let name = egui::pos2(foot.left(), foot.top() + 13.0);
        kit::elided(&painter, name, egui::Align2::LEFT_CENTER, &item.asset.name, kit::strong(13.0), color, foot.width() - 24.0);
        let engines = self.engines_of(&item.asset);
        let labels: Vec<&str> =
            engines.iter().map(|e| e.map_or(self.t(Text::EnginePending), |e| engine_name(e, self.t(Text::EngineOther)))).collect();
        let sub = if labels.is_empty() { self.t(Text::NoMaxFile).to_string() } else { labels.join(" · ") };
        let at = egui::pos2(foot.left(), foot.bottom() - 10.0);
        kit::elided(&painter, at, egui::Align2::LEFT_CENTER, &sub, egui::FontId::proportional(11.5), p.muted, foot.width());
    }

    // ---- the menu of a card -------------------------------------------------------------------------------

    fn card_menu(&mut self, ui: &mut egui::Ui, item: &Shown) {
        ui.set_min_width(290.0);
        let mut selection = self.models.selection();
        if !selection.iter().any(|s| s.key == item.key) {
            selection = vec![item.clone()];
        }
        let assets: Vec<Arc<Asset>> = selection.iter().map(|s| s.asset.clone()).collect();
        let many = if assets.len() > 1 { format!(" ({})", assets.len()) } else { String::new() };
        let all = selection.iter().all(|s| s.favorite);
        if ui.button(self.t(if all { Text::FavoriteRemove } else { Text::FavoriteAdd })).clicked() {
            let keys: Vec<String> = selection.iter().map(|s| s.key.clone()).collect();
            self.toggle_favorites(&keys);
        }
        ui.separator();
        let target = self.max.target();
        let proxy = target.as_ref().is_some_and(|t| matches!(t.engine, Engine::Corona | Engine::VRay));
        for (text, kind, enabled) in [
            (Text::MenuPlace, MergeKind::Place, target.is_some()),
            (Text::MenuProxy, MergeKind::Proxy, proxy),
            (Text::MenuOrigin, MergeKind::Origin, target.is_some()),
        ] {
            if ui.add_enabled(enabled, egui::Button::new(format!("{}{many}", self.t(text)))).clicked() {
                self.begin_merge(assets.clone(), kind);
            }
        }
        ui.separator();
        self.open_menu(ui, &item.asset, target.is_some());
        let file = item.asset.any_file().map_or_else(|| item.asset.folder.clone(), PathBuf::from);
        if ui.button(self.t(Text::FolderReveal)).clicked() {
            crate::platform::reveal(&file);
        }
        if ui.button(self.t(Text::CopyPath)).clicked() {
            ui.ctx().copy_text(file.display().to_string());
        }
        ui.separator();
        if ui.add_enabled(target.is_some(), egui::Button::new(self.t(Text::MenuPreviewSelection))).clicked() {
            self.live_preview(&item.asset, false);
        }
        if ui.add_enabled(target.is_some(), egui::Button::new(self.t(Text::MenuPreviewView))).clicked() {
            self.live_preview(&item.asset, true);
        }
        if ui.button(format!("{}{many}", self.t(Text::MenuPreview))).clicked() {
            self.ask_previews(&assets);
        }
        if ui.add_enabled(target.is_some(), egui::Button::new(self.t(Text::MenuRelink))).clicked() {
            self.ask_relink(&assets);
        }
        if ui.button(self.t(Text::MissingOpen)).clicked() {
            self.open_missing(&item.asset);
        }
        if ui.button(format!("{}{many}", self.t(Text::MenuData))).clicked() {
            self.read_stats(&assets, true);
        }
    }

    /// Opening the `.max` itself, in a new 3ds Max or in the connected one.
    fn open_menu(&mut self, ui: &mut egui::Ui, asset: &Asset, connected: bool) {
        let files: Vec<PathBuf> = asset.all_files().into_iter().map(PathBuf::from).collect();
        for (text, replace) in [(Text::OpenNewMax, false), (Text::OpenHere, true)] {
            if replace && !connected {
                continue;
            }
            match &files[..] {
                [] => {}
                [only] => {
                    if ui.button(self.t(text)).clicked() {
                        self.open_file(only, replace);
                    }
                }
                several => {
                    ui.menu_button(self.t(text), |ui| {
                        for file in several {
                            if ui.button(file_name(file)).clicked() {
                                self.open_file(file, replace);
                            }
                        }
                    });
                }
            }
        }
    }
}

pub(super) fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn whole() -> Rect {
    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

pub(crate) fn picture(painter: &egui::Painter, area: Rect, texture: &egui::TextureHandle) {
    let size = texture.size_vec2();
    let scale = (area.width() / size.x).min(area.height() / size.y);
    painter.image(texture.id(), Rect::from_center_size(area.center(), size * scale), whole(), Color32::WHITE);
}

/// The name over the foot of the image, on a shadow.
fn name_over(painter: &egui::Painter, image: Rect, name: &str) {
    let height = (image.height() * 0.42).min(64.0);
    let band = Rect::from_min_max(egui::pos2(image.left(), image.bottom() - height), image.max);
    let mut mesh = egui::Mesh::default();
    let (clear, dark) = (Color32::TRANSPARENT, Color32::from_black_alpha(205));
    for (at, color) in [(band.left_top(), clear), (band.right_top(), clear), (band.right_bottom(), dark), (band.left_bottom(), dark)] {
        mesh.colored_vertex(at, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    let at = band.left_bottom() + Vec2::new(9.0, -8.0);
    kit::elided(painter, at, egui::Align2::LEFT_BOTTOM, name, kit::strong(12.5), Color32::WHITE, band.width() - 46.0);
}

/// The heart of a card; `true` when it was clicked.
pub(crate) fn heart(ui: &egui::Ui, card: &egui::Response, rect: Rect, on: bool, on_image: bool) -> bool {
    let response = ui.interact(rect, card.id.with("heart"), Sense::click());
    let p = kit::pal(ui);
    let painter = ui.painter();
    if on_image {
        painter.circle_filled(rect.center(), 11.0, Color32::from_black_alpha(170));
    }
    let off = if on_image { Color32::from_gray(200) } else { p.faint };
    let color = if on || response.hovered() { p.accent } else { off };
    // two circles and the point below: the fonts have no heart
    let (at, r) = (rect.center(), 3.6);
    painter.circle_filled(at + Vec2::new(-r * 0.92, -r * 0.5), r, color);
    painter.circle_filled(at + Vec2::new(r * 0.92, -r * 0.5), r, color);
    let point = [Vec2::new(-r * 1.68, r * 0.12), Vec2::new(0.0, -r * 0.5), Vec2::new(r * 1.68, r * 0.12), Vec2::new(0.0, r * 1.9)];
    painter.add(egui::Shape::convex_polygon(point.iter().map(|p| at + *p).collect(), color, Stroke::NONE));
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cards_fill_the_row() {
        let grid = Grid::new(1000.0, 160.0, false);
        assert_eq!(grid.columns, 6);
        assert!(grid.cell.x * 6.0 <= 1000.0 && grid.cell.x * 6.0 > 990.0);
        assert_eq!(Grid::new(100.0, 400.0, true).columns, 1);
    }
}
