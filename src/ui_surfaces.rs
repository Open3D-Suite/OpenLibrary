// SPDX-License-Identifier: GPL-3.0-or-later
//! The tabs of materials, textures and interiors: folders, cards, the options they are sent with and what is
//! known of the selected one. Cards are dragged onto 3ds Max.

use crate::badges;
use crate::favorites::{self, TabView};
use crate::foldertree::TreeAction;
use crate::foldertree::{self, SideTree};
use crate::interiors::{self, Light, Room};
use crate::kit::{self, Kind};
use crate::library::{hidden_folders, norm_key};
use crate::max_session::{MaxReply, MaxSession};
use crate::maxlink::{MaxError, MaxInfo, PROTOCOL};
use crate::open3d_settings::Language;
use crate::settings::Settings;
use crate::surface_prefs::{self as prefs, Mapping};
use crate::surfaces::{self, Aspect, Channel, Material, Res, SurfaceIndex, Texture};
use crate::task::{Task, Wake};
use crate::texts::{Text, fill};
use crate::thumbs::ThumbLoader;
use eframe::egui::{self, Color32, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FOOTER: f32 = 44.0;
/// How many thumbnails become textures in one frame.
const TEXTURES_PER_FRAME: usize = 16;

/// A card of these tabs.
#[derive(Clone)]
pub enum Surface {
    Material(Arc<Material>),
    Texture(Arc<Texture>),
    Room(Arc<Room>),
}

impl Surface {
    fn name(&self) -> &str {
        match self {
            Self::Material(m) => &m.name,
            Self::Texture(t) => &t.name,
            Self::Room(r) => &r.name,
        }
    }

    fn folder(&self) -> &Path {
        match self {
            Self::Material(m) => &m.folder,
            Self::Texture(t) => &t.folder,
            Self::Room(r) => &r.folder,
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Material(m) => m.key(),
            Self::Texture(t) => norm_key(&t.path),
            Self::Room(r) => r.key.clone(),
        }
    }

    fn mtime(&self) -> u64 {
        match self {
            Self::Material(m) => m.mtime,
            Self::Texture(t) => t.mtime,
            Self::Room(r) => r.mtime,
        }
    }

    fn thumb(&self, light: Light) -> Option<PathBuf> {
        match self {
            Self::Material(m) => m.thumb_source(),
            Self::Texture(t) => Some(t.path.clone()),
            Self::Room(r) => r.thumb_source(light).cloned(),
        }
    }

    fn file(&self) -> PathBuf {
        match self {
            Self::Material(m) => m.mat_file.clone().or_else(|| m.pick(Res::Highest).into_values().next()).unwrap_or_else(|| m.folder.clone()),
            Self::Texture(t) => t.path.clone(),
            Self::Room(r) => r.pick(Light::Day, Res::Highest).cloned().unwrap_or_else(|| r.folder.clone()),
        }
    }

    fn favorite_key(&self, roots: &[PathBuf]) -> String {
        favorites::surface_key(roots, &self.key(), self.folder(), self.name())
    }

    /// Its image for the quick look: the maker's sample, or the largest there is.
    fn large(&self, light: Light) -> Option<PathBuf> {
        match self {
            Self::Material(m) if m.preview.is_none() => m.pick(Res::Highest).remove(&Channel::Color).or_else(|| m.thumb_source()),
            _ => self.thumb(light),
        }
    }
}

/// A card, and what is asked of it at every search, worked out once.
pub struct Card {
    item: Surface,
    key: String,
    folder: String,
    favorite: String,
    /// Where the words of a search are looked for.
    hay: String,
}

/// What a scan thread has read.
struct View {
    items: Vec<Arc<Card>>,
    folders: Vec<PathBuf>,
    pixels: HashMap<String, (u32, u32)>,
}

enum Event {
    Seen(usize),
    View(View),
    Done,
}

/// A request on its way.
struct Sent {
    reply: MaxReply,
    kind: SentKind,
}

enum SentKind {
    /// Where a drop goes, and whether the material is in the scene; then it is put.
    Check {
        info: MaxInfo,
        material: Value,
    },
    Put,
}

/// The question about a material the scene has already.
struct Ask {
    info: MaxInfo,
    material: Value,
    route: String,
}

/// What the panel draws with.
pub struct Tools<'a> {
    pub settings: &'a mut Settings,
    pub max: &'a mut MaxSession,
    pub notice: &'a mut Option<String>,
    /// The panel of the selected card is shown.
    pub info: bool,
}

pub struct SurfacePanel {
    pub kind: String,
    lang: Language,
    wake: Wake,
    roots: Vec<PathBuf>,
    measures: Vec<PathBuf>,
    task: Task<Event>,
    scanning: bool,
    seen: usize,
    view: Option<View>,
    hidden: HashSet<String>,
    side: SideTree,
    tab: TabView,
    favorite_count: usize,
    search: String,
    shown: Vec<Arc<Card>>,
    total: usize,
    dirty: bool,
    only_libraries: bool,
    only_sized: bool,
    recent: bool,
    selected: HashSet<String>,
    anchor: usize,
    textures: HashMap<String, Option<TextureHandle>>,
    loaded: Vec<crate::thumbs::Loaded>,
    loader: ThumbLoader,
    dragging: bool,
    look: bool,
    reveal: Option<usize>,
    tree_action: Option<TreeAction>,
    size: [f64; 3],
    sent: Vec<Sent>,
    ask: Option<Ask>,
}

fn read_view(kind: &str, index: &SurfaceIndex) -> View {
    let mut pixels = HashMap::new();
    let items: Vec<Surface> = match kind {
        "materials" => {
            let list = index.materials();
            pixels.extend(list.iter().filter_map(|m| Some((m.key(), index.dims_of(m)?))));
            list.into_iter().map(|m| Surface::Material(Arc::new(m))).collect()
        }
        "textures" => surfaces::build_textures(&index.listing).into_iter().map(|t| Surface::Texture(Arc::new(t))).collect(),
        _ => interiors::build_interiors(&index.listing, &index.roots).into_iter().map(|r| Surface::Room(Arc::new(r))).collect(),
    };
    let card = |item: Surface| {
        let hay = format!("{} {}", item.name(), item.folder().display()).to_lowercase();
        Arc::new(Card { key: item.key(), folder: norm_key(item.folder()), favorite: item.favorite_key(&index.roots), hay, item })
    };
    View { items: items.into_iter().map(card).collect(), folders: index.folders(), pixels }
}

/// "30×60 cm", "2.5×2.5 m".
fn size_text(size: [f64; 3]) -> String {
    let (w, h) = (size[0], size[1]);
    if w >= 100.0 && h >= 100.0 { format!("{}×{} m", trim(w / 100.0), trim(h / 100.0)) } else { format!("{}×{} cm", trim(w), trim(h)) }
}

fn trim(v: f64) -> String {
    format!("{v:.1}").trim_end_matches('0').trim_end_matches('.').to_string()
}

/// "2:1", "1:1.5".
fn ratio_text(aspect: Aspect, ratio: f64) -> String {
    let r = ratio.max(1.0 / ratio);
    let text = if (r - r.round()).abs() < 0.08 { format!("{r:.0}") } else { format!("{r:.1}") };
    match aspect {
        Aspect::Square => "1:1".to_string(),
        Aspect::Wide => format!("{text}:1"),
        Aspect::Tall => format!("1:{text}"),
    }
}

/// Reads the folders on a thread: those the cache knows and did not change are not listed again when `incremental`.
fn read(kind: &str, (roots, measures): (&[PathBuf], &[PathBuf]), data_dir: &Path, incremental: bool, wake: Wake) -> Task<Event> {
    let cache = data_dir.join("library").join(format!("surf_{}", crate::library::cache_name(roots)));
    let mut index = SurfaceIndex::new(roots, measures);
    let kind = kind.to_string();
    Task::start(wake, move |link| {
        if incremental && index.load(&cache) {
            link.send(Event::View(read_view(&kind, &index)));
        }
        if index.scan(&|| link.cancelled(), &mut |n| link.send(Event::Seen(n))) {
            link.send(Event::View(read_view(&kind, &index)));
            let _ = index.save(&cache); // only a cache: without it the next start reads the folders again
            let materials = if kind == "materials" { index.materials() } else { Vec::new() };
            if index.fill_dims(&materials, &|| link.cancelled()) {
                let _ = index.save(&cache);
                if !link.cancelled() {
                    link.send(Event::View(read_view(&kind, &index)));
                }
            }
        }
        link.send(Event::Done);
    })
}

impl SurfacePanel {
    pub fn new(kind: &str, tab: TabView, (roots, measures): (&[PathBuf], &[PathBuf]), data_dir: &Path, lang: Language, wake: Wake) -> Self {
        let task = read(kind, (roots, measures), data_dir, true, wake.clone());
        Self {
            kind: kind.to_string(),
            lang,
            loader: ThumbLoader::new(data_dir.join("thumbs"), 4, wake.clone()),
            wake,
            roots: roots.to_vec(),
            measures: measures.to_vec(),
            task,
            scanning: true,
            seen: 0,
            view: None,
            hidden: HashSet::new(),
            side: SideTree { expanded: roots.iter().map(|r| norm_key(r)).collect(), favorites: tab.opens_favorites, ..SideTree::default() },
            tab,
            favorite_count: 0,
            search: String::new(),
            shown: Vec::new(),
            total: 0,
            loaded: Vec::new(),
            reveal: None,
            tree_action: None,
            dirty: true,
            only_libraries: false,
            only_sized: false,
            recent: false,
            selected: HashSet::new(),
            anchor: 0,
            textures: HashMap::new(),
            dragging: false,
            look: false,
            size: [100.0; 3],
            sent: Vec::new(),
            ask: None,
        }
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    /// For the status bar.
    pub fn status(&self) -> String {
        match (self.scanning, &self.view) {
            (true, None) => fill(self.t(Text::SurfacesReading), &[("count", &self.seen)]),
            (true, _) => self.t(Text::Scanning).to_string(),
            _ => String::new(),
        }
    }

    /// Looks for changes in the folders, or reads them all again.
    pub fn rescan(&mut self, incremental: bool, data_dir: &Path) {
        self.task = read(&self.kind, (&self.roots, &self.measures), data_dir, incremental, self.wake.clone());
        (self.scanning, self.seen) = (true, 0);
    }

    /// What was chosen in the menu of the folders, for the window.
    pub fn take_action(&mut self) -> Option<TreeAction> {
        self.tree_action.take()
    }

    fn poll(&mut self, ctx: &egui::Context, settings: &Settings) {
        for event in self.task.poll() {
            match event {
                Event::Seen(n) => self.seen = n,
                Event::Done => self.scanning = false,
                Event::View(view) => {
                    self.view = Some(view);
                    self.settings_changed(settings);
                }
            }
        }
        if self.textures.len() > crate::thumbs::IN_MEMORY {
            self.textures.clear(); // those on screen come back from the cache
        }
        self.loaded.extend(self.loader.poll());
        let now = self.loaded.len().min(TEXTURES_PER_FRAME);
        for loaded in self.loaded.drain(..now) {
            let texture = loaded.image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(&loaded.key, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
            self.textures.insert(loaded.key, texture);
        }
        if !self.loaded.is_empty() {
            ctx.request_repaint();
        }
    }

    fn refresh(&mut self, settings: &Settings) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let Some(view) = &self.view else { return };
        let words: Vec<String> = self.search.to_lowercase().split_whitespace().map(str::to_string).collect();
        let base = self.side.folder.as_deref().map(norm_key);
        let prefix = base.as_ref().map(|b| format!("{}\\", b.trim_end_matches('\\')));
        let in_folder = |card: &Card| {
            let inside = match (&base, &prefix) {
                (Some(base), Some(prefix)) => card.folder == *base || card.folder.starts_with(prefix),
                _ => true,
            };
            inside && !self.hidden.contains(&card.folder)
        };
        let wanted = |card: &Card| match &card.item {
            Surface::Material(m) => {
                (!self.only_libraries || m.is_library()) && (!self.only_sized || prefs::known_size(settings, &self.roots, m).is_some())
            }
            _ => true,
        };
        let found = |card: &Card| words.iter().all(|w| card.hay.contains(w.as_str()));
        let mut shown: Vec<Arc<Card>> = view.items.iter().filter(|c| in_folder(c) && found(c) && wanted(c)).cloned().collect();
        if self.recent {
            shown.sort_by_key(|c| std::cmp::Reverse(c.item.mtime()));
        }
        let any = favorites::any(settings);
        let liked = |c: &Card| any && favorites::is_favorite(settings, &c.favorite);
        self.favorite_count = if any { view.items.iter().filter(|c| !self.hidden.contains(&c.folder) && liked(c)).count() } else { 0 };
        if self.side.favorites {
            shown.retain(|c| liked(c));
        } else if self.tab.favorites_first && self.favorite_count > 0 {
            shown.sort_by_cached_key(|c| !liked(c)); // stable: the order of before within each part
        }
        self.shown = shown;
        if !self.selected.is_empty() {
            let keys: HashSet<&str> = self.shown.iter().map(|c| c.key.as_str()).collect();
            self.selected.retain(|k| keys.contains(k.as_str()));
        }
        self.anchor = 0;
    }

    fn selection(&self) -> Vec<Surface> {
        if self.selected.is_empty() {
            return Vec::new();
        }
        self.shown.iter().filter(|c| self.selected.contains(&c.key)).map(|c| c.item.clone()).collect()
    }

    /// The material selected, when it is one alone and not a library.
    fn one_material(&self) -> Option<Arc<Material>> {
        match &self.selection()[..] {
            [Surface::Material(m)] if !m.is_library() => Some(m.clone()),
            _ => None,
        }
    }

    fn pixels(&self, material: &Material) -> Option<(u32, u32)> {
        self.view.as_ref()?.pixels.get(&material.key()).copied()
    }

    fn shape(&self, settings: &Settings, material: &Material) -> Option<(Aspect, f64)> {
        if material.is_library() {
            return None;
        }
        prefs::shape(prefs::known_size(settings, &self.roots, material), self.pixels(material))
    }

    // ---- sending to 3ds Max -------------------------------------------------------------------------------

    fn send(&mut self, target: &str, items: &[Surface], info: Option<MaxInfo>, tools: &mut Tools<'_>) {
        let Some(info) = info else {
            *tools.notice = Some(self.t(Text::NoMax).to_string());
            return;
        };
        if info.protocol < PROTOCOL {
            *tools.notice = Some(self.t(Text::BridgeTooOld).to_string());
            return;
        }
        if info.busy {
            *tools.notice = Some(self.t(Text::Busy).to_string());
            return;
        }
        let Some(first) = items.first() else { return };
        let settings = &*tools.settings;
        let request = match first {
            Surface::Room(room) => match prefs::apply_interior(settings, room, target) {
                Some(args) => ("apply_interior", args),
                None => {
                    *tools.notice = Some(self.t(Text::InteriorNoShader).to_string());
                    return;
                }
            },
            Surface::Texture(_) => {
                let list: Vec<Texture> = items.iter().filter_map(|i| if let Surface::Texture(t) = i { Some((**t).clone()) } else { None }).collect();
                ("add_textures", prefs::add_textures(settings, &list, self.size, target))
            }
            Surface::Material(m) if m.is_library() => {
                ("apply_library", json!({"mat_file": m.mat_file.as_ref().map(|p| p.display().to_string()), "target": target}))
            }
            Surface::Material(_) if target == "slate" => {
                let list: Vec<Value> = items
                    .iter()
                    .filter_map(|i| if let Surface::Material(m) = i { Some(m) } else { None })
                    .filter(|m| !m.is_library())
                    .map(|m| prefs::material_args(settings, &self.roots, m, self.pixels(m)))
                    .collect();
                ("apply_materials", prefs::apply_materials(settings, list))
            }
            Surface::Material(m) => {
                let material = prefs::material_args(settings, &self.roots, m, self.pixels(m));
                let reply = tools.max.request(&info, "check_drop", json!({"target": target, "names": [m.name]}), self.wake.clone());
                *tools.notice = Some(fill(self.t(Text::Sending), &[("name", &m.name)]));
                self.sent.extend(reply.map(|reply| Sent { reply, kind: SentKind::Check { info, material } }));
                return;
            }
        };
        *tools.notice = Some(fill(self.t(Text::Sending), &[("name", &first.name())]));
        self.put(&info, request.0, request.1, tools.max);
    }

    fn put(&mut self, info: &MaxInfo, cmd: &'static str, args: Value, max: &mut MaxSession) {
        let reply = max.request(info, cmd, args, self.wake.clone());
        self.sent.extend(reply.map(|reply| Sent { reply, kind: SentKind::Put }));
    }

    /// Goes on with what 3ds Max has answered.
    fn answers(&mut self, tools: &mut Tools<'_>) {
        for sent in std::mem::take(&mut self.sent) {
            match (sent.reply.try_recv(), sent.kind) {
                (Err(std::sync::mpsc::TryRecvError::Empty), kind) => self.sent.push(Sent { reply: sent.reply, kind }),
                (Err(_), _) => {}
                (Ok(Err(error)), _) => {
                    *tools.notice = Some(match error {
                        MaxError::Busy => self.t(Text::Busy).to_string(),
                        error => fill(self.t(Text::SendFailed), &[("error", &error)]),
                    });
                }
                (Ok(Ok(result)), SentKind::Put) => *tools.notice = Some(put_text(&result, &self.kind, self.lang)),
                (Ok(Ok(result)), SentKind::Check { info, material }) => {
                    let route = result.get("route").and_then(Value::as_str).unwrap_or("selection").to_string();
                    let there = result.get("existing").and_then(Value::as_array).is_some_and(|l| !l.is_empty());
                    if there && route != "slate" {
                        self.ask = Some(Ask { info, material, route });
                        continue;
                    }
                    let existing = if route == "slate" { "use" } else { "new" };
                    let args = prefs::apply_material(tools.settings, &material, &route, existing);
                    self.put(&info, "apply_material", args, tools.max);
                }
            }
        }
    }

    fn question(&mut self, ctx: &egui::Context, tools: &mut Tools<'_>) {
        let Some(ask) = &self.ask else { return };
        let name = ask.material.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("openlibrary-existing")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(fill(self.t(Text::MaterialExists), &[("name", &name)]));
            ui.horizontal(|ui| {
                if kit::button(ui, Kind::Primary, None, self.t(Text::UseExisting)).clicked() {
                    choice = Some(Some("use"));
                }
                if kit::button(ui, Kind::Action, None, self.t(Text::MakeNew)).clicked() {
                    choice = Some(Some("new"));
                }
                if kit::button(ui, Kind::Action, None, self.t(Text::Cancel)).clicked() {
                    choice = Some(None);
                }
            });
        });
        if let (Some(choice), Some(ask)) = (choice, self.ask.take_if(|_| choice.is_some())) {
            if let Some(existing) = choice {
                let args = prefs::apply_material(tools.settings, &ask.material, &ask.route, existing);
                self.put(&ask.info, "apply_material", args, tools.max);
            } else {
                *tools.notice = Some(self.t(Text::Cancelled).to_string());
            }
        }
    }

    fn drag(&mut self, ctx: &egui::Context, tools: &mut Tools<'_>) {
        if !self.dragging {
            return;
        }
        let (released, pointer) = ctx.input(|i| (i.pointer.any_released(), i.pointer.latest_pos()));
        let items = self.selection();
        if !released {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            let light = prefs::light(tools.settings);
            let texture = items.first().and_then(|i| self.textures.get(&thumb_key(i, light))).and_then(Option::as_ref);
            if let (Some(pos), Some(texture)) = (pointer, texture) {
                let size = texture.size_vec2() * (72.0 / texture.size_vec2().max_elem());
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("openlibrary-surface-drag")));
                painter.image(texture.id(), Rect::from_min_size(pos + Vec2::splat(12.0), size), whole(), Color32::from_white_alpha(210));
            }
            return;
        }
        self.dragging = false;
        if crate::platform::process_under_cursor() == Some(std::process::id()) {
            return; // dropped back on OpenLibrary
        }
        match tools.max.under_cursor() {
            Some(info) => {
                tools.max.last_pid = Some(info.pid);
                self.send("cursor", &items, Some(info), tools);
            }
            None => *tools.notice = Some(self.t(Text::DropOnMax).to_string()),
        }
    }

    // ---- drawing ------------------------------------------------------------------------------------------

    /// Draws the tab.
    pub fn show(&mut self, ui: &mut egui::Ui, search: &str, tools: &mut Tools<'_>) {
        let ctx = ui.ctx().clone();
        if self.search != search {
            self.search = search.to_string();
            self.dirty = true;
        }
        self.poll(&ctx, tools.settings);
        self.answers(tools);
        self.refresh(tools.settings);
        let p = kit::pal(ui);
        let side = egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin { left: 12, right: 8, top: 12, bottom: 8 });
        egui::Panel::left("tree").frame(side).resizable(true).default_size(230.0).min_size(170.0).show(ui, |ui| self.tree(ui, tools));
        if tools.info {
            let frame = egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::same(14));
            egui::Panel::right("inspector").frame(frame).resizable(true).default_size(310.0).min_size(250.0).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("surface-detail").auto_shrink(false).show(ui, |ui| self.detail(ui, tools));
            });
        }
        let main = egui::Frame::new().fill(p.bg).inner_margin(egui::Margin { left: 18, right: 10, top: 12, bottom: 4 });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            self.title_row(ui, tools);
            ui.add_space(4.0);
            self.option_row(ui, tools);
            ui.add_space(6.0);
            self.gallery(ui, tools);
        });
        self.drag(&ctx, tools);
        self.question(&ctx, tools);
    }

    fn tree(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        kit::caption(ui, self.t(Text::Folders));
        ui.add_space(2.0);
        let changed = self.side.show(ui, "surfaces", tools.settings, self.lang, self.tab, (self.favorite_count, self.total));
        if changed.settings {
            self.save_settings(tools);
            self.settings_changed(tools.settings);
        }
        if changed.shown {
            self.dirty = true;
            self.loader.clear_queue();
        }
        self.tree_action = changed.action.or(self.tree_action.take());
    }

    /// Works out again what is hidden, the rows of the tree and what is shown.
    pub fn settings_changed(&mut self, settings: &Settings) {
        self.dirty = true;
        let Some(view) = &self.view else { return };
        let (names, paths) = settings.hidden();
        self.hidden = hidden_folders(&view.folders, &self.roots, &names, &paths);
        let items = view.items.iter().map(|c| c.folder.as_str()).filter(|f| !self.hidden.contains(*f));
        self.total = items.clone().count();
        let totals = foldertree::totals(&self.roots, items);
        // a folder per material does not help to find one: only those with two or more are shown
        let least = if self.kind == "materials" { 2 } else { 1 };
        let folders: Vec<PathBuf> = view.folders.iter().filter(|f| totals.get(&norm_key(f)).is_some_and(|n| *n >= least)).cloned().collect();
        self.side.folders = foldertree::rows(&self.roots, &folders, &self.hidden);
        for folder in &mut self.side.folders {
            folder.count = Some(totals.get(&folder.key).copied().unwrap_or(0));
        }
    }

    /// The cards for the quick look, and which one is selected.
    pub fn looks(&self, settings: &Settings) -> (Vec<crate::quicklook::Look>, Option<usize>) {
        let light = prefs::light(settings);
        let at = self.shown.iter().position(|c| self.selected.contains(&c.key));
        (self.shown.iter().map(|c| (c.item.name().to_string(), c.item.large(light))).collect(), at)
    }

    /// Selects a card alone, and brings it into view.
    pub fn select_only(&mut self, at: usize) {
        if let Some(card) = self.shown.get(at).filter(|c| self.selected.len() != 1 || !self.selected.contains(&c.key)) {
            self.selected = HashSet::from([card.key.clone()]);
            (self.anchor, self.reveal) = (at, Some(at));
        }
    }

    /// Whether a card was double-clicked, for the quick look.
    pub fn take_look(&mut self) -> bool {
        std::mem::take(&mut self.look)
    }

    fn toggle(&mut self, keys: &[String], tools: &mut Tools<'_>) {
        favorites::toggle(tools.settings, keys);
        self.save_settings(tools);
        self.dirty = true;
    }

    /// A choice kept in the settings as a text.
    fn option_combo(&self, ui: &mut egui::Ui, tools: &mut Tools<'_>, key: &str, tip: Text, options: &[(&'static str, &str)]) {
        let saved = prefs::option(tools.settings, key).and_then(Value::as_str).unwrap_or_default().to_string();
        let mut current = options.iter().map(|(v, _)| *v).find(|v| *v == saved).or_else(|| options.first().map(|(v, _)| *v)).unwrap_or_default();
        let changed = ui.scope(|ui| kit::combo(ui, key, options, &mut current));
        changed.response.on_hover_text(self.t(tip));
        if changed.inner {
            self.save(tools, key, current.into());
        }
    }

    fn save(&self, tools: &mut Tools<'_>, key: &str, value: Value) {
        prefs::set_option(tools.settings, key, value);
        self.save_settings(tools);
    }

    fn save_settings(&self, tools: &mut Tools<'_>) {
        if let Err(error) = tools.settings.save() {
            *tools.notice = Some(fill(self.t(Text::SettingsError), &[("error", &error)]));
        }
    }

    /// The folder shown and how many cards, the filters, and what is done with those selected.
    fn title_row(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let p = kit::pal(ui);
        let title = if !self.search.trim().is_empty() {
            fill(self.t(Text::ResultsOf), &[("text", &self.search.trim())])
        } else if self.side.favorites {
            self.t(Text::Favorites).to_string()
        } else {
            self.side.folder.as_deref().map_or_else(|| self.t(Text::AllFolders).to_string(), file_name)
        };
        let count = fill(self.t(Text::SurfacesShown), &[("count", &self.shown.len())]);
        let mut target = None;
        ui.horizontal(|ui| {
            ui.set_height(32.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add_enabled_ui(!self.selected.is_empty(), |ui| {
                    let apply = self.t(if self.kind == "textures" { Text::QuickMaterial } else { Text::Apply });
                    if kit::button(ui, Kind::Primary, Some("check"), apply).on_hover_text(self.t(Text::MenuToSelection)).clicked() {
                        target = Some("selection");
                    }
                    if self.kind == "interiors" {
                        let window = kit::button(ui, Kind::Accent, None, self.t(Text::MakeWindow));
                        if window.on_hover_text(self.t(Text::MenuWindow)).clicked() {
                            target = Some("plane");
                        }
                    }
                    if kit::button(ui, Kind::Action, None, self.t(Text::ToSlate)).on_hover_text(self.t(Text::MenuToSlate)).clicked() {
                        target = Some("slate");
                    }
                });
                if self.kind == "materials" {
                    let balls = prefs::option(tools.settings, "balls").and_then(Value::as_bool).unwrap_or(true);
                    if kit::chip(ui, self.t(Text::ChipBalls), balls).on_hover_text(self.t(Text::OptBalls)).clicked() {
                        self.save(tools, "balls", (!balls).into());
                    }
                }
                let options = [(false, self.t(Text::SortName)), (true, self.t(Text::SortRecent))];
                self.dirty |= kit::combo(ui, "surface-sort", &options, &mut self.recent);
                if self.kind == "materials" {
                    if kit::chip(ui, self.t(Text::ChipSized), self.only_sized).on_hover_text(self.t(Text::OptOnlySized)).clicked() {
                        (self.only_sized, self.dirty) = (!self.only_sized, true);
                    }
                    if kit::chip(ui, ".mat", self.only_libraries).on_hover_text(self.t(Text::OptOnlyLibraries)).clicked() {
                        (self.only_libraries, self.dirty) = (!self.only_libraries, true);
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let room = (ui.available_width() - 90.0).max(40.0);
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(room, 30.0), Sense::hover());
                    let at = rect.left_center();
                    let used = kit::elided(ui.painter(), at, egui::Align2::LEFT_CENTER, &title, kit::strong(20.0), p.text, room);
                    let at = egui::pos2(used.right() + 10.0, rect.center().y + 3.0);
                    ui.painter().text(at, egui::Align2::LEFT_CENTER, count, egui::FontId::proportional(13.0), p.muted);
                });
            });
        });
        if let Some(target) = target {
            let (items, info) = (self.selection(), tools.max.target());
            self.send(target, &items, info, tools);
        }
    }

    /// What goes to 3ds Max with the cards.
    fn option_row(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        ui.horizontal_wrapped(|ui| {
            ui.set_row_height(28.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            let engines = [("auto", self.t(Text::EngineScene)), ("corona", "Corona"), ("vray", "V-Ray"), ("max", "Physical")];
            self.option_combo(ui, tools, "engine", Text::TipEngine, &engines);
            if self.kind != "textures" {
                let mut res = prefs::resolution(tools.settings);
                let mut list = vec![(Res::Highest, self.t(Text::ResHighest)), (Res::Lowest, self.t(Text::ResLowest))];
                list.extend([(Res::K(8), "8K"), (Res::K(4), "4K"), (Res::K(2), "2K"), (Res::K(1), "1K")]);
                let changed = ui.scope(|ui| kit::combo(ui, "res", &list, &mut res));
                changed.response.on_hover_text(self.t(Text::TipResolution));
                if changed.inner {
                    prefs::set_resolution(tools.settings, res);
                    self.save_settings(tools);
                }
            }
            match self.kind.as_str() {
                "materials" => self.material_options(ui, tools),
                "textures" => self.texture_options(ui, tools),
                _ => {
                    let lights = [("day", self.t(Text::LightDay)), ("night", self.t(Text::LightNight))];
                    self.option_combo(ui, tools, "light", Text::OptLight, &lights);
                    let mut depth = prefs::option(tools.settings, "depth").and_then(Value::as_i64).unwrap_or(-1);
                    let list = [(-1, self.t(Text::DepthAuto)), (1, "×1"), (3, "×1.5"), (4, "×2"), (5, "×3"), (6, "×4"), (7, "×5")];
                    let changed = ui.scope(|ui| kit::combo(ui, "depth", &list, &mut depth));
                    changed.response.on_hover_text(self.t(Text::TipDepth));
                    if changed.inner {
                        self.save(tools, "depth", depth.into());
                    }
                }
            }
        });
    }

    fn texture_options(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let roles = [("auto", self.t(Text::RoleAuto)), ("color", self.t(Text::RoleColor)), ("data", self.t(Text::RoleData))];
        self.option_combo(ui, tools, "role", Text::OptRole, &roles);
        let mappings = Mapping::ALL.map(|m| (m.key(), mapping_text(m, self.lang)));
        self.option_combo(ui, tools, "mapping", Text::TipMapping, &mappings);
        let mapping = prefs::option(tools.settings, "mapping").and_then(Value::as_str).and_then(Mapping::from_key);
        size_fields(ui, &mut self.size, mapping.unwrap_or(Mapping::Uv).axes());
    }

    /// The mapping and the real size of the selected material, kept for it as they are changed.
    fn material_options(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let one = self.one_material();
        match &one {
            None => {
                let mut mapping = Mapping::Uv;
                let mut size = self.size;
                ui.add_enabled_ui(false, |ui| {
                    kit::combo(ui, "material-mapping", &[(Mapping::Uv, mapping_text(Mapping::Uv, self.lang))], &mut mapping);
                    size_fields(ui, &mut size, 0);
                })
                .response
                .on_disabled_hover_text(self.t(Text::TipSizeSelect));
            }
            Some(material) => self.one_options(ui, material, tools),
        }
        let on = prefs::option(tools.settings, "displacement").and_then(Value::as_bool).unwrap_or(false);
        if kit::chip(ui, self.t(Text::ChipDisplacement), on).on_hover_text(self.t(Text::OptDisplacement)).clicked() {
            self.save(tools, "displacement", (!on).into());
        }
    }

    fn one_options(&mut self, ui: &mut egui::Ui, material: &Material, tools: &mut Tools<'_>) {
        let aspect = self.shape(tools.settings, material).map(|(a, _)| a);
        let mut mapping = prefs::mapping_of(tools.settings, &self.roots, material, aspect);
        let square = aspect.is_none_or(|a| a == Aspect::Square);
        let list: Vec<(Mapping, &str)> = Mapping::ALL
            .into_iter()
            .filter(|m| square || *m != Mapping::Triplanar) // a triplanar of a long texture would squash it
            .map(|m| (m, mapping_text(m, self.lang)))
            .collect();
        let changed = ui.scope(|ui| kit::combo(ui, "material-mapping", &list, &mut mapping));
        changed.response.on_hover_text(self.t(Text::TipMapping));
        if changed.inner {
            prefs::remember(tools.settings, prefs::MAPPINGS, &self.roots, material, Some(mapping.key().into()));
            self.save_settings(tools);
        }
        let mut size = prefs::size_of(tools.settings, &self.roots, material);
        let (changed, ended) = size_fields(ui, &mut size, mapping.axes());
        if changed {
            let same = material.size.is_some_and(|(w, h)| size == [w, h, h]);
            let value = (!same).then(|| json!(size.map(|v| (v * 100.0).round() / 100.0)));
            prefs::remember(tools.settings, prefs::SIZES, &self.roots, material, value);
        }
        if ended {
            self.save_settings(tools);
        }
        let saved = prefs::saved_size(tools.settings, &self.roots, material).is_some();
        if saved && kit::icon_button(ui, "refresh", self.t(Text::SizeForget), false).clicked() {
            self.forget_size(material, tools);
        }
    }

    fn forget_size(&self, material: &Material, tools: &mut Tools<'_>) {
        prefs::remember(tools.settings, prefs::SIZES, &self.roots, material, None);
        self.save_settings(tools);
    }

    fn detail(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let selection = self.selection();
        let [item] = &selection[..] else {
            if selection.len() > 1 {
                ui.label(egui::RichText::new(fill(self.t(Text::SelectedAny), &[("count", &selection.len())])).font(kit::strong(15.0)));
            }
            ui.weak(self.t(match self.kind.as_str() {
                "materials" => Text::HintMaterials,
                "textures" => Text::HintTextures,
                _ => Text::HintInteriors,
            }));
            return;
        };
        let light = prefs::light(tools.settings);
        if let Some(Some(texture)) = self.textures.get(&thumb_key(item, light)) {
            let size = texture.size_vec2() * (ui.available_width() / texture.size_vec2().max_elem());
            let image = ui.add(egui::Image::new((texture.id(), size)).sense(Sense::click()));
            self.look |= image.on_hover_text(self.t(Text::LookHint)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        }
        ui.add(egui::Label::new(egui::RichText::new(item.name()).font(kit::strong(16.0))).wrap());
        ui.add(egui::Label::new(egui::RichText::new(item.folder().display().to_string()).small().weak()).wrap());
        ui.add_space(6.0);
        match item {
            Surface::Material(m) if m.is_library() => {
                kit::caption(ui, self.t(Text::CapLibrary));
                ui.label(m.mat_file.as_deref().map(file_name).unwrap_or_default());
                ui.weak(self.t(Text::LibraryHint));
            }
            Surface::Material(m) => self.material_detail(ui, m, tools.settings),
            Surface::Texture(t) => {
                kit::caption(ui, self.t(Text::CapTexture));
                ui.label(file_name(&t.path));
                let role = surfaces::guess_role(&file_name(&t.path));
                ui.weak(fill(self.t(Text::TextureRole), &[("role", &self.t(if role == "color" { Text::RoleColor } else { Text::RoleData }))]));
            }
            Surface::Room(r) => self.room_detail(ui, r, light, tools.settings),
        }
        ui.add_space(10.0);
        let width = ui.available_width();
        if kit::sized_button(ui, Kind::Action, Some("folder"), self.t(Text::FolderReveal), width).clicked() {
            crate::platform::reveal(&item.file());
        }
    }

    fn room_detail(&self, ui: &mut egui::Ui, room: &Room, light: Light, settings: &Settings) {
        kit::caption(ui, self.t(Text::CapWindow));
        ui.label(self.room_text(room));
        ui.weak(self.t(Text::RoomPlane));
        ui.add_space(6.0);
        kit::caption(ui, self.t(Text::MapsToLoad));
        ui.label(room.pick(light, prefs::resolution(settings)).map(|p| file_name(p)).unwrap_or_default());
        ui.add_space(6.0);
        kit::caption(ui, "Shader OSL");
        match &room.osl {
            Some(osl) => ui.label(file_name(osl)),
            None => ui.colored_label(kit::pal(ui).warn, self.t(Text::InteriorNoShader)),
        };
    }

    fn room_text(&self, room: &Room) -> String {
        let values: [(&str, &dyn std::fmt::Display); 2] = [("width", &trim(room.size.width())), ("height", &trim(interiors::HEIGHT))];
        fill(self.t(Text::RoomWindow), &values)
    }

    fn material_detail(&self, ui: &mut egui::Ui, material: &Material, settings: &Settings) {
        kit::caption(ui, self.t(Text::CapSize));
        let saved = prefs::saved_size(settings, &self.roots, material).is_some();
        match prefs::known_size(settings, &self.roots, material) {
            Some(size) => {
                ui.label(format!("X {} · Y {} · Z {} cm", trim(size[0]), trim(size[1]), trim(size[2])));
                ui.label(egui::RichText::new(self.t(if saved { Text::SizeSaved } else { Text::SizeFromName })).small().weak());
            }
            None => {
                ui.weak(self.t(Text::SizeUnknown));
            }
        }
        let shape = self.shape(settings, material);
        if let Some((aspect, ratio)) = shape {
            let text = self.t(match aspect {
                Aspect::Square => Text::ShapeSquare,
                Aspect::Wide => Text::ShapeWide,
                Aspect::Tall => Text::ShapeTall,
            });
            ui.label(egui::RichText::new(format!("{text} · {}", ratio_text(aspect, ratio))).weak());
        }
        let mapping = prefs::mapping_of(settings, &self.roots, material, shape.map(|(a, _)| a));
        ui.label(egui::RichText::new(format!("{}: {}", self.t(Text::OptMapping), mapping_text(mapping, self.lang))).weak());
        let resolutions: Vec<String> = material.maps.keys().filter(|r| **r > 0).map(|r| format!("{r}K")).collect();
        if !resolutions.is_empty() {
            ui.add_space(6.0);
            kit::caption(ui, self.t(Text::CapResolutions));
            ui.label(resolutions.join(" · "));
        }
        ui.add_space(6.0);
        kit::caption(ui, self.t(Text::MapsToLoad));
        for (channel, file) in material.pick(prefs::resolution(settings)) {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                badges::channel(ui.painter(), rect, channel);
                ui.label(egui::RichText::new(channel_text(channel).get(self.lang)).font(kit::strong(13.0)));
                if channel == Channel::Ao {
                    ui.weak(self.t(Text::NotWired));
                }
            });
            ui.add(egui::Label::new(egui::RichText::new(file_name(&file)).small().weak()).wrap());
        }
        if material.tiles > 0 {
            ui.weak(fill(self.t(Text::Planks), &[("count", &material.tiles)]));
        }
    }

    /// Gives this size to the materials of the folder that have none.
    fn size_to_folder(&mut self, material: &Material, size: [f64; 3], tools: &mut Tools<'_>) {
        let folder = norm_key(&material.folder);
        let mut count = 0;
        for card in self.view.iter().flat_map(|v| &v.items) {
            if let Surface::Material(other) = &card.item
                && !other.is_library()
                && norm_key(&other.folder) == folder
                && prefs::known_size(tools.settings, &self.roots, other).is_none()
            {
                prefs::remember(tools.settings, prefs::SIZES, &self.roots, other, Some(json!(size)));
                count += 1;
            }
        }
        self.save_settings(tools);
        *tools.notice = Some(fill(self.t(Text::SizeToFolderDone), &[("size", &size_text(size)), ("count", &count)]));
    }

    fn gallery(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        if self.roots.is_empty() || self.shown.is_empty() {
            let text = if self.roots.is_empty() {
                self.t(Text::NoFolders).to_string()
            } else if self.scanning && self.view.is_none() {
                self.t(Text::Scanning).to_string()
            } else if !self.search.trim().is_empty() {
                fill(self.t(Text::NothingMatches), &[("text", &self.search.trim())])
            } else if self.side.favorites {
                self.t(Text::NoFavorites).to_string()
            } else {
                self.t(Text::NoSurfaces).to_string()
            };
            ui.centered_and_justified(|ui| ui.weak(text));
            return;
        }
        let thumb = tools.settings.data.get("thumb_size").and_then(Value::as_f64).unwrap_or(160.0).clamp(80.0, 400.0) as f32;
        let bar = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin + ui.spacing().scroll.bar_outer_margin;
        let grid = crate::app::Grid::new(ui.available_width() - bar, thumb, true);
        let rows = self.shown.len().div_ceil(grid.columns);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut area = egui::ScrollArea::vertical().id_salt("surface-gallery").auto_shrink(false);
        if let Some(at) = self.reveal.take() {
            let top = (at / grid.columns) as f32 * grid.cell.y;
            area = area.vertical_scroll_offset((top - ui.available_height() / 2.0 + grid.cell.y / 2.0).max(0.0));
        }
        let mut seen = 0..0;
        area.show_rows(ui, grid.cell.y, rows, |ui, range| {
            seen = range.start * grid.columns..range.end * grid.columns;
            for row in range {
                ui.horizontal(|ui| {
                    for at in row * grid.columns..((row + 1) * grid.columns).min(self.shown.len()) {
                        self.card(ui, at, grid, tools);
                    }
                });
            }
            let rest = ui.available_rect_before_wrap();
            if rest.is_positive() && ui.interact(rest, ui.scope_id().with("empty"), Sense::click()).clicked() {
                self.selected.clear();
            }
        });
        if self.loader.waiting() <= 4 {
            let light = prefs::light(tools.settings);
            let count = seen.len();
            for card in self.shown.iter().skip(seen.end).take(count * 2) {
                let key = thumb_key(&card.item, light);
                if !self.textures.contains_key(&key)
                    && let Some(src) = card.item.thumb(light)
                {
                    self.loader.prefetch(&key, &src);
                }
            }
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui, card: &Card, tools: &mut Tools<'_>) {
        ui.set_min_width(260.0);
        let items = self.selection();
        let keys: Vec<String> = self.shown.iter().filter(|c| self.selected.contains(&c.key)).map(|c| c.favorite.clone()).collect();
        let all = keys.iter().all(|k| favorites::is_favorite(tools.settings, k));
        if ui.button(self.t(if all { Text::FavoriteRemove } else { Text::FavoriteAdd })).clicked() {
            self.toggle(&keys, tools);
        }
        ui.separator();
        let mut entries = vec![("selection", if self.kind == "textures" { Text::MenuQuickMaterial } else { Text::MenuToSelection })];
        if self.kind == "interiors" {
            entries.push(("plane", Text::MenuWindow));
        }
        entries.push(("slate", Text::MenuToSlate));
        for (target, text) in entries {
            if ui.button(self.t(text)).clicked() {
                let info = tools.max.target();
                self.send(target, &items, info, tools);
            }
        }
        ui.separator();
        if let Some(material) = self.one_material() {
            if prefs::saved_size(tools.settings, &self.roots, &material).is_some() && ui.button(self.t(Text::SizeForget)).clicked() {
                self.forget_size(&material, tools);
            }
            if ui.button(self.t(Text::SizeToFolder)).clicked() {
                let size = prefs::size_of(tools.settings, &self.roots, &material);
                self.size_to_folder(&material, size, tools);
            }
            ui.separator();
        }
        if ui.button(self.t(Text::FolderReveal)).clicked() {
            crate::platform::reveal(&card.item.file());
        }
        match &card.item {
            Surface::Material(m) if !m.is_library() => {
                if ui.button(self.t(Text::CopyMaps)).clicked() {
                    let maps = m.pick(prefs::resolution(tools.settings)).into_values();
                    let list: Vec<String> = maps.map(|p| p.display().to_string()).collect();
                    ui.ctx().copy_text(list.join("\n"));
                }
            }
            item => {
                if ui.button(self.t(Text::CopyPath)).clicked() {
                    let file = match item {
                        Surface::Room(r) => r.pick(prefs::light(tools.settings), prefs::resolution(tools.settings)).cloned().unwrap_or_default(),
                        other => other.file(),
                    };
                    ui.ctx().copy_text(file.display().to_string());
                }
            }
        }
    }

    fn select(&mut self, at: usize, modifiers: egui::Modifiers) {
        let Some(key) = self.shown.get(at).map(|c| c.key.clone()) else { return };
        if modifiers.shift {
            let range = self.anchor.min(at)..=self.anchor.max(at);
            self.selected = self.shown.get(range).unwrap_or_default().iter().map(|c| c.key.clone()).collect();
            return;
        }
        self.anchor = at;
        if !modifiers.command {
            self.selected.clear();
        }
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, at: usize, grid: crate::app::Grid, tools: &mut Tools<'_>) {
        let (cell, response) = ui.allocate_exact_size(grid.cell, Sense::click_and_drag());
        let Some(card) = self.shown.get(at).cloned() else { return };
        if !ui.is_rect_visible(cell) {
            return;
        }
        let rect = cell.shrink(grid.margin);
        let selected = self.selected.contains(&card.key);
        let hovered = response.hovered();
        let p = kit::pal(ui);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, if selected || hovered { p.surface_hi } else { p.surface });
        let area = Rect::from_min_size(rect.min, Vec2::splat(grid.side));
        self.paint_image(&painter, area, &card.item, tools.settings, p);
        self.badges(&painter, area, &card.item, tools.settings);
        let foot = Rect::from_min_max(egui::pos2(rect.left() + 10.0, area.bottom()), rect.max - Vec2::new(10.0, 4.0));
        let color = if selected { p.accent } else { p.text };
        let name = egui::pos2(foot.left(), foot.top() + 13.0);
        kit::elided(&painter, name, egui::Align2::LEFT_CENTER, card.item.name(), kit::strong(13.0), color, foot.width() - 24.0);
        let sub = self.subtitle(&card.item, tools.settings);
        let under = egui::pos2(foot.left(), foot.bottom() - 10.0);
        kit::elided(&painter, under, egui::Align2::LEFT_CENTER, &sub, egui::FontId::proportional(11.5), p.muted, foot.width());
        let border = match (selected, hovered) {
            (true, _) => Stroke::new(2.5, p.accent),
            (false, true) => Stroke::new(1.2, p.border_hi),
            _ => Stroke::new(1.0, p.border),
        };
        ui.painter().rect_stroke(rect, 0.0, border, egui::StrokeKind::Inside);
        let on = favorites::is_favorite(tools.settings, &card.favorite);
        let heart = Rect::from_min_size(egui::pos2(rect.right() - 30.0, rect.bottom() - FOOTER + 2.0), Vec2::splat(24.0));
        if (on || hovered || selected) && crate::app::heart(ui, &response, heart, on, false) {
            self.toggle(std::slice::from_ref(&card.favorite), tools);
            return;
        }
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
            self.selected = HashSet::from([card.key.clone()]);
            self.look = true;
        } else if response.clicked() {
            self.select(at, ui.input(|i| i.modifiers));
        }
        response.context_menu(|ui| self.menu(ui, &card, tools));
    }

    /// Under the name: the size that is known, and the folder.
    fn subtitle(&self, item: &Surface, settings: &Settings) -> String {
        let folder = file_name(item.folder());
        match item {
            Surface::Material(m) if m.is_library() => self.t(Text::CapLibrary).to_string(),
            Surface::Material(m) => match prefs::known_size(settings, &self.roots, m) {
                Some(size) => format!("{} · {folder}", size_text(size)),
                None => folder,
            },
            Surface::Room(r) => self.room_text(r),
            Surface::Texture(_) => folder,
        }
    }

    fn paint_image(&mut self, painter: &egui::Painter, area: Rect, item: &Surface, settings: &Settings, p: &kit::Palette) {
        let light = prefs::light(settings);
        let key = thumb_key(item, light);
        let waiting = match self.textures.get(&key) {
            Some(Some(texture)) => {
                let balls = prefs::option(settings, "balls").and_then(Value::as_bool).unwrap_or(true);
                match item {
                    Surface::Material(m) if m.preview.is_none() && balls => badges::ball(painter, area, texture),
                    Surface::Texture(_) => crate::app::picture(painter, area, texture),
                    _ => cover(painter, area, texture),
                }
                return;
            }
            Some(None) => false,
            None => match item.thumb(light) {
                Some(src) => {
                    self.loader.request(&key, &src);
                    true
                }
                None => {
                    self.textures.insert(key, None);
                    false
                }
            },
        };
        let text = match item {
            _ if waiting => self.t(Text::Loading),
            Surface::Material(m) if m.is_library() => ".mat",
            _ => self.t(Text::NoPreview),
        };
        painter.text(area.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.5), p.faint);
    }

    fn badges(&self, painter: &egui::Painter, area: Rect, item: &Surface, settings: &Settings) {
        let size = badges::size(area.width());
        let top = area.top() + 7.0;
        let mut x = area.left() + 7.0;
        let top_res = match item {
            Surface::Material(m) => m.top_res(),
            Surface::Room(r) => r.top_res(),
            Surface::Texture(_) => 0,
        };
        if top_res > 0 {
            x = badges::resolution(painter, egui::pos2(x, top), size, top_res) + 4.0;
        }
        let bottom = area.bottom() - size - 6.0;
        match item {
            Surface::Material(m) if !m.is_library() => {
                if let Some((aspect, ratio)) = self.shape(settings, m) {
                    badges::shape(painter, egui::pos2(area.right() - 7.0, top), size, aspect, &ratio_text(aspect, ratio));
                }
                let mut x = area.left() + 7.0;
                for channel in m.channels().into_iter().filter(|c| *c != Channel::Refl) {
                    if x + size > area.right() - 4.0 {
                        break;
                    }
                    badges::channel(painter, Rect::from_min_size(egui::pos2(x, bottom), Vec2::splat(size)), channel);
                    x += size + 3.0;
                }
            }
            Surface::Room(r) => {
                let p = kit::palette(true);
                badges::pill(painter, egui::pos2(x, top), size, r.size.code(), (p.accent, p.accent_text));
                let mut x = area.left() + 7.0;
                for light in r.exrs.keys() {
                    let (text, back, ink) = match light {
                        Light::Day => (Text::LightDay, Color32::from_rgb(0xE0, 0xB5, 0x3A), Color32::from_gray(30)),
                        Light::Night => (Text::LightNight, Color32::from_rgb(0x3B, 0x5B, 0xA8), Color32::WHITE),
                    };
                    x = badges::pill(painter, egui::pos2(x, bottom), size, self.t(text), (back, ink)) + 4.0;
                }
            }
            _ => {}
        }
    }
}

fn channel_text(channel: Channel) -> Text {
    match channel {
        Channel::Color => Text::ChannelColor,
        Channel::Rough => Text::ChannelRough,
        Channel::Gloss => Text::ChannelGloss,
        Channel::Refl => Text::ChannelRefl,
        Channel::Metal => Text::ChannelMetal,
        Channel::Normal => Text::ChannelNormal,
        Channel::Bump => Text::ChannelBump,
        Channel::Disp => Text::ChannelDisp,
        Channel::Ao => Text::ChannelAo,
        Channel::Opacity => Text::ChannelOpacity,
        Channel::Emission => Text::ChannelEmission,
        Channel::Transl => Text::ChannelTransl,
    }
}

fn thumb_key(item: &Surface, light: Light) -> String {
    match (item, light) {
        (Surface::Room(_), Light::Night) => format!("{}|night", item.key()),
        _ => item.key(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn mapping_text(mapping: Mapping, lang: Language) -> &'static str {
    match mapping {
        Mapping::Uv => Text::MapUv.get(lang),
        Mapping::RealWorld => "Real World",
        Mapping::Box => "UVW Map",
        Mapping::Triplanar => "Triplanar",
    }
}

/// X, Y, Z in cm, the first `axes` of them enabled: whether one changed, and whether the change has ended.
fn size_fields(ui: &mut egui::Ui, size: &mut [f64; 3], axes: usize) -> (bool, bool) {
    let (mut changed, mut ended) = (false, false);
    ui.horizontal(|ui| {
        for (i, (label, value)) in ["X", "Y", "Z"].iter().zip(size.iter_mut()).enumerate() {
            ui.add_enabled_ui(i < axes, |ui| {
                ui.weak(*label);
                let response = ui.add(egui::DragValue::new(value).speed(1.0).range(0.1..=100_000.0).max_decimals(2));
                changed |= response.changed();
                ended |= response.drag_stopped() || response.lost_focus();
            });
        }
        ui.weak("cm");
    });
    (changed, ended)
}

fn whole() -> Rect {
    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// The image over the whole area, cut where it does not fit.
fn cover(painter: &egui::Painter, area: Rect, texture: &TextureHandle) {
    let size = texture.size_vec2();
    let part = Vec2::new(area.width() / size.x, area.height() / size.y);
    let seen = part / part.max_elem();
    let uv = Rect::from_center_size(egui::pos2(0.5, 0.5), seen);
    painter.image(texture.id(), area, uv, Color32::WHITE);
}

/// What 3ds Max did, in words.
fn put_text(result: &Value, kind: &str, lang: Language) -> String {
    let get = |key: &str| result.get(key).and_then(Value::as_u64);
    let name = result.get("name").and_then(Value::as_str).unwrap_or_default();
    let count = get("count").unwrap_or(0);
    let place = result.get("where").and_then(Value::as_str).unwrap_or_default();
    let values: [(&str, &dyn std::fmt::Display); 2] = [("name", &name), ("count", &count)];
    match place {
        "objects" | "objects+slate" => {
            let mut text = fill(Text::PutObjects.get(lang), &values);
            if place == "objects+slate" {
                text = format!("{text} {}", Text::PutAlsoSlate.get(lang));
            }
            match get("library") {
                Some(n) if n > 1 => format!("{text} {}", fill(Text::PutLibrary.get(lang), &[("count", &n)])),
                _ => text,
            }
        }
        "plane" => fill(Text::PutWindow.get(lang), &values),
        "slate" if count == 0 => Text::PutNoSlate.get(lang).to_string(),
        "slate" => match get("added") {
            Some(added) if added < count => fill(Text::PutSlateFound.get(lang), &[("count", &(count - added))]),
            _ => fill(Text::PutSlate.get(lang), &[("count", &count)]),
        },
        _ if kind == "interiors" => Text::PutNoWindow.get(lang).to_string(),
        _ => Text::PutNothing.get(lang).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_shapes_are_easy_to_read() {
        assert_eq!(size_text([250.0, 250.0, 250.0]), "2.5×2.5 m");
        assert_eq!(size_text([30.0, 60.0, 60.0]), "30×60 cm");
        assert_eq!(ratio_text(Aspect::Wide, 2.04), "2:1");
        assert_eq!(ratio_text(Aspect::Tall, 0.667), "1:1.5");
    }

    #[test]
    fn what_3ds_max_did_is_told() {
        let text = |result: Value| put_text(&result, "materials", Language::English);
        assert!(text(json!({"where": "objects", "count": 3, "name": "Oak"})).contains("Oak"));
        let found = text(json!({"where": "slate", "count": 3, "added": 1, "name": ""}));
        assert!(found.contains('2'), "{found}");
        assert_eq!(text(json!({"where": "none", "count": 0})), Text::PutNothing.get(Language::English));
        assert_eq!(text(json!({"where": "slate", "count": 0})), Text::PutNoSlate.get(Language::English));
    }
}
