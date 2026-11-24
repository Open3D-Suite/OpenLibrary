// SPDX-License-Identifier: GPL-3.0-or-later
//! The settings window: the tabs and their folders, how models come in, previews, the bridge in each 3ds Max,
//! the team's workspace and the language. Nothing changes until it is accepted, but the bridge.

use crate::bridge_install::MaxTarget;
use crate::max_session::MaxSession;
use crate::open3d_settings::{self, Language};
use crate::settings::workspace::{self, WORKSPACE_FILE, WsState};
use crate::settings::{KINDS, SHOW_MODES, Settings};
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
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(key).selected_text(shown).width(300.0).show_ui(ui, |ui| {
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
    ui.horizontal(|ui| {
        ui.label(label);
        let speed = if whole { 8.0 } else { 0.05 };
        if ui.add(egui::DragValue::new(&mut value).range(range).speed(speed).max_decimals(if whole { 0 } else { 2 })).changed() {
            map.insert(key.into(), if whole { json!(value.round() as i64) } else { json!(value) });
        }
    });
}

/// The list of folders of a key: shown, added to and taken from.
fn folders(ui: &mut egui::Ui, lang: Language, map: &mut Map<String, Value>, key: &str) {
    let mut list: Vec<String> =
        map.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let before = list.clone();
    let mut remove = None;
    for (i, path) in list.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui.small_button("×").on_hover_text(Text::SetRemoveFolder.get(lang)).clicked() {
                remove = Some(i);
            }
            ui.label(path);
        });
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    if ui.button(Text::SetAddFolder.get(lang)).clicked()
        && let Some(path) = rfd::FileDialog::new().pick_folder()
    {
        let path = path.display().to_string();
        if !list.contains(&path) {
            list.push(path);
        }
    }
    if list != before {
        map.insert(key.into(), json!(list));
    }
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

    fn tabs(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        ui.columns(2, |columns| {
            let [left, right] = columns else { return };
            left.label(Text::SetTabs.get(lang));
            let mut swap = None;
            for i in 0..self.libraries.len() {
                let name = text_of(&self.libraries[i], "name");
                left.horizontal(|ui| {
                    if ui.small_button("↑").clicked() && i > 0 {
                        swap = Some((i, i - 1));
                    }
                    if ui.selectable_label(self.tab == i, name).clicked() {
                        self.tab = i;
                    }
                });
            }
            if let Some((a, b)) = swap {
                self.libraries.swap(a, b);
                self.tab = b;
            }
            left.horizontal(|ui| {
                if ui.button(Text::SetAddTab.get(lang)).clicked() {
                    let mut tab = Map::new();
                    tab.insert("name".into(), Text::SetNewTab.get(lang).into());
                    tab.insert("kind".into(), "models".into());
                    tab.insert("paths".into(), json!([]));
                    self.libraries.push(tab);
                    self.tab = self.libraries.len() - 1;
                }
                if ui.add_enabled(!self.libraries.is_empty(), egui::Button::new(Text::SetRemoveTab.get(lang))).clicked() {
                    if self.tab < self.libraries.len() {
                        self.libraries.remove(self.tab);
                    }
                    self.tab = self.tab.min(self.libraries.len().saturating_sub(1));
                }
            });
            left.weak(Text::SetNothingDeleted.get(lang));
            let Some(tab) = self.libraries.get_mut(self.tab) else { return };
            let mut name = text_of(tab, "name");
            right.horizontal(|ui| {
                ui.label(Text::SetName.get(lang));
                if ui.text_edit_singleline(&mut name).changed() {
                    tab.insert("name".into(), name.clone().into());
                }
            });
            let kinds = [Text::KindModels, Text::KindMaterials, Text::KindTextures, Text::KindInteriors].map(|t| t.get(lang));
            let kinds: Vec<(&str, &str)> = KINDS.into_iter().zip(kinds).collect();
            choice(right, Text::SetContent.get(lang), tab, "kind", &kinds);
            let shows = [Text::ShowBoth, Text::ShowFavorites, Text::ShowAll].map(|t| t.get(lang));
            let shows: Vec<(&str, &str)> = SHOW_MODES.into_iter().zip(shows).collect();
            choice(right, Text::SetOpens.get(lang), tab, "show", &shows);
            flag(right, Text::SetRowFavorites.get(lang), tab, "tree_fav", true);
            flag(right, Text::SetRowAll.get(lang), tab, "tree_all", true);
            right.separator();
            right.label(Text::SetFolders.get(lang));
            folders(right, lang, tab, "paths");
        });
    }

    fn import(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |text: Text| text.get(lang);
        let map = &mut self.import;
        ui.strong("Merge");
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
        flag(ui, t(Text::ImpLights), map, "import_lights", false);
        flag(ui, t(Text::ImpCameras), map, "import_cameras", false);
        flag(ui, t(Text::ImpSurface), map, "place_on_surface", true);
        flag(ui, t(Text::ImpMultiple), map, "place_multiple", true);
        ui.separator();
        ui.strong("Merge as proxy");
        choice(ui, t(Text::ProxyMode), map, "proxy_mode", &[("single", t(Text::ProxySingle)), ("per_object", t(Text::ProxyPerObject))]);
        let display = [("bbox", t(Text::ProxyBox)), ("preview", t(Text::ProxyPoints)), ("full", t(Text::ProxyMesh))];
        choice(ui, t(Text::ProxyDisplay), map, "proxy_display", &display);
        choice(ui, t(Text::ProxyWhere), map, "proxy_location", &[("asset", t(Text::ProxyBeside)), ("custom", t(Text::ProxyCustom))]);
        if text_of(map, "proxy_location") == "custom" {
            ui.horizontal(|ui| {
                ui.label(text_of(map, "proxy_custom_dir"));
                if ui.button(t(Text::Choose)).clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    map.insert("proxy_custom_dir".into(), path.display().to_string().into());
                }
            });
        }
        ui.separator();
        ui.strong(t(Text::KindTextures));
        flag(ui, t(Text::ImpRelink), map, "relink_on_import", true);
        ui.label(t(Text::ImpDirs));
        folders(ui, lang, map, "relink_dirs");
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let map = &mut self.preview;
        number(ui, Text::PrevSize.get(lang), map, "size", 128.0..=2048.0, true);
        let methods = [("render", Text::PrevRender.get(lang)), ("viewport", Text::PrevViewport.get(lang))];
        choice(ui, Text::PrevMethod.get(lang), map, "method", &methods);
        number(ui, Text::PrevMargin.get(lang), map, "margin", 1.0..=2.0, false);
        number(ui, Text::PrevElevation.get(lang), map, "elevation", -80.0..=80.0, false);
        number(ui, Text::PrevAzimuth.get(lang), map, "azimuth", -180.0..=180.0, false);
    }

    fn max_page(&mut self, ui: &mut egui::Ui, max: &mut MaxSession) {
        ui.label(self.t(Text::BridgeWhat));
        ui.checkbox(&mut self.toolbar, Text::BridgeToolbar.get(self.lang));
        let mut action = None;
        for (i, target) in self.targets.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.strong(target.label());
                let (state, button) = match (target.installed(), target.outdated()) {
                    (false, _) => (Text::BridgeAbsent, Text::Install),
                    (true, true) => (Text::BridgeIsOld, Text::Update),
                    (true, false) => (Text::BridgeInstalled, Text::InstallAgain),
                };
                ui.label(state.get(self.lang));
                if ui.button(button.get(self.lang)).clicked() {
                    action = Some((i, true));
                }
                if target.installed() && ui.button(Text::BridgeRemove.get(self.lang)).clicked() {
                    action = Some((i, false));
                }
            });
        }
        if self.targets.is_empty() {
            ui.label(self.t(Text::BridgeNoMax));
        }
        match action {
            Some((i, true)) => self.status = max.install_with(&self.targets[i..=i], Some(self.toolbar), self.lang),
            Some((i, false)) => self.remove = self.targets.get(i).cloned(),
            None => {}
        }
        if let Some(target) = self.remove.clone() {
            ui.separator();
            ui.label(fill(self.t(Text::BridgeAskRemove), &[("max", &target.label())]));
            ui.horizontal(|ui| {
                if ui.button(self.t(Text::BridgeRemove)).clicked() {
                    self.status = match target.uninstall() {
                        Ok(()) => fill(self.t(Text::BridgeRemoved), &[("max", &target.label())]),
                        Err(error) => fill(self.t(Text::BridgeError), &[("max", &target.label()), ("error", &error)]),
                    };
                    self.remove = None;
                }
                if ui.button(self.t(Text::Cancel)).clicked() {
                    self.remove = None;
                }
            });
        }
    }

    fn workspace_page(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        ui.label(self.t(Text::WsWhat));
        ui.add_space(6.0);
        let (alone, shared) = (self.t(Text::WsAlone), self.t(Text::WsShared));
        ui.radio_value(&mut self.shared, false, alone);
        ui.radio_value(&mut self.shared, true, shared);
        ui.add_enabled_ui(self.shared, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.workspace).desired_width(420.0));
                if ui.button(Text::Choose.get(self.lang)).clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    self.workspace = workspace::workspace_dir(&path).display().to_string();
                }
            });
        });
        let same = self.chosen() == settings.workspace();
        let state = match (self.chosen(), same, settings.ws_state()) {
            (None, _, _) => Text::WsLocal,
            (Some(_), true, WsState::Offline) => Text::WsOffline,
            (Some(_), true, _) => Text::WsJoined,
            // the folder may be on a server: it is looked at when it is accepted, not at every frame
            (Some(_), false, _) => Text::WsWillJoin,
        };
        ui.weak(self.t(state));
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
                ui.selectable_value(&mut self.page, page, text.get(self.lang));
            }
        });
    }

    fn bottom(&mut self, ui: &mut egui::Ui, settings: &mut Settings, answer: &mut Answer) {
        ui.label(egui::RichText::new(&self.status).small());
        if self.ask_theirs {
            ui.label(self.t(Text::WsAskTheirs));
        }
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new(self.t(Text::Accept)).strong()).clicked() && self.accept(settings) {
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
            if ui.button(self.t(Text::Cancel)).clicked() {
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
            ui.set_width(size.x.clamp(400.0, 820.0));
            ui.heading(self.t(Text::Settings));
            if self.first {
                ui.label(self.t(Text::Welcome));
            }
            self.pages(ui);
            ui.separator();
            let height = (size.y - 190.0).clamp(160.0, 520.0);
            egui::ScrollArea::vertical().auto_shrink(false).max_height(height).show(ui, |ui| match self.page {
                Page::Tabs => self.tabs(ui),
                Page::Import => self.import(ui),
                Page::Preview => self.preview(ui),
                Page::Max => self.max_page(ui, max),
                Page::Workspace => self.workspace_page(ui, settings),
                Page::Language => {
                    ui.label(self.t(Text::LanguageWhat));
                    let windows = self.t(Text::LanguageWindows);
                    ui.radio_value(&mut self.language, None, windows);
                    ui.radio_value(&mut self.language, Some(Language::English), "English");
                    ui.radio_value(&mut self.language, Some(Language::Spanish), "Español");
                }
            });
            ui.separator();
            self.bottom(ui, settings, &mut answer);
        });
        answer
    }
}
