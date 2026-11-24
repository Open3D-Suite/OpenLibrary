// SPDX-License-Identifier: GPL-3.0-or-later
//! The window's work with a 3ds Max in the background: previews and data of the models; and the windows to
//! catalogue and for the missing files.

use super::OpenLibraryApp;
use crate::library::Asset;
use crate::maxbatch::{self, Job, Outcome};
use crate::maxfile::Engine;
use crate::modelstats;
use crate::task::Task;
use crate::texts::{Text, fill};
use crate::ui_missing::MissingWindow;
use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub(super) enum Event {
    Line(usize, Outcome),
    Ended(Result<(), String>),
}

/// A preview to make.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreviewJob {
    key: String,
    file: PathBuf,
    out: PathBuf,
    replaces: bool,
}

#[derive(Clone)]
enum Kind {
    Previews(Vec<PreviewJob>),
    Stats(Vec<PathBuf>),
}

/// The batch that is running.
pub(super) struct Work {
    kind: Kind,
    task: Task<Event>,
    total: usize,
    ok: usize,
    failed: usize,
}

/// Where the preview of a model goes: in the place of the one there is, if 3ds Max can write it, or beside it.
pub(super) fn preview_out(asset: &Asset) -> PathBuf {
    let writable = |p: &&PathBuf| p.extension().is_some_and(|e| ["jpg", "jpeg", "png"].iter().any(|x| e.eq_ignore_ascii_case(x)));
    asset.preview.as_ref().filter(writable).cloned().unwrap_or_else(|| asset.folder.join(format!("{}.jpg", asset.name)))
}

/// The previews to make of these models: the file by engine (Corona first), and where it goes: in the place of
/// the one there is, if 3ds Max can write it, or beside the model.
fn preview_jobs(assets: &[Arc<Asset>]) -> Vec<PreviewJob> {
    assets
        .iter()
        .filter_map(|asset| {
            let file = [Engine::Corona, Engine::VRay, Engine::FStorm]
                .iter()
                .find_map(|e| asset.versions.get(e).map(PathBuf::as_path))
                .or_else(|| asset.any_file())?;
            let out = preview_out(asset);
            let replaces = asset.preview.as_ref() == Some(&out);
            Some(PreviewJob { key: asset.key(), file: file.to_path_buf(), out, replaces })
        })
        .collect()
}

fn preview_job(jobs: &[PreviewJob], settings: Option<&Value>) -> Job {
    let number = |key: &str, default: f64| settings.and_then(|s| s.get(key)).and_then(Value::as_f64).unwrap_or(default);
    Job {
        function: "olib_batchPreviews",
        arguments: vec![number("size", 512.0).round(), number("margin", 1.15), number("elevation", 20.0), number("azimuth", 35.0)],
        lines: jobs.iter().map(|j| vec![j.file.display().to_string(), j.out.display().to_string()]).collect(),
        stall: Duration::from_secs(300),
    }
}

impl OpenLibraryApp {
    fn work_dir(&self, name: &str) -> PathBuf {
        self.data_dir.join("batch").join(format!("{name}-{}", std::process::id()))
    }

    fn start(&mut self, kind: Kind) {
        if self.work.is_some() {
            self.notice = Some(self.t(Text::BatchBusy).to_string());
            return;
        }
        let Some(exe) = maxbatch::find() else {
            self.notice = Some(self.t(Text::NoMaxBatch).to_string());
            return;
        };
        let (job, name) = match &kind {
            Kind::Previews(jobs) => (preview_job(jobs, self.settings.data.get("preview")), "previews"),
            Kind::Stats(files) => (modelstats::job(files), "stats"),
        };
        if job.lines.is_empty() {
            return;
        }
        let (dir, total) = (self.work_dir(name), job.lines.len());
        let task = Task::start(self.wake.clone(), move |link| {
            let result = maxbatch::run(&exe, &dir, &job, &|| link.cancelled(), &mut |n, o| link.send(Event::Line(n, o)));
            link.send(Event::Ended(result));
        });
        self.notice = None;
        self.work = Some(Work { kind, task, total, ok: 0, failed: 0 });
    }

    /// Asks before making previews, because it writes in the library.
    pub(super) fn ask_previews(&mut self, assets: &[Arc<Asset>]) {
        let jobs = preview_jobs(assets);
        if !jobs.is_empty() {
            self.preview_ask = Some(jobs);
        }
    }

    /// Reads faces, vertices and animation of these models; those read already too when `again` and all are.
    pub(super) fn read_stats(&mut self, assets: &[Arc<Asset>], again: bool) {
        let all: Vec<PathBuf> = assets.iter().flat_map(|a| a.all_files()).map(Path::to_path_buf).collect();
        let mut files: Vec<PathBuf> = all.iter().filter(|f| self.stats.get(f).is_none()).cloned().collect();
        if files.is_empty() && again {
            files = all;
        }
        if files.is_empty() {
            self.notice = Some(self.t(Text::StatsAllRead).to_string());
            return;
        }
        self.start(Kind::Stats(files));
    }

    /// Whether the batch that is running reads data of models.
    pub(super) fn reading_stats(&self) -> bool {
        self.work.as_ref().is_some_and(|w| matches!(w.kind, Kind::Stats(_)))
    }

    /// How far the running batch is.
    pub(super) fn work_progress(&self) -> Option<(usize, usize)> {
        self.work.as_ref().map(|w| (w.ok + w.failed, w.total))
    }

    pub(super) fn poll_work(&mut self) {
        let Some(work) = &mut self.work else { return };
        let mut ended = None;
        for event in work.task.poll() {
            match event {
                Event::Line(n, outcome) => {
                    let file = match &work.kind {
                        Kind::Previews(jobs) => jobs.get(n).map(|j| j.file.clone()),
                        Kind::Stats(files) => files.get(n).cloned(),
                    };
                    let is_stats = matches!(work.kind, Kind::Stats(_));
                    match outcome {
                        Ok(fields) => {
                            work.ok += 1;
                            if let (Some(file), Some(stats)) = (file, modelstats::parse(&fields)) {
                                self.stats.put(&file, Some(stats));
                            }
                        }
                        Err(_) => {
                            work.failed += 1;
                            if let (Some(file), true) = (file, is_stats && !work.task.cancelled()) {
                                self.stats.put(&file, None);
                            }
                        }
                    }
                }
                Event::Ended(result) => ended = Some(result),
            }
        }
        if let Some(result) = ended {
            self.end_work(result);
        }
    }

    fn end_work(&mut self, result: Result<(), String>) {
        let Some(work) = self.work.take() else { return };
        let mut text = fill(self.t(Text::BatchDone), &[("ok", &work.ok), ("failed", &work.failed)]);
        if work.task.cancelled() {
            text = format!("{} {text}", self.t(Text::Cancelled));
        }
        if let Err(error) = result {
            text = fill(self.t(Text::BatchFailed), &[("error", &error)]);
        }
        if let Err(error) = self.stats.save() {
            text = fill(self.t(Text::SettingsError), &[("error", &error)]);
        }
        if let Kind::Previews(jobs) = &work.kind {
            let mut folders = HashSet::new();
            for job in jobs.iter().filter(|j| j.out.is_file()) {
                self.textures.remove(&job.key);
                self.big = None;
                folders.extend(job.out.parent().map(Path::to_path_buf));
            }
            self.rescan(folders);
        }
        self.notice = Some(text);
    }

    /// The text of the running batch, for the status bar.
    pub(super) fn work_text(&self) -> Option<String> {
        let work = self.work.as_ref()?;
        let done = work.ok + work.failed;
        Some(if done == 0 {
            self.t(Text::BatchStarting).to_string()
        } else {
            fill(self.t(Text::BatchProgress), &[("done", &done), ("count", &work.total)])
        })
    }

    pub(super) fn cancel_work(&mut self) {
        if let Some(work) = &self.work {
            work.task.cancel();
        }
    }

    /// Opens the window of the missing files of a model.
    pub(super) fn open_missing(&mut self, asset: &Asset) {
        let files = asset.all_files().into_iter().map(Path::to_path_buf).collect();
        self.missing = Some(MissingWindow::new(&asset.name, (files, self.all_roots()), &self.data_dir, self.lang, self.wake.clone()));
    }

    /// The windows over the gallery: the missing files and the question before previews.
    pub(super) fn windows(&mut self, ctx: &egui::Context) {
        if let Some(window) = &mut self.missing {
            let answer = window.show(ctx);
            if answer.close {
                self.missing = None;
            }
            if !answer.saved.is_empty() {
                self.forget_detail(&answer.saved); // looked at again
            }
        }
        let Some(jobs) = &self.preview_ask else { return };
        let replaced = jobs.iter().filter(|j| j.replaces).count();
        let mut text = fill(self.t(Text::PreviewAsk), &[("count", &jobs.len())]);
        if replaced > 0 {
            text = format!("{text}\n\n{}", fill(self.t(Text::PreviewAskReplace), &[("count", &replaced)]));
        }
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("openlibrary-previews")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(text);
            ui.horizontal(|ui| {
                if crate::kit::button(ui, crate::kit::Kind::Primary, None, self.t(Text::Continue)).clicked() {
                    choice = Some(true);
                }
                if crate::kit::button(ui, crate::kit::Kind::Action, None, self.t(Text::Cancel)).clicked() {
                    choice = Some(false);
                }
            });
        });
        if let Some(yes) = choice
            && let Some(jobs) = self.preview_ask.take().filter(|_| yes)
        {
            self.start(Kind::Previews(jobs));
        }
    }

    /// Draws the catalogue when it is open; `false` when it is not.
    pub(super) fn catalogue(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(window) = &mut self.catalog else { return false };
        let answer = window.show(ui);
        if answer.close || answer.saved.is_some() {
            let config = window.config();
            if let Some(Value::Object(catalog)) = self.settings.data.get_mut("catalog") {
                catalog.extend(config.map(|(k, v)| (k.to_string(), v)));
            }
            if let Some(name) = &answer.hide {
                self.settings.hide_name(name);
            }
            if let Err(error) = self.settings.save() {
                self.notice = Some(fill(self.t(Text::SettingsError), &[("error", &error)]));
            }
        }
        if let Some(folder) = answer.saved {
            // a new folder is found from the one it is in
            self.rescan(folder.parent().map(Path::to_path_buf).into_iter().chain([folder.clone()]));
        }
        if answer.close {
            self.catalog = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_preview_takes_the_place_of_one_3ds_max_can_write() {
        let mut asset: Asset =
            serde_json::from_value(json!({"n": "Sofa", "v": {"vray": "S:\\Sofa_vray.max"}, "a": ["S:\\Sofa.max"], "p": "S:\\Sofa.PNG", "m": 0.0}))
                .unwrap();
        asset.folder = PathBuf::from("S:\\");
        let jobs = preview_jobs(&[Arc::new(asset.clone())]);
        assert_eq!((jobs[0].file.clone(), jobs[0].out.clone(), jobs[0].replaces), ("S:\\Sofa_vray.max".into(), "S:\\Sofa.PNG".into(), true));
        asset.preview = Some(PathBuf::from("S:\\Sofa.webp"));
        let jobs = preview_jobs(&[Arc::new(asset)]);
        assert_eq!((jobs[0].out.clone(), jobs[0].replaces), ("S:\\Sofa.jpg".into(), false));
        let job = preview_job(&jobs, Some(&json!({"size": 256, "margin": 1.2})));
        assert_eq!(job.arguments, [256.0, 1.2, 20.0, 35.0]);
        assert_eq!(job.lines, [["S:\\Sofa_vray.max", "S:\\Sofa.jpg"]]);
    }
}
