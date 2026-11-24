// SPDX-License-Identifier: GPL-3.0-or-later
//! The window of the external files a model does not find: each can be pointed at another file or removed, and
//! nothing changes until they are saved in the `.max`, which a 3ds Max in the background does.

use crate::kit::{Kind, button};
use crate::maxbatch::{self, Outcome};
use crate::missing::{self, Change, Edit, State, base_name, folder_name};
use crate::open3d_settings::Language;
use crate::task::{Task, Wake};
use crate::texts::{Text, fill};
use eframe::egui::{self, Color32};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

struct Row {
    file: PathBuf,
    kind: String,
    stored: String,
    state: State,
    change: Option<Change>,
    selected: bool,
}

enum Event {
    Rows(Vec<Row>),
    Found(HashMap<String, PathBuf>),
    Saved(usize, Outcome),
    SaveEnded(Result<(), String>),
}

enum Ask {
    Save,
    Close,
}

/// What the menu of a row does.
#[derive(Clone, Copy)]
enum Act {
    Relink,
    Remove,
    Undo,
    Copy,
}

pub struct MissingWindow {
    name: String,
    files: Vec<PathBuf>,
    roots: Vec<PathBuf>,
    work_dir: PathBuf,
    rows: Vec<Row>,
    task: Option<Task<Event>>,
    saving: Vec<PathBuf>,
    changed: u64,
    failed: Vec<String>,
    status: String,
    ask: Option<Ask>,
    last_dir: PathBuf,
    lang: Language,
    wake: Wake,
}

/// What the window asks of the app after a frame.
#[derive(Default)]
pub struct Answer {
    pub close: bool,
    /// The `.max` written.
    pub saved: Vec<PathBuf>,
}

impl MissingWindow {
    pub fn new(name: &str, (files, roots): (Vec<PathBuf>, Vec<PathBuf>), data_dir: &Path, lang: Language, wake: Wake) -> Self {
        let last_dir = files.first().and_then(|f| f.parent()).map(Path::to_path_buf).unwrap_or_default();
        let mut window = Self {
            name: name.to_string(),
            files,
            roots,
            work_dir: data_dir.join("batch").join(format!("paths-{}", std::process::id())),
            rows: Vec::new(),
            task: None,
            saving: Vec::new(),
            changed: 0,
            failed: Vec::new(),
            status: String::new(),
            ask: None,
            last_dir,
            lang,
            wake,
        };
        window.read();
        window
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    fn read(&mut self) {
        let (files, roots) = (self.files.clone(), self.roots.clone());
        self.task = Some(Task::start(self.wake.clone(), move |link| {
            let mut rows = Vec::new();
            for file in files {
                for e in missing::externals(&file, &roots).into_iter().filter(|e| e.state != State::Found) {
                    rows.push(Row { file: file.clone(), kind: e.kind, stored: e.stored, state: e.state, change: None, selected: false });
                }
            }
            link.send(Event::Rows(rows));
        }));
    }

    fn poll(&mut self, answer: &mut Answer) {
        let Some(task) = &self.task else { return };
        for event in task.poll() {
            match event {
                Event::Rows(rows) => {
                    self.rows = rows;
                    self.task = None;
                }
                Event::Found(found) => {
                    let wanted: HashSet<String> = self.target_rows().into_iter().map(|i| base_name(&self.rows[i].stored).to_lowercase()).collect();
                    for i in self.target_rows() {
                        if let Some(path) = found.get(&base_name(&self.rows[i].stored).to_lowercase()) {
                            self.rows[i].change = Some(Change::To(path.clone()));
                        }
                    }
                    self.status = fill(self.t(Text::MissingFound), &[("found", &found.len()), ("count", &wanted.len())]);
                    self.task = None;
                }
                Event::Saved(_, Ok(fields)) => self.changed += fields.first().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0),
                Event::Saved(n, Err(_)) => {
                    self.failed.extend(self.saving.get(n).and_then(|f| f.file_name()).map(|f| f.to_string_lossy().into_owned()));
                }
                Event::SaveEnded(result) => {
                    self.task = None;
                    self.status = match (result, self.failed.is_empty()) {
                        (Err(error), _) => fill(self.t(Text::BatchFailed), &[("error", &error)]),
                        (Ok(()), true) => fill(self.t(Text::MissingSaved), &[("count", &self.changed)]),
                        (Ok(()), false) => fill(self.t(Text::MissingNotSaved), &[("files", &self.failed.join(", "))]),
                    };
                    answer.saved = std::mem::take(&mut self.saving);
                    self.read();
                }
            }
            if self.task.is_none() {
                break;
            }
        }
    }

    fn pending(&self) -> Vec<Edit> {
        self.rows.iter().filter_map(|r| Some((r.file.clone(), r.stored.clone(), r.change.clone()?))).collect()
    }

    fn selected(&self) -> Vec<usize> {
        (0..self.rows.len()).filter(|&i| self.rows[i].selected).collect()
    }

    /// The selected rows, or all of them when there are none.
    fn target_rows(&self) -> Vec<usize> {
        let selected = self.selected();
        if selected.is_empty() { (0..self.rows.len()).collect() } else { selected }
    }

    fn save(&mut self) {
        let Some(exe) = maxbatch::find() else {
            self.status = self.t(Text::NoMaxBatch).to_string();
            return;
        };
        let (job, files) = missing::job(&self.pending());
        (self.saving, self.changed, self.failed) = (files, 0, Vec::new());
        self.status = self.t(Text::MissingSaving).to_string();
        let dir = self.work_dir.clone();
        self.task = Some(Task::start(self.wake.clone(), move |link| {
            let result = maxbatch::run(&exe, &dir, &job, &|| link.cancelled(), &mut |n, o| link.send(Event::Saved(n, o)));
            link.send(Event::SaveEnded(result));
        }));
    }

    fn relink(&mut self) {
        let selected = self.selected();
        if let [only] = selected[..] {
            let name = base_name(&self.rows[only].stored).to_string();
            if let Some(path) = rfd::FileDialog::new().set_directory(&self.last_dir).set_file_name(name).pick_file() {
                self.last_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
                self.rows[only].change = Some(Change::To(path));
            }
            return;
        }
        let Some(folder) = rfd::FileDialog::new().set_directory(&self.last_dir).pick_folder() else { return };
        let mut hits = 0;
        for &i in &selected {
            let path = folder.join(base_name(&self.rows[i].stored));
            if path.is_file() {
                hits += 1;
                self.rows[i].change = Some(Change::To(path));
            }
        }
        self.status = fill(self.t(Text::MissingFound), &[("found", &hits), ("count", &selected.len())]);
        self.last_dir = folder;
    }

    fn search(&mut self) {
        let Some(folder) = rfd::FileDialog::new().set_directory(&self.last_dir).pick_folder() else { return };
        let names: HashSet<String> = self.target_rows().into_iter().map(|i| base_name(&self.rows[i].stored).to_lowercase()).collect();
        self.status = fill(self.t(Text::MissingSearching), &[("folder", &folder.display())]);
        self.last_dir = folder.clone();
        self.task = Some(Task::start(self.wake.clone(), move |link| {
            link.send(Event::Found(missing::find_by_name(&folder, &names, &|| link.cancelled())));
        }));
    }

    fn buttons(&mut self, ui: &mut egui::Ui) {
        let idle = self.task.is_none();
        let some = !self.selected().is_empty();
        let disposable = |r: &Row| r.state == State::Missing && r.change.is_none() && missing::is_disposable(&r.kind, &r.stored);
        let junk = self.rows.iter().filter(|r| disposable(r)).count();
        let near = self.rows.iter().filter(|r| r.state == State::Nearby && r.change.is_none()).count();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled_ui(idle && some, |ui| button(ui, Kind::Action, None, self.t(Text::MissingRelink))).inner.clicked() {
                self.relink();
            }
            if ui.add_enabled_ui(idle && !self.rows.is_empty(), |ui| button(ui, Kind::Action, None, self.t(Text::MissingSearch))).inner.clicked() {
                self.search();
            }
            if ui
                .add_enabled_ui(idle && near > 0, |ui| button(ui, Kind::Action, None, &fill(self.t(Text::MissingUseNearby), &[("count", &near)])))
                .inner
                .clicked()
            {
                for row in self.rows.iter_mut().filter(|r| r.state == State::Nearby && r.change.is_none()) {
                    row.change = missing::nearby(&row.file, &row.stored).map(Change::To);
                }
            }
            if ui.add_enabled_ui(idle && some, |ui| button(ui, Kind::Action, None, self.t(Text::MissingRemove))).inner.clicked() {
                self.rows.iter_mut().filter(|r| r.selected).for_each(|r| r.change = Some(Change::Remove));
            }
            if ui
                .add_enabled_ui(idle && junk > 0, |ui| button(ui, Kind::Action, None, &fill(self.t(Text::MissingRemoveJunk), &[("count", &junk)])))
                .inner
                .clicked()
            {
                self.rows.iter_mut().filter(|r| disposable(r)).for_each(|r| r.change = Some(Change::Remove));
            }
            if ui.add_enabled_ui(idle && some, |ui| button(ui, Kind::Action, None, self.t(Text::Undo))).inner.clicked() {
                self.rows.iter_mut().filter(|r| r.selected).for_each(|r| r.change = None);
            }
            if ui.add_enabled_ui(!self.rows.is_empty(), |ui| button(ui, Kind::Action, None, self.t(Text::CopyList))).inner.clicked() {
                let list: Vec<&str> = self.rows.iter().map(|r| r.stored.as_str()).collect();
                ui.ctx().copy_text(list.join("\n"));
            }
        });
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let (remove, near, gone) = (self.t(Text::MissingToRemove), self.t(Text::MissingNearby), self.t(Text::Missing));
        let height = ui.text_style_height(&egui::TextStyle::Body) * 2.0 + 8.0;
        let count = self.rows.len();
        let several = self.files.len() > 1;
        let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
        let mut clicked = None;
        let mut act = None;
        let menu = [
            (self.t(Text::MissingRelink), Act::Relink),
            (self.t(Text::MissingRemove), Act::Remove),
            (self.t(Text::Undo), Act::Undo),
            (self.t(Text::CopyPath), Act::Copy),
        ];
        egui::ScrollArea::vertical().auto_shrink(false).max_height(360.0).show_rows(ui, height, count, |ui, range| {
            for i in range {
                let row = &self.rows[i];
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click());
                let painter = ui.painter_at(rect);
                let visuals = ui.visuals();
                if row.selected {
                    painter.rect_filled(rect, 0.0, visuals.selection.bg_fill);
                } else if response.hovered() {
                    painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
                }
                let (state, color) = match &row.change {
                    Some(Change::Remove) => (remove.to_string(), Color32::from_rgb(0xD0, 0x6A, 0x3A)),
                    Some(Change::To(path)) => (format!("-> {}", path.display()), Color32::from_rgb(0x4C, 0xA3, 0x5A)),
                    None if row.state == State::Nearby => (near.to_string(), visuals.weak_text_color()),
                    None => (gone.to_string(), visuals.weak_text_color()),
                };
                let file = row.file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                let place = if several { format!("{file}  ·  {}", folder_name(&row.stored)) } else { folder_name(&row.stored).to_string() };
                let font = egui::TextStyle::Body.resolve(ui.style());
                let left = rect.left() + 6.0;
                let middle = rect.left() + rect.width() * 0.55;
                let part = painter.with_clip_rect(egui::Rect::from_min_max(rect.min, egui::pos2(middle - 6.0, rect.bottom())));
                part.text(egui::pos2(left, rect.top() + 2.0), egui::Align2::LEFT_TOP, base_name(&row.stored), font.clone(), visuals.text_color());
                let small = egui::FontId::proportional(font.size * 0.85);
                part.text(egui::pos2(left, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, place, small, visuals.weak_text_color());
                painter.text(egui::pos2(middle, rect.center().y), egui::Align2::LEFT_CENTER, &state, font, color);
                if response.clicked() {
                    clicked = Some(i);
                }
                if response.double_clicked() {
                    act = Some((i, Act::Relink));
                }
                response.context_menu(|ui| {
                    for (text, what) in &menu {
                        if ui.button(*text).clicked() {
                            act = Some((i, *what));
                        }
                    }
                });
                response.on_hover_text(format!("{}\n{state}", row.stored));
            }
        });
        if let Some((i, act)) = act.filter(|_| self.task.is_none()) {
            if !self.rows[i].selected {
                self.rows.iter_mut().enumerate().for_each(|(n, r)| r.selected = n == i);
            }
            let selected = self.rows.iter_mut().filter(|r| r.selected);
            match act {
                Act::Relink => self.relink(),
                Act::Remove => selected.for_each(|r| r.change = Some(Change::Remove)),
                Act::Undo => selected.for_each(|r| r.change = None),
                Act::Copy => {
                    let list: Vec<&str> = self.rows.iter().filter(|r| r.selected).map(|r| r.stored.as_str()).collect();
                    ui.ctx().copy_text(list.join("\n"));
                }
            }
            return;
        }
        if let Some(i) = clicked {
            let anchor = self.rows.iter().position(|r| r.selected).filter(|_| shift).unwrap_or(i);
            let range = anchor.min(i)..=anchor.max(i);
            for (n, row) in self.rows.iter_mut().enumerate() {
                row.selected = match (ctrl, n == i) {
                    (true, true) => !row.selected,
                    (true, false) => row.selected,
                    (false, _) => range.contains(&n),
                };
            }
        }
    }

    fn question(&mut self, ui: &mut egui::Ui, answer: &mut Answer) {
        let pending = self.pending();
        let text = match self.ask {
            Some(Ask::Save) => {
                let removed = pending.iter().filter(|(_, _, c)| *c == Change::Remove).count();
                let files: HashSet<&PathBuf> = pending.iter().map(|(f, _, _)| f).collect();
                let values: [(&str, &dyn std::fmt::Display); 3] =
                    [("files", &files.len()), ("linked", &(pending.len() - removed)), ("removed", &removed)];
                fill(self.t(Text::MissingAskSave), &values)
            }
            Some(Ask::Close) => self.t(Text::MissingAskClose).to_string(),
            None => return,
        };
        ui.label(text);
        ui.horizontal(|ui| {
            if button(ui, Kind::Primary, None, self.t(Text::Continue)).clicked() {
                match self.ask.take() {
                    Some(Ask::Save) => self.save(),
                    _ => answer.close = true,
                }
            }
            if button(ui, Kind::Action, None, self.t(Text::Cancel)).clicked() {
                self.ask = None;
            }
        });
    }

    /// Draws the window.
    pub fn show(&mut self, ctx: &egui::Context) -> Answer {
        let mut answer = Answer::default();
        self.poll(&mut answer);
        let title = fill(self.t(Text::MissingTitle), &[("name", &self.name)]);
        egui::Modal::new(egui::Id::unique("openlibrary-missing")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 80.0).clamp(320.0, 900.0));
            ui.heading(title);
            let head = if self.task.is_none() && self.rows.is_empty() {
                self.t(Text::MissingNothing).to_string()
            } else {
                fill(self.t(Text::MissingHead), &[("count", &self.rows.len())])
            };
            ui.label(head);
            ui.separator();
            self.table(ui);
            ui.separator();
            if self.ask.is_some() {
                self.question(ui, &mut answer);
                return;
            }
            self.buttons(ui);
            ui.label(egui::RichText::new(&self.status).small());
            ui.horizontal(|ui| {
                let pending = self.pending().len();
                let save = fill(self.t(Text::MissingSave), &[("count", &pending)]);
                let can = pending > 0 && self.task.is_none();
                if ui.add_enabled_ui(can, |ui| button(ui, Kind::Primary, None, &save)).inner.clicked() {
                    self.ask = Some(Ask::Save);
                }
                // closing in the middle of a save would leave the .max half done
                if ui.add_enabled_ui(self.saving.is_empty(), |ui| button(ui, Kind::Action, None, self.t(Text::Close))).inner.clicked() {
                    if pending > 0 {
                        self.ask = Some(Ask::Close);
                    } else {
                        answer.close = true;
                    }
                }
                if self.task.is_some() {
                    ui.spinner();
                }
            });
        });
        answer
    }
}
