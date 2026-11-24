// SPDX-License-Identifier: GPL-3.0-or-later
//! The settings window: the tabs and their folders, how models come in, previews, the bridge in each 3ds Max,
//! the team's workspace and the language. Nothing changes until it is accepted, but the bridge.

use crate::bridge_install::MaxTarget;
use crate::kit::{self, Kind};
use crate::max_session::MaxSession;
use crate::open3d_settings::{self, Language};
use crate::settings::workspace::{self, WORKSPACE_FILE, WsState};
use crate::settings::{KINDS, SHOW_MODES, Settings};
use crate::task::Task;
use crate::texts::{Text, fill};
use eframe::egui;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Tabs,
    Import,
    Preview,
    Max,
    Workspace,
    Language,
}

pub struct SettingsWindow {
    lang: Language,
    page: Page,
    libraries: Vec<Map<String, Value>>,
    tab: usize,
    /// The folder selected in the list of the tab, and in that of the textures.
    folder: Option<usize>,
    relink: Option<usize>,
    /// The folders asked for, and those that cannot be reached.
    looked: Vec<String>,
    looking: Option<Task<Vec<String>>>,
    lost: Vec<String>,
    import: Map<String, Value>,
    preview: Map<String, Value>,
    shared: bool,
    workspace: String,
    language: Option<Language>,
    old_language: Option<Language>,
    targets: Vec<MaxTarget>,
    toolbar: bool,
    remove: Option<MaxTarget>,
    /// The team's workspace would take the place of the person's tabs: asked first.
    ask_theirs: bool,
    status: String,
    first: bool,
}

#[derive(Default)]
pub struct Answer {
    pub close: bool,
    /// The settings were saved: what is shown has to be read again.
    pub saved: bool,
    pub language: Option<Language>,
}

fn object(settings: &Settings, key: &str) -> Map<String, Value> {
    settings.data.get(key).and_then(Value::as_object).cloned().unwrap_or_default()
}

fn text_of(map: &Map<String, Value>, key: &str) -> String {
    map.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// A drop-down of a text value; `true` when it changed.
fn choice(ui: &mut egui::Ui, label: &str, map: &mut Map<String, Value>, key: &str, options: &[(&str, &str)]) -> bool {
    let current = text_of(map, key);
    let shown = options.iter().find(|(v, _)| *v == current).or_else(|| options.first()).map_or("", |(_, t)| *t);
    let mut picked = None;
    kit::field(ui, label, |ui| {
        let width = ui.available_width().min(340.0);
        egui::ComboBox::from_id_salt(key).selected_text(shown).width(width).truncate().show_ui(ui, |ui| {
            for (value, text) in options {
                if ui.selectable_label(*value == current, *text).clicked() {
                    picked = Some(*value);
                }
            }
        });
    });
    if let Some(value) = picked {
        map.insert(key.into(), value.into());
    }
    picked.is_some()
}

fn flag(ui: &mut egui::Ui, label: &str, map: &mut Map<String, Value>, key: &str, default: bool) {
    let mut on = map.get(key).and_then(Value::as_bool).unwrap_or(default);
    if ui.checkbox(&mut on, label).changed() {
        map.insert(key.into(), on.into());
    }
}

fn number(ui: &mut egui::Ui, label: &str, map: &mut Map<String, Value>, key: &str, range: std::ops::RangeInclusive<f64>, whole: bool) {
    let mut value = map.get(key).and_then(Value::as_f64).unwrap_or(*range.start());
    kit::field(ui, label, |ui| {
        let speed = if whole { 8.0 } else { 0.05 };
        let drag = egui::DragValue::new(&mut value).range(range).speed(speed).max_decimals(if whole { 0 } else { 2 });
        if ui.add_sized([110.0, 26.0], drag).changed() {
            map.insert(key.into(), if whole { json!(value.round() as i64) } else { json!(value) });
        }
    });
}

fn list_of(map: &Map<String, Value>, key: &str) -> Vec<String> {
    map.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// The frame of a list, as tall as it is told.
fn list_box(ui: &mut egui::Ui, id: &str, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    let p = kit::pal(ui);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), height), egui::Sense::hover());
    ui.painter().rect(rect, 0.0, p.surface, egui::Stroke::new(1.0, p.border_hi), egui::StrokeKind::Inside);
    kit::inside(ui, rect.shrink(4.0), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show(ui, add);
    });
}

/// A row of a list, the whole width of it; `true` when it was clicked.
fn list_row(ui: &mut egui::Ui, on: bool, text: &str, note: &str, color: Option<egui::Color32>) -> bool {
    let p = kit::pal(ui);
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), 28.0), egui::Sense::click());
    if on {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, p.surface_hi);
    }
    let room = rect.width() - if note.is_empty() { 16.0 } else { 56.0 };
    let at = rect.left_center() + egui::Vec2::new(8.0, 0.0);
    let ink = color.unwrap_or(p.text);
    kit::elided(ui.painter(), at, egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(13.5), ink, room);
    let at = rect.right_center() - egui::Vec2::new(8.0, 0.0);
    ui.painter().text(at, egui::Align2::RIGHT_CENTER, note, egui::FontId::proportional(12.0), p.muted);
    response.on_hover_text(text).clicked()
}

/// The buttons under a list of folders. `at` is the one selected.
fn folder_buttons(ui: &mut egui::Ui, lang: Language, list: &mut Vec<String>, at: &mut Option<usize>) {
    ui.horizontal(|ui| {
        if kit::button(ui, Kind::Action, Some("plus"), Text::SetAddFolder.get(lang)).clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            let path = path.display().to_string();
            if !list.contains(&path) {
                list.push(path);
                *at = Some(list.len() - 1);
            }
        }
        let some = at.is_some_and(|i| i < list.len());
        ui.add_enabled_ui(some, |ui| {
            if kit::button(ui, Kind::Action, None, Text::SetChangeFolder.get(lang)).clicked()
                && let Some(old) = at.and_then(|i| list.get_mut(i))
                && let Some(path) = rfd::FileDialog::new().set_directory(&*old).pick_folder()
            {
                *old = path.display().to_string();
            }
            if kit::button(ui, Kind::Action, None, Text::SetRemoveTab.get(lang)).clicked()
                && let Some(i) = at.take()
            {
                list.remove(i);
            }
        });
    });
}

impl SettingsWindow {
    pub fn new(settings: &Settings, max: &MaxSession, lang: Language, page: Page, first: bool) -> Self {
        let targets = max.targets();
        let dir = open3d_settings::settings_dir();
        let language = dir.as_deref().and_then(open3d_settings::read_language);
        let workspace = settings.workspace();
        Self {
            lang,
            page,
            libraries: settings.libraries().iter().filter_map(Value::as_object).cloned().collect(),
            tab: settings.current_library(),
            folder: None,
            relink: None,
            looked: Vec::new(),
            looking: None,
            lost: Vec::new(),
            import: object(settings, "import"),
            preview: object(settings, "preview"),
            shared: workspace.is_some(),
            workspace: workspace.map(|w| w.display().to_string()).unwrap_or_default(),
            language,
            old_language: language,
            toolbar: targets.iter().all(MaxTarget::toolbar),
            targets,
            remove: None,
            ask_theirs: false,
            status: String::new(),
            first,
        }
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    /// The workspace chosen, if one is.
    fn chosen(&self) -> Option<PathBuf> {
        let text = self.workspace.trim();
        (self.shared && !text.is_empty()).then(|| PathBuf::from(text))
    }

    /// The tabs of the window at the left, with what each shows; at the right, the folders of the one selected.
    fn tabs(&mut self, ui: &mut egui::Ui) {
        self.look_at_folders();
        let height = ui.available_height().max(340.0);
        let whole = egui::Rect::from_min_size(ui.cursor().min, egui::Vec2::new(ui.available_width(), height));
        let left = egui::Rect::from_min_size(whole.min, egui::Vec2::new(whole.width() * 0.42 - 10.0, height));
        let right = egui::Rect::from_min_max(egui::pos2(left.right() + 20.0, whole.top()), whole.max);
        kit::inside(ui, left, |ui| self.tab_list(ui));
        kit::inside(ui, right, |ui| self.tab_folders(ui));
        ui.painter().vline(left.right() + 10.0, whole.y_range(), egui::Stroke::new(1.0, kit::pal(ui).border));
        ui.allocate_rect(whole, egui::Sense::hover());
    }

    fn tab_list(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        ui.label(egui::RichText::new(Text::SetTabs.get(lang)).font(kit::strong(14.0)));
        let height = (ui.available_height() - 176.0).max(110.0);
        list_box(ui, "settings-tabs", height, |ui| {
            for i in 0..self.libraries.len() {
                let count = list_of(&self.libraries[i], "paths").len();
                if list_row(ui, self.tab == i, &text_of(&self.libraries[i], "name"), &count.to_string(), None) {
                    (self.tab, self.folder) = (i, None);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if kit::icon_button(ui, "plus", Text::SetNewTab.get(lang), false).clicked() {
                let mut tab = Map::new();
                tab.insert("name".into(), Text::SetNewTab.get(lang).into());
                tab.insert("kind".into(), "models".into());
                tab.insert("paths".into(), json!([]));
                self.libraries.push(tab);
                (self.tab, self.folder) = (self.libraries.len() - 1, None);
            }
            let some = self.tab < self.libraries.len();
            ui.add_enabled_ui(some, |ui| {
                if kit::icon_button(ui, "minus", Text::SetRemoveThisTab.get(lang), false).clicked() && some {
                    self.libraries.remove(self.tab);
                    (self.tab, self.folder) = (self.tab.min(self.libraries.len().saturating_sub(1)), None);
                }
            });
            ui.add_enabled_ui(some && self.tab > 0, |ui| {
                if kit::icon_button(ui, "up", Text::SetMoveUp.get(lang), false).clicked() && self.tab > 0 {
                    self.libraries.swap(self.tab, self.tab - 1);
                    self.tab -= 1;
                }
            });
            ui.add_enabled_ui(self.tab + 1 < self.libraries.len(), |ui| {
                if kit::icon_button(ui, "dropdown", Text::SetMoveDown.get(lang), false).clicked() && self.tab + 1 < self.libraries.len() {
                    self.libraries.swap(self.tab, self.tab + 1);
                    self.tab += 1;
                }
            });
        });
        let Some(tab) = self.libraries.get_mut(self.tab) else { return };
        ui.add_space(4.0);
        let row = |ui: &mut egui::Ui, label: Text| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(110.0, 26.0), egui::Sense::hover());
            let ink = kit::pal(ui).muted;
            kit::elided(ui.painter(), rect.left_center(), egui::Align2::LEFT_CENTER, label.get(lang), egui::FontId::proportional(13.0), ink, 110.0);
        };
        ui.horizontal(|ui| {
            row(ui, Text::SetName);
            let mut name = text_of(tab, "name");
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(ui.available_width())).changed() {
                tab.insert("name".into(), name.into());
            }
        });
        let kinds = [Text::KindModels, Text::KindMaterials, Text::KindTextures, Text::KindInteriors].map(|t| t.get(lang));
        let shows = [Text::ShowBoth, Text::ShowFavorites, Text::ShowAll].map(|t| t.get(lang));
        let kinds: Vec<(&str, &str)> = KINDS.into_iter().zip(kinds).collect();
        let shows: Vec<(&str, &str)> = SHOW_MODES.into_iter().zip(shows).collect();
        for (label, key, options) in [(Text::SetContent, "kind", kinds), (Text::SetOpens, "show", shows)] {
            ui.horizontal(|ui| {
                row(ui, label);
                let current = text_of(tab, key);
                let shown = options.iter().find(|(v, _)| *v == current).or_else(|| options.first()).map_or("", |(_, t)| *t);
                let width = ui.available_width();
                egui::ComboBox::from_id_salt(key).selected_text(shown).width(width).truncate().show_ui(ui, |ui| {
                    for (value, text) in &options {
                        if ui.selectable_label(*value == current, *text).clicked() {
                            tab.insert(key.into(), (*value).into());
                        }
                    }
                });
            });
        }
    }

    fn tab_folders(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let Some(tab) = self.libraries.get_mut(self.tab) else {
            ui.weak(Text::SetNoTabs.get(lang));
            return;
        };
        let title = fill(Text::SetFoldersOf.get(lang), &[("name", &text_of(tab, "name"))]);
        ui.label(egui::RichText::new(title).font(kit::strong(14.0)));
        ui.label(egui::RichText::new(Text::SetRowsHint.get(lang)).small().weak());
        let mut list = list_of(tab, "paths");
        let before = list.clone();
        let height = (ui.available_height() - 66.0).max(110.0);
        let lost = kit::pal(ui).warn;
        list_box(ui, "settings-folders", height, |ui| {
            for (key, text) in [("tree_fav", Text::Favorites), ("tree_all", Text::AllFolders)] {
                ui.horizontal(|ui| {
                    ui.set_height(28.0);
                    ui.add_space(6.0);
                    flag(ui, text.get(lang), tab, key, true);
                });
            }
            ui.separator();
            for (i, path) in list.iter().enumerate() {
                let gone = self.lost.contains(path);
                let note = if gone { "!" } else { "" };
                if list_row(ui, self.folder == Some(i), path, note, gone.then_some(lost)) {
                    self.folder = Some(i);
                }
            }
        });
        folder_buttons(ui, lang, &mut list, &mut self.folder);
        if list.iter().any(|p| self.lost.contains(p)) {
            ui.label(egui::RichText::new(Text::SetLostFolder.get(lang)).small().color(lost));
        } else {
            ui.label(egui::RichText::new(Text::SetNothingDeleted.get(lang)).small().weak());
        }
        if list != before {
            tab.insert("paths".into(), json!(list));
        }
    }

    /// Finds out once, on a thread, which folders cannot be reached: a server may be slow to say.
    fn look_at_folders(&mut self) {
        let all: Vec<String> = self.libraries.iter().flat_map(|l| list_of(l, "paths")).collect();
        if all != self.looked {
            self.looked.clone_from(&all);
            let task = Task::start(std::sync::Arc::new(|| {}), move |link| {
                link.send(all.into_iter().filter(|p| !std::path::Path::new(p).is_dir()).collect());
            });
            if let Some(old) = self.looking.replace(task) {
                std::thread::spawn(move || drop(old)); // it may still be waiting for the server
            }
        }
        if let Some(lost) = self.looking.as_ref().and_then(|t| t.poll().pop()) {
            self.lost = lost;
        }
    }

    fn import(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |text: Text| text.get(lang);
        let (map, relink) = (&mut self.import, &mut self.relink);
        // in two columns, so that it all fits without scrolling
        ui.columns(2, |columns| {
            let [left, right] = columns else { return };
            kit::group(left, "Merge", |ui| {
                flag(ui, t(Text::ImpGroup), map, "group", true);
                let layers = [("asset", t(Text::LayerAsset)), ("current", t(Text::LayerCurrent)), ("file", t(Text::LayerFile))];
                choice(ui, t(Text::ImpLayer), map, "layer_mode", &layers);
                let objects = [
                    ("autorename", t(Text::DupRenameAuto)),
                    ("merge", t(Text::DupBoth)),
                    ("skip", t(Text::DupSkip)),
                    ("replace", t(Text::DupReplace)),
                    ("ask", t(Text::DupAsk)),
                ];
                choice(ui, t(Text::ImpObjects), map, "object_dups", &objects);
                let materials = [("scene", t(Text::MatScene)), ("file", t(Text::MatFile)), ("rename", t(Text::MatRename)), ("ask", t(Text::DupAsk))];
                choice(ui, t(Text::ImpMaterials), map, "material_dups", &materials);
                let missing = [("ask", t(Text::MissingAsk)), ("fallback", t(Text::MissingFallback))];
                choice(ui, t(Text::ImpMissingEngine), map, "missing_engine", &missing);
                ui.horizontal(|ui| {
                    flag(ui, t(Text::ImpLights), map, "import_lights", false);
                    ui.add_space(16.0);
                    flag(ui, t(Text::ImpCameras), map, "import_cameras", false);
                });
                flag(ui, t(Text::ImpSurface), map, "place_on_surface", true);
                flag(ui, t(Text::ImpMultiple), map, "place_multiple", true);
            });
            kit::group(right, "Merge as proxy", |ui| {
                choice(ui, t(Text::ProxyMode), map, "proxy_mode", &[("single", t(Text::ProxySingle)), ("per_object", t(Text::ProxyPerObject))]);
                let display = [("bbox", t(Text::ProxyBox)), ("preview", t(Text::ProxyPoints)), ("full", t(Text::ProxyMesh))];
                choice(ui, t(Text::ProxyDisplay), map, "proxy_display", &display);
                choice(ui, t(Text::ProxyWhere), map, "proxy_location", &[("asset", t(Text::ProxyBeside)), ("custom", t(Text::ProxyCustom))]);
                ui.add_enabled_ui(text_of(map, "proxy_location") == "custom", |ui| {
                    kit::field(ui, t(Text::Folder), |ui| {
                        let mut dir = text_of(map, "proxy_custom_dir");
                        if ui.add(egui::TextEdit::singleline(&mut dir).desired_width((ui.available_width() - 90.0).max(60.0))).changed() {
                            map.insert("proxy_custom_dir".into(), dir.into());
                        }
                        if kit::button(ui, Kind::Action, None, t(Text::Choose)).clicked()
                            && let Some(path) = rfd::FileDialog::new().pick_folder()
                        {
                            map.insert("proxy_custom_dir".into(), path.display().to_string().into());
                        }
                    });
                });
            });
            kit::group(right, t(Text::KindTextures), |ui| {
                flag(ui, t(Text::ImpRelink), map, "relink_on_import", true);
                ui.weak(t(Text::ImpDirs));
                let mut list = list_of(map, "relink_dirs");
                let before = list.clone();
                list_box(ui, "settings-relink", 92.0, |ui| {
                    for (i, path) in list.iter().enumerate() {
                        if list_row(ui, *relink == Some(i), path, "", None) {
                            *relink = Some(i);
                        }
                    }
                });
                folder_buttons(ui, lang, &mut list, relink);
                if list != before {
                    map.insert("relink_dirs".into(), json!(list));
                }
            });
        });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let map = &mut self.preview;
        kit::group(ui, Text::PrevImage.get(lang), |ui| {
            number(ui, Text::PrevSize.get(lang), map, "size", 128.0..=2048.0, true);
            let methods = [("render", Text::PrevRender.get(lang)), ("viewport", Text::PrevViewport.get(lang))];
            choice(ui, Text::PrevMethod.get(lang), map, "method", &methods);
        });
        kit::group(ui, Text::PrevCamera.get(lang), |ui| {
            number(ui, Text::PrevMargin.get(lang), map, "margin", 1.0..=2.0, false);
            number(ui, Text::PrevElevation.get(lang), map, "elevation", -80.0..=80.0, false);
            number(ui, Text::PrevAzimuth.get(lang), map, "azimuth", -180.0..=180.0, false);
            ui.weak(Text::PrevNote.get(lang));
        });
    }

    /// The bridge of each 3ds Max: what there is, and installing it or taking it out.
    fn max_page(&mut self, ui: &mut egui::Ui, max: &mut MaxSession) {
        let lang = self.lang;
        ui.weak(self.t(Text::BridgeWhat));
        let mut action = None;
        kit::group(ui, "3ds Max", |ui| {
            if self.targets.is_empty() {
                ui.label(Text::BridgeNoMax.get(lang));
            }
            let p = kit::pal(ui);
            for (i, target) in self.targets.iter().enumerate() {
                let (state, button, kind, color) = match (target.installed(), target.outdated()) {
                    (false, _) => (Text::BridgeAbsent, Text::Install, Kind::Accent, p.faint),
                    (true, true) => (Text::BridgeIsOld, Text::Update, Kind::Accent, p.warn),
                    (true, false) => (Text::BridgeInstalled, Text::InstallAgain, Kind::Action, p.ok),
                };
                ui.horizontal(|ui| {
                    ui.set_height(34.0);
                    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(150.0, 30.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.left_center() + egui::Vec2::new(6.0, 0.0), 4.0, color);
                    let at = rect.left_center() + egui::Vec2::new(20.0, 0.0);
                    kit::elided(ui.painter(), at, egui::Align2::LEFT_CENTER, &target.label(), kit::strong(13.5), p.text, 130.0);
                    ui.label(egui::RichText::new(state.get(lang)).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if target.installed() && kit::button(ui, Kind::Action, None, Text::BridgeRemove.get(lang)).clicked() {
                            action = Some((i, false));
                        }
                        if kit::button(ui, kind, None, button.get(lang)).clicked() {
                            action = Some((i, true));
                        }
                    });
                });
            }
        });
        ui.checkbox(&mut self.toolbar, Text::BridgeToolbar.get(lang));
        ui.weak(Text::BridgeRestart.get(lang));
        match action {
            Some((i, true)) => self.status = max.install_with(&self.targets[i..=i], Some(self.toolbar), self.lang),
            Some((i, false)) => self.remove = self.targets.get(i).cloned(),
            None => {}
        }
        if let Some(target) = self.remove.clone() {
            ui.separator();
            ui.label(fill(self.t(Text::BridgeAskRemove), &[("max", &target.label())]));
            ui.horizontal(|ui| {
                if kit::button(ui, Kind::Action, None, self.t(Text::BridgeRemove)).clicked() {
                    self.status = match target.uninstall() {
                        Ok(()) => fill(self.t(Text::BridgeRemoved), &[("max", &target.label())]),
                        Err(error) => fill(self.t(Text::BridgeError), &[("max", &target.label()), ("error", &error)]),
                    };
                    self.remove = None;
                }
                if kit::button(ui, Kind::Action, None, self.t(Text::Cancel)).clicked() {
                    self.remove = None;
                }
            });
        }
    }

    fn workspace_page(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        ui.weak(self.t(Text::WsWhat));
        let same = self.chosen() == settings.workspace();
        let state = match (self.chosen(), same, settings.ws_state()) {
            (None, _, _) => Text::WsLocal,
            (Some(_), true, WsState::Offline) => Text::WsOffline,
            (Some(_), true, _) => Text::WsJoined,
            // the folder may be on a server: it is looked at when it is accepted, not at every frame
            (Some(_), false, _) => Text::WsWillJoin,
        };
        kit::group(ui, Text::PageWorkspace.get(self.lang), |ui| {
            let (alone, shared) = (self.t(Text::WsAlone), self.t(Text::WsShared));
            ui.radio_value(&mut self.shared, false, alone);
            ui.radio_value(&mut self.shared, true, shared);
            ui.add_enabled_ui(self.shared, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    ui.add(egui::TextEdit::singleline(&mut self.workspace).desired_width(420.0));
                    if kit::button(ui, Kind::Action, None, Text::Choose.get(self.lang)).clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        self.workspace = workspace::workspace_dir(&path).display().to_string();
                    }
                });
            });
            ui.label(self.t(state));
        });
        ui.weak(self.t(Text::WsTip));
    }

    /// Saves everything; `false` when something has to be put right or answered first.
    fn accept(&mut self, settings: &mut Settings) -> bool {
        let new = self.chosen();
        let moved = new != settings.workspace();
        let theirs = moved && new.as_ref().is_some_and(|f| f.join(WORKSPACE_FILE).is_file());
        if theirs && !self.ask_theirs {
            self.ask_theirs = true;
            return false;
        }
        if moved && let Some(folder) = &new {
            if !folder.is_absolute() || !(folder.is_dir() || folder.parent().is_some_and(|p| p.is_dir())) {
                (self.page, self.status) = (Page::Workspace, self.t(Text::WsNotFound).to_string());
                return false;
            }
            if let Err(error) = workspace::prepare_workspace(folder) {
                (self.page, self.status) = (Page::Workspace, fill(self.t(Text::SettingsError), &[("error", &error)]));
                return false;
            }
        }
        settings.data.insert("libraries".into(), Value::Array(self.libraries.iter().cloned().map(Value::Object).collect()));
        settings.data.insert("import".into(), Value::Object(self.import.clone()));
        settings.data.insert("preview".into(), Value::Object(self.preview.clone()));
        settings.data.insert("setup_done".into(), true.into());
        let saved = if moved { settings.set_workspace(new.as_deref(), theirs) } else { settings.save() };
        if let Err(error) = saved {
            self.status = fill(self.t(Text::SettingsError), &[("error", &error)]);
            return false;
        }
        true
    }

    fn pages(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (page, text) in [
                (Page::Tabs, Text::PageTabs),
                (Page::Import, Text::PageImport),
                (Page::Preview, Text::PagePreview),
                (Page::Max, Text::PageMax),
                (Page::Workspace, Text::PageWorkspace),
                (Page::Language, Text::PageLanguage),
            ] {
                if kit::tab(ui, text.get(self.lang), self.page == page).clicked() {
                    self.page = page;
                }
            }
        });
    }

    fn bottom(&mut self, ui: &mut egui::Ui, settings: &mut Settings, answer: &mut Answer) {
        ui.label(egui::RichText::new(&self.status).small());
        if self.ask_theirs {
            ui.label(self.t(Text::WsAskTheirs));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.set_height(34.0);
            if kit::sized_button(ui, Kind::Primary, None, self.t(Text::Accept), 110.0).clicked() && self.accept(settings) {
                (answer.close, answer.saved) = (true, true);
                if self.language != self.old_language
                    && let Some(dir) = open3d_settings::settings_dir()
                {
                    match open3d_settings::write_language(&dir, self.language) {
                        Ok(()) => answer.language = Some(open3d_settings::language(Some(&dir))),
                        Err(error) => {
                            self.status = fill(self.t(Text::SettingsError), &[("error", &error)]);
                            answer.close = false;
                        }
                    }
                }
            }
            if kit::sized_button(ui, Kind::Action, None, self.t(Text::Cancel), 110.0).clicked() {
                if std::mem::take(&mut self.ask_theirs) {
                    return;
                }
                answer.close = true;
            }
        });
    }

    pub fn show(&mut self, ctx: &egui::Context, settings: &mut Settings, max: &mut MaxSession) -> Answer {
        let mut answer = Answer::default();
        let size = ctx.content_rect().size() - egui::Vec2::splat(80.0);
        egui::Modal::new(egui::Id::unique("openlibrary-settings")).show(ctx, |ui| {
            ui.set_width(size.x.clamp(400.0, 1040.0));
            ui.label(egui::RichText::new(self.t(Text::Settings)).font(kit::strong(20.0)));
            if self.first {
                ui.label(self.t(Text::Welcome));
            }
            self.pages(ui);
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(6.0);
            let height = (size.y - 180.0).clamp(160.0, 560.0);
            egui::ScrollArea::vertical().auto_shrink(false).max_height(height).show(ui, |ui| match self.page {
                Page::Tabs => self.tabs(ui),
                Page::Import => self.import(ui),
                Page::Preview => self.preview(ui),
                Page::Max => self.max_page(ui, max),
                Page::Workspace => self.workspace_page(ui, settings),
                Page::Language => {
                    ui.weak(self.t(Text::LanguageWhat));
                    let windows = self.t(Text::LanguageWindows);
                    kit::group(ui, Text::PageLanguage.get(self.lang), |ui| {
                        ui.radio_value(&mut self.language, None, windows);
                        ui.radio_value(&mut self.language, Some(Language::English), "English");
                        ui.radio_value(&mut self.language, Some(Language::Spanish), "Español");
                    });
                }
            });
            ui.separator();
            self.bottom(ui, settings, &mut answer);
        });
        answer
    }
}
