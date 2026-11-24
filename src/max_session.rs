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
use std::sync::mpsc::{Receiver, Sender, channel};
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

/// A merge ready to send: the command, its arguments, and what to say about the file chosen.
#[derive(Debug, PartialEq)]
pub struct MergeRequest {
    pub cmd: &'static str,
    pub args: Value,
    pub used: Engine,
    pub exact: bool,
}

fn engine_label(engine: Engine) -> &'static str {
    match engine {
        Engine::Corona => "Corona",
        Engine::VRay => "V-Ray",
        Engine::FStorm => "FStorm",
        Engine::Other => "other",
    }
}

fn writable(folder: &Path) -> bool {
    let probe = folder.join(".olib_write_test");
    std::fs::create_dir_all(folder).is_ok() && std::fs::write(&probe, b"ok").is_ok() && std::fs::remove_file(&probe).is_ok()
}

/// The request for a merge of `asset` into the 3ds Max `info`, with the import settings `imp` (the `import`
/// object of the settings). `local_proxies` is where a proxy goes when it cannot be written beside the model.
/// The second value is the folder of a proxy that went there.
pub fn merge_request(
    asset: &Asset,
    info: &MaxInfo,
    kind: MergeKind,
    imp: &Value,
    roots: &[PathBuf],
    engines: &EngineCache,
    local_proxies: &Path,
) -> Result<(MergeRequest, Option<PathBuf>), String> {
    let (path, used, exact) = maxlink::choose_file(asset, info.engine, |f| engines.detect(f)).ok_or_else(|| asset.name.clone())?;
    let get = |key: &str| imp.get(key);
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
        "place": kind != MergeKind::Origin,
        "relink_dirs": relink,
    });
    if kind != MergeKind::Proxy {
        return Ok((MergeRequest { cmd: "merge", args, used, exact }, None));
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
    Ok((MergeRequest { cmd: "merge_proxy", args, used, exact }, moved))
}

/// Where the answer to [`MaxSession::request`] arrives.
pub type MaxReply = Receiver<Result<Value, MaxError>>;

/// What a request sent back, for the status bar.
pub struct Done {
    pub text: String,
}

pub struct MaxSession {
    token: Option<String>,
    sealed: Option<String>,
    maxes: Arc<Mutex<Vec<MaxInfo>>>,
    done: (Sender<Done>, Receiver<Done>),
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
        if let Some((plain, _)) = &token {
            let (plain, maxes) = (plain.clone(), maxes.clone());
            std::thread::spawn(move || {
                loop {
                    let found = maxlink::discover(&plain);
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
                    std::thread::sleep(Duration::from_secs(3));
                }
            });
        }
        let (sealed, token) = match token {
            Some((plain, sealed)) => (Some(sealed), Some(plain)),
            None => (None, None),
        };
        Self { token, sealed, maxes, done: channel(), last_pid: None, prompt, targets }
    }

    pub fn maxes(&self) -> Vec<MaxInfo> {
        self.maxes.lock().map(|m| m.clone()).unwrap_or_default()
    }

    /// The 3ds Max a command without a drop goes to: the one dropped on last, or the first one open.
    pub fn target(&self) -> Option<MaxInfo> {
        let maxes = self.maxes();
        self.last_pid.and_then(|pid| maxes.iter().find(|m| m.pid == pid).cloned()).or_else(|| maxes.first().cloned())
    }

    /// The 3ds Max whose window is under the mouse pointer, if it has the bridge.
    pub fn under_cursor(&self) -> Option<MaxInfo> {
        let pid = crate::platform::process_under_cursor()?;
        self.maxes().into_iter().find(|m| m.pid == pid)
    }

    /// What came back from the requests since the last call.
    pub fn poll(&self) -> Vec<Done> {
        self.done.1.try_iter().collect()
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

    /// Sends a merge to `info` on a thread; its result comes back through [`Self::poll`].
    pub fn send_merge(
        &mut self,
        request: MergeRequest,
        name: &str,
        info: &MaxInfo,
        lang: crate::open3d_settings::Language,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) {
        let Some(token) = self.token.clone() else { return };
        self.last_pid = Some(info.pid);
        crate::platform::allow_foreground(info.pid);
        let (send, name, label, port) = (self.done.0.clone(), name.to_string(), format!("3ds Max {}", info.year), info.port);
        let note = (!request.exact).then(|| {
            fill(Text::OtherEngine.get(lang), &[("name", &name), ("engine", &engine_label(info.engine)), ("used", &engine_label(request.used))])
        });
        std::thread::spawn(move || {
            let text = match maxlink::call(port, &token, request.cmd, request.args, None) {
                Ok(count) => {
                    let merged = fill(Text::Merged.get(lang), &[("name", &name), ("count", &count.as_u64().unwrap_or(0)), ("max", &label)]);
                    note.map_or(merged.clone(), |n| format!("{merged} · {n}"))
                }
                Err(MaxError::Busy) => Text::Busy.get(lang).to_string(),
                Err(error) => fill(Text::MergeFailed.get(lang), &[("name", &name), ("error", &error)]),
            };
            let _ = send.send(Done { text });
            wake();
        });
    }

    /// Sends a command to `info` on a thread. A 3ds Max with a window of its own open does not answer: after two
    /// minutes it is not waited for. `None` without a token.
    pub fn request(&mut self, info: &MaxInfo, cmd: &'static str, args: Value, wake: Arc<dyn Fn() + Send + Sync>) -> Option<MaxReply> {
        let token = self.token.clone()?;
        self.last_pid = Some(info.pid);
        crate::platform::allow_foreground(info.pid);
        let (send, receive) = channel();
        let port = info.port;
        std::thread::spawn(move || {
            let _ = send.send(maxlink::call(port, &token, cmd, args, Some(Duration::from_secs(120)))); // the tab may have closed
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

    #[test]
    fn a_merge_carries_the_file_the_settings_and_the_relink_folders() {
        let dir = scratch("merge");
        let engines = EngineCache::load(&dir.join("engines.json"));
        let imp = json!({"group": true, "relink_dirs": ["T:\\Textures"]});
        let (request, moved) = merge_request(&chair(&dir), &max(Engine::VRay), MergeKind::Place, &imp, &[], &engines, &dir).unwrap();
        assert_eq!(request.cmd, "merge");
        assert!(request.exact);
        assert_eq!(request.args["place"], true);
        assert_eq!(request.args["imp"]["group"], true);
        assert_eq!(request.args["relink_dirs"][1], "T:\\Textures");
        assert_eq!(moved, None);
        let (origin, _) =
            merge_request(&chair(&dir), &max(Engine::Corona), MergeKind::Origin, &json!({"relink_on_import": false}), &[], &engines, &dir).unwrap();
        assert_eq!(origin.args["place"], false);
        assert_eq!(origin.args["relink_dirs"], Value::Null);
        assert!(!origin.exact); // a Corona scene, and the model has only V-Ray
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_proxy_goes_beside_the_model() {
        let dir = scratch("proxy");
        let engines = EngineCache::load(&dir.join("engines.json"));
        let (request, moved) =
            merge_request(&chair(&dir), &max(Engine::VRay), MergeKind::Proxy, &json!({}), &[], &engines, &dir.join("local")).unwrap();
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
        assert_eq!(merge_request(&empty, &max(Engine::VRay), MergeKind::Place, &json!({}), &[], &engines, &dir), Err("Chair".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
