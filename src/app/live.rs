// SPDX-License-Identifier: GPL-3.0-or-later
//! What the window asks of an open 3ds Max besides placing models: the preview of what is selected in it, and
//! finding the lost paths of its scene and of `.max` files of the library.

use super::OpenLibraryApp;
use crate::library::Asset;
use crate::max_session::MaxReply;
use crate::maxlink::{self, MaxError, MaxInfo};
use crate::texts::{Text, fill};
use eframe::egui;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;

pub(super) enum Live {
    Preview {
        key: String,
        out: PathBuf,
        reply: MaxReply,
    },
    Scene {
        reply: MaxReply,
    },
    Files(Relink),
    /// A `.max` being opened in the place of the scene.
    Open {
        name: String,
        reply: MaxReply,
    },
}

/// Relinking `.max` files one after another.
pub(super) struct Relink {
    info: MaxInfo,
    queue: VecDeque<(PathBuf, Vec<PathBuf>)>,
    total: usize,
    reply: Option<MaxReply>,
    fixed: u64,
    missing: u64,
    failed: usize,
}

fn pair(result: &Value) -> (u64, u64) {
    let at = |i: usize| result.get(i).and_then(Value::as_u64).unwrap_or(0);
    (at(0), at(1))
}

fn texts(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

impl OpenLibraryApp {
    /// The 3ds Max to ask, if it can be asked now.
    fn free_max(&mut self) -> Option<MaxInfo> {
        let text = match self.max.target() {
            None => Text::NoMax,
            Some(info) if info.busy => Text::Busy,
            Some(info) if self.live.is_none() => return Some(info),
            Some(_) => Text::Busy,
        };
        self.notice = Some(self.t(text).to_string());
        None
    }

    fn relink_dirs(&self, asset: &Asset) -> Vec<PathBuf> {
        let roots = self.models.index.as_ref().map(|i| i.roots.clone()).unwrap_or_default();
        maxlink::relink_dirs(asset, &roots, &self.extra_dirs())
    }

    /// The folders of textures of the settings.
    fn extra_dirs(&self) -> Vec<PathBuf> {
        let list = self.settings.data.get("import").and_then(|i| i.get("relink_dirs")).and_then(Value::as_array);
        list.map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect()).unwrap_or_default()
    }

    pub(super) fn live_preview(&mut self, asset: &Asset, from_view: bool) {
        let Some(info) = self.free_max() else { return };
        let out = super::work::preview_out(asset);
        let prev = self.settings.data.get("preview").cloned().unwrap_or_default();
        let args = json!({"out_file": out.display().to_string(), "prev": prev, "from_view": from_view});
        if let Some(reply) = self.max.request(&info, "make_preview", args, self.wake.clone()) {
            self.notice = Some(self.t(Text::PreviewMaking).to_string());
            self.live = Some(Live::Preview { key: asset.key(), out, reply });
        }
    }

    /// Looks for the lost files of the open scene in the selected model's folders, or else the folder shown.
    pub(super) fn relink_scene(&mut self, selected: &[Arc<Asset>]) {
        let Some(info) = self.free_max() else { return };
        let mut dirs: Vec<PathBuf> = Vec::new();
        for dir in selected.iter().flat_map(|a| self.relink_dirs(a)) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        if selected.is_empty() {
            let shown = self.models.side.folder.clone().map(|f| vec![f]);
            dirs = shown.unwrap_or_else(|| self.models.index.as_ref().map(|i| i.roots.clone()).unwrap_or_default());
            dirs.extend(self.extra_dirs());
        }
        if let Some(reply) = self.max.request(&info, "relink_scene", json!({"search_dirs": texts(&dirs)}), self.wake.clone()) {
            self.notice = Some(self.t(Text::RelinkingScene).to_string());
            self.live = Some(Live::Scene { reply });
        }
    }

    /// Asks before relinking the `.max` of these models, because it writes in the library.
    pub(super) fn ask_relink(&mut self, assets: &[Arc<Asset>]) {
        self.relink_ask = Some(assets.iter().flat_map(|a| a.all_files().into_iter().map(|f| (f.to_path_buf(), a.clone()))).collect());
    }

    pub(super) fn relink_question(&mut self, ctx: &egui::Context) {
        let Some(files) = &self.relink_ask else { return };
        let text = fill(self.t(Text::RelinkAsk), &[("count", &files.len())]);
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("openlibrary-relink")).show(ctx, |ui| {
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
        let Some(yes) = choice else { return };
        let files = self.relink_ask.take().unwrap_or_default();
        if yes && let Some(info) = self.free_max() {
            let queue: VecDeque<_> = files.into_iter().map(|(file, asset)| (file, self.relink_dirs(&asset))).collect();
            self.live = Some(Live::Files(Relink { info, total: queue.len(), queue, reply: None, fixed: 0, missing: 0, failed: 0 }));
        }
    }

    pub(super) fn poll_live(&mut self) {
        match self.live.take() {
            None => {}
            Some(Live::Preview { key, out, reply }) => match reply.try_recv() {
                Err(TryRecvError::Empty) => self.live = Some(Live::Preview { key, out, reply }),
                Ok(Ok(Value::Bool(true))) => {
                    self.textures.remove(&key);
                    self.big = None;
                    self.rescan(out.parent().map(PathBuf::from));
                    self.notice = Some(self.t(Text::PreviewMade).to_string());
                }
                Ok(Err(error)) => self.tell(&error),
                _ => self.notice = Some(self.t(Text::PreviewNotMade).to_string()),
            },
            Some(Live::Scene { reply }) => match reply.try_recv() {
                Err(TryRecvError::Empty) => self.live = Some(Live::Scene { reply }),
                Ok(Ok(result)) => {
                    let (fixed, missing) = pair(&result);
                    self.notice = Some(fill(self.t(Text::Relinked), &[("fixed", &fixed), ("missing", &missing)]));
                }
                Ok(Err(error)) => self.tell(&error),
                Err(TryRecvError::Disconnected) => {}
            },
            Some(Live::Open { name, reply }) => match reply.try_recv() {
                Err(TryRecvError::Empty) => self.live = Some(Live::Open { name, reply }),
                Ok(Ok(_)) => self.notice = Some(fill(self.t(Text::Opened), &[("name", &name)])),
                Ok(Err(error)) => self.tell(&error),
                Err(TryRecvError::Disconnected) => {}
            },
            Some(Live::Files(relink)) => self.poll_relink(relink),
        }
    }

    fn poll_relink(&mut self, mut relink: Relink) {
        if let Some(reply) = &relink.reply {
            match reply.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.live = Some(Live::Files(relink));
                    return;
                }
                Ok(Ok(result)) => {
                    let (fixed, missing) = pair(&result);
                    relink.fixed += fixed;
                    relink.missing += missing;
                }
                Ok(Err(MaxError::NotThere)) => {
                    relink.failed += 1 + relink.queue.len(); // it closed: the rest cannot be done
                    relink.queue.clear();
                }
                Ok(Err(_)) | Err(TryRecvError::Disconnected) => relink.failed += 1,
            }
        }
        if let Some((file, dirs)) = relink.queue.pop_front() {
            self.forget_detail(std::slice::from_ref(&file)); // looked at again
            let args = json!({"max_path": file.display().to_string(), "search_dirs": texts(&dirs)});
            relink.reply = self.max.request(&relink.info, "relink_file", args, self.wake.clone());
            let done = relink.total - relink.queue.len();
            self.notice = Some(fill(self.t(Text::BatchProgress), &[("done", &done), ("count", &relink.total)]));
            if relink.reply.is_some() {
                self.live = Some(Live::Files(relink));
            }
            return;
        }
        let mut text = fill(self.t(Text::Relinked), &[("fixed", &relink.fixed), ("missing", &relink.missing)]);
        if relink.failed > 0 {
            text = format!("{text} {}", fill(self.t(Text::RelinkFailed), &[("count", &relink.failed)]));
        }
        self.notice = Some(text);
    }

    fn tell(&mut self, error: &MaxError) {
        self.notice = Some(match error {
            MaxError::Busy => self.t(Text::Busy).to_string(),
            error => fill(self.t(Text::SendFailed), &[("error", error)]),
        });
    }
}
