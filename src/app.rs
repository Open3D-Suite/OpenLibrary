// SPDX-License-Identifier: GPL-3.0-or-later
//! The window: what it holds and how it hears from its threads. It is drawn in the files of `app`: the frame
//! (`frame.rs`), the models (`gallery.rs`, `inspector.rs`) and what goes over them; materials, textures and interiors
//! are in `ui_surfaces.rs`. What it shows comes from the modules of the library.

mod frame;
mod gallery;
mod inspector;
mod live;
mod merge;
mod work;

pub(crate) use gallery::{Grid, heart, picture};

use crate::favorites::{self, TabView};
use crate::foldertree::{self, SideTree};
use crate::kit;
use crate::library::{Asset, LibraryIndex, hidden_folders, norm_key};
use crate::max_session::MaxSession;
use crate::maxfile::EngineCache;
use crate::missing::CheckCache;
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
use eframe::egui::{self, Color32, TextureHandle, TextureOptions};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The years of 3ds Max where the person chose not to be asked about the bridge again.
const BRIDGE_SKIP: &str = "olib_bridge_skip";
/// How many thumbnails become textures in one frame: more would make it late.
const TEXTURES_PER_FRAME: usize = 16;

/// What a scan thread sends: how far it has got, or the index as far as it knows it and whether it has ended.
enum Snapshot {
    Progress(usize, usize),
    Index { index: LibraryIndex, done: bool },
}

/// What the background reader of `.max` files works on.
enum Job {
    Engine(PathBuf),
    /// A `.max`, and the folders of the libraries.
    Check(PathBuf, Vec<PathBuf>),
    /// The card of a model, its files, and what finds the folders its proxy may be in (it looks at the disk).
    Detail(String, Vec<PathBuf>, FindProxies),
}

type FindProxies = Box<dyn FnOnce() -> Vec<PathBuf> + Send>;

enum Found {
    /// A renderer, or external files: what is shown may change.
    File,
    Detail(String, Detail),
}

/// What the inspector asks the disk.
#[derive(Clone, Default)]
struct Detail {
    /// Size and date of each file; `None` when it cannot be reached.
    files: HashMap<PathBuf, Option<(u64, std::time::SystemTime)>>,
    proxy_ready: bool,
}

/// A model on screen.
#[derive(Clone)]
struct Shown {
    asset: Arc<Asset>,
    key: String,
    favorite: bool,
}

/// A tab of models; those not on screen are kept as they were.
#[derive(Default)]
struct ModelsTab {
    index: Option<LibraryIndex>,
    scan: Option<(Receiver<Snapshot>, Arc<AtomicBool>)>,
    progress: Option<(usize, usize)>,
    side: SideTree,
    hidden: HashSet<String>,
    all_count: usize,
    favorite_count: usize,
    shown: Vec<Shown>,
    dirty: bool,
    selected: HashSet<String>,
    anchor: usize,
    /// Scroll to this card in the next frame.
    reveal: Option<usize>,
}

impl ModelsTab {
    fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    fn selection(&self) -> Vec<Shown> {
        if self.selected.is_empty() {
            return Vec::new();
        }
        self.shown.iter().filter(|s| self.selected.contains(&s.key)).cloned().collect()
    }

    /// The selected models, without copying them.
    fn selected_assets(&self) -> Vec<Arc<Asset>> {
        self.selection().into_iter().map(|s| s.asset).collect()
    }
}

/// A question with a line to write.
enum Ask {
    HideNames,
    NewFolder(PathBuf),
}

pub struct OpenLibraryApp {
    lang: Language,
    settings: Settings,
    /// The settings changed and are saved a little later, all at once.
    save_at: Option<Instant>,
    notice: Option<String>,
    data_dir: PathBuf,
    tab: usize,
    tab_names: Vec<String>,
    models: ModelsTab,
    surfaces: Option<SurfacePanel>,
    kept_models: HashMap<usize, ModelsTab>,
    kept_surfaces: HashMap<usize, SurfacePanel>,
    search: String,
    textures: HashMap<String, Option<TextureHandle>>,
    edges: HashMap<String, Color32>,
    loaded: Vec<crate::thumbs::Loaded>,
    loader: ThumbLoader,
    engines: Arc<EngineCache>,
    checks: Arc<CheckCache>,
    jobs: Sender<Job>,
    found: Receiver<Found>,
    asked: HashSet<PathBuf>,
    details: HashMap<String, Detail>,
    detail_asked: HashSet<String>,
    big: Option<inspector::Big>,
    max: MaxSession,
    wake: Arc<dyn Fn() + Send + Sync>,
    dragging: bool,
    drop: Option<merge::DropMenu>,
    merge: Option<merge::Run>,
    merge_ask: Option<merge::Question>,
    stats: StatsCache,
    work: Option<work::Work>,
    preview_ask: Option<Vec<work::PreviewJob>>,
    missing: Option<MissingWindow>,
    catalog: Option<CatalogWindow>,
    live: Option<live::Live>,
    relink_ask: Option<Vec<(PathBuf, Arc<Asset>)>>,
    look: Option<QuickLook>,
    settings_window: Option<SettingsWindow>,
    setup: Option<crate::ui_setup::SetupWindow>,
    ask: Option<Ask>,
    ask_text: String,
    /// Frames left to tell Windows the theme of the title bar: the window may not be there in the first ones.
    title_bar: u8,
    /// Looks at the workspace's file every few seconds, on a thread: the server may be slow.
    ws_watch: Option<Task<WsStamp>>,
}

impl OpenLibraryApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        kit::apply(&cc.egui_ctx);
        let lang = open3d_settings::language(open3d_settings::settings_dir().as_deref());
        let settings_dir = open3d_settings::app_settings_dir(crate::APP);
        let data_dir = open3d_settings::app_data_dir(crate::APP).unwrap_or_else(std::env::temp_dir);
        let mut notice = None;
        // The Python version kept its settings in %LOCALAPPDATA%\OpenLibrary: bring them over once.
        if let (Some(new), Some(local)) = (&settings_dir, std::env::var_os("LOCALAPPDATA")) {
            match settings::carry_over_old_settings(&PathBuf::from(local).join(crate::APP), new) {
                Ok(true) => notice = Some(Text::CarriedOver.get(lang).to_string()),
                Ok(false) => {}
                Err(error) => notice = Some(fill(Text::SettingsError.get(lang), &[("error", &error)])),
            }
        }
        let settings = Settings::load(&settings_dir.unwrap_or_default().join(settings::FILE_NAME));
        kit::set_theme(&cc.egui_ctx, settings.data.get("theme").and_then(Value::as_str));
        let ctx = cc.egui_ctx.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let loader = ThumbLoader::new(data_dir.join("thumbs"), 8, wake.clone());
        let engines = Arc::new(EngineCache::load(&data_dir.join("engines.json")));
        let checks = Arc::new(CheckCache::load(&data_dir.join("external_files.json")));
        let (jobs, found) = spawn_reader(engines.clone(), checks.clone(), wake.clone());
        let skip: Vec<u32> = settings
            .data
            .get(BRIDGE_SKIP)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).filter_map(|y| u32::try_from(y).ok()).collect())
            .unwrap_or_default();
        let max = MaxSession::start(Some(&data_dir), &skip, wake.clone());
        let tab = settings.current_library();
        let stats = StatsCache::load(&data_dir.join("model_stats.json"));
        let mut app = Self {
            lang,
            settings,
            save_at: None,
            notice,
            data_dir,
            tab,
            tab_names: Vec::new(),
            models: ModelsTab::default(),
            surfaces: None,
            kept_models: HashMap::new(),
            kept_surfaces: HashMap::new(),
            search: String::new(),
            textures: HashMap::new(),
            edges: HashMap::new(),
            loaded: Vec::new(),
            loader,
            engines,
            checks,
            jobs,
            found,
            asked: HashSet::new(),
            details: HashMap::new(),
            detail_asked: HashSet::new(),
            big: None,
            max,
            wake,
            dragging: false,
            drop: None,
            merge: None,
            merge_ask: None,
            stats,
            work: None,
            preview_ask: None,
            missing: None,
            catalog: None,
            live: None,
            relink_ask: None,
            look: None,
            settings_window: None,
            setup: None,
            ask: None,
            ask_text: String::new(),
            title_bar: 8,
            ws_watch: None,
        };
        app.reset_tabs();
        app.watch_workspace();
        app.set_on_top(&cc.egui_ctx);
        // the first time: the welcome, which asks for the workspace, the button in 3ds Max and the shortcuts
        if app.settings.data.get(crate::ui_setup::DONE).and_then(Value::as_bool) != Some(true) {
            app.setup = Some(crate::ui_setup::SetupWindow::new(&app.settings, &app.max, lang, app.wake.clone()));
        }
        app
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    fn flag(&self, key: &str, default: bool) -> bool {
        self.settings.data.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    fn option(&self, key: &str) -> &str {
        self.settings.data.get(key).and_then(Value::as_str).unwrap_or_default()
    }

    /// Sets a value of the settings, which are saved a little later.
    fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.settings.data.insert(key.into(), value.into());
        self.save_at.get_or_insert_with(|| Instant::now() + Duration::from_millis(800));
    }

    fn save_settings(&mut self) {
        self.save_at = None;
        if let Err(error) = self.settings.save() {
            self.notice = Some(fill(self.t(Text::SettingsError), &[("error", &error)]));
        }
    }

    fn kind_of(&self, tab: usize) -> &str {
        self.settings.libraries().get(tab).and_then(|l| l.get("kind")).and_then(Value::as_str).unwrap_or("models")
    }

    /// Forgets what the tabs had and opens the one of the settings.
    fn reset_tabs(&mut self) {
        for (_, tab) in self.kept_models.drain() {
            stop(&tab);
        }
        stop(&self.models);
        self.kept_surfaces.clear();
        (self.models, self.surfaces) = (ModelsTab::default(), None);
        self.tab_names = self.settings.libraries().iter().map(|l| l.get("name").and_then(Value::as_str).unwrap_or("?").to_string()).collect();
        self.tab = self.settings.current_library();
        self.show_tab();
    }

    /// Goes to another tab; the one left stays as it is, and goes on reading its folders.
    fn open_tab(&mut self, tab: usize) {
        if tab == self.tab {
            return;
        }
        match self.surfaces.take() {
            Some(panel) => {
                self.kept_surfaces.insert(self.tab, panel);
            }
            None => {
                self.kept_models.insert(self.tab, std::mem::take(&mut self.models));
            }
        }
        self.loader.clear_queue();
        self.tab = tab;
        self.notice = None;
        self.set("current_library", tab);
        self.show_tab();
    }

    fn show_tab(&mut self) {
        let tab = self.tab;
        let roots = self.settings.library_paths(tab);
        let view = favorites::tab_view(self.settings.libraries().get(tab));
        let kind = self.kind_of(tab).to_string();
        if kind != "models" {
            let panel = self
                .kept_surfaces
                .remove(&tab)
                .unwrap_or_else(|| SurfacePanel::new(&kind, view, (&roots, &self.measures_dirs()), &self.data_dir, self.lang, self.wake.clone()));
            self.surfaces = Some(panel);
            return;
        }
        if let Some(kept) = self.kept_models.remove(&tab) {
            self.models = kept;
            self.models.dirty = true;
            return;
        }
        self.models = ModelsTab { side: SideTree { favorites: view.opens_favorites, ..SideTree::default() }, dirty: true, ..ModelsTab::default() };
        if !roots.is_empty() {
            self.scan(true);
        }
    }

    /// Reads the folders of the tab: only those that changed when `incremental`.
    fn scan(&mut self, incremental: bool) {
        if let Some((_, cancel)) = self.models.scan.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let roots = self.settings.library_paths(self.tab);
        let cache = self.data_dir.join("library").join(crate::library::cache_name(&roots));
        let known = self.models.index.clone();
        let (send, receive) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (stop, wake) = (cancel.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let fresh = known.is_none();
            let mut index = known.unwrap_or_else(|| LibraryIndex::new(&roots));
            if fresh && index.load(&cache) && send.send(Snapshot::Index { index: index.clone(), done: false }).is_ok() {
                wake();
            }
            let (progress, tell) = (send.clone(), wake.clone());
            index.scan_reporting(incremental, &|| stop.load(Ordering::Relaxed), &mut |folders, models| {
                let _ = progress.send(Snapshot::Progress(folders, models));
                tell();
            });
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let _ = cache.parent().map(std::fs::create_dir_all);
            let _ = index.save(&cache); // without a cache the next start only reads the folders again
            let _ = send.send(Snapshot::Index { index, done: true });
            wake();
        });
        self.models.scan = Some((receive, cancel));
        self.models.progress = None;
    }

    /// Where the sizes written down by hand are: the team's folder and this computer's, of both versions.
    fn measures_dirs(&self) -> Vec<PathBuf> {
        let old = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join(crate::APP));
        [old, Some(self.data_dir.clone()), self.settings.workspace()].into_iter().flatten().map(|d| d.join("medidas")).collect()
    }

    fn take_snapshots(&mut self) {
        let Some((receive, _)) = &self.models.scan else { return };
        let mut latest = None;
        let mut progress = None;
        while let Ok(snapshot) = receive.try_recv() {
            match snapshot {
                Snapshot::Progress(folders, models) => progress = Some((folders, models)),
                Snapshot::Index { index, done } => latest = Some((index, done)),
            }
        }
        self.models.progress = progress.or(self.models.progress);
        let Some((index, done)) = latest else { return };
        if self.models.side.expanded.is_empty() {
            self.models.side.expanded.extend(index.roots.iter().map(|r| norm_key(r)));
        }
        self.models.index = Some(index);
        if done {
            self.models.scan = None;
            self.models.progress = None;
        }
        self.rebuild_tree();
    }

    fn take_loaded(&mut self, ctx: &egui::Context) {
        if self.textures.len() > crate::thumbs::IN_MEMORY {
            self.textures.clear(); // those on screen come back from the cache
            self.edges.clear();
        }
        self.loaded.extend(self.loader.poll());
        let now = self.loaded.len().min(TEXTURES_PER_FRAME);
        for loaded in self.loaded.drain(..now) {
            let texture = loaded.image.map(|img| {
                let edge = img.get_pixel(1.min(img.width() - 1), 1.min(img.height() - 1)).0;
                self.edges.insert(loaded.key.clone(), Color32::from_rgb(edge[0], edge[1], edge[2]));
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(&loaded.key, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
            self.textures.insert(loaded.key, texture);
        }
        if !self.loaded.is_empty() {
            ctx.request_repaint();
        }
        while let Ok(found) = self.found.try_recv() {
            match found {
                Found::Detail(key, detail) => {
                    self.details.insert(key, detail);
                }
                // a filter may hide or show it now
                Found::File => self.models.dirty |= self.filters_need_files(),
            }
        }
    }

    /// The folders of every tab.
    fn all_roots(&self) -> Vec<PathBuf> {
        (0..self.settings.libraries().len()).flat_map(|i| self.settings.library_paths(i)).collect()
    }

    /// How the tab of this moment shows its folders and favourites.
    fn tab_view(&self) -> TabView {
        favorites::tab_view(self.settings.libraries().get(self.tab))
    }

    fn favorite_key(&self, key: &str) -> String {
        favorites::model_key(self.models.index.as_ref().map_or(&[], |i| i.roots.as_slice()), key)
    }

    fn toggle_favorites(&mut self, keys: &[String]) {
        let keys: Vec<String> = keys.iter().map(|k| self.favorite_key(k)).collect();
        favorites::toggle(&mut self.settings, &keys);
        self.save_settings();
        self.models.dirty = true;
    }

    /// Works out again what is hidden, and the rows of the tree.
    fn rebuild_tree(&mut self) {
        let (names, paths) = self.settings.hidden();
        let Some(index) = &self.models.index else { return };
        self.models.hidden = hidden_folders(&index.folders, &index.roots, &names, &paths);
        let rows = index.rows().iter().filter(|r| !self.models.hidden.contains(&r.folder));
        let totals = foldertree::totals(&index.roots, rows.clone().map(|r| r.folder.as_str()));
        self.models.all_count = rows.count();
        self.models.side.folders = foldertree::rows(&index.roots, &index.folders, &self.models.hidden);
        for folder in &mut self.models.side.folders {
            folder.count = Some(totals.get(&folder.key).copied().unwrap_or(0));
        }
        self.models.dirty = true;
    }

    /// Reads these folders again, after the app wrote in them.
    fn rescan(&mut self, folders: impl IntoIterator<Item = PathBuf>) {
        let cache = self.data_dir.join("library").join(crate::library::cache_name(&self.settings.library_paths(self.tab)));
        let Some(index) = &mut self.models.index else { return };
        for folder in folders {
            index.rescan_folder(&folder);
        }
        let _ = index.save(&cache); // only a cache
        self.rebuild_tree();
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
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            })
        });
        if self.settings.ws_state() == WsState::Offline {
            self.notice = Some(self.t(Text::WsOffline).to_string());
        }
    }

    /// Brings what the team changed.
    fn poll_workspace(&mut self) {
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
            self.reset_tabs();
        } else if let Some(panel) = &mut self.surfaces {
            panel.settings_changed(&self.settings);
        } else {
            self.rebuild_tree();
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
            self.save_at = None;
            self.watch_workspace();
            self.reset_tabs();
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
        egui::Modal::new(egui::Id::unique("openlibrary-bridge")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            if !old.is_empty() {
                ui.label(fill(self.t(Text::BridgeOld), &[("maxes", &names(&old))]));
            }
            if !missing.is_empty() {
                ui.label(fill(self.t(Text::BridgeMissing), &[("maxes", &names(&missing))]));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if kit::button(ui, kit::Kind::Accent, None, self.t(action)).clicked() {
                    choice = Some(0);
                }
                if kit::button(ui, kit::Kind::Action, None, self.t(Text::Later)).clicked() {
                    choice = Some(1);
                }
                if !missing.is_empty() && kit::button(ui, kit::Kind::Action, None, self.t(Text::DontAskAgain)).clicked() {
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
                let mut years: Vec<Value> = self.settings.data.get(BRIDGE_SKIP).and_then(Value::as_array).cloned().unwrap_or_default();
                years.extend(missing.iter().filter_map(|t| t.year).map(Value::from));
                self.settings.data.insert(BRIDGE_SKIP.into(), years.into());
                self.save_settings();
                self.max.prompt = None;
            }
            _ => {}
        }
    }

    /// The question with a line to write.
    fn ask_ui(&mut self, ctx: &egui::Context) {
        let Some(ask) = &self.ask else { return };
        let text = match ask {
            Ask::HideNames => self.t(Text::HideNamesAsk).to_string(),
            Ask::NewFolder(parent) => fill(self.t(Text::CatalogAskFolder), &[("folder", &parent.display())]),
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("openlibrary-ask")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(text);
            ui.add(egui::TextEdit::singleline(&mut self.ask_text).desired_width(f32::INFINITY)).request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                choice = Some(true);
            }
            ui.horizontal(|ui| {
                if kit::button(ui, kit::Kind::Primary, None, self.t(Text::Accept)).clicked() {
                    choice = Some(true);
                }
                if kit::button(ui, kit::Kind::Action, None, self.t(Text::Cancel)).clicked() {
                    choice = Some(false);
                }
            });
        });
        let Some(yes) = choice else { return };
        let (Some(ask), text) = (self.ask.take(), std::mem::take(&mut self.ask_text)) else { return };
        if !yes {
            return;
        }
        match ask {
            Ask::HideNames => {
                self.settings.set_hidden_names(&text);
                self.after_tree_settings();
            }
            Ask::NewFolder(parent) => {
                let name = crate::catalog::clean_name(&text);
                if name.is_empty() {
                    return;
                }
                match std::fs::create_dir_all(parent.join(name)) {
                    Ok(()) => self.scan(true),
                    Err(error) => self.notice = Some(fill(self.t(Text::CatalogSaveFailed), &[("error", &error)])),
                }
            }
        }
    }

    fn after_tree_settings(&mut self) {
        self.save_settings();
        match &mut self.surfaces {
            Some(panel) => panel.settings_changed(&self.settings),
            None => self.rebuild_tree(),
        }
    }

    /// What goes over the gallery.
    fn over(&mut self, ctx: &egui::Context) {
        if let Some(setup) = &mut self.setup {
            let answer = setup.show(ctx, &mut self.settings, &mut self.max);
            if answer.workspace {
                self.watch_workspace();
                self.reset_tabs();
            }
            if answer.close {
                self.setup = None;
                self.max.prompt = None; // the welcome asked it already
                let empty = (0..self.settings.libraries().len()).all(|i| self.settings.library_paths(i).is_empty());
                if empty {
                    self.settings_window = Some(SettingsWindow::new(&self.settings, &self.max, self.lang, Page::Tabs, false));
                }
            }
            return;
        }
        if self.settings_window.is_none() {
            self.bridge_prompt(ctx);
        }
        self.windows(ctx);
        self.relink_question(ctx);
        self.merge_question(ctx);
        self.ask_ui(ctx);
        self.settings_ui(ctx);
        self.drop_menu(ctx);
        if let Some(look) = &mut self.look {
            let open = look.show(ctx);
            let name = look.at();
            if !open {
                self.look = None;
            }
            self.look_moved(name);
        }
    }

    /// The card shown large is the one selected.
    fn look_moved(&mut self, at: usize) {
        if let Some(panel) = &mut self.surfaces {
            panel.select_only(at);
        } else if let Some(key) = self.models.shown.get(at).map(|s| s.key.clone())
            && (self.models.selected.len() != 1 || !self.models.selected.contains(&key))
        {
            self.models.selected = HashSet::from([key]);
            self.models.anchor = at;
            self.models.reveal = Some(at);
        }
    }

    fn modal_open(&self) -> bool {
        self.look.is_some()
            || self.settings_window.is_some()
            || self.setup.is_some()
            || self.missing.is_some()
            || self.ask.is_some()
            || self.merge_ask.is_some()
            || self.preview_ask.is_some()
            || self.relink_ask.is_some()
    }

    fn quick_look(&mut self) {
        let (items, at) = match &self.surfaces {
            Some(panel) => panel.looks(&self.settings),
            None => {
                let at = self.models.shown.iter().position(|s| self.models.selected.contains(&s.key));
                let look = |s: &Shown| (s.asset.name.clone(), s.asset.thumb_source().map(Path::to_path_buf));
                (self.models.shown.iter().map(look).collect(), at)
            }
        };
        if let Some(at) = at {
            self.look = Some(QuickLook::new(items, at, self.lang, self.wake.clone()));
        }
    }

    /// The keys of the gallery, when no text is being written.
    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.modal_open() {
            return;
        }
        let (space, enter, all) =
            ctx.input(|i| (i.key_pressed(egui::Key::Space), i.key_pressed(egui::Key::Enter), i.modifiers.command && i.key_pressed(egui::Key::A)));
        if space {
            self.quick_look();
        }
        if self.surfaces.is_some() {
            return;
        }
        if enter {
            self.open_drop_menu(None);
        }
        if all {
            self.models.selected = self.models.shown.iter().map(|s| s.key.clone()).collect();
        }
        self.arrow_keys(ctx);
    }
}

/// Stops a tab's reading.
fn stop(tab: &ModelsTab) {
    if let Some((_, cancel)) = &tab.scan {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// The readers of `.max` files in the background, the last asked first.
fn spawn_reader(engines: Arc<EngineCache>, checks: Arc<CheckCache>, wake: Arc<dyn Fn() + Send + Sync>) -> (Sender<Job>, Receiver<Found>) {
    let (jobs, receive) = channel::<Job>();
    let (send, found) = channel::<Found>();
    let queue = Arc::new(Mutex::new((receive, Vec::<Job>::new())));
    for _ in 0..3 {
        let (queue, send, engines, checks, wake) = (queue.clone(), send.clone(), engines.clone(), checks.clone(), wake.clone());
        std::thread::spawn(move || {
            loop {
                let job = {
                    let Ok(mut queue) = queue.lock() else { return };
                    let (receive, waiting) = &mut *queue;
                    if waiting.is_empty() {
                        match receive.recv() {
                            Ok(job) => waiting.push(job),
                            Err(_) => return,
                        }
                    }
                    waiting.extend(receive.try_iter());
                    waiting.pop()
                };
                let result = match job {
                    None => continue,
                    Some(Job::Engine(path)) => {
                        engines.detect(&path);
                        Found::File
                    }
                    Some(Job::Check(path, roots)) => {
                        checks.check(&path, &roots);
                        Found::File
                    }
                    Some(Job::Detail(key, files, proxies)) => {
                        let stat = |f: &PathBuf| std::fs::metadata(f).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
                        let made = |d: &PathBuf| d.join("done.olb").is_file() && d.join("proxies.max").is_file();
                        let files = files.into_iter().map(|f| (f.clone(), stat(&f))).collect();
                        Found::Detail(key, Detail { files, proxy_ready: proxies().iter().any(made) })
                    }
                };
                if send.send(result).is_err() {
                    return;
                }
                wake();
            }
        });
    }
    (jobs, found)
}

impl eframe::App for OpenLibraryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.title_bar > 0 {
            self.title_bar -= 1;
            kit::set_theme(&ctx, self.settings.data.get("theme").and_then(Value::as_str));
            ctx.request_repaint();
        }
        self.take_snapshots();
        self.take_loaded(&ctx);
        self.poll_work();
        self.poll_live();
        self.poll_merge();
        self.poll_workspace();
        if self.save_at.is_some_and(|at| at <= Instant::now()) {
            self.save_settings();
        } else if self.save_at.is_some() {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        if self.catalogue(ui) {
            return;
        }
        self.keys(&ctx);
        self.header(ui);
        self.status_bar(ui);
        if let Some(mut panel) = self.surfaces.take() {
            let info = self.flag("show_info_bar", true);
            let mut tools = Tools { settings: &mut self.settings, max: &mut self.max, notice: &mut self.notice, info };
            panel.show(ui, &self.search, &mut tools);
            let look = panel.take_look();
            let action = panel.take_action();
            self.surfaces = Some(panel);
            if look {
                self.quick_look();
            }
            self.tree_action(action);
        } else {
            self.refresh_shown();
            self.models_ui(ui);
        }
        self.over(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.save_at.is_some() {
            let _ = self.settings.save(); // the window is closing: there is nowhere to tell
        }
        let _ = self.engines.save(); // only caches: the next start reads the files again
        let _ = self.checks.save();
    }
}

/// Opens the window.
pub fn run() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default().with_title(crate::APP).with_inner_size([1360.0, 860.0]).with_min_inner_size([760.0, 480.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(crate::APP, options, Box::new(|cc| Ok(Box::new(OpenLibraryApp::new(cc)))))
}
