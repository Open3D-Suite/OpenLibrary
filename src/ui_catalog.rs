// SPDX-License-Identifier: GPL-3.0-or-later
//! The window to catalogue a download: it opens a zip, rar, 7z... or a folder, shares its files out between
//! `.max`, preview and maps, and leaves them in a folder of the library under one name.

use crate::archives::{self, ArchiveError};
use crate::catalog::{self, Bucket, Item, Plan, Request};
use crate::foldertree::{self, Folder};
use crate::library::{LibraryIndex, norm_key};
use crate::maxbatch::{self, Outcome};
use crate::maxfile::{self, Engine};
use crate::open3d_settings::Language;
use crate::platform;
use crate::task::{Task, Wake};
use crate::texts::{Text, fill};
use crate::thumbs::ThumbLoader;
use eframe::egui::{self, Color32, TextureHandle, TextureOptions, Vec2};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const ROW: f32 = 44.0;
/// More files than this are a whole library, not a download.
const LIMIT: usize = 5000;
const WARN: Color32 = Color32::from_rgb(0xD0, 0x6A, 0x3A);

struct Entry {
    item: Item,
    bucket: Bucket,
    engine: Engine,
    selected: bool,
}

enum Event {
    Opened(Result<Vec<(Item, Engine)>, ArchiveError>),
    Copied(usize),
    Saved(Result<usize, String>),
    Prepared(usize, Outcome),
    PrepareEnded(Result<(), String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    NoMax,
    Overwrite,
    Password { wrong: bool },
    NewFolder,
}

/// The save that is going on.
struct Saving {
    plan: Plan,
    name: String,
    dest: PathBuf,
    done: usize,
    warnings: Vec<String>,
}

/// What the window asks of the app after a frame.
#[derive(Default)]
pub struct Answer {
    pub close: bool,
    /// The folder where something was saved.
    pub saved: Option<PathBuf>,
    /// The folder of the maps, to hide it in the tree.
    pub hide: Option<String>,
}

pub struct CatalogWindow {
    lang: Language,
    wake: Wake,
    tmp: PathBuf,
    opened: usize,
    /// Where the download was extracted; `None` when a folder was opened, which is never touched.
    work_dir: Option<PathBuf>,
    source: Option<PathBuf>,
    entries: Vec<Entry>,
    name: String,
    maps_folder: String,
    to_corona: bool,
    clean_layers: bool,
    folders: Vec<Folder>,
    expanded: HashSet<String>,
    filter: String,
    dest: Option<PathBuf>,
    last_dir: PathBuf,
    task: Option<Task<Event>>,
    saving: Option<Saving>,
    ask: Option<Ask>,
    field: String,
    status: String,
    textures: HashMap<String, Option<TextureHandle>>,
    loader: ThumbLoader,
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn read_items(folder: &Path) -> Vec<(Item, Engine)> {
    let engine = |item: &Item| {
        if item.is_max() { catalog::engine_of(&item.name, maxfile::detect_engine(&item.path)) } else { Engine::Other }
    };
    catalog::list_files(folder, LIMIT).into_iter().map(|i| (engine(&i), i)).map(|(e, i)| (i, e)).collect()
}

impl CatalogWindow {
    pub fn new(
        config: Option<&Map<String, Value>>,
        index: Option<&LibraryIndex>,
        hidden: &HashSet<String>,
        dest: Option<&Path>,
        data_dir: &Path,
        (lang, wake): (Language, Wake),
    ) -> Self {
        let text = |key: &str| config.and_then(|c| c.get(key)).and_then(Value::as_str).unwrap_or_default().to_string();
        let flag = |key: &str| config.and_then(|c| c.get(key)).and_then(Value::as_bool).unwrap_or(true);
        // a folder of this window alone: another copy of the app may be cataloguing too
        let tmp = data_dir.join("catalog").join(std::process::id().to_string());
        let _ = std::fs::remove_dir_all(&tmp); // what an earlier window of this process left
        let folders = index.map(|i| foldertree::rows(&i.roots, &i.folders, hidden)).unwrap_or_default();
        let mut expanded: HashSet<String> = folders.iter().filter(|f| f.parent.is_none()).map(|f| f.key.clone()).collect();
        let mut at = dest.and_then(|d| folders.iter().position(|f| f.key == norm_key(d)));
        while let Some(n) = at {
            expanded.insert(folders[n].key.clone());
            at = folders[n].parent;
        }
        let maps_folder = Some(text("maps_folder")).filter(|m| !m.is_empty()).unwrap_or_else(|| catalog::MAPS_FOLDER.to_string());
        Self {
            lang,
            loader: ThumbLoader::new(tmp.join("thumbs"), 2, wake.clone()),
            wake,
            tmp,
            opened: 0,
            work_dir: None,
            source: None,
            entries: Vec::new(),
            name: String::new(),
            maps_folder,
            to_corona: flag("to_corona"),
            clean_layers: flag("clean_layers"),
            folders,
            expanded,
            filter: String::new(),
            dest: dest.map(Path::to_path_buf),
            last_dir: PathBuf::from(text("last_dir")),
            task: None,
            saving: None,
            ask: None,
            field: String::new(),
            status: Text::CatalogHint.get(lang).to_string(),
            textures: HashMap::new(),
        }
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    /// The choices to keep in the settings.
    pub fn config(&self) -> [(&'static str, Value); 4] {
        [
            ("maps_folder", self.maps_folder.trim().into()),
            ("to_corona", self.to_corona.into()),
            ("clean_layers", self.clean_layers.into()),
            ("last_dir", self.last_dir.display().to_string().into()),
        ]
    }

    fn busy(&self) -> bool {
        self.task.is_some()
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.textures.clear();
        self.loader.clear_queue();
        self.source = None;
        if let Some(dir) = self.work_dir.take() {
            let _ = std::fs::remove_dir_all(dir); // a copy of the download; the download itself stays
        }
    }

    /// A download to open: an archive, or a folder already extracted.
    fn open(&mut self, path: &Path) {
        if self.busy() {
            return;
        }
        self.clear();
        self.last_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.source = Some(path.to_path_buf());
        if path.is_dir() {
            let folder = path.to_path_buf();
            self.status = fill(self.t(Text::CatalogOpening), &[("name", &file_name(path))]);
            self.task = Some(Task::start(self.wake.clone(), move |link| link.send(Event::Opened(Ok(read_items(&folder))))));
        } else {
            self.extract(None);
        }
    }

    fn extract(&mut self, password: Option<String>) {
        let Some(path) = self.source.clone() else { return };
        let names: Vec<String> = path
            .parent()
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let volumes = archives::volumes(&path, &names);
        if !volumes.missing.is_empty() {
            self.status = fill(self.t(Text::CatalogParts), &[("parts", &volumes.missing.join(", "))]);
            self.source = None;
            return;
        }
        self.opened += 1;
        let work = self.tmp.join(format!("open{}", self.opened));
        self.work_dir = Some(work.clone());
        self.status = fill(self.t(Text::CatalogOpening), &[("name", &file_name(&volumes.first))]);
        self.task = Some(Task::start(self.wake.clone(), move |link| {
            let result = archives::extract(&volumes.first, &work, password.as_deref(), &|| link.cancelled());
            link.send(Event::Opened(result.map(|()| read_items(&work))));
        }));
    }

    fn opened(&mut self, result: Result<Vec<(Item, Engine)>, ArchiveError>) {
        let items = match result {
            Ok(items) => items,
            Err(error) => {
                if let Some(dir) = self.work_dir.take() {
                    let _ = std::fs::remove_dir_all(dir);
                }
                self.status = match error {
                    ArchiveError::Password { wrong } => {
                        self.ask = Some(Ask::Password { wrong });
                        self.field.clear();
                        String::new()
                    }
                    ArchiveError::NoTool => self.t(Text::CatalogNoTool).to_string(),
                    ArchiveError::Incomplete => self.t(Text::CatalogIncomplete).to_string(),
                    ArchiveError::Cancelled => self.t(Text::Cancelled).to_string(),
                    ArchiveError::Failed(error) => fill(self.t(Text::CatalogOpenFailed), &[("error", &error)]),
                };
                return;
            }
        };
        if items.len() > LIMIT {
            self.status = fill(self.t(Text::CatalogTooMany), &[("count", &LIMIT)]);
            self.clear();
            return;
        }
        let Some(source) = self.source.clone() else { return };
        let name = if self.work_dir.is_some() { archives::display_name(&source) } else { file_name(&source) };
        let list: Vec<Item> = items.iter().map(|(i, _)| i.clone()).collect();
        let buckets = catalog::guess(&list, &name);
        self.entries = items.into_iter().zip(buckets).map(|((item, engine), bucket)| Entry { item, bucket, engine, selected: false }).collect();
        self.name = catalog::clean_name(&name);
        let total: u64 = self.entries.iter().map(|e| e.item.size).sum();
        self.status = fill(
            self.t(if self.entries.iter().any(|e| e.item.is_max()) { Text::CatalogOpened } else { Text::CatalogOpenedNoMax }),
            &[("name", &file_name(&source)), ("count", &self.entries.len()), ("size", &catalog::human_size(total))],
        );
    }

    /// Images from outside, for the preview: copied (what is in the lists may be moved) and put first.
    fn add_previews(&mut self, paths: &[PathBuf]) {
        self.opened += 1;
        let dir = self.tmp.join(format!("added{}", self.opened));
        for path in paths.iter().rev() {
            let dst = dir.join(path.file_name().unwrap_or_default());
            match std::fs::create_dir_all(&dir).and_then(|()| std::fs::copy(path, &dst)) {
                Ok(_) => self.put_first(&dst),
                Err(error) => self.status = fill(self.t(Text::CatalogOpenFailed), &[("error", &error)]),
            }
        }
    }

    fn put_first(&mut self, path: &Path) {
        self.entries.iter_mut().for_each(|e| e.selected = false);
        let entry = Entry { item: Item::new(path, ""), bucket: Bucket::Preview, engine: Engine::Other, selected: true };
        self.entries.insert(0, entry);
    }

    /// The image of the clipboard ("Copy image" in the browser) as the preview.
    fn paste(&mut self) {
        let image = platform::clipboard_dib()
            .and_then(|dib| maxfile::dib_to_bmp(&dib))
            .and_then(|bmp| image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok());
        let Some(image) = image else {
            self.status = self.t(Text::CatalogNoImage).to_string();
            return;
        };
        self.opened += 1;
        let dir = self.tmp.join(format!("added{}", self.opened));
        let path = dir.join("preview.png");
        let saved = std::fs::create_dir_all(&dir).map_err(|e| e.to_string());
        match saved.and_then(|()| image.to_rgb8().save(&path).map_err(|e| e.to_string())) {
            Ok(()) => self.put_first(&path),
            Err(error) => self.status = fill(self.t(Text::CatalogOpenFailed), &[("error", &error)]),
        }
    }

    fn paths(&self, bucket: Bucket) -> Vec<PathBuf> {
        self.entries.iter().filter(|e| e.bucket == bucket).map(|e| e.item.path.clone()).collect()
    }

    fn maxes(&self) -> Vec<(PathBuf, Engine)> {
        self.entries.iter().filter(|e| e.bucket == Bucket::Max).map(|e| (e.item.path.clone(), e.engine)).collect()
    }

    fn plan(&self) -> Option<Plan> {
        let name = catalog::clean_name(&self.name);
        let dest = self.dest.as_deref()?;
        if name.is_empty() || self.entries.iter().all(|e| e.bucket == Bucket::Skip) {
            return None;
        }
        Some(Plan::new(&Request {
            dest,
            name: &name,
            maxes: &self.maxes(),
            previews: &self.paths(Bucket::Preview),
            others: &self.paths(Bucket::Maps),
            maps_folder: &self.maps_folder,
            to_corona: self.to_corona,
            clean_layers: self.clean_layers,
        }))
    }

    /// Saves, asking first what has to be asked; `asked` is the last thing the person said yes to.
    fn save(&mut self, asked: Option<Ask>) {
        let Some(plan) = self.plan() else { return };
        if asked.is_none() && plan.max_dsts.is_empty() {
            self.ask = Some(Ask::NoMax);
            return;
        }
        if asked != Some(Ask::Overwrite) && !plan.conflicts().is_empty() {
            self.ask = Some(Ask::Overwrite);
            return;
        }
        let Some(dest) = self.dest.clone() else { return };
        let name = catalog::clean_name(&self.name);
        self.status = fill(self.t(Text::CatalogSaving), &[("name", &name)]);
        self.saving = Some(Saving { plan: plan.clone(), name, dest, done: 0, warnings: Vec::new() });
        let moving = self.work_dir.is_some();
        self.task = Some(Task::start(self.wake.clone(), move |link| {
            let result = plan.apply(moving, &|| link.cancelled(), &mut |n| link.send(Event::Copied(n)));
            link.send(Event::Saved(result.map_err(|(path, error)| format!("{}: {error}", path.display()))));
        }));
    }

    fn saved(&mut self, result: Result<usize, String>, cancelled: bool, answer: &mut Answer) {
        let Some(saving) = &mut self.saving else { return };
        answer.saved = Some(saving.dest.clone());
        match result {
            Err(error) => {
                self.status = fill(self.t(Text::CatalogSaveFailed), &[("error", &error)]);
                self.saving = None;
                return;
            }
            Ok(done) => saving.done = done,
        }
        if cancelled {
            self.finish(Text::CatalogCancelled);
            return;
        }
        if saving.plan.maps_dir.is_some() {
            answer.hide = saving.plan.maps_dir.as_deref().and_then(Path::parent).map(file_name);
        }
        let Some(job) = saving.plan.job() else {
            self.finish(Text::CatalogSaved);
            return;
        };
        let Some(exe) = maxbatch::find() else {
            let warning = self.t(Text::NoMaxBatch).to_string();
            if let Some(saving) = &mut self.saving {
                saving.warnings.push(warning);
            }
            self.finish(Text::CatalogSaved);
            return;
        };
        self.status = self.t(Text::CatalogPreparing).to_string();
        let dir = self.tmp.join("prepare");
        self.task = Some(Task::start(self.wake.clone(), move |link| {
            let result = maxbatch::run(&exe, &dir, &job, &|| link.cancelled(), &mut |n, o| link.send(Event::Prepared(n, o)));
            link.send(Event::PrepareEnded(result));
        }));
    }

    fn finish(&mut self, text: Text) {
        let Some(saving) = self.saving.take() else { return };
        let values: [(&str, &dyn std::fmt::Display); 3] = [("name", &saving.name), ("folder", &saving.dest.display()), ("count", &saving.done)];
        self.status = fill(self.t(text), &values);
        for warning in &saving.warnings {
            self.status = format!("{}\n{warning}", self.status);
        }
        self.clear();
        self.name.clear();
    }

    /// Why 3ds Max could not prepare a file, in words.
    fn reason(&self, code: &str) -> String {
        match code {
            "open" => self.t(Text::ReasonOpen).to_string(),
            "save" => self.t(Text::ReasonSave).to_string(),
            "converter" => self.t(Text::ReasonConverter).to_string(),
            "vray" => self.t(Text::ReasonVray).to_string(),
            maxbatch::STUCK => self.t(Text::ReasonStuck).to_string(),
            other => other.to_string(),
        }
    }

    fn poll(&mut self, answer: &mut Answer) {
        let Some(task) = &self.task else { return };
        let cancelled = task.cancelled();
        for event in task.poll() {
            match event {
                Event::Opened(result) => {
                    self.task = None;
                    self.opened(result);
                }
                Event::Copied(done) => {
                    if let Some(saving) = &mut self.saving {
                        saving.done = done;
                    }
                }
                Event::Saved(result) => {
                    self.task = None;
                    self.saved(result, cancelled, answer);
                }
                Event::Prepared(n, outcome) => {
                    let reason = outcome.err().map(|code| self.reason(&code));
                    if let (Some(saving), Some(reason)) = (&mut self.saving, reason) {
                        let job = saving.plan.job();
                        let file = job.as_ref().and_then(|j| j.lines.get(n)?.first()).map(|f| file_name(Path::new(f))).unwrap_or_default();
                        saving.warnings.push(format!("{file}: {reason}"));
                    }
                }
                Event::PrepareEnded(result) => {
                    self.task = None;
                    answer.saved = self.saving.as_ref().map(|s| s.dest.clone());
                    let warning = match result {
                        _ if cancelled => Some(self.t(Text::CatalogPrepareCancelled).to_string()),
                        Err(error) => Some(fill(self.t(Text::BatchFailed), &[("error", &error)])),
                        Ok(()) => None,
                    };
                    if let Some(saving) = &mut self.saving {
                        saving.warnings.extend(warning);
                    }
                    self.finish(Text::CatalogSaved);
                }
            }
        }
    }

    fn take_thumbs(&mut self, ctx: &egui::Context) {
        for loaded in self.loader.poll() {
            let texture = loaded.image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(&loaded.key, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
            self.textures.insert(loaded.key, texture);
        }
    }

    fn take_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        let Some(first) = dropped.first() else { return };
        if self.busy() {
            return;
        }
        if dropped.iter().all(|p| p.is_file() && Item::new(p, "").is_image()) {
            self.add_previews(&dropped);
        } else {
            self.open(&first.clone());
        }
    }

    fn top(&mut self, ui: &mut egui::Ui) {
        let idle = !self.busy();
        ui.horizontal(|ui| {
            if ui.add_enabled(idle, egui::Button::new(self.t(Text::CatalogOpenArchive))).clicked() {
                let dialog = rfd::FileDialog::new().set_directory(&self.last_dir).add_filter("zip, rar, 7z", &archives::EXTENSIONS);
                if let Some(path) = dialog.pick_file() {
                    self.open(&path);
                }
            }
            if ui.add_enabled(idle, egui::Button::new(self.t(Text::CatalogOpenFolder))).clicked()
                && let Some(path) = rfd::FileDialog::new().set_directory(&self.last_dir).pick_folder()
            {
                self.open(&path);
            }
            if ui.add_enabled(idle, egui::Button::new(self.t(Text::CatalogPaste))).on_hover_text(self.t(Text::CatalogPasteHint)).clicked() {
                self.paste();
            }
            ui.label(egui::RichText::new(&self.status).small());
            if self.busy() {
                ui.spinner();
            }
        });
    }

    fn bucket(&mut self, ui: &mut egui::Ui, bucket: Bucket, title: Text, height: f32) {
        let rows: Vec<usize> = (0..self.entries.len()).filter(|&i| self.entries[i].bucket == bucket).collect();
        ui.label(egui::RichText::new(format!("{} ({})", self.t(title).to_uppercase(), rows.len())).small().strong());
        let mut clicked = None;
        let mut action = None;
        let frame = egui::Frame::group(ui.style()).inner_margin(2.0);
        let (_, dropped) = ui.dnd_drop_zone::<usize, ()>(frame, |ui| {
            ui.set_min_size(Vec2::new(ui.available_width(), height));
            egui::ScrollArea::vertical().id_salt(title.get(Language::English)).auto_shrink(false).max_height(height).show_rows(
                ui,
                ROW,
                rows.len(),
                |ui, range| {
                    for &i in &rows[range] {
                        let response = self.row(ui, i);
                        response.dnd_set_drag_payload(i);
                        if response.clicked() {
                            clicked = Some(i);
                        }
                        response.context_menu(|ui| {
                            for (to, text) in [
                                (Bucket::Max, Text::CatalogToMax),
                                (Bucket::Preview, Text::CatalogToPreview),
                                (Bucket::Maps, Text::CatalogToMaps),
                                (Bucket::Skip, Text::CatalogToSkip),
                            ] {
                                if to != bucket && ui.button(self.t(text)).clicked() {
                                    action = Some((i, Some(to)));
                                    ui.close();
                                }
                            }
                            if bucket == Bucket::Preview && rows.first() != Some(&i) && ui.button(self.t(Text::CatalogFirst)).clicked() {
                                action = Some((i, None));
                                ui.close();
                            }
                        });
                    }
                },
            );
            if bucket == Bucket::Preview && rows.is_empty() {
                ui.centered_and_justified(|ui| ui.weak(self.t(Text::CatalogPreviewHint)));
            }
        });
        if let Some(i) = clicked {
            self.select(i, ui.input(|s| (s.modifiers.command, s.modifiers.shift)));
        }
        if let Some(i) = dropped {
            action = Some((*i, Some(bucket)));
        }
        if self.busy() {
            return;
        }
        match action {
            Some((i, Some(to))) => self.move_to(i, to),
            Some((i, None)) => {
                let entry = self.entries.remove(i);
                self.entries.insert(0, entry);
            }
            None => {}
        }
    }

    fn select(&mut self, i: usize, (ctrl, shift): (bool, bool)) {
        let bucket = self.entries[i].bucket;
        let anchor = self.entries.iter().position(|e| e.selected && e.bucket == bucket).filter(|_| shift).unwrap_or(i);
        let range = anchor.min(i)..=anchor.max(i);
        for (n, entry) in self.entries.iter_mut().enumerate() {
            entry.selected = match (ctrl, n == i) {
                (true, true) => !entry.selected,
                (true, false) => entry.selected,
                (false, _) => range.contains(&n) && entry.bucket == bucket,
            };
        }
    }

    /// Moves an entry, and the selected ones with it when it is one of them.
    fn move_to(&mut self, i: usize, to: Bucket) {
        let (from, all) = (self.entries[i].bucket, self.entries[i].selected);
        for (n, entry) in self.entries.iter_mut().enumerate() {
            let goes = n == i || (all && entry.selected && entry.bucket == from);
            entry.selected = goes;
            if goes {
                entry.bucket = to;
            }
        }
    }

    fn row(&mut self, ui: &mut egui::Ui, i: usize) -> egui::Response {
        let entry = &self.entries[i];
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        if entry.selected {
            painter.rect_filled(rect, 0.0, visuals.selection.bg_fill);
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
        }
        let key = entry.item.path.display().to_string();
        let box_ = egui::Rect::from_min_size(rect.min + Vec2::splat(2.0), Vec2::splat(ROW - 4.0));
        match self.textures.get(&key) {
            Some(Some(texture)) => {
                let size = texture.size_vec2();
                let fit = egui::Rect::from_center_size(box_.center(), size * (box_.width() / size.max_elem()));
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(texture.id(), fit, uv, Color32::WHITE);
            }
            Some(None) => {}
            None if (entry.item.is_image() && entry.item.size < (40 << 20)) || entry.item.is_max() => self.loader.request(&key, &entry.item.path),
            None => {}
        }
        let engine = match (entry.item.is_max(), entry.engine) {
            (false, _) => "",
            (true, Engine::Corona) => "  ·  Corona",
            (true, Engine::VRay) => "  ·  V-Ray",
            (true, Engine::FStorm) => "  ·  FStorm",
            (true, Engine::Other) => Text::EngineUnknown.get(self.lang),
        };
        let left = box_.right() + 6.0;
        let font = egui::FontId::proportional(12.5);
        painter.text(egui::pos2(left, rect.top() + 5.0), egui::Align2::LEFT_TOP, &entry.item.name, font, visuals.text_color());
        let small = egui::FontId::proportional(11.0);
        let detail = format!("{}{engine}", catalog::human_size(entry.item.size));
        painter.text(egui::pos2(left, rect.bottom() - 5.0), egui::Align2::LEFT_BOTTOM, detail, small, visuals.weak_text_color());
        let full = if entry.item.rel.is_empty() { entry.item.name.clone() } else { format!("{}\\{}", entry.item.rel, entry.item.name) };
        response.on_hover_text(full)
    }

    fn tree(&mut self, ui: &mut egui::Ui, height: f32) {
        let hint = self.t(Text::CatalogFindFolder);
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text(hint).desired_width(f32::INFINITY));
        let filter = self.filter.clone();
        ui.allocate_ui(Vec2::new(ui.available_width(), height), |ui| {
            foldertree::show(ui, "catalog", &self.folders, &mut self.expanded, &filter, &mut self.dest);
        });
    }

    fn new_folder(&mut self) {
        let name = catalog::clean_name(&self.field);
        let Some(parent) = self.dest.clone().filter(|_| !name.is_empty()) else { return };
        let path = parent.join(&name);
        if let Err(error) = std::fs::create_dir_all(&path) {
            self.status = fill(self.t(Text::CatalogSaveFailed), &[("error", &error)]);
            return;
        }
        foldertree::insert(&mut self.folders, &parent, &path);
        self.expanded.insert(norm_key(&parent));
        self.filter.clear();
        self.dest = Some(path);
    }

    fn options(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(self.t(Text::CatalogName).to_uppercase()).small().strong());
        ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(self.t(Text::CatalogWhere).to_uppercase()).small().strong());
        let height = (ui.available_height() - 260.0).max(80.0);
        self.tree(ui, height);
        ui.horizontal(|ui| {
            if ui.add_enabled(self.dest.is_some() && !self.busy(), egui::Button::new(self.t(Text::CatalogNewFolder))).clicked() {
                self.field.clear();
                self.ask = Some(Ask::NewFolder);
            }
            ui.label(self.t(Text::CatalogMapsIn));
            ui.add(egui::TextEdit::singleline(&mut self.maps_folder).desired_width(100.0));
        });
        let maxes = self.maxes();
        let can = catalog::can_make_corona(&maxes);
        let (corona, layers) = (self.t(Text::CatalogToCorona), self.t(Text::CatalogCleanLayers));
        ui.add_enabled(can || maxes.is_empty(), egui::Checkbox::new(&mut self.to_corona, corona)).on_hover_text(self.t(Text::CatalogToCoronaHint));
        ui.checkbox(&mut self.clean_layers, layers);
        ui.separator();
        self.summary(ui);
    }

    fn summary(&self, ui: &mut egui::Ui) {
        let Some(plan) = self.plan() else {
            let text = if self.source.is_none() {
                Text::CatalogHint
            } else if catalog::clean_name(&self.name).is_empty() {
                Text::CatalogNeedName
            } else if self.dest.is_none() {
                Text::CatalogNeedFolder
            } else {
                Text::CatalogNeedFiles
            };
            ui.weak(self.t(text));
            return;
        };
        egui::ScrollArea::vertical().id_salt("catalog-summary").max_height(90.0).show(ui, |ui| {
            let top: Vec<&PathBuf> = plan.ops.iter().map(|(_, d)| d).filter(|d| d.parent() == self.dest.as_deref()).collect();
            for dst in &top {
                ui.label(file_name(dst));
            }
            for (_, dst) in &plan.corona {
                ui.label(format!("{}  ·  {}", file_name(dst), self.t(Text::CatalogMade)));
            }
            if let Some(dir) = &plan.maps_dir {
                let count = plan.ops.len() - top.len();
                let maps = dir.parent().map(file_name).unwrap_or_default();
                let values: [(&str, &dyn std::fmt::Display); 2] = [("count", &count), ("skipped", &plan.skipped)];
                ui.label(format!("{maps}\\{}\\  ·  {}", file_name(dir), fill(self.t(Text::CatalogMapsCount), &values)));
            }
            if self.paths(Bucket::Preview).is_empty() {
                ui.weak(self.t(Text::CatalogNoPreview));
            }
        });
    }

    fn question(&mut self, ctx: &egui::Context) {
        let Some(ask) = self.ask else { return };
        let text = match ask {
            Ask::NoMax => self.t(Text::CatalogAskNoMax).to_string(),
            Ask::Overwrite => {
                let list = self.plan().map(|p| p.conflicts()).unwrap_or_default();
                let names: Vec<String> = list.iter().take(10).map(|p| p.display().to_string()).collect();
                let values: [(&str, &dyn std::fmt::Display); 2] = [("count", &list.len()), ("files", &names.join("\n"))];
                fill(self.t(Text::CatalogAskOverwrite), &values)
            }
            Ask::Password { wrong: false } => self.t(Text::CatalogPassword).to_string(),
            Ask::Password { wrong: true } => self.t(Text::CatalogWrongPassword).to_string(),
            Ask::NewFolder => {
                let parent = self.dest.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
                fill(self.t(Text::CatalogAskFolder), &[("folder", &parent)])
            }
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("openlibrary-catalog-ask")).show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.label(text);
            if matches!(ask, Ask::Password { .. } | Ask::NewFolder) {
                let field = ui.add(egui::TextEdit::singleline(&mut self.field).desired_width(f32::INFINITY));
                field.request_focus();
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    choice = Some(true);
                }
            }
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new(self.t(Text::Continue)).strong()).clicked() {
                    choice = Some(true);
                }
                if ui.button(self.t(Text::Cancel)).clicked() {
                    choice = Some(false);
                }
            });
        });
        let Some(yes) = choice else { return };
        self.ask = None;
        match ask {
            Ask::Password { .. } if yes && !self.field.is_empty() => {
                let password = std::mem::take(&mut self.field);
                self.extract(Some(password));
            }
            Ask::Password { .. } => {
                self.source = None;
                self.status = self.t(Text::Cancelled).to_string();
            }
            Ask::NewFolder if yes => self.new_folder(),
            Ask::NoMax | Ask::Overwrite if yes => self.save(Some(ask)),
            _ => {}
        }
    }

    fn bottom(&mut self, ui: &mut egui::Ui, answer: &mut Answer) {
        ui.horizontal(|ui| {
            let ready = !self.busy() && self.plan().is_some();
            let go = egui::Button::new(egui::RichText::new(self.t(Text::CatalogGo)).strong());
            if ui.add_enabled(ready, go).clicked() {
                self.save(None);
            }
            if let Some(task) = &self.task {
                if ui.add_enabled(!task.cancelled(), egui::Button::new(self.t(Text::Cancel))).clicked() {
                    task.cancel();
                }
            } else if ui.button(self.t(Text::Close)).clicked() {
                answer.close = true;
            }
            if let Some(saving) = &self.saving {
                let part = saving.done as f32 / saving.plan.ops.len().max(1) as f32;
                ui.add(egui::ProgressBar::new(part).desired_width(160.0));
            }
            if !self.busy() && self.plan().is_some_and(|p| !p.conflicts().is_empty()) {
                ui.colored_label(WARN, self.t(Text::CatalogExists));
            }
        });
    }

    /// Draws the window over the whole of the app's.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Answer {
        let mut answer = Answer::default();
        let ctx = ui.ctx().clone();
        self.poll(&mut answer);
        self.take_thumbs(&ctx);
        self.take_drops(&ctx);
        egui::Panel::top("catalog-top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("catalog-bottom").show(ui, |ui| {
            ui.add_space(4.0);
            self.bottom(ui, &mut answer);
            ui.add_space(4.0);
        });
        egui::Panel::right("catalog-options").resizable(true).default_size(360.0).show(ui, |ui| self.options(ui));
        egui::Panel::left("catalog-skip").resizable(true).default_size(260.0).show(ui, |ui| {
            let height = ui.available_height() - 40.0;
            self.bucket(ui, Bucket::Skip, Text::CatalogSkip, height);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let height = (ui.available_height() - 110.0).max(200.0);
            self.bucket(ui, Bucket::Max, Text::CatalogMax, height * 0.25);
            self.bucket(ui, Bucket::Preview, Text::CatalogPreview, height * 0.25);
            self.bucket(ui, Bucket::Maps, Text::CatalogMaps, height * 0.5);
        });
        self.question(&ctx);
        answer
    }
}

impl Drop for CatalogWindow {
    fn drop(&mut self) {
        self.task = None; // stops the work before its folder goes
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}
