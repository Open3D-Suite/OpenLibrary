// SPDX-License-Identifier: GPL-3.0-or-later
//! Bringing models into 3ds Max: the menu where they are dropped, the question when the scene has the model already,
//! that of which version when there is none of the scene's renderer, and the models one after another.

use super::OpenLibraryApp;
use super::frame::engine_name;
use super::gallery::file_name;
use crate::kit::{self, Kind};
use crate::library::Asset;
use crate::max_session::{self, MaxReply, MergeKind, MergeWhat, Prepared};
use crate::maxfile::Engine;
use crate::maxlink::{MaxError, MaxInfo};
use crate::task::Task;
use crate::texts::{Text, fill};
use eframe::egui::{self, Vec2};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

/// How many objects of a model the scene has.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Here {
    proxy: u64,
    model: u64,
}

impl Here {
    fn any(self) -> bool {
        self.proxy > 0 || self.model > 0
    }

    fn from(result: &Value) -> Self {
        let get = |key: &str| result.get(key).and_then(Value::as_u64).unwrap_or(0);
        Self { proxy: get("proxy"), model: get("model") }
    }
}

/// The menu of models dropped on a 3ds Max.
pub(super) struct DropMenu {
    assets: Vec<Arc<Asset>>,
    info: MaxInfo,
    at: egui::Pos2,
    check: Option<MaxReply>,
    since: Instant,
    here: Here,
    /// It has had the focus: losing it closes the menu.
    focused: bool,
}

/// What was chosen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Merge(MergeKind),
    /// An instance of what the scene has: "proxy" or "model".
    Instance(&'static str),
}

/// A question before a merge, in the window.
pub(super) enum Question {
    /// Whether the scene has it is being asked.
    Checking {
        assets: Vec<Arc<Asset>>,
        kind: MergeKind,
        info: MaxInfo,
        reply: MaxReply,
    },
    Exists {
        assets: Vec<Arc<Asset>>,
        kind: MergeKind,
        info: MaxInfo,
        here: Here,
    },
    Version {
        files: Vec<(Engine, PathBuf)>,
    },
}

/// Models on their way into a 3ds Max, one after another.
pub(super) struct Run {
    queue: VecDeque<Arc<Asset>>,
    total: usize,
    kind: MergeKind,
    info: MaxInfo,
    /// The scene has it: 3ds Max asks about names and materials.
    again: bool,
    current: Option<Arc<Asset>>,
    chosen: Option<PathBuf>,
    preparing: Option<Task<Prepared>>,
    reply: Option<MaxReply>,
    used: Engine,
    instance: bool,
}

/// The mark of a model in the scene: letters and digits, the same for everyone.
fn asset_id(key: &str) -> String {
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("a{hash:016x}")
}

impl OpenLibraryApp {
    fn id_of(&self, asset: &Asset) -> String {
        asset_id(&self.favorite_key(&asset.key()))
    }

    /// Asks the scene whether it has a model; `None` when the bridge is too old to say.
    fn ask_here(&mut self, asset: &Asset, info: &MaxInfo) -> Option<MaxReply> {
        if info.protocol < 3 || info.busy {
            return None;
        }
        let args = json!({"asset_id": self.id_of(asset), "asset_name": asset.name});
        self.max.request(info, "check_asset", args, self.wake.clone())
    }

    // ---- dragging -----------------------------------------------------------------------------------------

    /// While cards are dragged: the thumbnail under the pointer; on release over a 3ds Max, the menu.
    pub(super) fn drag(&mut self, ctx: &egui::Context) {
        if !self.dragging {
            return;
        }
        let (released, pointer, modifiers) = ctx.input(|i| (i.pointer.any_released(), i.pointer.latest_pos(), i.modifiers));
        let assets = self.models.selected_assets();
        if !released {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            let texture = self.models.selected.iter().next().and_then(|k| self.textures.get(k)).and_then(Option::as_ref);
            if let (Some(pos), Some(texture)) = (pointer, texture) {
                let size = texture.size_vec2() * (96.0 / texture.size_vec2().max_elem());
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("openlibrary-drag")));
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                let rect = egui::Rect::from_min_size(pos + Vec2::splat(12.0), size);
                painter.image(texture.id(), rect, uv, egui::Color32::from_white_alpha(200));
                if assets.len() > 1 {
                    let p = kit::palette(ctx.global_style().visuals.dark_mode);
                    painter.circle_filled(rect.right_top(), 11.0, p.accent);
                    let font = kit::mono_bold(12.0);
                    painter.text(rect.right_top(), egui::Align2::CENTER_CENTER, assets.len().to_string(), font, p.accent_text);
                }
            }
            return;
        }
        self.dragging = false;
        if assets.is_empty() || crate::platform::process_under_cursor() == Some(std::process::id()) {
            return; // dropped back on OpenLibrary
        }
        let Some(info) = self.max.under_cursor() else {
            let text = if self.max.maxes().is_empty() { Text::NoMax } else { Text::DropOnMax };
            self.notice = Some(self.t(text).to_string());
            return;
        };
        self.max.last_pid = Some(info.pid);
        // the keys skip the menu
        if modifiers.shift && matches!(info.engine, Engine::Corona | Engine::VRay) {
            self.begin_merge_in(assets, MergeKind::Proxy, info);
        } else if modifiers.ctrl {
            self.begin_merge_in(assets, MergeKind::Origin, info);
        } else {
            self.open_drop_menu(Some(info));
        }
    }

    /// The menu to choose how the selected models come in, where the mouse is.
    pub(super) fn open_drop_menu(&mut self, info: Option<MaxInfo>) {
        let assets = self.models.selected_assets();
        let Some(first) = assets.first().cloned() else { return };
        let Some(info) = info.or_else(|| self.max.target()) else {
            self.notice = Some(self.t(Text::NoMax).to_string());
            return;
        };
        let check = if assets.len() == 1 { self.ask_here(&first, &info) } else { None };
        let at = crate::platform::cursor_pos().unwrap_or((200, 200));
        let at = egui::pos2(at.0 as f32, at.1 as f32);
        self.drop = Some(DropMenu { assets, info, at, check, since: Instant::now(), here: Here::default(), focused: false });
    }

    /// Draws the menu, in a small window of its own: the mouse is over 3ds Max.
    pub(super) fn drop_menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = &mut self.drop else { return };
        if let Some(reply) = &menu.check {
            match reply.try_recv() {
                Err(TryRecvError::Empty) => {}
                Ok(Ok(result)) => {
                    menu.here = Here::from(&result);
                    menu.check = None;
                }
                _ => menu.check = None,
            }
            if menu.since.elapsed() > Duration::from_millis(1500) {
                menu.check = None; // 3ds Max is slow to say: the choices cannot wait
            }
        }
        let (lang, here, checking) = (self.lang, menu.here, menu.check.is_some());
        let t = |text: Text| text.get(lang);
        let proxy_ok = matches!(menu.info.engine, Engine::Corona | Engine::VRay);
        let title = match &menu.assets[..] {
            [only] => only.name.clone(),
            several => fill(t(Text::Models), &[("count", &several.len())]),
        };
        let mut rows: Vec<(String, bool, Option<Choice>)> = Vec::new();
        if here.proxy > 0 {
            rows.push((t(Text::InstanceProxy).to_string(), true, Some(Choice::Instance("proxy"))));
        }
        if here.model > 0 {
            rows.push((t(Text::InstanceModel).to_string(), true, Some(Choice::Instance("model"))));
        }
        let again = if here.any() { format!(" · {}", t(Text::Again)) } else { String::new() };
        rows.push((format!("{}{again}", t(Text::MenuPlace)), true, Some(Choice::Merge(MergeKind::Place))));
        let proxy = if proxy_ok { Text::MenuProxy } else { Text::ProxyNeedsEngine };
        rows.push((format!("{}{again}", t(proxy)), proxy_ok, Some(Choice::Merge(MergeKind::Proxy))));
        rows.push((format!("{}{again}", t(Text::MenuOrigin)), true, Some(Choice::Merge(MergeKind::Origin))));
        let note = if here.any() {
            t(Text::HereAlready)
        } else if checking {
            t(Text::HereAsking)
        } else {
            t(Text::HereNot)
        };
        // until 3ds Max says whether it has the model nothing can be clicked: the rows may move
        if checking {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let height = 78.0 + 34.0 * 5.0;
        let scale = ctx.native_pixels_per_point().unwrap_or(1.0);
        let builder = egui::ViewportBuilder::default()
            .with_title("OpenLibrary - menu")
            .with_inner_size([330.0, height])
            .with_position(egui::pos2(menu.at.x / scale, menu.at.y / scale))
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_always_on_top()
            .with_active(true);
        let mut choice = None;
        let mut close = false;
        let mut focused = menu.focused;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("drop-menu"), builder, |ui, _| {
            let p = kit::pal(ui);
            let frame = egui::Frame::new().fill(p.surface).stroke(egui::Stroke::new(1.0, p.border_hi)).inner_margin(egui::Margin::same(10));
            egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), egui::Sense::hover());
                kit::elided(ui.painter(), rect.left_center(), egui::Align2::LEFT_CENTER, &title, kit::strong(14.0), p.text, rect.width());
                ui.label(egui::RichText::new(note).small().weak());
                for (text, enabled, what) in &rows {
                    let button = egui::Button::new(egui::RichText::new(text)).min_size(Vec2::new(ui.available_width(), 30.0));
                    if ui.add_enabled(*enabled && !checking, button).clicked() {
                        choice = *what;
                    }
                }
            });
            let (has, escape, closing) = ui.ctx().input(|i| (i.focused, i.key_pressed(egui::Key::Escape), i.viewport().close_requested()));
            close = escape || closing || (focused && !has);
            focused |= has;
        });
        menu.focused = focused;
        if let Some(choice) = choice {
            let Some(menu) = self.drop.take() else { return };
            match choice {
                Choice::Merge(kind) => self.run_merge(menu.assets, kind, menu.info, here.any()),
                Choice::Instance(kind) => self.run_instance(menu.assets, kind, menu.info),
            }
        } else if close {
            self.drop = None;
        }
    }

    // ---- starting -----------------------------------------------------------------------------------------

    /// Brings models into the 3ds Max they go to; asks first when the scene has the model.
    pub(super) fn begin_merge(&mut self, assets: Vec<Arc<Asset>>, kind: MergeKind) {
        match self.max.target() {
            Some(info) => self.begin_merge_in(assets, kind, info),
            None => self.notice = Some(self.t(Text::NoMax).to_string()),
        }
    }

    fn begin_merge_in(&mut self, assets: Vec<Arc<Asset>>, kind: MergeKind, info: MaxInfo) {
        if info.busy || self.merge.is_some() {
            self.notice = Some(self.t(Text::Busy).to_string());
            return;
        }
        if let [only] = &assets[..] {
            let only = only.clone();
            if let Some(reply) = self.ask_here(&only, &info) {
                self.merge_ask = Some(Question::Checking { assets, kind, info, reply });
                return;
            }
        }
        self.run_merge(assets, kind, info, false);
    }

    fn run_merge(&mut self, assets: Vec<Arc<Asset>>, kind: MergeKind, info: MaxInfo, again: bool) {
        if self.merge.is_some() {
            self.notice = Some(self.t(Text::Busy).to_string());
            return;
        }
        let total = assets.len();
        self.merge = Some(Run {
            queue: assets.into(),
            total,
            kind,
            info,
            again,
            current: None,
            chosen: None,
            preparing: None,
            reply: None,
            used: Engine::Other,
            instance: false,
        });
        self.next_merge();
    }

    fn run_instance(&mut self, assets: Vec<Arc<Asset>>, kind: &str, info: MaxInfo) {
        let Some(asset) = assets.first().cloned() else { return };
        if self.merge.is_some() {
            self.notice = Some(self.t(Text::Busy).to_string());
            return;
        }
        let imp = self.settings.data.get("import").cloned().unwrap_or_default();
        let args = json!({"asset_id": self.id_of(&asset), "asset_name": asset.name, "kind": kind, "imp": imp});
        let reply = self.max.request_long(&info, "instance_asset", args, self.wake.clone());
        self.notice = Some(fill(self.t(Text::Placing), &[("name", &asset.name)]));
        self.merge = Some(Run {
            queue: VecDeque::new(),
            total: 1,
            kind: MergeKind::Place,
            used: info.engine,
            info,
            again: false,
            current: Some(asset),
            chosen: None,
            preparing: None,
            reply,
            instance: true,
        });
    }

    // ---- one after another --------------------------------------------------------------------------------

    /// Starts preparing the next model: its file and folders are looked for on a thread.
    fn next_merge(&mut self) {
        let Some(run) = &mut self.merge else { return };
        let Some(asset) = run.current.clone().or_else(|| run.queue.pop_front()) else {
            self.merge = None;
            return;
        };
        run.current = Some(asset.clone());
        let mut imp = self.settings.data.get("import").cloned().unwrap_or_default();
        if run.again {
            imp["object_dups"] = "ask".into();
            imp["material_dups"] = "ask".into();
        }
        let (info, kind, chosen) = (run.info.clone(), run.kind, run.chosen.take());
        let id = if info.protocol >= 3 { self.id_of(&asset) } else { String::new() };
        let roots = self.models.index.as_ref().map(|i| i.roots.clone()).unwrap_or_default();
        let (engines, local) = (self.engines.clone(), self.data_dir.join("proxies"));
        let task = Task::start(self.wake.clone(), move |link| {
            let what = MergeWhat { asset: &asset, kind, chosen: chosen.as_deref(), id: &id };
            link.send(max_session::prepare(&what, &info, &imp, &roots, &engines, &local));
        });
        if let Some(run) = &mut self.merge {
            run.preparing = Some(task);
        }
    }

    pub(super) fn poll_merge(&mut self) {
        self.poll_question();
        let Some(run) = &mut self.merge else { return };
        if let Some(prepared) = run.preparing.as_ref().and_then(|t| t.poll().pop()) {
            run.preparing = None;
            self.prepared(prepared);
            return;
        }
        let Some(reply) = &run.reply else { return };
        let result = match reply.try_recv() {
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(MaxError::NotThere),
            Ok(result) => result,
        };
        run.reply = None;
        let name = run.current.take().map(|a| a.name.clone()).unwrap_or_default();
        let step = run.step();
        let (info, used, instance) = (run.info.clone(), run.used, run.instance);
        match result {
            Ok(count) => {
                let max = format!("3ds Max {}", info.year);
                let mut text = fill(self.t(Text::Merged), &[("name", &name), ("count", &count.as_u64().unwrap_or(0)), ("max", &max)]);
                if !instance && used != info.engine && info.engine != Engine::Other {
                    let other = self.t(Text::EngineOther);
                    let values: [(&str, &dyn std::fmt::Display); 3] =
                        [("name", &name), ("engine", &engine_name(info.engine, other)), ("used", &engine_name(used, other))];
                    text = format!("{text} · {}", fill(self.t(Text::OtherEngine), &values));
                }
                self.notice = Some(format!("{step}{text}"));
                self.detail_asked.clear(); // its proxy may be made now
                self.next_merge();
            }
            Err(error) => {
                self.notice = Some(match error {
                    MaxError::Busy => self.t(Text::Busy).to_string(),
                    error => fill(self.t(Text::MergeFailed), &[("name", &name), ("error", &error)]),
                });
                self.merge = None;
            }
        }
    }

    fn prepared(&mut self, prepared: Prepared) {
        let Some(run) = &mut self.merge else { return };
        let name = run.current.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        match prepared {
            Prepared::NoFile => {
                self.merge = None;
                self.notice = Some(fill(self.t(Text::NoFile), &[("name", &name)]));
            }
            Prepared::Choose(files) => self.merge_ask = Some(Question::Version { files }),
            Prepared::Ready { request, moved } => {
                run.used = request.used;
                let (info, step, kind) = (run.info.clone(), run.step(), run.kind);
                let reply = self.max.request_long(&info, request.cmd, request.args, self.wake.clone());
                let text = if kind == MergeKind::Origin { Text::Sending } else { Text::Placing };
                let mut notice = format!("{step}{}", fill(self.t(text), &[("name", &name)]));
                if let Some(folder) = moved {
                    notice = format!("{notice} · {}", fill(self.t(Text::ProxyLocal), &[("folder", &folder.display())]));
                }
                self.notice = Some(notice);
                match (&mut self.merge, reply) {
                    (Some(run), Some(reply)) => run.reply = Some(reply),
                    _ => self.merge = None,
                }
            }
        }
    }

    fn poll_question(&mut self) {
        let Some(Question::Checking { reply, .. }) = &self.merge_ask else { return };
        let here = match reply.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(Ok(result)) => Here::from(&result),
            _ => Here::default(), // it cannot say: it comes in as always
        };
        let Some(Question::Checking { assets, kind, info, .. }) = self.merge_ask.take() else { return };
        if here.any() {
            self.merge_ask = Some(Question::Exists { assets, kind, info, here });
        } else {
            self.run_merge(assets, kind, info, false);
        }
    }

    pub(super) fn merge_question(&mut self, ctx: &egui::Context) {
        match &self.merge_ask {
            Some(Question::Exists { .. }) => self.exists_question(ctx),
            Some(Question::Version { .. }) => self.version_question(ctx),
            _ => {}
        }
    }

    fn exists_question(&mut self, ctx: &egui::Context) {
        let Some(Question::Exists { assets, here, .. }) = &self.merge_ask else { return };
        let name = assets.first().map(|a| a.name.clone()).unwrap_or_default();
        let here = *here;
        let mut choice: Option<Option<Option<&'static str>>> = None;
        egui::Modal::new(egui::Id::unique("openlibrary-exists")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(fill(self.t(Text::ExistsAsk), &[("name", &name)]));
            ui.add_space(6.0);
            let width = ui.available_width();
            if here.proxy > 0 && kit::sized_button(ui, Kind::Primary, None, self.t(Text::InstanceProxy), width).clicked() {
                choice = Some(Some(Some("proxy")));
            }
            if here.model > 0 && kit::sized_button(ui, Kind::Primary, None, self.t(Text::InstanceModel), width).clicked() {
                choice = Some(Some(Some("model")));
            }
            if kit::sized_button(ui, Kind::Action, None, self.t(Text::BringAgain), width).clicked() {
                choice = Some(Some(None));
            }
            if kit::sized_button(ui, Kind::Action, None, self.t(Text::Cancel), width).clicked() {
                choice = Some(None);
            }
        });
        let Some(choice) = choice else { return };
        let Some(Question::Exists { assets, kind, info, .. }) = self.merge_ask.take() else { return };
        match choice {
            Some(Some(what)) => self.run_instance(assets, what, info),
            Some(None) => self.run_merge(assets, kind, info, true),
            None => {}
        }
    }

    fn version_question(&mut self, ctx: &egui::Context) {
        let Some(Question::Version { files }) = &self.merge_ask else { return };
        let files = files.clone();
        let (name, engine) = match &self.merge {
            Some(run) => (run.current.as_ref().map(|a| a.name.clone()).unwrap_or_default(), run.info.engine),
            None => return,
        };
        let other = self.t(Text::EngineOther);
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("openlibrary-version")).show(ctx, |ui| {
            ui.set_width(440.0);
            let values: [(&str, &dyn std::fmt::Display); 2] = [("name", &name), ("engine", &engine_name(engine, other))];
            ui.label(fill(self.t(Text::VersionAsk), &values));
            ui.add_space(6.0);
            for (engine, file) in &files {
                let text = format!("{}  ·  {}", engine_name(*engine, other), file_name(file));
                if ui.add(egui::Button::new(text).min_size(Vec2::new(ui.available_width(), 28.0))).clicked() {
                    choice = Some(Some(file.clone()));
                }
            }
            ui.add_space(4.0);
            if kit::button(ui, Kind::Action, None, self.t(Text::Cancel)).clicked() {
                choice = Some(None);
            }
        });
        match choice {
            None => {}
            Some(None) => (self.merge_ask, self.merge) = (None, None),
            Some(Some(file)) => {
                self.merge_ask = None;
                if let Some(run) = &mut self.merge {
                    run.chosen = Some(file);
                }
                self.next_merge();
            }
        }
    }

    // ---- opening the file itself --------------------------------------------------------------------------

    /// Opens the `.max` of a model to edit it: that of the scene's renderer, or the only one.
    pub(super) fn open_asset(&mut self, asset: &Asset) {
        let engine = self.scene_engine();
        let of_scene = asset.file_for_engine(engine, |f| self.engines.peek(f).unwrap_or(Engine::Other));
        let file = of_scene.or_else(|| asset.any_file()).map(Path::to_path_buf);
        match file {
            Some(file) => self.open_file(&file, false),
            None => self.notice = Some(fill(self.t(Text::NoFile), &[("name", &asset.name)])),
        }
    }

    /// Opens a `.max` in a new 3ds Max, or in the connected one in the place of its scene.
    pub(super) fn open_file(&mut self, file: &Path, replace: bool) {
        let target = self.max.target();
        if replace && let Some(info) = target {
            if info.busy {
                self.notice = Some(self.t(Text::Busy).to_string());
                return;
            }
            self.notice = Some(self.t(Text::Opening).to_string());
            let args = json!({"max_path": file.display().to_string()});
            if let Some(reply) = self.max.request_long(&info, "open_file", args, self.wake.clone()) {
                self.live = Some(super::live::Live::Open { name: file_name(file), reply });
            }
            return;
        }
        let name = file_name(file);
        let Some(exe) = crate::maxbatch::find_max(target.map(|t| t.year)) else {
            self.notice = Some(self.t(Text::NoMaxBatch).to_string());
            return;
        };
        self.notice = Some(match std::process::Command::new(exe).arg(file).spawn() {
            Ok(_) => fill(self.t(Text::OpeningNew), &[("name", &name)]),
            Err(error) => fill(self.t(Text::CatalogOpenFailed), &[("error", &error)]),
        });
    }

    /// Anything of the app is on its way in 3ds Max.
    pub(super) fn busy(&self) -> bool {
        self.merge.is_some() || self.live.is_some() || matches!(self.merge_ask, Some(Question::Checking { .. }))
    }
}

impl Run {
    /// "[2/5] " when there are several.
    fn step(&self) -> String {
        if self.total > 1 { format!("[{}/{}] ", self.total - self.queue.len(), self.total) } else { String::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_of_a_model_is_letters_and_digits() {
        let id = asset_id("01_3d models/chairs/oak|model");
        assert!(id.len() == 17 && id.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_ne!(id, asset_id("01_3d models/chairs/pine|model"));
    }

    #[test]
    fn what_the_scene_has_is_read() {
        let here = Here::from(&json!({"proxy": 1, "model": 0}));
        assert!(here.any() && here.proxy == 1 && here.model == 0);
        assert!(!Here::from(&json!({})).any());
    }
}
