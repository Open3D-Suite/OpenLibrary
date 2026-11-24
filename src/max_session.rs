// SPDX-License-Identifier: GPL-3.0-or-later
//! The window's side of 3ds Max: which ones are open, the bridge prompt when the app opens, and the merges.
//! Every request runs on a thread of its own; the window only reads what came back.

use crate::bridge_install::{self, MaxTarget};
use crate::library::Asset;
use crate::maxfile::{Engine, EngineCache};
use crate::maxlink::{self, MaxError, MaxInfo};
use crate::texts::{Text, fill};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How a model comes into the scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeKind {
    /// It follows the mouse and rests on surfaces until a click.
    Place,
    /// As a Corona or V-Ray proxy, placed the same way.
    Proxy,
    /// Where it is in its own file.
    Origin,
}

/// A merge ready to send: the command, its arguments, and the renderer of the file.
#[derive(Debug, PartialEq)]
pub struct MergeRequest {
    pub cmd: &'static str,
    pub args: Value,
    pub used: Engine,
}

fn writable(folder: &Path) -> bool {
    let probe = folder.join(".olib_write_test");
    std::fs::create_dir_all(folder).is_ok() && std::fs::write(&probe, b"ok").is_ok() && std::fs::remove_file(&probe).is_ok()
}

/// What is brought in, and how.
pub struct MergeWhat<'a> {
    pub asset: &'a Asset,
    pub kind: MergeKind,
    /// The file the person chose, when the model has none of the scene's renderer.
    pub chosen: Option<&'a Path>,
    /// The mark left in the scene; empty for a bridge that does not know it.
    pub id: &'a str,
}

/// What came of looking at the model's files.
#[derive(Debug, PartialEq)]
pub enum Prepared {
    Ready {
        request: MergeRequest,
        /// The folder of a proxy that could not be written beside the model.
        moved: Option<PathBuf>,
    },
    /// None is of the scene's renderer: the person chooses.
    Choose(Vec<(Engine, PathBuf)>),
    NoFile,
}

/// Prepares a merge into the 3ds Max `info`, with the import settings `imp` (the `import` object of the settings).
/// `local_proxies` is where a proxy goes when it cannot be written beside the model. It looks at the disk: not for
/// the window's thread.
pub fn prepare(what: &MergeWhat<'_>, info: &MaxInfo, imp: &Value, roots: &[PathBuf], engines: &EngineCache, local_proxies: &Path) -> Prepared {
    let get = |key: &str| imp.get(key);
    let asset = what.asset;
    let chosen = match what.chosen {
        Some(file) => {
            let named = asset.versions.iter().find(|(_, f)| f.as_path() == file).map(|(e, _)| *e);
            Some((file.to_path_buf(), named.unwrap_or_else(|| engines.detect(file)), true))
        }
        None => maxlink::choose_file(asset, info.engine, |f| engines.detect(f)),
    };
    let Some((path, used, exact)) = chosen else { return Prepared::NoFile };
    if !exact && asset.all_files().len() > 1 && get("missing_engine").and_then(Value::as_str) == Some("ask") {
        let named = asset.versions.iter().map(|(e, f)| (*e, f.clone()));
        return Prepared::Choose(named.chain(asset.auto_files.iter().map(|f| (engines.detect(f), f.clone()))).collect());
    }
    let relink = if get("relink_on_import").and_then(Value::as_bool).unwrap_or(true) {
        let extra: Vec<PathBuf> =
            get("relink_dirs").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect()).unwrap_or_default();
        json!(maxlink::relink_dirs(asset, roots, &extra).iter().map(|p| p.display().to_string()).collect::<Vec<_>>())
    } else {
        Value::Null
    };
    let mut args = json!({
        "max_path": path.display().to_string(),
        "asset_name": asset.name,
        "imp": imp,
        "place": what.kind != MergeKind::Origin,
        "relink_dirs": relink,
    });
    if !what.id.is_empty() {
        args["asset_id"] = json!(what.id);
    }
    if what.kind != MergeKind::Proxy {
        return Prepared::Ready { request: MergeRequest { cmd: "merge", args, used }, moved: None };
    }
    let mode = get("proxy_mode").and_then(Value::as_str).unwrap_or("single");
    let custom = (get("proxy_location").and_then(Value::as_str) == Some("custom"))
        .then(|| get("proxy_custom_dir").and_then(Value::as_str).filter(|d| !d.is_empty()).map(PathBuf::from))
        .flatten();
    let mut folder = maxlink::proxy_dir(&path, &asset.name, info.engine, mode, custom.as_deref());
    let mut moved = None;
    if !writable(&folder) {
        folder = maxlink::proxy_dir(&path, &asset.name, info.engine, mode, Some(local_proxies));
        moved = Some(folder.clone());
    }
    args["engine"] = json!(info.engine.key());
    args["proxy_dir"] = json!(folder.display().to_string());
    Prepared::Ready { request: MergeRequest { cmd: "merge_proxy", args, used }, moved }
}

/// Where the answer to [`MaxSession::request`] arrives.
pub type MaxReply = Receiver<Result<Value, MaxError>>;

pub struct MaxSession {
    token: Option<String>,
    sealed: Option<String>,
    maxes: Arc<Mutex<Vec<MaxInfo>>>,
    /// The one in front, or with the mouse over it, when several are open.
    front: Arc<Mutex<Option<u32>>>,
    /// The 3ds Max the last model was dropped on: the one the context menu uses when several are open.
    pub last_pid: Option<u32>,
    /// The bridge prompt, while it is shown: the 3ds Max without a bridge and those with an old one.
    pub prompt: Option<(Vec<MaxTarget>, Vec<MaxTarget>)>,
    targets: Vec<MaxTarget>,
}

impl MaxSession {
    /// Makes the token, points installed bridges at this app, finds what to tell, and starts looking for open
    /// 3ds Max every few seconds.
    pub fn start(data_dir: Option<&Path>, skip: &[u32], wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let token = data_dir.and_then(maxlink::bridge_token);
        let targets = std::env::var_os("LOCALAPPDATA").map(|l| bridge_install::find_targets(Path::new(&l))).unwrap_or_default();
        if let (Some((_, sealed)), Ok(exe)) = (&token, std::env::current_exe()) {
            for target in targets.iter().filter(|t| t.supported()) {
                let _ = target.fix_app_json(&exe, sealed); // a bridge that cannot be pointed here is told as missing or old
            }
        }
        let (missing, old) = bridge_install::pending(&targets, skip);
        let prompt = (token.is_some() && (!missing.is_empty() || !old.is_empty())).then_some((missing, old));
        let maxes = Arc::new(Mutex::new(Vec::new()));
        let front = Arc::new(Mutex::new(None));
        if let Some((plain, _)) = &token {
            let (plain, maxes, top) = (plain.clone(), maxes.clone(), front.clone());
            std::thread::spawn(move || {
                loop {
                    let found = maxlink::discover(&plain);
                    let pids: Vec<u32> = found.iter().map(|m| m.pid).collect();
                    let changed = match maxes.lock() {
                        Ok(mut list) => {
                            let changed = *list != found;
                            *list = found;
                            changed
                        }
                        Err(_) => return,
                    };
                    if changed {
                        wake();
                    }
                    // with several open, which one is in front is looked at more often
                    for _ in 0..4 {
                        if pids.len() > 1 {
                            let under = crate::platform::process_under_cursor().filter(|p| pids.contains(p));
                            let now = under.or_else(|| crate::platform::front_pid(&pids));
                            let Ok(mut front) = top.lock() else { return };
                            if *front != now && now.is_some() {
                                *front = now;
                                wake();
                            }
                        }
                        std::thread::sleep(Duration::from_millis(700));
                    }
                }
            });
        }
        let (sealed, token) = match token {
            Some((plain, sealed)) => (Some(sealed), Some(plain)),
            None => (None, None),
        };
        Self { token, sealed, maxes, front, last_pid: None, prompt, targets }
    }

    pub fn maxes(&self) -> Vec<MaxInfo> {
        self.maxes.lock().map(|m| m.clone()).unwrap_or_default()
    }

    /// The 3ds Max a command without a drop goes to: the one in front, or the one used last, or the first one open.
    pub fn target(&self) -> Option<MaxInfo> {
        let maxes = self.maxes();
        let front = self.front.lock().ok().and_then(|f| *f);
        let of = |pid: Option<u32>| pid.and_then(|pid| maxes.iter().find(|m| m.pid == pid).cloned());
        of(front).or_else(|| of(self.last_pid)).or_else(|| maxes.first().cloned())
    }

    /// The 3ds Max whose window is under the mouse pointer, if it has the bridge.
    pub fn under_cursor(&self) -> Option<MaxInfo> {
        let pid = crate::platform::process_under_cursor()?;
        self.maxes().into_iter().find(|m| m.pid == pid)
    }

    /// Installs or updates the bridge in these 3ds Max; the text to show.
    pub fn install(&mut self, targets: &[MaxTarget], lang: crate::open3d_settings::Language) -> String {
        self.install_with(targets, None, lang)
    }

    /// The same, with or without the toolbar button; `None` keeps what each has.
    pub fn install_with(&mut self, targets: &[MaxTarget], toolbar: Option<bool>, lang: crate::open3d_settings::Language) -> String {
        let (Some(sealed), Ok(exe)) = (&self.sealed, std::env::current_exe()) else {
            return fill(Text::BridgeError.get(lang), &[("max", &"3ds Max"), ("error", &"no token")]);
        };
        let mut errors = Vec::new();
        for target in targets {
            if let Err(error) = target.install(&exe, sealed, toolbar) {
                errors.push(fill(Text::BridgeError.get(lang), &[("max", &target.label()), ("error", &error)]));
            }
        }
        if errors.is_empty() {
            let names: Vec<String> = targets.iter().map(MaxTarget::label).collect();
            fill(Text::BridgeDone.get(lang), &[("maxes", &names.join(", "))])
        } else {
            errors.join(" · ")
        }
    }

    /// Sends a command to `info` on a thread. A 3ds Max with a window of its own open does not answer: after two
    /// minutes it is not waited for. `None` without a token.
    pub fn request(&mut self, info: &MaxInfo, cmd: &'static str, args: Value, wake: Arc<dyn Fn() + Send + Sync>) -> Option<MaxReply> {
        self.send(info, (cmd, args), Some(Duration::from_secs(120)), wake)
    }

    /// The same, for what lasts as long as the person wants: placing models.
    pub fn request_long(&mut self, info: &MaxInfo, cmd: &'static str, args: Value, wake: Arc<dyn Fn() + Send + Sync>) -> Option<MaxReply> {
        self.send(info, (cmd, args), None, wake)
    }

    fn send(
        &mut self,
        info: &MaxInfo,
        (cmd, args): (&'static str, Value),
        timeout: Option<Duration>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<MaxReply> {
        let token = self.token.clone()?;
        self.last_pid = Some(info.pid);
        crate::platform::allow_foreground(info.pid);
        let (send, receive) = channel();
        let port = info.port;
        std::thread::spawn(move || {
            let _ = send.send(maxlink::call(port, &token, cmd, args, timeout)); // the tab may have closed
            wake();
        });
        Some(receive)
    }

    /// The supported 3ds Max of this user.
    pub fn targets(&self) -> Vec<MaxTarget> {
        self.targets.iter().filter(|t| t.supported()).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openlibrary-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn chair(dir: &Path) -> Asset {
        let file = dir.join("Chair_vray.max");
        std::fs::write(&file, b"x").unwrap();
        let mut asset: Asset = serde_json::from_value(json!({"n": "Chair", "v": {"vray": file}, "a": [], "p": null, "m": 0.0})).unwrap();
        asset.folder = dir.to_path_buf();
        asset
    }

    fn max(engine: Engine) -> MaxInfo {
        MaxInfo { port: 47820, pid: 7, year: 2026, engine, file: String::new(), protocol: 1, busy: false }
    }

    fn what(asset: &Asset, kind: MergeKind) -> MergeWhat<'_> {
        MergeWhat { asset, kind, chosen: None, id: "a1" }
    }

    fn ready(prepared: Prepared) -> (MergeRequest, Option<PathBuf>) {
        match prepared {
            Prepared::Ready { request, moved } => (request, moved),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_merge_carries_the_file_the_settings_and_the_relink_folders() {
        let dir = scratch("merge");
        let engines = EngineCache::load(&dir.join("engines.json"));
        let (chair, imp) = (chair(&dir), json!({"group": true, "relink_dirs": ["T:\\Textures"]}));
        let (request, moved) = ready(prepare(&what(&chair, MergeKind::Place), &max(Engine::VRay), &imp, &[], &engines, &dir));
        assert_eq!((request.cmd, request.used), ("merge", Engine::VRay));
        assert_eq!(request.args["place"], true);
        assert_eq!(request.args["asset_id"], "a1");
        assert_eq!(request.args["imp"]["group"], true);
        assert_eq!(request.args["relink_dirs"][1], "T:\\Textures");
        assert_eq!(moved, None);
        let imp = json!({"relink_on_import": false, "missing_engine": "ask"});
        let (origin, _) = ready(prepare(&what(&chair, MergeKind::Origin), &max(Engine::Corona), &imp, &[], &engines, &dir));
        assert_eq!(origin.args["place"], false);
        assert_eq!(origin.args["relink_dirs"], Value::Null);
        assert_eq!(origin.used, Engine::VRay); // a Corona scene, and the model has only V-Ray
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn with_several_versions_and_none_of_the_scene_the_person_chooses() {
        let dir = scratch("choose");
        let engines = EngineCache::load(&dir.join("engines.json"));
        let mut chair = chair(&dir);
        let other = dir.join("Chair_fstorm.max");
        std::fs::write(&other, b"x").unwrap();
        chair.versions.insert(Engine::FStorm, other.clone());
        let imp = json!({"missing_engine": "ask"});
        let Prepared::Choose(files) = prepare(&what(&chair, MergeKind::Place), &max(Engine::Corona), &imp, &[], &engines, &dir) else {
            panic!("it must ask");
        };
        assert_eq!(files.len(), 2);
        let chosen = MergeWhat { asset: &chair, kind: MergeKind::Place, chosen: Some(&other), id: "" };
        let (request, _) = ready(prepare(&chosen, &max(Engine::Corona), &imp, &[], &engines, &dir));
        assert_eq!((request.used, request.args["max_path"].as_str()), (Engine::FStorm, other.to_str()));
        assert_eq!(request.args.get("asset_id"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_proxy_goes_beside_the_model() {
        let dir = scratch("proxy");
        let engines = EngineCache::load(&dir.join("engines.json"));
        let chair = chair(&dir);
        let (request, moved) = ready(prepare(&what(&chair, MergeKind::Proxy), &max(Engine::VRay), &json!({}), &[], &engines, &dir.join("local")));
        assert_eq!(request.cmd, "merge_proxy");
        assert_eq!(request.args["engine"], "vray");
        assert!(request.args["proxy_dir"].as_str().unwrap().contains("_proxies"));
        assert_eq!(moved, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_model_without_files_is_said() {
        let dir = scratch("nofile");
        let engines = EngineCache::load(&dir.join("engines.json"));
        let mut empty = chair(&dir);
        empty.versions.clear();
        let prepared = prepare(&what(&empty, MergeKind::Place), &max(Engine::VRay), &json!({}), &[], &engines, &dir);
        assert_eq!(prepared, Prepared::NoFile);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
