// SPDX-License-Identifier: GPL-3.0-or-later
//! The window: the tabs and the search box above, the folder tree on the left, the gallery in the middle and
//! the selected model on the right. What it shows comes from the modules of the library; this file only draws
//! it and passes the clicks on.

mod live;
mod work;

use crate::favorites::{self, TabView};
use crate::foldertree::{self, SideTree};
use crate::library::{Asset, LibraryIndex, hidden_folders, norm_key};
use crate::max_session::{MaxSession, MergeKind, merge_request};
use crate::maxfile::{Engine, EngineCache};
use crate::maxlink::MaxInfo;
use crate::missing;
use crate::modelstats::StatsCache;
use crate::open3d_settings::{self, Language};
use crate::quicklook::QuickLook;
use crate::settings::workspace::{self, WsStamp, WsState};
use crate::settings::{self, Settings};
use crate::task::Task;
use crate::texts::{Text, fill};
use crate::thumbs::ThumbLoader;
use crate::ui_catalog::CatalogWindow;
use crate::ui_missing::MissingWindow;
use crate::ui_settings::{Page, SettingsWindow};
use crate::ui_surfaces::{SurfacePanel, Tools};
use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

const ACCENT: Color32 = Color32::from_rgb(0xF0, 0xA9, 0x3B);
const CORONA: Color32 = Color32::from_rgb(0xE8, 0x6A, 0x1C);
const VRAY: Color32 = Color32::from_rgb(0x2F, 0x7F, 0xD8);
const FSTORM: Color32 = Color32::from_rgb(0x8A, 0x5C, 0xD6);
const FOOTER: f32 = 30.0;
const GAP: f32 = 8.0;
/// The years of 3ds Max where the person chose not to be asked about the bridge again.
const BRIDGE_SKIP: &str = "olib_bridge_skip";

/// What a scan thread sends: how far it has got, or the index as far as it knows it and whether it has ended.
enum Snapshot {
    Progress(usize, usize),
    Index { index: LibraryIndex, done: bool },
}

/// What the background reader of `.max` files works on, and what it sends back.
enum Job {
    Engine(PathBuf),
    Files(PathBuf),
}

enum Found {
    Engine,
    Files(PathBuf, usize, usize),
}

/// The files of one `.max` that are not where it says: (all of them, missing).
fn count_missing(max: &Path) -> (usize, usize) {
    let externals = missing::externals(max);
    let missing = externals.iter().filter(|e| e.state != missing::State::Found).count();
    (externals.len(), missing)
}

pub struct OpenLibraryApp {
    lang: Language,
    settings: Settings,
    notice: Option<String>,
    data_dir: Option<PathBuf>,
    tab: usize,
    index: Option<LibraryIndex>,
    scan: Option<(Receiver<Snapshot>, Arc<AtomicBool>)>,
    scanning: bool,
    progress: Option<(usize, usize)>,
    side: SideTree,
    favorite_count: usize,
    all_count: usize,
    hidden: HashSet<String>,
    search: String,
    shown: Vec<Asset>,
    shown_dirty: bool,
    selected: Option<String>,
    textures: HashMap<String, Option<TextureHandle>>,
    loader: ThumbLoader,
    engines: Arc<EngineCache>,
    jobs: Sender<Job>,
    found: Receiver<Found>,
    asked: HashSet<PathBuf>,
    files: HashMap<PathBuf, (usize, usize)>,
    max: MaxSession,
    wake: Arc<dyn Fn() + Send + Sync>,
    dragging: Option<Asset>,
    stats: StatsCache,
    work: Option<work::Work>,
    preview_ask: Option<Vec<work::PreviewJob>>,
    missing: Option<MissingWindow>,
    catalog: Option<CatalogWindow>,
    surfaces: Option<SurfacePanel>,
    live: Option<live::Live>,
    relink_ask: Option<Vec<(PathBuf, Asset)>>,
    look: Option<QuickLook>,
    settings_window: Option<SettingsWindow>,
    /// Looks at the workspace's file every few seconds, on a thread: the server may be slow.
    ws_watch: Option<Task<WsStamp>>,
}

impl OpenLibraryApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        style(&cc.egui_ctx);
        let lang = open3d_settings::language(open3d_settings::settings_dir().as_deref());
        let settings_dir = open3d_settings::app_settings_dir(crate::APP);
        let data_dir = open3d_settings::app_data_dir(crate::APP);
        let mut notice = None;
        // The Python version kept its settings in %LOCALAPPDATA%\OpenLibrary: bring them over once.
        if let (Some(new), Some(local)) = (&settings_dir, std::env::var_os("LOCALAPPDATA")) {
            match settings::carry_over_old_settings(&PathBuf::from(local).join(crate::APP), new) {
                Ok(true) => notice = Some(Text::CarriedOver.get(lang).to_string()),
                Ok(false) => {}
                Err(error) => notice = Some(fill(Text::SettingsError.get(lang), &[("error", &error)])),
            }
        }
        let settings_path = settings_dir.unwrap_or_default().join(settings::FILE_NAME);
        let settings = Settings::load(&settings_path);
        let ctx = cc.egui_ctx.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let thumbs_dir = data_dir.clone().unwrap_or_else(std::env::temp_dir).join("thumbs");
        let loader = ThumbLoader::new(thumbs_dir, 6, wake.clone());
        let engines = Arc::new(EngineCache::load(&data_dir.clone().unwrap_or_else(std::env::temp_dir).join("engines.json")));
        let (jobs, found) = spawn_reader(engines.clone(), wake.clone());
        let skip: Vec<u32> = settings
            .data
            .get(BRIDGE_SKIP)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|y| y.as_u64()).filter_map(|y| u32::try_from(y).ok()).collect())
            .unwrap_or_default();
        let max = MaxSession::start(data_dir.as_deref(), &skip, wake.clone());
        let tab = settings.current_library();
        let stats = StatsCache::load(&data_dir.clone().unwrap_or_else(std::env::temp_dir).join("model_stats.json"));
        let mut app = Self {
            lang,
            settings,
            notice,
            data_dir,
            tab,
            index: None,
            scan: None,
            scanning: false,
            progress: None,
            side: SideTree::default(),
            favorite_count: 0,
            all_count: 0,
            hidden: HashSet::new(),
            search: String::new(),
            shown: Vec::new(),
            shown_dirty: true,
            selected: None,
            textures: HashMap::new(),
            loader,
            engines,
            jobs,
            found,
            asked: HashSet::new(),
            files: HashMap::new(),
            max,
            wake,
            dragging: None,
            stats,
            work: None,
            preview_ask: None,
            missing: None,
            catalog: None,
            surfaces: None,
            live: None,
            relink_ask: None,
            look: None,
            settings_window: None,
            ws_watch: None,
        };
        app.open_tab(tab, &cc.egui_ctx);
        app.watch_workspace();
        let empty = (0..app.settings.libraries().len()).all(|i| app.settings.library_paths(i).is_empty());
        if empty && app.settings.data.get("setup_done").and_then(|d| d.as_bool()) != Some(true) {
            app.settings_window = Some(SettingsWindow::new(&app.settings, &app.max, lang, Page::Workspace, true));
        }
        app
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    fn tab_kind(&self) -> &str {
        self.settings.libraries().get(self.tab).and_then(|l| l.get("kind")).and_then(|k| k.as_str()).unwrap_or("models")
    }

    fn open_tab(&mut self, tab: usize, ctx: &egui::Context) {
        if let Some((_, cancel)) = self.scan.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.tab = tab;
        self.index = None;
        self.side = SideTree { favorites: self.tab_view().opens_favorites, ..SideTree::default() };
        self.selected = None;
        self.shown.clear();
        self.shown_dirty = true;
        self.loader.clear_queue();
        self.scanning = false;
        self.progress = None;
        self.surfaces = None;
        let roots = self.settings.library_paths(tab);
        if self.tab_kind() != "models" {
            let data = self.data_dir.clone().unwrap_or_else(std::env::temp_dir);
            let kind = self.tab_kind().to_string();
            self.surfaces = Some(SurfacePanel::new(&kind, self.tab_view(), (&roots, &self.measures_dirs()), &data, self.lang, self.wake.clone()));
            return;
        }
        if roots.is_empty() {
            return;
        }
        let cache = self.data_dir.as_ref().map(|d| d.join("library").join(crate::library::cache_name(&roots)));
        let (send, receive) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut index = LibraryIndex::new(&roots);
            if cache.as_ref().is_some_and(|c| index.load(c)) && send.send(Snapshot::Index { index: index.clone(), done: false }).is_ok() {
                ctx.request_repaint();
            }
            let (progress, wake) = (send.clone(), ctx.clone());
            index.scan_reporting(true, &|| stop.load(Ordering::Relaxed), &mut |folders, models| {
                let _ = progress.send(Snapshot::Progress(folders, models));
                wake.request_repaint();
            });
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if let Some(cache) = &cache {
                let _ = cache.parent().map(std::fs::create_dir_all);
                let _ = index.save(cache); // without a cache the next start only reads the folders again
            }
            let _ = send.send(Snapshot::Index { index, done: true });
            ctx.request_repaint();
        });
        self.scan = Some((receive, cancel));
        self.scanning = true;
    }

    /// Where the sizes written down by hand are: the team's folder and this computer's, of both versions.
    fn measures_dirs(&self) -> Vec<PathBuf> {
        let workspace = self.settings.data.get("workspace").and_then(|w| w.as_str()).filter(|w| !w.is_empty()).map(PathBuf::from);
        let old = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join(crate::APP));
        [old, self.data_dir.clone(), workspace].into_iter().flatten().map(|d| d.join("medidas")).collect()
    }

    fn take_snapshots(&mut self) {
        let Some((receive, _)) = &self.scan else { return };
        let (mut ended, mut fresh) = (false, false);
        while let Ok(snapshot) = receive.try_recv() {
            let (index, done) = match snapshot {
                Snapshot::Progress(folders, models) => {
                    self.progress = Some((folders, models));
                    continue;
                }
                Snapshot::Index { index, done } => (index, done),
            };
            if self.side.expanded.is_empty() {
                self.side.expanded.extend(index.roots.iter().map(|r| norm_key(r)));
            }
            self.index = Some(index);
            fresh = true;
            ended |= done;
        }
        if fresh {
            self.rebuild_tree();
        }
        if ended {
            self.scanning = false;
            self.scan = None;
        }
    }

    fn take_loaded(&mut self, ctx: &egui::Context) {
        if self.textures.len() > crate::thumbs::IN_MEMORY {
            self.textures.clear(); // those on screen come back from the cache
        }
        for loaded in self.loader.poll() {
            let texture = loaded.image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(&loaded.key, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
            self.textures.insert(loaded.key, texture);
        }
        while let Ok(found) = self.found.try_recv() {
            if let Found::Files(path, total, missing) = found {
                self.files.insert(path, (total, missing));
            }
        }
    }

    fn refresh_shown(&mut self) {
        if !self.shown_dirty {
            return;
        }
        self.shown_dirty = false;
        let Some(index) = &self.index else {
            self.shown.clear();
            return;
        };
        let hidden = &self.hidden;
        let shown: Vec<Asset> = index
            .search(&self.search, self.side.folder.as_deref(), true)
            .into_iter()
            .filter(|a| !hidden.contains(&norm_key(&a.folder)))
            .cloned()
            .collect();
        self.favorite_count = if favorites::any(&self.settings) {
            index.assets().into_iter().filter(|a| !hidden.contains(&norm_key(&a.folder)) && self.is_favorite(a)).count()
        } else {
            0
        };
        let (mine, rest): (Vec<Asset>, Vec<Asset>) = shown.into_iter().partition(|a| self.is_favorite(a));
        self.shown = if self.side.favorites {
            mine
        } else if self.tab_view().favorites_first {
            [mine, rest].concat()
        } else {
            let mut all = [mine, rest].concat();
            all.sort_by_cached_key(|a| a.name.to_lowercase());
            all
        };
    }

    /// The badges of an asset: its engines, read in the background for files without a suffix.
    fn engines_of(&mut self, asset: &Asset) -> Vec<Option<Engine>> {
        let mut out: Vec<Option<Engine>> = asset.versions.keys().map(|e| Some(*e)).collect();
        for file in &asset.auto_files {
            match self.engines.get(file) {
                Some(engine) => out.push(Some(engine)),
                None => {
                    if self.asked.insert(file.clone()) {
                        let _ = self.jobs.send(Job::Engine(file.clone()));
                    }
                    out.push(None);
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let names: Vec<String> =
                self.settings.libraries().iter().map(|l| l.get("name").and_then(|n| n.as_str()).unwrap_or("?").to_uppercase()).collect();
            for (i, name) in names.into_iter().enumerate() {
                if ui.selectable_label(i == self.tab, name).clicked() && i != self.tab {
                    self.open_tab(i, ui.ctx());
                    self.settings.data.insert("current_library".into(), i.into());
                    if let Err(error) = self.settings.save() {
                        self.notice = Some(fill(self.t(Text::SettingsError), &[("error", &error)]));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(self.t(Text::Settings)).clicked() {
                    self.settings_window = Some(SettingsWindow::new(&self.settings, &self.max, self.lang, Page::Tabs, false));
                }
            });
        });
        ui.horizontal(|ui| {
            let hint = self.t(Text::Search);
            let search = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(hint).desired_width(260.0));
            if search.changed() {
                self.shown_dirty = true;
            }
            if self.tab_kind() == "models" {
                ui.separator();
                self.tools(ui);
            }
        });
    }

    /// How the tab of this moment shows its folders and favourites.
    fn tab_view(&self) -> TabView {
        favorites::tab_view(self.settings.libraries().get(self.tab))
    }

    fn favorite_key(&self, asset: &Asset) -> String {
        favorites::model_key(self.index.as_ref().map_or(&[], |i| i.roots.as_slice()), &asset.key())
    }

    fn is_favorite(&self, asset: &Asset) -> bool {
        favorites::any(&self.settings) && favorites::is_favorite(&self.settings, &self.favorite_key(asset))
    }

    fn toggle_favorite(&mut self, asset: &Asset) {
        let key = self.favorite_key(asset);
        favorites::toggle(&mut self.settings, &[key]);
        self.save_settings();
        self.shown_dirty = true;
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            self.notice = Some(fill(self.t(Text::SettingsError), &[("error", &error)]));
        }
    }

    /// Works out again what is hidden, and the rows of the tree.
    fn rebuild_tree(&mut self) {
        let Some(index) = &self.index else { return };
        let (names, paths) = self.settings.hidden();
        self.hidden = hidden_folders(&index.folders, &index.roots, &names, &paths);
        let assets: Vec<&Asset> = index.assets().into_iter().filter(|a| !self.hidden.contains(&norm_key(&a.folder))).collect();
        let totals = foldertree::totals(&index.roots, assets.iter().map(|a| a.folder.as_path()));
        self.side.folders = foldertree::rows(&index.roots, &index.folders, &self.hidden);
        for folder in &mut self.side.folders {
            folder.count = Some(totals.get(&folder.key).copied().unwrap_or(0));
        }
        self.all_count = assets.len();
        self.shown_dirty = true;
    }

    fn tree(&mut self, ui: &mut egui::Ui) {
        let (view, counts) = (self.tab_view(), (self.favorite_count, self.all_count));
        let changed = self.side.show(ui, "models", &mut self.settings, self.lang, view, counts);
        if changed.settings {
            self.save_settings();
            self.rebuild_tree();
        }
        if changed.shown {
            self.shown_dirty = true;
            self.loader.clear_queue();
        }
    }

    /// What goes over the gallery.
    fn over(&mut self, ctx: &egui::Context) {
        if self.settings_window.is_none() {
            self.bridge_prompt(ctx);
        }
        self.windows(ctx);
        self.relink_question(ctx);
        self.settings_ui(ctx);
        if self.look.as_mut().is_some_and(|look| !look.show(ctx)) {
            self.look = None;
        }
    }

    /// The size of the thumbnails.
    fn thumb_slider(&mut self, ui: &mut egui::Ui) {
        let mut size = self.settings.data.get("thumb_size").and_then(|v| v.as_f64()).unwrap_or(160.0);
        let response = ui.add(egui::Slider::new(&mut size, 80.0..=400.0).show_value(false));
        if response.changed() {
            self.settings.data.insert("thumb_size".into(), (size.round() as i64).into());
        }
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            self.save_settings();
        }
    }

    /// The space bar opens the quick look, when no text is being written.
    fn keys(&mut self, ctx: &egui::Context) {
        let space = ctx.input(|i| i.key_pressed(egui::Key::Space));
        if space && !ctx.egui_wants_keyboard_input() && self.look.is_none() && self.settings_window.is_none() && self.missing.is_none() {
            self.quick_look();
        }
    }

    fn quick_look(&mut self) {
        let (items, at) = match &self.surfaces {
            Some(panel) => panel.looks(&self.settings),
            None => {
                let at = self.shown.iter().position(|a| Some(a.key()) == self.selected);
                (self.shown.iter().map(|a| (a.name.clone(), a.thumb_source().map(Path::to_path_buf))).collect(), at)
            }
        };
        if let Some(at) = at {
            self.look = Some(QuickLook::new(items, at, self.lang, self.wake.clone()));
        }
    }

    /// Starts looking at the workspace's file, if one is used.
    fn watch_workspace(&mut self) {
        self.ws_watch = self.settings.workspace().map(|folder| {
            Task::start(self.wake.clone(), move |link| {
                let mut last = None;
                while !link.cancelled() {
                    let stamp = workspace::workspace_stamp(&folder);
                    if last != Some(stamp) {
                        last = Some(stamp);
                        link.send(stamp);
                    }
                    for _ in 0..100 {
                        if link.cancelled() {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            })
        });
        if self.settings.ws_state() == WsState::Offline {
            self.notice = Some(self.t(Text::WsOffline).to_string());
        }
    }

    /// Brings what the team changed.
    fn poll_workspace(&mut self, ctx: &egui::Context) {
        let Some(stamp) = self.ws_watch.as_ref().and_then(|w| w.poll().pop()) else { return };
        if stamp.is_none() || (stamp == self.settings.ws_stamp() && self.settings.ws_state() == WsState::Ok) {
            return;
        }
        let changed = self.settings.refresh_shared();
        if changed.is_empty() {
            return;
        }
        self.notice = Some(self.t(Text::WsRefreshed).to_string());
        if changed.contains(&"libraries") {
            self.open_tab(self.settings.current_library(), ctx);
        } else if self.surfaces.is_none() {
            self.rebuild_tree();
        } else if let Some(panel) = &mut self.surfaces {
            panel.settings_changed(&self.settings);
        }
    }

    fn settings_ui(&mut self, ctx: &egui::Context) {
        let Some(window) = &mut self.settings_window else { return };
        let answer = window.show(ctx, &mut self.settings, &mut self.max);
        if answer.close {
            self.settings_window = None;
        }
        if let Some(lang) = answer.language {
            self.lang = lang;
        }
        if answer.saved {
            self.watch_workspace();
            self.open_tab(self.settings.current_library(), ctx);
        }
    }

    fn gallery(&mut self, ui: &mut egui::Ui) {
        if self.settings.library_paths(self.tab).is_empty() {
            ui.centered_and_justified(|ui| ui.label(self.t(Text::NoFolders)));
            return;
        }
        if self.shown.is_empty() {
            let text = if self.scanning { self.t(Text::Scanning) } else { self.t(Text::NoModels) };
            ui.centered_and_justified(|ui| ui.label(text));
            return;
        }
        let side = self.settings.data.get("thumb_size").and_then(|v| v.as_f64()).unwrap_or(160.0).clamp(80.0, 400.0) as f32;
        let card = Vec2::new(side, side + FOOTER);
        let width = ui.available_width();
        let columns = (((width + GAP) / (side + GAP)).floor() as usize).max(1);
        let rows = self.shown.len().div_ceil(columns);
        let mut select: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, card.y + GAP, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for col in 0..columns {
                        let Some(asset) = self.shown.get(row * columns + col).cloned() else { break };
                        if self.card(ui, &asset, card) {
                            select = Some(asset.key());
                        }
                    }
                });
                ui.add_space(GAP - ui.spacing().item_spacing.y);
            }
        });
        if select.is_some() {
            self.selected = select;
        }
    }

    /// Draws one card; true when it was clicked.
    fn card(&mut self, ui: &mut egui::Ui, asset: &Asset, size: Vec2) -> bool {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        if response.drag_started() {
            self.dragging = Some(asset.clone());
        }
        response.context_menu(|ui| {
            self.merge_menu(ui, asset);
            ui.separator();
            let favorite = if self.is_favorite(asset) { Text::FavoriteRemove } else { Text::FavoriteAdd };
            if ui.button(self.t(favorite)).clicked() {
                self.toggle_favorite(asset);
                ui.close();
            }
            if ui.button(self.t(Text::MenuPreviewSelection)).clicked() {
                self.live_preview(asset, false);
                ui.close();
            }
            if ui.button(self.t(Text::MenuPreviewView)).clicked() {
                self.live_preview(asset, true);
                ui.close();
            }
            if ui.button(self.t(Text::MenuPreview)).clicked() {
                self.ask_previews(std::slice::from_ref(asset));
                ui.close();
            }
            if ui.button(self.t(Text::MenuData)).clicked() {
                self.read_stats(std::slice::from_ref(asset));
                ui.close();
            }
        });
        if !ui.is_rect_visible(rect) {
            return false;
        }
        let key = asset.key();
        let selected = self.selected.as_deref() == Some(key.as_str());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        let fill = if response.hovered() || selected { visuals.widgets.hovered.weak_bg_fill } else { visuals.faint_bg_color };
        painter.rect_filled(rect, CornerRadius::ZERO, fill);
        let (text_color, weak) = (visuals.text_color(), visuals.weak_text_color());
        let image_rect = Rect::from_min_size(rect.min, Vec2::new(size.x, size.y - FOOTER));
        match self.textures.get(&key) {
            Some(Some(texture)) => {
                let fit = fit_inside(texture.size_vec2(), image_rect);
                painter.image(texture.id(), fit, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            Some(None) => {
                painter.text(image_rect.center(), egui::Align2::CENTER_CENTER, self.t(Text::NoPreview), egui::FontId::proportional(11.0), weak);
            }
            None => {
                if let Some(src) = asset.thumb_source() {
                    self.loader.request(&key, src);
                    painter.text(image_rect.center(), egui::Align2::CENTER_CENTER, self.t(Text::Loading), egui::FontId::proportional(11.0), weak);
                } else {
                    self.textures.insert(key.clone(), None);
                }
            }
        }
        // the engine badges, top right
        let mut x = rect.right() - 4.0;
        for engine in self.engines_of(asset).into_iter().rev() {
            let (label, color) = match engine {
                Some(Engine::Corona) => ("CO", CORONA),
                Some(Engine::VRay) => ("VR", VRAY),
                Some(Engine::FStorm) => ("FS", FSTORM),
                Some(Engine::Other) => ("··", Color32::DARK_GRAY),
                None => ("…", Color32::DARK_GRAY),
            };
            let badge = Rect::from_min_max(egui::pos2(x - 24.0, rect.top() + 4.0), egui::pos2(x, rect.top() + 20.0));
            painter.rect_filled(badge, CornerRadius::ZERO, color);
            painter.text(badge.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(10.0), Color32::WHITE);
            x -= 27.0;
        }
        let name_pos = egui::pos2(rect.left() + 6.0, rect.bottom() - FOOTER / 2.0);
        let galley = ui.painter().layout(asset.name.clone(), egui::FontId::proportional(12.0), text_color, size.x - 12.0);
        painter.galley(egui::pos2(name_pos.x, name_pos.y - galley.size().y.min(FOOTER) / 2.0), galley, Color32::WHITE);
        if selected {
            painter.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
        }
        if heart(ui, &response, self.is_favorite(asset)) {
            self.toggle_favorite(asset);
            return false;
        }
        if response.double_clicked() {
            self.selected = Some(key);
            self.quick_look();
        }
        response.clicked()
    }

    /// The menu of a model, for the 3ds Max of the last drop (or the only one open).
    fn merge_menu(&mut self, ui: &mut egui::Ui, asset: &Asset) {
        let Some(info) = self.max.target() else {
            ui.label(self.t(Text::NoMax));
            return;
        };
        ui.label(egui::RichText::new(format!("3ds Max {}", info.year)).small().weak());
        if ui.button(self.t(Text::MenuPlace)).clicked() {
            self.merge(asset, &info, MergeKind::Place);
            ui.close();
        }
        let proxy_ok = matches!(info.engine, Engine::Corona | Engine::VRay);
        let proxy_text = if proxy_ok { self.t(Text::MenuProxy) } else { self.t(Text::ProxyNeedsEngine) };
        if ui.add_enabled(proxy_ok, egui::Button::new(proxy_text)).clicked() {
            self.merge(asset, &info, MergeKind::Proxy);
            ui.close();
        }
        if ui.button(self.t(Text::MenuOrigin)).clicked() {
            self.merge(asset, &info, MergeKind::Origin);
            ui.close();
        }
    }

    /// Sends a model to a 3ds Max; what happens is told in the status bar.
    fn merge(&mut self, asset: &Asset, info: &MaxInfo, kind: MergeKind) {
        if info.busy {
            self.notice = Some(self.t(Text::Busy).to_string());
            return;
        }
        let proxy_ok = matches!(info.engine, Engine::Corona | Engine::VRay);
        let kind = if kind == MergeKind::Proxy && !proxy_ok { MergeKind::Place } else { kind };
        let imp = self.settings.data.get("import").cloned().unwrap_or_default();
        let roots = self.index.as_ref().map(|i| i.roots.clone()).unwrap_or_default();
        let local = self.data_dir.clone().unwrap_or_else(std::env::temp_dir).join("proxies");
        match merge_request(asset, info, kind, &imp, &roots, &self.engines, &local) {
            Err(_) => self.notice = Some(fill(self.t(Text::NoFile), &[("name", &asset.name)])),
            Ok((request, moved)) => {
                let mut notice = if kind == MergeKind::Origin { String::new() } else { fill(self.t(Text::Placing), &[("name", &asset.name)]) };
                if let Some(folder) = moved {
                    notice = format!("{notice} · {}", fill(self.t(Text::ProxyLocal), &[("folder", &folder.display())]));
                }
                self.notice = (!notice.is_empty()).then_some(notice);
                self.max.send_merge(request, &asset.name, info, self.lang, self.wake.clone());
            }
        }
    }

    /// While a card is dragged: its thumbnail under the pointer; on release over a 3ds Max, the merge.
    fn drag(&mut self, ctx: &egui::Context) {
        let Some(asset) = self.dragging.clone() else { return };
        let (released, pointer, modifiers) = ctx.input(|i| (i.pointer.any_released(), i.pointer.latest_pos(), i.modifiers));
        if !released {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            if let (Some(pos), Some(Some(texture))) = (pointer, self.textures.get(&asset.key())) {
                let size = texture.size_vec2() * (96.0 / texture.size_vec2().max_elem());
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("openlibrary-drag")));
                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(texture.id(), Rect::from_min_size(pos + Vec2::splat(12.0), size), uv, Color32::from_white_alpha(200));
            }
            return;
        }
        self.dragging = None;
        let kind = if modifiers.shift {
            MergeKind::Proxy
        } else if modifiers.ctrl {
            MergeKind::Origin
        } else {
            MergeKind::Place
        };
        if crate::platform::process_under_cursor() == Some(std::process::id()) {
            return; // dropped back on OpenLibrary
        }
        match self.max.under_cursor() {
            Some(info) => self.merge(&asset, &info, kind),
            None => {
                let text = if self.max.maxes().is_empty() { Text::NoMax } else { Text::DropOnMax };
                self.notice = Some(self.t(text).to_string());
            }
        }
    }

    /// The bridge prompt when the app opens: install it where it is missing, update it where it is old.
    fn bridge_prompt(&mut self, ctx: &egui::Context) {
        let Some((missing, old)) = self.max.prompt.clone() else { return };
        let names = |list: &[crate::bridge_install::MaxTarget]| list.iter().map(|t| t.label()).collect::<Vec<_>>().join(", ");
        let action = match (missing.is_empty(), old.is_empty()) {
            (false, false) => Text::InstallUpdate,
            (true, false) => Text::Update,
            _ => Text::Install,
        };
        let mut choice = None;
        let window = egui::Window::new("OpenLibrary").collapsible(false).resizable(false);
        window.anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(420.0);
            if !old.is_empty() {
                ui.label(fill(self.t(Text::BridgeOld), &[("maxes", &names(&old))]));
            }
            if !missing.is_empty() {
                ui.label(fill(self.t(Text::BridgeMissing), &[("maxes", &names(&missing))]));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new(self.t(action)).strong()).clicked() {
                    choice = Some(0);
                }
                if ui.button(self.t(Text::Later)).clicked() {
                    choice = Some(1);
                }
                if !missing.is_empty() && ui.button(self.t(Text::DontAskAgain)).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                let all: Vec<_> = old.iter().chain(&missing).cloned().collect();
                self.notice = Some(self.max.install(&all, self.lang));
                self.max.prompt = None;
            }
            Some(1) => self.max.prompt = None,
            Some(2) => {
                let mut years: Vec<serde_json::Value> = self.settings.data.get(BRIDGE_SKIP).and_then(|v| v.as_array()).cloned().unwrap_or_default();
                years.extend(missing.iter().filter_map(|t| t.year).map(serde_json::Value::from));
                self.settings.data.insert(BRIDGE_SKIP.into(), years.into());
                if let Err(error) = self.settings.save() {
                    self.notice = Some(fill(self.t(Text::SettingsError), &[("error", &error)]));
                }
                self.max.prompt = None;
            }
            _ => {}
        }
    }

    /// The right of the status bar: the 3ds Max this app talks to, and how to drag.
    fn connection_text(&self) -> String {
        match self.max.target() {
            None => self.t(Text::NoMax).to_string(),
            Some(info) => {
                let engine = match info.engine {
                    Engine::Corona => "Corona",
                    Engine::VRay => "V-Ray",
                    Engine::FStorm => "FStorm",
                    Engine::Other => self.t(Text::EngineOther),
                };
                let max = format!("3ds Max {}", info.year);
                format!("{} · {}", fill(self.t(Text::Connected), &[("max", &max), ("engine", &engine)]), self.t(Text::DragHint))
            }
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.selected.clone() else { return };
        let Some(asset) = self.shown.iter().find(|a| a.key() == key).cloned() else { return };
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(Some(texture)) = self.textures.get(&key) {
                let width = ui.available_width();
                let size = texture.size_vec2() * (width / texture.size_vec2().x);
                ui.image((texture.id(), size));
            }
            ui.add_space(6.0);
            ui.heading(&asset.name);
            ui.label(egui::RichText::new(asset.folder.display().to_string()).small().weak());
            if asset.preview.is_none() && matches!(self.textures.get(&key), Some(Some(_))) {
                ui.label(egui::RichText::new(self.t(Text::ViewportCapture)).small().italics());
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new(self.t(Text::Versions).to_uppercase()).small().strong());
            for file in asset.all_files() {
                let engine = asset.versions.iter().find(|(_, f)| f.as_path() == file).map(|(e, _)| *e).or_else(|| self.engines.get(file));
                let label = match engine {
                    Some(Engine::Corona) => "Corona",
                    Some(Engine::VRay) => "V-Ray",
                    Some(Engine::FStorm) => "FStorm",
                    Some(Engine::Other) => self.t(Text::EngineOther),
                    None => self.t(Text::EnginePending),
                };
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ui.label(format!("{label}  ·  {name}"));
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new(self.t(Text::ExternalFiles).to_uppercase()).small().strong());
            let mut any_missing = false;
            for file in asset.all_files() {
                match self.files.get(file) {
                    Some((total, 0)) => {
                        ui.label(fill(self.t(Text::FilesOk), &[("count", total)]));
                    }
                    Some((total, missing)) => {
                        ui.colored_label(ACCENT, fill(self.t(Text::FilesMissing), &[("missing", missing), ("count", total)]));
                        any_missing = true;
                    }
                    None => {
                        if self.asked.insert(file.with_extension("files")) {
                            let _ = self.jobs.send(Job::Files(file.to_path_buf()));
                        }
                        ui.label(self.t(Text::EnginePending));
                    }
                }
            }
            if any_missing && ui.button(self.t(Text::MissingOpen)).clicked() {
                self.open_missing(&asset);
            }
            ui.add_space(8.0);
            self.stats_panel(ui, &asset);
            ui.add_space(10.0);
            if ui.button(self.t(Text::OpenFolder)).clicked() {
                let target = asset.any_file().map_or_else(|| asset.folder.clone(), Path::to_path_buf);
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", target.display())).spawn();
            }
        });
    }
}

/// The reader of `.max` files in the background: renderers and external files.
fn spawn_reader(engines: Arc<EngineCache>, wake: Arc<dyn Fn() + Send + Sync>) -> (Sender<Job>, Receiver<Found>) {
    let (jobs, receive_jobs) = channel::<Job>();
    let (send_found, found) = channel::<Found>();
    let receive_jobs = Arc::new(Mutex::new(receive_jobs));
    for _ in 0..2 {
        let (receive_jobs, send_found, engines, wake) = (receive_jobs.clone(), send_found.clone(), engines.clone(), wake.clone());
        std::thread::spawn(move || {
            loop {
                let job = match receive_jobs.lock() {
                    Ok(r) => r.recv(),
                    Err(_) => return,
                };
                let Ok(job) = job else { return };
                let result = match job {
                    Job::Engine(path) => {
                        engines.detect(&path);
                        Found::Engine
                    }
                    Job::Files(path) => {
                        let (total, missing) = count_missing(&path);
                        Found::Files(path, total, missing)
                    }
                };
                if send_found.send(result).is_err() {
                    return;
                }
                wake();
            }
        });
    }
    (jobs, found)
}

/// The heart at the corner of a card; `true` when it was clicked.
pub(crate) fn heart(ui: &mut egui::Ui, card: &egui::Response, on: bool) -> bool {
    let rect = Rect::from_min_size(card.rect.right_bottom() - Vec2::splat(FOOTER), Vec2::splat(FOOTER)).shrink(3.0);
    if !on && !card.hovered() && !ui.rect_contains_pointer(rect) {
        return false;
    }
    let response = ui.interact(rect, card.id.with("heart"), Sense::click());
    let color = if on { ACCENT } else { ui.visuals().weak_text_color() };
    // two circles and the point below: the fonts have no heart
    let (painter, at, r) = (ui.painter(), rect.center(), 3.6);
    painter.circle_filled(at + Vec2::new(-r * 0.92, -r * 0.5), r, color);
    painter.circle_filled(at + Vec2::new(r * 0.92, -r * 0.5), r, color);
    let point = [Vec2::new(-r * 1.68, r * 0.12), Vec2::new(0.0, -r * 0.5), Vec2::new(r * 1.68, r * 0.12), Vec2::new(0.0, r * 1.9)];
    painter.add(egui::Shape::convex_polygon(point.iter().map(|p| at + *p).collect(), color, Stroke::NONE));
    response.clicked()
}

fn fit_inside(size: Vec2, area: Rect) -> Rect {
    let scale = (area.width() / size.x).min(area.height() / size.y);
    Rect::from_center_size(area.center(), size * scale)
}

/// The look of the Open3D programs: square corners and the amber accent, in the dark and in the light theme
/// (the window follows the one Windows uses).
fn style(ctx: &egui::Context) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut visuals = match theme {
            egui::Theme::Dark => egui::Visuals::dark(),
            egui::Theme::Light => egui::Visuals::light(),
        };
        visuals.selection.bg_fill = ACCENT.gamma_multiply(0.6);
        visuals.selection.stroke = Stroke::new(1.0, if theme == egui::Theme::Dark { Color32::WHITE } else { Color32::BLACK });
        visuals.hyperlink_color = ACCENT;
        visuals.window_corner_radius = CornerRadius::ZERO;
        visuals.menu_corner_radius = CornerRadius::ZERO;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::ZERO;
        }
        ctx.set_visuals_of(theme, visuals);
    }
}

impl eframe::App for OpenLibraryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_snapshots();
        self.take_loaded(&ctx);
        self.poll_work();
        self.poll_live();
        self.poll_workspace(&ctx);
        self.refresh_shown();
        for done in self.max.poll() {
            self.notice = Some(done.text);
        }
        if self.catalogue(ui) {
            return;
        }
        self.keys(&ctx);
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            let text = if let Some(work) = self.work_text() {
                work
            } else if let Some(notice) = &self.notice {
                notice.clone()
            } else if let Some(panel) = &self.surfaces {
                panel.status()
            } else if let (true, Some((folders, models))) = (self.scanning, self.progress) {
                fill(self.t(Text::ScanProgress), &[("folders", &folders), ("models", &models)])
            } else if self.scanning {
                self.t(Text::Scanning).to_string()
            } else {
                fill(self.t(Text::Models), &[("count", &self.shown.len())])
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text).small());
                if self.work.is_some() && ui.small_button(self.t(Text::Cancel)).clicked() {
                    self.cancel_work();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.thumb_slider(ui);
                    ui.label(egui::RichText::new(self.connection_text()).small().weak());
                });
            });
        });
        if let Some(mut panel) = self.surfaces.take() {
            let mut tools = Tools { settings: &mut self.settings, max: &mut self.max, notice: &mut self.notice };
            panel.show(ui, &self.search, &mut tools);
            let look = panel.take_look();
            self.surfaces = Some(panel);
            if look {
                self.quick_look();
            }
            self.over(&ctx);
            return;
        }
        if self.index.is_some() {
            egui::Panel::left("tree").resizable(true).default_size(240.0).show(ui, |ui| self.tree(ui));
        }
        if self.selected.is_some() {
            egui::Panel::right("inspector").resizable(true).default_size(300.0).show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| self.gallery(ui));
        self.drag(&ctx);
        self.over(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.engines.save(); // only a cache: the next start reads the files again
    }
}

/// Opens the window.
pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(crate::APP).with_inner_size([1200.0, 780.0]).with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(crate::APP, options, Box::new(|cc| Ok(Box::new(OpenLibraryApp::new(cc)))))
}
