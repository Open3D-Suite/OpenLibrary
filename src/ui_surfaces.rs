// SPDX-License-Identifier: GPL-3.0-or-later
//! The tabs of materials, textures and interiors: folders, cards, the options they are sent with and what is
//! known of the selected one. Cards are dragged onto 3ds Max.

use crate::favorites::{self, TabView};
use crate::foldertree::{self, SideTree};
use crate::interiors::{self, Light, Room};
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

const ACCENT: Color32 = Color32::from_rgb(0xF0, 0xA9, 0x3B);
const DARK: Color32 = Color32::from_rgb(20, 24, 34);
const FOOTER: f32 = 30.0;
const GAP: f32 = 8.0;

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

    fn matches(&self, words: &[String]) -> bool {
        let hay = format!("{} {}", self.name(), self.folder().display()).to_lowercase();
        words.iter().all(|w| hay.contains(w.as_str()))
    }
}

/// What a scan thread has read.
struct View {
    items: Vec<Surface>,
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
}

pub struct SurfacePanel {
    pub kind: String,
    lang: Language,
    wake: Wake,
    roots: Vec<PathBuf>,
    task: Task<Event>,
    scanning: bool,
    seen: usize,
    view: Option<View>,
    hidden: HashSet<String>,
    side: SideTree,
    tab: TabView,
    favorite_count: usize,
    search: String,
    shown: Vec<Surface>,
    dirty: bool,
    only_libraries: bool,
    only_sized: bool,
    recent: bool,
    selected: HashSet<String>,
    anchor: usize,
    textures: HashMap<String, Option<TextureHandle>>,
    loader: ThumbLoader,
    dragging: bool,
    look: bool,
    size: [f64; 3],
    sent: Vec<Sent>,
    ask: Option<Ask>,
}

fn read_view(kind: &str, index: &SurfaceIndex) -> View {
    let mut pixels = HashMap::new();
    let items = match kind {
        "materials" => {
            let list = index.materials();
            pixels.extend(list.iter().filter_map(|m| Some((m.key(), index.dims_of(m)?))));
            list.into_iter().map(|m| Surface::Material(Arc::new(m))).collect()
        }
        "textures" => surfaces::build_textures(&index.listing).into_iter().map(|t| Surface::Texture(Arc::new(t))).collect(),
        _ => interiors::build_interiors(&index.listing, &index.roots).into_iter().map(|r| Surface::Room(Arc::new(r))).collect(),
    };
    View { items, folders: index.folders(), pixels }
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

impl SurfacePanel {
    pub fn new(kind: &str, tab: TabView, (roots, measures): (&[PathBuf], &[PathBuf]), data_dir: &Path, lang: Language, wake: Wake) -> Self {
        let cache = data_dir.join("library").join(format!("surf_{}", crate::library::cache_name(roots)));
        let mut index = SurfaceIndex::new(roots, measures);
        let kind_name = kind.to_string();
        let task = Task::start(wake.clone(), move |link| {
            if index.load(&cache) {
                link.send(Event::View(read_view(&kind_name, &index)));
            }
            if index.scan(&|| link.cancelled(), &mut |n| link.send(Event::Seen(n))) {
                link.send(Event::View(read_view(&kind_name, &index)));
                let _ = index.save(&cache); // only a cache: without it the next start reads the folders again
                let materials = if kind_name == "materials" { index.materials() } else { Vec::new() };
                if index.fill_dims(&materials, &|| link.cancelled()) {
                    let _ = index.save(&cache);
                    if !link.cancelled() {
                        link.send(Event::View(read_view(&kind_name, &index)));
                    }
                }
            }
            link.send(Event::Done);
        });
        Self {
            kind: kind.to_string(),
            lang,
            loader: ThumbLoader::new(data_dir.join("thumbs"), 4, wake.clone()),
            wake,
            roots: roots.to_vec(),
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
        if self.scanning && self.view.is_none() {
            return fill(self.t(Text::SurfacesReading), &[("count", &self.seen)]);
        }
        fill(self.t(Text::SurfacesShown), &[("count", &self.shown.len())])
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
        for loaded in self.loader.poll() {
            let texture = loaded.image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(&loaded.key, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
            self.textures.insert(loaded.key, texture);
        }
    }

    fn refresh(&mut self, settings: &Settings) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let Some(view) = &self.view else { return };
        let words: Vec<String> = self.search.to_lowercase().split_whitespace().map(str::to_string).collect();
        let base = self.side.folder.as_deref().map(norm_key);
        let in_folder = |item: &Surface| {
            let key = norm_key(item.folder());
            let inside = base.as_ref().is_none_or(|b| key == *b || key.starts_with(&format!("{}\\", b.trim_end_matches('\\'))));
            inside && !self.hidden.contains(&key)
        };
        let wanted = |item: &Surface| match item {
            Surface::Material(m) => {
                (!self.only_libraries || m.is_library()) && (!self.only_sized || prefs::known_size(settings, &self.roots, m).is_some())
            }
            _ => true,
        };
        let mut shown: Vec<Surface> = view.items.iter().filter(|i| in_folder(i) && i.matches(&words) && wanted(i)).cloned().collect();
        if self.recent {
            shown.sort_by_key(|i| std::cmp::Reverse(i.mtime()));
        }
        let liked = |i: &Surface| favorites::is_favorite(settings, &i.favorite_key(&self.roots));
        self.favorite_count =
            if favorites::any(settings) { view.items.iter().filter(|i| !self.hidden.contains(&norm_key(i.folder())) && liked(i)).count() } else { 0 };
        if self.side.favorites {
            shown.retain(liked);
        } else if self.tab.favorites_first && self.favorite_count > 0 {
            shown.sort_by_cached_key(|i| !liked(i)); // stable: the order of before within each part
        }
        self.shown = shown;
        let keys: HashSet<String> = self.shown.iter().map(Surface::key).collect();
        self.selected.retain(|k| keys.contains(k));
        self.anchor = 0;
    }

    fn selection(&self) -> Vec<Surface> {
        self.shown.iter().filter(|i| self.selected.contains(&i.key())).cloned().collect()
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
                if ui.button(egui::RichText::new(self.t(Text::UseExisting)).strong()).clicked() {
                    choice = Some(Some("use"));
                }
                if ui.button(self.t(Text::MakeNew)).clicked() {
                    choice = Some(Some("new"));
                }
                if ui.button(self.t(Text::Cancel)).clicked() {
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
        egui::Panel::left("surface-tree").resizable(true).default_size(240.0).show(ui, |ui| self.tree(ui, tools));
        egui::Panel::right("surface-detail").resizable(true).default_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                self.options(ui, tools);
                ui.separator();
                self.detail(ui, tools);
            });
        });
        egui::CentralPanel::default().show(ui, |ui| self.gallery(ui, tools));
        self.drag(&ctx, tools);
        self.question(&ctx, tools);
    }

    fn tree(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let total = self.view.as_ref().map_or(0, |v| v.items.iter().filter(|i| !self.hidden.contains(&norm_key(i.folder()))).count());
        let changed = self.side.show(ui, "surfaces", tools.settings, self.lang, self.tab, (self.favorite_count, total));
        if changed.settings {
            self.save_settings(tools);
            self.settings_changed(tools.settings);
        }
        if changed.shown {
            self.dirty = true;
            self.loader.clear_queue();
        }
    }

    /// Works out again what is hidden, the rows of the tree and what is shown.
    pub fn settings_changed(&mut self, settings: &Settings) {
        self.dirty = true;
        let Some(view) = &self.view else { return };
        let (names, paths) = settings.hidden();
        self.hidden = hidden_folders(&view.folders, &self.roots, &names, &paths);
        let items = view.items.iter().map(Surface::folder).filter(|f| !self.hidden.contains(&norm_key(f)));
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
        let at = self.shown.iter().position(|i| self.selected.contains(&i.key()));
        (self.shown.iter().map(|i| (i.name().to_string(), i.large(light))).collect(), at)
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

    /// Draws the heart of a card; `true` when it was clicked.
    fn heart(&mut self, ui: &mut egui::Ui, at: usize, response: &egui::Response, tools: &mut Tools<'_>) -> bool {
        self.look |= response.double_clicked();
        let Some(key) = self.shown.get(at).map(|i| i.favorite_key(&self.roots)) else { return false };
        let on = favorites::is_favorite(tools.settings, &key);
        let clicked = crate::app::heart(ui, response, on);
        if clicked {
            self.toggle(&[key], tools);
        }
        clicked
    }

    fn combo<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, label: &str, options: &[(T, String)], current: &mut T) -> bool {
        let text = options.iter().find(|(v, _)| v == current).map(|(_, t)| t.clone()).unwrap_or_default();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
                for (value, text) in options {
                    changed |= ui.selectable_value(current, *value, text).changed();
                }
            });
        });
        changed
    }

    /// A choice kept in the settings as a text.
    fn option_combo(&self, ui: &mut egui::Ui, tools: &mut Tools<'_>, key: &str, label: Text, options: &[(&'static str, String)]) {
        let saved = prefs::option(tools.settings, key).and_then(Value::as_str).unwrap_or_default().to_string();
        let mut current = options.iter().map(|(v, _)| *v).find(|v| *v == saved).or_else(|| options.first().map(|(v, _)| *v)).unwrap_or_default();
        if Self::combo(ui, key, self.t(label), options, &mut current) {
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

    fn options(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let engines = [
            ("auto", self.t(Text::EngineScene).to_string()),
            ("corona", "Corona".to_string()),
            ("vray", "V-Ray".to_string()),
            ("max", "Physical".to_string()),
        ];
        self.option_combo(ui, tools, "engine", Text::OptEngine, &engines);
        if self.kind != "textures" {
            let mut res = prefs::resolution(tools.settings);
            let mut list = vec![(Res::Highest, self.t(Text::ResHighest).to_string()), (Res::Lowest, self.t(Text::ResLowest).to_string())];
            list.extend([8, 4, 2, 1].map(|k| (Res::K(k), format!("{k}K"))));
            if Self::combo(ui, "res", self.t(Text::OptResolution), &list, &mut res) {
                prefs::set_resolution(tools.settings, res);
                self.save_settings(tools);
            }
        }
        match self.kind.as_str() {
            "materials" => {
                let mut on = prefs::option(tools.settings, "displacement").and_then(Value::as_bool).unwrap_or(false);
                if ui.checkbox(&mut on, self.t(Text::OptDisplacement)).changed() {
                    self.save(tools, "displacement", on.into());
                }
                let mut on = prefs::option(tools.settings, "balls").and_then(Value::as_bool).unwrap_or(true);
                if ui.checkbox(&mut on, self.t(Text::OptBalls)).changed() {
                    self.save(tools, "balls", on.into());
                }
                let libraries = self.t(Text::OptOnlyLibraries);
                self.dirty |= ui.checkbox(&mut self.only_libraries, libraries).changed();
                let sized = self.t(Text::OptOnlySized);
                self.dirty |= ui.checkbox(&mut self.only_sized, sized).changed();
            }
            "textures" => {
                let roles = [
                    ("auto", self.t(Text::RoleAuto).to_string()),
                    ("color", self.t(Text::RoleColor).to_string()),
                    ("data", self.t(Text::RoleData).to_string()),
                ];
                self.option_combo(ui, tools, "role", Text::OptRole, &roles);
                let mappings = Mapping::ALL.map(|m| (m.key(), mapping_text(m, self.lang).to_string()));
                self.option_combo(ui, tools, "mapping", Text::OptMapping, &mappings);
                let mapping = prefs::option(tools.settings, "mapping").and_then(Value::as_str).and_then(Mapping::from_key);
                size_fields(ui, &mut self.size, mapping.unwrap_or(Mapping::Uv).axes());
            }
            _ => {
                let lights = [("day", self.t(Text::LightDay).to_string()), ("night", self.t(Text::LightNight).to_string())];
                self.option_combo(ui, tools, "light", Text::OptLight, &lights);
                let mut depth = prefs::option(tools.settings, "depth").and_then(Value::as_i64).unwrap_or(-1);
                let mut list = vec![(-1, self.t(Text::DepthAuto).to_string())];
                list.extend([(1, "×1"), (3, "×1.5"), (4, "×2"), (5, "×3"), (6, "×4"), (7, "×5")].map(|(v, t)| (v, t.to_string())));
                if Self::combo(ui, "depth", self.t(Text::OptDepth), &list, &mut depth) {
                    self.save(tools, "depth", depth.into());
                }
            }
        }
        let recent = self.t(Text::OptRecent);
        self.dirty |= ui.checkbox(&mut self.recent, recent).changed();
    }

    fn detail(&mut self, ui: &mut egui::Ui, tools: &mut Tools<'_>) {
        let selection = self.selection();
        let [item] = &selection[..] else {
            ui.weak(self.t(match self.kind.as_str() {
                "materials" => Text::HintMaterials,
                "textures" => Text::HintTextures,
                _ => Text::HintInteriors,
            }));
            return;
        };
        let light = prefs::light(tools.settings);
        if let Some(Some(texture)) = self.textures.get(&thumb_key(item, light)) {
            let size = texture.size_vec2() * (ui.available_width() / texture.size_vec2().x);
            ui.image((texture.id(), size));
        }
        ui.heading(item.name());
        ui.label(egui::RichText::new(item.folder().display().to_string()).small().weak());
        ui.add_space(6.0);
        match item {
            Surface::Material(m) if m.is_library() => {
                ui.weak(self.t(Text::LibraryHint));
            }
            Surface::Material(m) => self.material_detail(ui, m, tools),
            Surface::Texture(t) => {
                let role = surfaces::guess_role(&file_name(&t.path));
                ui.label(fill(self.t(Text::TextureRole), &[("role", &self.t(if role == "color" { Text::RoleColor } else { Text::RoleData }))]));
            }
            Surface::Room(r) => {
                let values: [(&str, &dyn std::fmt::Display); 2] = [("width", &trim(r.size.width())), ("height", &trim(interiors::HEIGHT))];
                ui.label(fill(self.t(Text::RoomWindow), &values));
                let exr = r.pick(light, prefs::resolution(tools.settings)).map(|p| file_name(p)).unwrap_or_default();
                ui.label(exr);
                match &r.osl {
                    Some(osl) => ui.label(file_name(osl)),
                    None => ui.colored_label(ACCENT, self.t(Text::InteriorNoShader)),
                };
            }
        }
        ui.add_space(8.0);
        if ui.button(self.t(Text::OpenFolder)).clicked() {
            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", item.file().display())).spawn();
        }
    }

    fn material_detail(&mut self, ui: &mut egui::Ui, material: &Material, tools: &mut Tools<'_>) {
        let shape = self.shape(tools.settings, material);
        let aspect = shape.map(|(a, _)| a);
        let mut mapping = prefs::mapping_of(tools.settings, &self.roots, material, aspect);
        let square = aspect.is_none_or(|a| a == Aspect::Square);
        let list: Vec<(Mapping, String)> = Mapping::ALL
            .into_iter()
            .filter(|m| square || *m != Mapping::Triplanar) // a triplanar of a long texture would squash it
            .map(|m| (m, mapping_text(m, self.lang).to_string()))
            .collect();
        if Self::combo(ui, "material-mapping", self.t(Text::OptMapping), &list, &mut mapping) {
            prefs::remember(tools.settings, prefs::MAPPINGS, &self.roots, material, Some(mapping.key().into()));
            self.save_settings(tools);
        }
        let mut size = prefs::size_of(tools.settings, &self.roots, material);
        let (changed, ended) = size_fields(ui, &mut size, 3);
        if changed {
            let same = material.size.is_some_and(|(w, h)| size == [w, h, h]);
            let value = (!same).then(|| json!(size.map(|v| (v * 100.0).round() / 100.0)));
            prefs::remember(tools.settings, prefs::SIZES, &self.roots, material, value);
        }
        if ended {
            self.save_settings(tools);
        }
        let saved = prefs::saved_size(tools.settings, &self.roots, material).is_some();
        let note = match (saved, material.size.is_some()) {
            (true, _) => Text::SizeSaved,
            (false, true) => Text::SizeFromName,
            (false, false) => Text::SizeUnknown,
        };
        ui.label(egui::RichText::new(self.t(note)).small().weak());
        if saved && ui.small_button(self.t(Text::SizeForget)).clicked() {
            prefs::remember(tools.settings, prefs::SIZES, &self.roots, material, None);
            self.save_settings(tools);
        }
        if ui.small_button(self.t(Text::SizeToFolder)).clicked() {
            self.size_to_folder(material, size, tools);
        }
        if let Some((aspect, ratio)) = shape {
            ui.label(ratio_text(aspect, ratio));
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new(self.t(Text::MapsToLoad).to_uppercase()).small().strong());
        for (channel, file) in material.pick(prefs::resolution(tools.settings)) {
            let (letters, [r, g, b]) = channel.badge();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(letters).monospace().color(Color32::WHITE).background_color(Color32::from_rgb(r, g, b)));
                ui.label(egui::RichText::new(file_name(&file)).small());
                if channel == Channel::Ao {
                    ui.weak(self.t(Text::NotWired));
                }
            });
        }
        if material.tiles > 0 {
            ui.weak(fill(self.t(Text::Planks), &[("count", &material.tiles)]));
        }
    }

    /// Gives this size to the materials of the folder that have none.
    fn size_to_folder(&mut self, material: &Material, size: [f64; 3], tools: &mut Tools<'_>) {
        let folder = norm_key(&material.folder);
        let mut count = 0;
        for item in self.view.iter().flat_map(|v| &v.items) {
            if let Surface::Material(other) = item
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
                Text::NoFolders
            } else if self.scanning {
                Text::Scanning
            } else {
                Text::NoSurfaces
            };
            ui.centered_and_justified(|ui| ui.label(self.t(text)));
            return;
        }
        let side = tools.settings.data.get("thumb_size").and_then(Value::as_f64).unwrap_or(160.0).clamp(80.0, 400.0) as f32;
        let card = Vec2::new(side, side + FOOTER);
        let columns = (((ui.available_width() + GAP) / (side + GAP)).floor() as usize).max(1);
        let rows = self.shown.len().div_ceil(columns);
        let mut clicked = None;
        let mut action: Option<&'static str> = None;
        let mut favorite = false;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, card.y + GAP, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for at in row * columns..((row + 1) * columns).min(self.shown.len()) {
                        let response = self.card(ui, at, card, tools.settings);
                        let here = self.shown.get(at).map(|i| self.selected.contains(&i.key())).unwrap_or(false);
                        if response.drag_started() {
                            if !here {
                                self.select(at, (false, false));
                            }
                            self.dragging = true;
                        }
                        if response.clicked() {
                            clicked = Some(at);
                        }
                        if self.heart(ui, at, &response, tools) {
                            clicked = None;
                        }
                        if response.secondary_clicked() && !here {
                            self.select(at, (false, false));
                        }
                        response.context_menu(|ui| {
                            action = self.menu(ui).or(action);
                            if ui.button(self.t(Text::FavoriteToggle)).clicked() {
                                favorite = true;
                                ui.close();
                            }
                        });
                    }
                });
                ui.add_space(GAP - ui.spacing().item_spacing.y);
            }
        });
        if let Some(at) = clicked {
            self.select(at, ui.input(|i| (i.modifiers.command, i.modifiers.shift)));
        }
        if favorite {
            let keys: Vec<String> = self.selection().iter().map(|i| i.favorite_key(&self.roots)).collect();
            self.toggle(&keys, tools);
        }
        if let Some(target) = action {
            let (items, info) = (self.selection(), tools.max.target());
            self.send(target, &items, info, tools);
        }
    }

    fn menu(&self, ui: &mut egui::Ui) -> Option<&'static str> {
        let mut entries = vec![("selection", if self.kind == "textures" { Text::MenuQuickMaterial } else { Text::MenuToSelection })];
        if self.kind == "interiors" {
            entries.push(("plane", Text::MenuWindow));
        }
        entries.push(("slate", Text::MenuToSlate));
        let mut out = None;
        for (target, text) in entries {
            if ui.button(self.t(text)).clicked() {
                out = Some(target);
                ui.close();
            }
        }
        out
    }

    fn select(&mut self, at: usize, (ctrl, shift): (bool, bool)) {
        let Some(key) = self.shown.get(at).map(Surface::key) else { return };
        if shift {
            let range = self.anchor.min(at)..=self.anchor.max(at);
            self.selected = self.shown.get(range).unwrap_or_default().iter().map(Surface::key).collect();
            return;
        }
        self.anchor = at;
        if !ctrl {
            self.selected.clear();
        }
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, at: usize, size: Vec2, settings: &Settings) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let Some(item) = self.shown.get(at).cloned() else { return response };
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let selected = self.selected.contains(&item.key());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        let back = if selected || response.hovered() { visuals.widgets.hovered.weak_bg_fill } else { visuals.faint_bg_color };
        painter.rect_filled(rect, 0.0, back);
        let area = Rect::from_min_size(rect.min, Vec2::new(size.x, size.y - FOOTER));
        let light = prefs::light(settings);
        let key = thumb_key(&item, light);
        match self.textures.get(&key) {
            Some(Some(texture)) => {
                let balls = prefs::option(settings, "balls").and_then(Value::as_bool).unwrap_or(true);
                let plain = matches!(&item, Surface::Material(m) if m.preview.is_none());
                if balls && plain { ball(&painter, area, texture) } else { picture(&painter, area, texture) }
            }
            Some(None) => {
                let font = egui::FontId::proportional(11.0);
                painter.text(area.center(), egui::Align2::CENTER_CENTER, self.t(Text::NoPreview), font, visuals.weak_text_color());
            }
            None => match item.thumb(light) {
                Some(src) => self.loader.request(&key, &src),
                None => {
                    self.textures.insert(key, None);
                }
            },
        }
        self.badges(&painter, area, &item, settings);
        let galley = painter.layout(item.name().to_string(), egui::FontId::proportional(12.0), visuals.text_color(), size.x - 12.0);
        let top = rect.bottom() - FOOTER / 2.0 - galley.size().y.min(FOOTER) / 2.0;
        painter.galley(egui::pos2(rect.left() + 6.0, top), galley, Color32::WHITE);
        if selected {
            painter.rect_stroke(rect, 0.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
        }
        response
    }

    fn badges(&self, painter: &egui::Painter, area: Rect, item: &Surface, settings: &Settings) {
        let mut tags: Vec<(String, Color32)> = Vec::new();
        let top_res = match item {
            Surface::Material(m) => m.top_res(),
            Surface::Room(r) => r.top_res(),
            Surface::Texture(_) => 0,
        };
        if top_res > 0 {
            tags.push((format!("{top_res}K"), DARK));
        }
        match item {
            Surface::Room(r) => tags.push((r.size.code().to_string(), Color32::from_rgb(0xA8, 0x6E, 0x14))),
            Surface::Material(m) if m.is_library() => tags.push((".MAT".to_string(), DARK)),
            _ => {}
        }
        let mut x = area.left() + 4.0;
        for (text, color) in tags {
            x = tag(painter, egui::pos2(x, area.top() + 4.0), &text, color) + 3.0;
        }
        let bottom = area.bottom() - 20.0;
        let mut x = area.left() + 4.0;
        match item {
            Surface::Material(m) if !m.is_library() => {
                if let Some((aspect, ratio)) = self.shape(settings, m) {
                    let text = ratio_text(aspect, ratio);
                    let color = if aspect == Aspect::Square { DARK } else { Color32::from_rgb(0xA8, 0x6E, 0x14) };
                    let width = text.chars().count() as f32 * 6.0 + 8.0;
                    tag(painter, egui::pos2(area.right() - width - 4.0, area.top() + 4.0), &text, color);
                }
                for channel in m.channels().into_iter().filter(|c| *c != Channel::Refl) {
                    if x + 20.0 > area.right() {
                        break;
                    }
                    let (letters, [r, g, b]) = channel.badge();
                    x = tag(painter, egui::pos2(x, bottom), letters, Color32::from_rgb(r, g, b)) + 2.0;
                }
            }
            Surface::Room(r) => {
                for light in r.exrs.keys() {
                    let (text, color) = match light {
                        Light::Day => (Text::LightDay, Color32::from_rgb(0xA8, 0x80, 0x10)),
                        Light::Night => (Text::LightNight, Color32::from_rgb(0x3B, 0x5B, 0xA8)),
                    };
                    x = tag(painter, egui::pos2(x, bottom), self.t(text), color) + 3.0;
                }
            }
            _ => {}
        }
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
                ui.label(*label);
                let response = ui.add(egui::DragValue::new(value).speed(1.0).range(0.1..=100_000.0).max_decimals(2));
                changed |= response.changed();
                ended |= response.drag_stopped() || response.lost_focus();
            });
        }
        ui.label("cm");
    });
    (changed, ended)
}

fn whole() -> Rect {
    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

fn picture(painter: &egui::Painter, area: Rect, texture: &TextureHandle) {
    let size = texture.size_vec2();
    let scale = (area.width() / size.x).min(area.height() / size.y);
    painter.image(texture.id(), Rect::from_center_size(area.center(), size * scale), whole(), Color32::WHITE);
}

/// A material ball: the texture in a circle, with light from the upper left. Not a render.
fn ball(painter: &egui::Painter, area: Rect, texture: &TextureHandle) {
    const RINGS: u32 = 12;
    const SIDES: u32 = 40;
    let radius = area.width().min(area.height()) * 0.42;
    let size = texture.size_vec2();
    let crop = size.min_elem() / 2.0;
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for ring in 0..=RINGS {
        let r = ring as f32 / RINGS as f32;
        for side in 0..=SIDES {
            let angle = side as f32 / SIDES as f32 * std::f32::consts::TAU;
            let (x, y) = (angle.cos() * r, angle.sin() * r);
            let z = (1.0 - r * r).max(0.0).sqrt();
            let lit = 0.2 + 0.95 * (-0.42 * x - 0.5 * y + 0.75 * z).max(0.0);
            let grey = (lit.min(1.0) * 255.0) as u8;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: area.center() + Vec2::new(x, y) * radius,
                uv: egui::pos2(0.5 + x * crop / size.x, 0.5 + y * crop / size.y),
                color: Color32::from_gray(grey),
            });
        }
    }
    for ring in 0..RINGS {
        for side in 0..SIDES {
            let a = ring * (SIDES + 1) + side;
            let b = a + SIDES + 1;
            mesh.add_triangle(a, b, a + 1);
            mesh.add_triangle(a + 1, b, b + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Draws a small label; where it ends.
fn tag(painter: &egui::Painter, at: egui::Pos2, text: &str, color: Color32) -> f32 {
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::monospace(10.0), Color32::WHITE);
    let rect = Rect::from_min_size(at, Vec2::new(galley.size().x + 8.0, 16.0));
    painter.rect_filled(rect, 0.0, color);
    painter.galley(egui::pos2(rect.left() + 4.0, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
    rect.right()
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
