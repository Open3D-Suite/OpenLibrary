// SPDX-License-Identifier: GPL-3.0-or-later
//! Talking to the bridge inside each open 3ds Max.
//!
//! The bridge listens on 127.0.0.1, on one port of 47820-47829 per 3ds Max. One JSON line goes per request,
//! `{"cmd", "args", "token"}`, and one comes back, `{"ok", "result" | "error"}`. The token is made once on this
//! computer, kept encrypted for the Windows user (DPAPI) in the app's data folder, and written, encrypted,
//! into each bridge's `app.json`.

use crate::library::Asset;
use crate::maxfile::Engine;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PORTS: std::ops::RangeInclusive<u16> = 47820..=47829;
/// The bridge's protocol this app speaks.
pub const PROTOCOL: u64 = 2;
const TOKEN_FILE: &str = "bridge-token";

/// What went wrong with a request.
#[derive(Debug, PartialEq, Eq)]
pub enum MaxError {
    /// Nothing listens on that port: that 3ds Max closed, or has no bridge.
    NotThere,
    /// 3ds Max is still on the command before.
    Busy,
    /// The bridge answered with an error.
    Failed(String),
}

impl std::fmt::Display for MaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotThere => write!(f, "3ds Max is not answering"),
            Self::Busy => write!(f, "3ds Max is busy with the command before"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// The token: `(in clear, for the requests; encrypted, in hex, for app.json)`. Made the first time and kept,
/// encrypted, in `data_dir`; `None` when Windows cannot make or keep it.
pub fn bridge_token(data_dir: &Path) -> Option<(String, String)> {
    let file = data_dir.join(TOKEN_FILE);
    if let Some(sealed) = std::fs::read_to_string(&file).ok().and_then(|t| unhex(&t))
        && let Some(plain) = crate::platform::unprotect(&sealed).and_then(|p| String::from_utf8(p).ok())
    {
        return Some((plain, hex(&sealed)));
    }
    let plain = hex(&crate::platform::random_bytes(32)?);
    let sealed = crate::platform::protect(plain.as_bytes())?;
    std::fs::create_dir_all(data_dir).ok()?;
    std::fs::write(&file, hex(&sealed)).ok()?;
    Some((plain, hex(&sealed)))
}

/// One request to the bridge on `port`. `timeout: None` waits as long as 3ds Max takes (placing a model waits
/// for the person's clicks).
pub fn call(port: u16, token: &str, cmd: &str, args: Value, timeout: Option<Duration>) -> Result<Value, MaxError> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&address, Duration::from_millis(400)).map_err(|_| MaxError::NotThere)?;
    stream.set_read_timeout(timeout).map_err(|_| MaxError::NotThere)?;
    let mut line = serde_json::to_vec(&json!({"cmd": cmd, "args": args, "token": token})).map_err(|e| MaxError::Failed(e.to_string()))?;
    line.push(b'\n');
    (&stream).write_all(&line).map_err(|_| MaxError::NotThere)?;
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer).map_err(|_| MaxError::NotThere)?;
    let answer: Value = serde_json::from_str(&answer).map_err(|_| MaxError::NotThere)?;
    if answer.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(answer.get("result").cloned().unwrap_or(Value::Null));
    }
    if answer.get("busy").and_then(Value::as_bool) == Some(true) {
        return Err(MaxError::Busy);
    }
    Err(MaxError::Failed(answer.get("error").and_then(Value::as_str).unwrap_or("unknown error").to_string()))
}

/// An open 3ds Max with the bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct MaxInfo {
    pub port: u16,
    pub pid: u32,
    pub year: u32,
    pub engine: Engine,
    pub file: String,
    pub protocol: u64,
    pub busy: bool,
}

impl MaxInfo {
    fn from_ping(port: u16, result: &Value) -> Option<Self> {
        let engine = result.get("engine").and_then(Value::as_str).and_then(Engine::from_key).unwrap_or(Engine::Other);
        Some(Self {
            port,
            pid: u32::try_from(result.get("pid")?.as_u64()?).ok()?,
            year: result.get("max_version").and_then(Value::as_u64).and_then(|y| u32::try_from(y).ok()).unwrap_or(0),
            engine,
            file: result.get("file").and_then(Value::as_str).unwrap_or("").to_string(),
            protocol: result.get("protocol").and_then(Value::as_u64).unwrap_or(0),
            busy: result.get("busy").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

/// The open 3ds Max with the bridge, asked all at once.
pub fn discover(token: &str) -> Vec<MaxInfo> {
    let threads: Vec<_> = PORTS
        .map(|port| {
            let token = token.to_string();
            std::thread::spawn(move || {
                call(port, &token, "ping", json!({}), Some(Duration::from_millis(600))).ok().and_then(|r| MaxInfo::from_ping(port, &r))
            })
        })
        .collect();
    threads.into_iter().filter_map(|t| t.join().ok().flatten()).collect()
}

/// The file of an asset to merge into a scene of `engine`: the version of that renderer, or else the first one
/// it has. The bool says whether it is of the scene's renderer.
pub fn choose_file(asset: &Asset, engine: Engine, detect: impl Fn(&Path) -> Engine) -> Option<(PathBuf, Engine, bool)> {
    if let Some(file) = asset.file_for_engine(engine, &detect) {
        return Some((file.to_path_buf(), engine, true));
    }
    let first = asset.versions.iter().map(|(e, p)| (p.clone(), *e)).chain(asset.auto_files.iter().map(|p| (p.clone(), detect(p)))).next()?;
    Some((first.0, first.1, false))
}

/// Where 3ds Max looks for the textures of a merged model that are not where it says: the model's folder, the
/// texture folders at the root of the library, and the person's own.
pub fn relink_dirs(asset: &Asset, roots: &[PathBuf], extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = vec![asset.folder.clone()];
    for root in roots {
        for sub in ["maps", "Maps", "textures", "Textures", "tex"] {
            let candidate = root.join(sub);
            if candidate.is_dir() && !out.contains(&candidate) {
                out.push(candidate);
            }
        }
    }
    out.extend(extra.iter().filter(|e| !e.as_os_str().is_empty()).cloned());
    out
}

/// The folder of a model's proxy: one per model, renderer, mode and date of the `.max`, so that a proxy made in
/// any project is used again while the `.max` does not change. Beside the model (`_proxies`), or under `custom`.
pub fn proxy_dir(max_path: &Path, asset_name: &str, engine: Engine, mode: &str, custom: Option<&Path>) -> PathBuf {
    let stamp = std::fs::metadata(max_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let sub = format!("{asset_name}_{}_{mode}_{stamp:x}", engine.key());
    match custom {
        Some(dir) => {
            let key = crate::library::norm_key(max_path);
            let hash = key.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
            dir.join(format!("{sub}_{hash:08x}"))
        }
        None => max_path.parent().unwrap_or(Path::new("")).join("_proxies").join(sub),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::net::TcpListener;

    /// A stand-in for the bridge: answers `ping` with the token, and errors otherwise.
    fn fake_bridge(token: &'static str) -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                let answer = if request["token"] != token {
                    json!({"ok": false, "error": "Refused"})
                } else if request["cmd"] == "ping" {
                    json!({"ok": true, "result": {"pid": 1234, "max_version": 2026, "engine": "corona", "file": "scene.max", "protocol": 1}})
                } else if request["cmd"] == "merge" {
                    json!({"ok": false, "busy": true, "error": "busy"})
                } else {
                    json!({"ok": false, "error": "Unknown command"})
                };
                let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
            }
        });
        port
    }

    #[test]
    fn a_request_and_its_answer() {
        let port = fake_bridge("t0k3n");
        let result = call(port, "t0k3n", "ping", json!({}), Some(Duration::from_secs(2))).unwrap();
        let info = MaxInfo::from_ping(port, &result).unwrap();
        assert_eq!((info.pid, info.year, info.engine, info.protocol), (1234, 2026, Engine::Corona, 1));
        assert_eq!(call(port, "wrong", "ping", json!({}), None), Err(MaxError::Failed("Refused".into())));
        assert_eq!(call(port, "t0k3n", "merge", json!({}), None), Err(MaxError::Busy));
    }

    #[test]
    fn nobody_listening() {
        // no program listens on port 1; one just freed may be taken by another test
        assert_eq!(call(1, "x", "ping", json!({}), None), Err(MaxError::NotThere));
    }

    #[test]
    fn the_token_is_made_once_and_kept() {
        let dir = std::env::temp_dir().join(format!("openlibrary-token-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (plain, sealed) = bridge_token(&dir).unwrap();
        assert_eq!(plain.len(), 64);
        assert_eq!(bridge_token(&dir), Some((plain.clone(), sealed.clone())));
        assert!(!sealed.contains(&plain)); // what is kept is encrypted
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(unhex(&hex(&[0, 1, 254, 255])), Some(vec![0, 1, 254, 255]));
        assert_eq!(unhex("abc"), None);
        assert_eq!(unhex("zz"), None);
    }

    fn asset() -> Asset {
        let mut versions = BTreeMap::new();
        versions.insert(Engine::VRay, PathBuf::from(r"X:\Lib\Chair_vray.max"));
        serde_json::from_value::<Asset>(json!({"n": "Chair", "v": versions, "a": [r"X:\Lib\Chair.max"], "p": null, "m": 0.0}))
            .map(|mut a| {
                a.folder = PathBuf::from(r"X:\Lib");
                a
            })
            .unwrap()
    }

    #[test]
    fn the_file_for_the_scene() {
        let a = asset();
        assert_eq!(choose_file(&a, Engine::VRay, |_| Engine::Other), Some((PathBuf::from(r"X:\Lib\Chair_vray.max"), Engine::VRay, true)));
        assert_eq!(choose_file(&a, Engine::Corona, |_| Engine::Corona), Some((PathBuf::from(r"X:\Lib\Chair.max"), Engine::Corona, true)));
        // no version of the scene's renderer: the first one, said so
        assert_eq!(choose_file(&a, Engine::FStorm, |_| Engine::Corona), Some((PathBuf::from(r"X:\Lib\Chair_vray.max"), Engine::VRay, false)));
    }

    #[test]
    fn proxy_folders() {
        let max = PathBuf::from(r"X:\Lib\Chair_vray.max");
        let beside = proxy_dir(&max, "Chair", Engine::VRay, "single", None);
        assert_eq!(beside, PathBuf::from(r"X:\Lib\_proxies\Chair_vray_single_0"));
        let custom = proxy_dir(&max, "Chair", Engine::VRay, "single", Some(Path::new(r"P:\Proxies")));
        assert!(custom.starts_with(r"P:\Proxies"));
        assert_ne!(custom, proxy_dir(&PathBuf::from(r"Y:\Chair_vray.max"), "Chair", Engine::VRay, "single", Some(Path::new(r"P:\Proxies"))));
    }

    #[test]
    fn relink_folders_start_with_the_model() {
        let a = asset();
        let dirs = relink_dirs(&a, &[], &[PathBuf::from(r"T:\Textures"), PathBuf::new()]);
        assert_eq!(dirs, [PathBuf::from(r"X:\Lib"), PathBuf::from(r"T:\Textures")]);
    }
}
