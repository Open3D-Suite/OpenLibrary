// SPDX-License-Identifier: GPL-3.0-or-later
//! The welcome, the first time OpenLibrary opens: 1) whether a team shares a workspace, 2) where to have it at hand
//! (the button in 3ds Max, the desktop, the Start menu) and the bridge in each 3ds Max. Everything can be changed later
//! in the settings.

use crate::bridge_install::MaxTarget;
use crate::kit::{self, Kind};
use crate::max_session::MaxSession;
use crate::open3d_settings::Language;
use crate::settings::Settings;
use crate::settings::workspace::{self, WORKSPACE_FILE};
use crate::shortcuts::{self, Folders, Place};
use crate::task::{Task, Wake};
use crate::texts::{Text, fill};
use eframe::egui;
use std::path::PathBuf;

/// The key of the settings that says the welcome was seen.
pub const DONE: &str = "olib_welcome_done";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Workspace,
    Install,
    Report,
}

pub struct SetupWindow {
    lang: Language,
    step: Step,
    shared: bool,
    folder: String,
    status: String,
    targets: Vec<(MaxTarget, bool)>,
    button: bool,
    desktop: bool,
    start_menu: bool,
    folders: Option<Folders>,
    finding: Option<Task<Folders>>,
    report: Vec<String>,
}

#[derive(Default)]
pub struct Answer {
    pub close: bool,
    /// The workspace changed: the tabs are read again.
    pub workspace: bool,
}

impl SetupWindow {
    pub fn new(settings: &Settings, max: &MaxSession, lang: Language, wake: Wake) -> Self {
        let workspace = settings.workspace();
        Self {
            lang,
            step: Step::Workspace,
            shared: workspace.is_some(),
            folder: workspace.map(|w| w.display().to_string()).unwrap_or_default(),
            status: String::new(),
            targets: max.targets().into_iter().map(|t| (t, true)).collect(),
            button: true,
            desktop: true,
            start_menu: true,
            folders: None,
            finding: Some(Task::start(wake, |link| link.send(shortcuts::folders()))),
            report: Vec::new(),
        }
    }

    fn t(&self, text: Text) -> &'static str {
        text.get(self.lang)
    }

    fn chosen(&self) -> Option<PathBuf> {
        let text = self.folder.trim();
        (self.shared && !text.is_empty()).then(|| workspace::workspace_dir(&PathBuf::from(text)))
    }

    fn workspace_step(&mut self, ui: &mut egui::Ui, settings: &mut Settings, answer: &mut Answer) {
        ui.label(egui::RichText::new(self.t(Text::SetupAlone)).font(kit::strong(15.0)));
        ui.add_space(4.0);
        ui.weak(self.t(Text::WsWhat));
        ui.add_space(6.0);
        let (alone, shared) = (self.t(Text::WsAlone), self.t(Text::WsShared));
        ui.radio_value(&mut self.shared, false, alone);
        ui.radio_value(&mut self.shared, true, shared);
        ui.add_enabled_ui(self.shared, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                let width = (ui.available_width() - 110.0).max(120.0);
                ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(width).hint_text(r"\\SERVER\LIBRARY"));
                if kit::button(ui, Kind::Action, None, self.t(Text::Choose)).clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    self.folder = path.display().to_string();
                }
            });
        });
        let note = match self.chosen() {
            None if self.shared => self.t(Text::SetupPickFolder).to_string(),
            None => self.t(Text::WsLocal).to_string(),
            // the folder may be on a server: it is looked at when Next is pressed
            Some(folder) => fill(self.t(Text::SetupWillUse), &[("folder", &folder.display())]),
        };
        ui.weak(note);
        if !self.status.is_empty() {
            ui.colored_label(kit::pal(ui).warn, &self.status);
        }
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::sized_button(ui, Kind::Primary, None, self.t(Text::SetupNext), 120.0).clicked() {
                self.next(settings, answer);
            }
        });
    }

    fn next(&mut self, settings: &mut Settings, answer: &mut Answer) {
        let new = self.chosen();
        if new != settings.workspace() {
            if let Some(folder) = &new {
                let parent_there = folder.parent().is_some_and(std::path::Path::is_dir);
                if !folder.is_absolute() || !(folder.is_dir() || parent_there) {
                    self.status = self.t(Text::WsNotFound).to_string();
                    return;
                }
                if let Err(error) = workspace::prepare_workspace(folder) {
                    self.status = fill(self.t(Text::SettingsError), &[("error", &error)]);
                    return;
                }
            }
            let theirs = new.as_ref().is_some_and(|f| f.join(WORKSPACE_FILE).is_file());
            if let Err(error) = settings.set_workspace(new.as_deref(), theirs) {
                self.status = fill(self.t(Text::SettingsError), &[("error", &error)]);
                return;
            }
            answer.workspace = true;
        }
        self.status.clear();
        self.step = Step::Install;
    }

    fn install_step(&mut self, ui: &mut egui::Ui, max: &mut MaxSession) {
        if let Some(found) = self.finding.as_ref().and_then(|t| t.poll().pop()) {
            self.finding = None;
            (self.desktop, self.start_menu) = (!found.has(Place::Desktop), !found.has(Place::StartMenu));
            self.folders = Some(found);
        }
        let lang = self.lang;
        kit::group(ui, "3ds Max", |ui| {
            if self.targets.is_empty() {
                ui.weak(Text::BridgeNoMax.get(lang));
            }
            for (target, on) in &mut self.targets {
                let mut text = fill(Text::SetupConnect.get(lang), &[("max", &target.label())]);
                if target.installed() && !target.outdated() {
                    text = format!("{text}  ({})", Text::BridgeInstalled.get(lang));
                }
                ui.checkbox(on, text);
            }
            ui.add_enabled(!self.targets.is_empty(), egui::Checkbox::new(&mut self.button, Text::BridgeToolbar.get(lang)));
            ui.weak(Text::SetupBridgeNote.get(lang));
        });
        kit::group(ui, "Windows", |ui| match &self.folders {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak(Text::Checking.get(lang));
                });
            }
            Some(found) => {
                for (place, text, on) in
                    [(Place::Desktop, Text::SetupDesktop, &mut self.desktop), (Place::StartMenu, Text::SetupStartMenu, &mut self.start_menu)]
                {
                    if found.has(place) {
                        ui.label(fill(Text::SetupHasIt.get(lang), &[("place", &text.get(lang))]));
                    } else {
                        ui.add_enabled(found.of(place).is_some(), egui::Checkbox::new(on, text.get(lang)));
                    }
                }
                ui.weak(Text::SetupPin.get(lang));
            }
        });
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::sized_button(ui, Kind::Primary, None, self.t(Text::Install), 120.0).clicked() {
                self.install(max);
            }
            if kit::sized_button(ui, Kind::Action, None, self.t(Text::SetupNotNow), 120.0).clicked() {
                self.step = Step::Report;
            }
        });
    }

    fn install(&mut self, max: &mut MaxSession) {
        let mut report = Vec::new();
        let chosen: Vec<MaxTarget> = self.targets.iter().filter(|(_, on)| *on).map(|(t, _)| t.clone()).collect();
        if !chosen.is_empty() {
            report.push(max.install_with(&chosen, Some(self.button), self.lang));
            if crate::platform::max_running() {
                report.push(self.t(Text::SetupRestartMax).to_string());
            }
        }
        if let (Some(found), Ok(exe)) = (&self.folders, std::env::current_exe()) {
            for (place, text, on) in [(Place::Desktop, Text::SetupDesktop, self.desktop), (Place::StartMenu, Text::SetupStartMenu, self.start_menu)] {
                let Some(folder) = found.of(place).filter(|_| on && !found.has(place)) else { continue };
                report.push(match shortcuts::create(&folder.join(shortcuts::NAME), &exe) {
                    Ok(()) => fill(self.t(Text::SetupMade), &[("place", &text.get(self.lang))]),
                    Err(error) => format!("{}: {error}", text.get(self.lang)),
                });
            }
        }
        report.push(self.t(Text::SetupPin).to_string());
        self.report = report;
        self.step = Step::Report;
    }

    /// Draws it; the welcome is seen once, whatever is chosen.
    pub fn show(&mut self, ctx: &egui::Context, settings: &mut Settings, max: &mut MaxSession) -> Answer {
        let mut answer = Answer::default();
        let width = (ctx.content_rect().width() - 80.0).clamp(380.0, 680.0);
        egui::Modal::new(egui::Id::unique("openlibrary-welcome")).show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(self.t(Text::SetupTitle)).font(kit::strong(20.0)));
                let step = match self.step {
                    Step::Workspace => self.t(Text::SetupStep1),
                    Step::Install => self.t(Text::SetupStep2),
                    Step::Report => "",
                };
                ui.weak(step);
            });
            ui.separator();
            ui.add_space(4.0);
            match self.step {
                Step::Workspace => self.workspace_step(ui, settings, &mut answer),
                Step::Install => self.install_step(ui, max),
                Step::Report => {
                    if self.report.is_empty() {
                        ui.label(self.t(Text::SetupLater));
                    }
                    for line in &self.report {
                        ui.add(egui::Label::new(line).wrap());
                    }
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::sized_button(ui, Kind::Primary, None, self.t(Text::Accept), 120.0).clicked() {
                            answer.close = true;
                        }
                    });
                }
            }
        });
        if answer.close {
            settings.data.insert(DONE.into(), true.into());
            settings.data.insert("setup_done".into(), true.into());
            if let Err(error) = settings.save() {
                self.report.push(fill(self.t(Text::SettingsError), &[("error", &error)]));
            }
        }
        answer
    }
}
