// SPDX-License-Identifier: GPL-3.0-or-later
//! The panel of the selected model: its preview, what to do with it, its versions and data, its external files,
//! and its proxy. The button of the status bar shows and hides it.

use super::frame::engine_name;
use super::gallery::{engine_color, file_name, picture};
use super::{Detail, Job, OpenLibraryApp, Shown};
use crate::kit::{self, Kind};
use crate::library::Asset;
use crate::max_session::MergeKind;
use crate::maxfile::Engine;
use crate::maxlink;
use crate::missing::Check;
use crate::modelstats;
use crate::task::Task;
use crate::texts::{Text, fill};
use eframe::egui::{self, Sense, TextureHandle, TextureOptions, Vec2};
use image::RgbaImage;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The longest side of the preview of the panel.
const BIG: u32 = 700;

/// The preview of the selected model, larger than its thumbnail.
pub(super) struct Big {
    path: PathBuf,
    task: Option<Task<Option<RgbaImage>>>,
    texture: Option<TextureHandle>,
}

/// 3.4 MB · 12/11/2026
fn size_and_date(size: u64, date: std::time::SystemTime) -> String {
    let days = date.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400);
    let (y, m, d) = civil(days);
    format!("{:.1} MB · {d:02}/{m:02}/{y}", size as f64 / 1_048_576.0)
}

/// The day of a number of days since 1970 (Howard Hinnant's algorithm).
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + u64::from(m <= 2), m, d)
}

impl OpenLibraryApp {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        let selection = self.models.selection();
        let [item] = &selection[..] else {
            let text = match selection.len() {
                0 => self.t(Text::SelectOne).to_string(),
                n => fill(self.t(Text::SelectedCount), &[("count", &n)]),
            };
            ui.weak(text);
            if selection.len() > 1 {
                ui.add_space(8.0);
                self.merge_buttons(ui);
            }
            return;
        };
        let item = item.clone();
        self.ask_detail(&item);
        egui::ScrollArea::vertical().id_salt("inspector").auto_shrink(false).show(ui, |ui| {
            self.preview_image(ui, &item);
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(&item.asset.name).font(kit::strong(16.0))).wrap());
            ui.add(egui::Label::new(egui::RichText::new(item.asset.folder.display().to_string()).small().weak()).wrap());
            ui.add_space(4.0);
            self.actions(ui, &item.asset);
            self.versions(ui, &item);
            self.externals(ui, &item.asset);
            self.proxy_and_preview(ui, &item);
        });
    }

    /// Asks the disk once for what the panel says of a model.
    fn ask_detail(&mut self, item: &Shown) {
        if !self.detail_asked.insert(item.key.clone()) {
            return;
        }
        let files: Vec<PathBuf> = item.asset.all_files().into_iter().map(Path::to_path_buf).collect();
        let _ = self.jobs.send(Job::Detail(item.key.clone(), files, self.proxy_folders(&item.asset)));
    }

    /// Where the proxy of a model may be, for the scene's renderer.
    fn proxy_folders(&self, asset: &Asset) -> super::FindProxies {
        let engine = self.scene_engine();
        let file = asset.file_for_engine(engine, |f| self.engines.peek(f).unwrap_or(Engine::Other)).or_else(|| asset.any_file());
        let Some(file) = file.filter(|_| matches!(engine, Engine::Corona | Engine::VRay)).map(Path::to_path_buf) else {
            return Box::new(Vec::new);
        };
        let import = self.settings.data.get("import");
        let get = |key: &str| import.and_then(|i| i.get(key)).and_then(Value::as_str).unwrap_or_default();
        let mode = Some(get("proxy_mode")).filter(|m| !m.is_empty()).unwrap_or("single").to_string();
        let custom = (get("proxy_location") == "custom").then(|| PathBuf::from(get("proxy_custom_dir")));
        let local = self.data_dir.join("proxies");
        let name = asset.name.clone();
        Box::new(move || [custom, Some(local)].into_iter().map(|c| maxlink::proxy_dir(&file, &name, engine, &mode, c.as_deref())).collect())
    }

    /// Forgets what the disk said of these files.
    pub(super) fn forget_detail(&mut self, files: &[PathBuf]) {
        self.detail_asked.clear();
        for file in files {
            self.checks.forget(file);
            self.asked.remove(file);
        }
    }

    fn preview_image(&mut self, ui: &mut egui::Ui, item: &Shown) {
        let p = kit::pal(ui);
        let side = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
        ui.painter().rect_filled(rect, 0.0, self.edges.get(&item.key).copied().unwrap_or(p.surface));
        let large = item.asset.preview.as_ref().and_then(|path| self.large(ui.ctx(), path));
        match large.as_ref().or(self.textures.get(&item.key).and_then(Option::as_ref)) {
            Some(texture) => picture(ui.painter(), rect, texture),
            None => {
                let text = self.t(if self.textures.contains_key(&item.key) { Text::NoPreview } else { Text::Loading });
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), p.faint);
            }
        }
        if response.on_hover_text(self.t(Text::LookHint)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.quick_look();
        }
    }

    /// The larger preview, when it is here.
    fn large(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
        if self.big.as_ref().is_none_or(|b| b.path != path) {
            let src = path.to_path_buf();
            let task = Task::start(self.wake.clone(), move |link| link.send(crate::thumbs::load_large(&src, BIG)));
            // the one before is let go on a thread: its image may still be on its way from the server
            if let Some(old) = self.big.replace(Big { path: path.to_path_buf(), task: Some(task), texture: None }) {
                std::thread::spawn(move || drop(old.task));
            }
        }
        let big = self.big.as_mut()?;
        if let Some(image) = big.task.as_ref().and_then(|t| t.poll().pop()) {
            big.task = None;
            big.texture = image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture("inspector", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
        }
        big.texture.clone()
    }

    fn actions(&mut self, ui: &mut egui::Ui, asset: &Asset) {
        let width = ui.available_width();
        let files = !asset.all_files().is_empty();
        ui.add_enabled_ui(files, |ui| {
            if kit::sized_button(ui, Kind::Primary, None, self.t(Text::OpenInMax), width).on_hover_text(self.t(Text::OpenInMaxTip)).clicked() {
                self.open_asset(asset);
            }
        });
        self.merge_buttons(ui);
        let hint = match self.max.target() {
            None => Some(Text::HintNoMax),
            Some(t) if !matches!(t.engine, Engine::Corona | Engine::VRay) => Some(Text::HintProxyEngine),
            _ => None,
        };
        if let Some(hint) = hint {
            ui.add(egui::Label::new(egui::RichText::new(self.t(hint)).small().weak()).wrap());
        }
    }

    /// Place, proxy, original and Explorer, for what is selected.
    fn merge_buttons(&mut self, ui: &mut egui::Ui) {
        let target = self.max.target();
        let proxy = target.as_ref().is_some_and(|t| matches!(t.engine, Engine::Corona | Engine::VRay));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let width = ((ui.available_width() - 28.0 - 18.0) / 3.0).max(60.0);
            let mut merge = None;
            for (text, tip, kind, enabled) in [
                (Text::Place, Text::PlaceTip, MergeKind::Place, target.is_some()),
                (Text::Proxy, Text::ProxyTip, MergeKind::Proxy, proxy),
                (Text::Original, Text::OriginalTip, MergeKind::Origin, target.is_some()),
            ] {
                ui.add_enabled_ui(enabled, |ui| {
                    if kit::sized_button(ui, Kind::Action, None, self.t(text), width).on_hover_text(self.t(tip)).clicked() {
                        merge = Some(kind);
                    }
                });
            }
            if kit::icon_button(ui, "folder", self.t(Text::FolderReveal), false).clicked()
                && let Some(asset) = self.models.selected_assets().first()
            {
                crate::platform::reveal(asset.any_file().unwrap_or(&asset.folder));
            }
            if let Some(kind) = merge {
                self.begin_merge(self.models.selected_assets(), kind);
            }
        });
    }

    fn versions(&mut self, ui: &mut egui::Ui, item: &Shown) {
        let p = kit::pal(ui);
        ui.add_space(8.0);
        kit::caption(ui, self.t(Text::Versions));
        let scene = self.scene_engine();
        let detail: Detail = self.details.get(&item.key).cloned().unwrap_or_default();
        let files: Vec<PathBuf> = item.asset.all_files().into_iter().map(Path::to_path_buf).collect();
        if files.is_empty() {
            ui.weak(self.t(Text::NoMaxFile));
        }
        let mut unread = false;
        for file in &files {
            let engine = item.asset.versions.iter().find(|(_, f)| *f == file).map(|(e, _)| *e).or_else(|| self.engines.peek(file));
            ui.horizontal_wrapped(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, engine_color(engine));
                ui.label(engine.map_or(self.t(Text::EnginePending), |e| engine_name(e, self.t(Text::EngineOther))));
                if engine == Some(scene) && scene != Engine::Other {
                    ui.colored_label(p.ok, self.t(Text::SceneEngine));
                }
            });
            ui.add(egui::Label::new(egui::RichText::new(file_name(file)).small().weak()).wrap());
            match detail.files.get(file) {
                Some(Some((size, date))) => {
                    ui.small(size_and_date(*size, *date));
                }
                Some(None) => {
                    ui.colored_label(p.warn, self.t(Text::Unreachable));
                }
                None => {}
            }
            unread |= !self.stats_lines(ui, file);
            ui.add_space(4.0);
        }
        if !files.is_empty() && self.work.is_none() {
            let text = self.t(if unread { Text::StatsRead } else { Text::StatsReadAgain });
            let width = ui.available_width();
            if kit::sized_button(ui, Kind::Action, None, text, width).on_hover_text(self.t(Text::StatsTip)).clicked() {
                self.read_stats(std::slice::from_ref(&item.asset), true);
            }
        }
    }

    /// Faces, vertices, displacement and animation of a file; `false` when they are not read.
    fn stats_lines(&self, ui: &mut egui::Ui, file: &Path) -> bool {
        let p = kit::pal(ui);
        match self.stats.peek(file) {
            None => {
                let text = if self.reading_stats() { Text::StatsReading } else { Text::StatsUnread };
                ui.label(egui::RichText::new(self.t(text)).small().color(p.faint));
                false
            }
            Some(None) => {
                ui.label(egui::RichText::new(self.t(Text::StatsUnreadable)).small().color(p.warn));
                true
            }
            Some(Some(stats)) => {
                let (polys, verts) = (modelstats::thousands(stats.polys), modelstats::thousands(stats.verts));
                let values: [(&str, &dyn std::fmt::Display); 3] = [("polys", &polys), ("verts", &verts), ("objects", &stats.objects)];
                ui.small(fill(self.t(Text::StatsLine), &values));
                let mut tags = Vec::new();
                if stats.disp {
                    tags.push(self.t(Text::StatsDisplacement).to_string());
                }
                if stats.anim {
                    tags.push(fill(self.t(Text::StatsAnimation), &[("start", &stats.start), ("end", &stats.end)]));
                }
                if tags.is_empty() {
                    ui.label(egui::RichText::new(self.t(Text::StatsPlain)).small().weak());
                } else {
                    ui.label(egui::RichText::new(tags.join(" · ")).small().color(p.accent));
                }
                true
            }
        }
    }

    fn externals(&mut self, ui: &mut egui::Ui, asset: &Asset) {
        let p = kit::pal(ui);
        ui.add_space(6.0);
        kit::caption(ui, self.t(Text::ExternalFiles));
        let checks: Vec<Check> = asset.all_files().iter().filter_map(|f| self.checks.peek(f)).collect();
        if checks.is_empty() {
            ui.weak(self.t(Text::Checking));
            return;
        }
        let total: usize = checks.iter().map(|c| c.total).sum();
        let nearby: usize = checks.iter().map(|c| c.nearby).sum();
        let mut names: Vec<&String> = checks.iter().flat_map(|c| &c.missing).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        if names.is_empty() && nearby == 0 {
            let text = if total == 0 { self.t(Text::FilesNone).to_string() } else { fill(self.t(Text::FilesOk), &[("count", &total)]) };
            ui.label(text);
            return;
        }
        let lost: usize = checks.iter().map(|c| c.missing.len()).sum::<usize>() + nearby;
        ui.colored_label(p.warn, fill(self.t(Text::FilesMissing), &[("missing", &lost), ("count", &total)]));
        if nearby > 0 {
            ui.small(fill(self.t(Text::FilesNearby), &[("count", &nearby)]));
        }
        if !names.is_empty() {
            ui.label(egui::RichText::new(fill(self.t(Text::FilesNotFound), &[("count", &names.len())])).small().weak());
            for name in names.iter().take(60) {
                ui.add(egui::Label::new(egui::RichText::new(format!("· {name}")).small().weak()).wrap());
            }
            if names.len() > 60 {
                ui.label(egui::RichText::new(fill(self.t(Text::FilesMore), &[("count", &(names.len() - 60))])).small().weak());
            }
        }
        let width = ui.available_width();
        if kit::sized_button(ui, Kind::Action, None, self.t(Text::MissingOpen), width).on_hover_text(self.t(Text::MissingTip)).clicked() {
            self.open_missing(asset);
        }
    }

    fn proxy_and_preview(&mut self, ui: &mut egui::Ui, item: &Shown) {
        ui.add_space(6.0);
        kit::caption(ui, "Proxy");
        let ready = self.details.get(&item.key).is_some_and(|d| d.proxy_ready);
        let text = self.t(if ready { Text::ProxyReady } else { Text::ProxyNotYet });
        ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
        if item.asset.preview.is_none() {
            ui.add_space(6.0);
            kit::caption(ui, "Preview");
            let captured = matches!(self.textures.get(&item.key), Some(Some(_)));
            let text = self.t(if captured { Text::ViewportCapture } else { Text::PreviewNone });
            ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_become_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_781), (2026, 11, 24));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }
}
