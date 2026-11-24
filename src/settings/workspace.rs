// SPDX-License-Identifier: GPL-3.0-or-later
//! The workspace a team shares: a folder (on the studio's server, say) whose `workspace.json` holds what is
//! everyone's: the tabs and their folders, the hidden folders, the folder icons, the sizes and mapping of each
//! material and the preview settings. What is personal stays in this computer's file.
//!
//! The file is the Python version's, so that both may share it. Saving mixes with what others saved: of the
//! [`ENTRIES`] each entry apart, so that two people who write sizes at once do not undo each other. Without the
//! folder, this computer's last copy is used, and the changes are joined when it comes back.

use super::{Settings, migrate_libraries};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const WORKSPACE_FILE: &str = "workspace.json";
/// The folder made inside the one the team chooses.
pub const WORKSPACE_DIR: &str = "_OpenLibrary";
pub const SHARED: [&str; 6] = ["libraries", "hidden_folders", "folder_marks", "surface_sizes", "surface_mapping", "preview"];
const ENTRIES: [&str; 3] = ["folder_marks", "surface_sizes", "surface_mapping"];
const README: &str = "Shared workspace of OpenLibrary.

Used by everyone who chooses this folder in OpenLibrary (Settings > Workspace):
  workspace.json  tabs and their folders, hidden folders, folder icons, sizes and mapping of each
                  material and preview settings. The app joins it with what each person has.
  medidas/        sizes from the makers, one file per library folder.

There is nothing to edit by hand. If it is deleted, each person goes on with the last copy they have.
";

/// How the workspace is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WsState {
    /// None is used.
    #[default]
    Local,
    Ok,
    /// Its file is not there yet.
    New,
    /// It cannot be reached or read: the last copy is used.
    Offline,
}

pub type WsStamp = Option<(SystemTime, u64)>;

/// What is known of the shared file.
#[derive(Clone, Default)]
pub(super) struct Shared {
    pub state: WsState,
    /// What the shared file held when it was last read or written.
    synced: Map<String, Value>,
    stamp: WsStamp,
}

/// The workspace folder for the one the person chose: that one when it is a workspace (or is called so), or the
/// one inside it.
pub fn workspace_dir(chosen: &Path) -> PathBuf {
    let named = chosen.file_name().is_some_and(|n| n.eq_ignore_ascii_case(WORKSPACE_DIR));
    if named || chosen.join(WORKSPACE_FILE).is_file() { chosen.to_path_buf() } else { chosen.join(WORKSPACE_DIR) }
}

/// Makes the folder with what it needs.
pub fn prepare_workspace(folder: &Path) -> std::io::Result<()> {
    fs::create_dir_all(folder.join("medidas"))?;
    let readme = folder.join("README.txt");
    if !readme.exists() && !folder.join("LEEME.txt").exists() {
        fs::write(readme, README)?;
    }
    Ok(())
}

/// The date and size of a workspace's file, to tell when someone changed it. It may wait for the network.
pub fn workspace_stamp(folder: &Path) -> WsStamp {
    super::stamp(&folder.join(WORKSPACE_FILE))
}

fn read(path: &Path) -> Result<Map<String, Value>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let Value::Object(mut data) = serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())? else {
        return Err("not a JSON object".into());
    };
    if data.contains_key("libraries") {
        let libraries = migrate_libraries(data.get("libraries"));
        data.insert("libraries".into(), libraries);
    }
    Ok(data)
}

/// Sets or removes a key.
fn put(map: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    match value {
        Some(value) => {
            map.insert(key.to_string(), value.clone());
        }
        None => {
            map.remove(key);
        }
    }
}

/// What `mine` changed since `base`, entry by entry, on top of `theirs`.
fn join_entries(mine: &Value, base: Option<&Value>, theirs: &Value) -> Value {
    let empty = Map::new();
    let (Some(mine), base) = (mine.as_object(), base.and_then(Value::as_object).unwrap_or(&empty)) else { return mine.clone() };
    let mut out = theirs.as_object().cloned().unwrap_or_default();
    for key in mine.keys().chain(base.keys()) {
        if mine.get(key) != base.get(key) {
            put(&mut out, key, mine.get(key));
        }
    }
    Value::Object(out)
}

impl Settings {
    /// The workspace's folder, if one is used.
    pub fn workspace(&self) -> Option<PathBuf> {
        self.data.get("workspace").and_then(Value::as_str).map(str::trim).filter(|w| !w.is_empty()).map(PathBuf::from)
    }

    pub fn ws_state(&self) -> WsState {
        self.shared.state
    }

    /// The file's date and size when it was last read or written.
    pub fn ws_stamp(&self) -> WsStamp {
        self.shared.stamp
    }

    fn note_synced(&mut self) {
        self.shared.synced = SHARED.iter().filter_map(|k| Some((k.to_string(), self.data.get(*k)?.clone()))).collect();
    }

    fn shared_changed(&self) -> bool {
        SHARED.iter().any(|k| self.data.get(*k) != self.shared.synced.get(*k))
    }

    /// Takes what is everyone's from the workspace, if it can be read.
    pub(super) fn load_shared(&mut self) {
        self.shared.synced.clear();
        let Some(folder) = self.workspace() else {
            self.shared.state = WsState::Local;
            return;
        };
        let path = folder.join(WORKSPACE_FILE);
        self.shared.state = match (folder.is_dir(), path.is_file()) {
            (false, _) => WsState::Offline,
            (true, false) => WsState::New,
            (true, true) => match read(&path) {
                Err(_) => WsState::Offline,
                Ok(shared) => {
                    for key in SHARED {
                        if let Some(value) = shared.get(key) {
                            self.data.insert(key.into(), value.clone());
                        }
                    }
                    self.note_synced();
                    self.shared.stamp = super::stamp(&path);
                    WsState::Ok
                }
            },
        };
    }

    /// Writes what is everyone's, mixed with what others wrote. A file that cannot be read is never written over.
    pub(super) fn save_shared(&mut self) {
        let Some(folder) = self.workspace() else { return };
        let path = folder.join(WORKSPACE_FILE);
        let new = match self.shared.state {
            WsState::New => true,
            WsState::Ok if self.shared_changed() => false,
            _ => return, // nothing of everyone's has changed: the server's file is left alone
        };
        // a new one may be there by now, made by somebody else; one that was there and is gone is not made again
        let disk = if new && !path.is_file() { Ok(Map::new()) } else { read(&path) };
        let Ok(disk) = disk else {
            self.shared.state = WsState::Offline;
            return;
        };
        let mut out = disk.clone();
        for key in SHARED {
            let (mine, base, theirs) = (self.data.get(key), self.shared.synced.get(key), disk.get(key));
            match (theirs, mine) {
                (None, Some(mine)) => put(&mut out, key, Some(mine)),
                (Some(theirs), Some(mine)) if ENTRIES.contains(&key) && mine != theirs && Some(mine) != base => {
                    put(&mut out, key, Some(&join_entries(mine, base, theirs)));
                }
                (Some(_), mine) if new || mine != base => put(&mut out, key, mine),
                _ => {} // not touched here: what is there stays, it may be somebody else's
            }
        }
        let tmp = folder.join(format!("{WORKSPACE_FILE}.{}.tmp", std::process::id()));
        let written = serde_json::to_vec_pretty(&out)
            .map_err(std::io::Error::other)
            .and_then(|text| fs::create_dir_all(&folder).and_then(|()| fs::write(&tmp, text)))
            .and_then(|()| fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = fs::remove_file(&tmp); // half a file nobody reads
            self.shared.state = WsState::Offline;
            return;
        }
        for key in SHARED {
            if let Some(value) = out.get(key) {
                self.data.insert(key.into(), value.clone());
            }
        }
        self.note_synced();
        self.shared.stamp = super::stamp(&path);
        self.shared.state = WsState::Ok;
    }

    /// Brings what others changed in the workspace, without losing what is not saved yet here. The keys that
    /// changed.
    pub fn refresh_shared(&mut self) -> Vec<&'static str> {
        let Some(folder) = self.workspace() else { return Vec::new() };
        let path = folder.join(WORKSPACE_FILE);
        let state = self.shared.state;
        if state == WsState::Local || (state == WsState::Ok && super::stamp(&path) == self.shared.stamp) || !path.is_file() {
            return Vec::new();
        }
        let before = self.data.clone();
        if state == WsState::Ok && !self.shared_changed() {
            self.load_shared();
        } else if state == WsState::Offline && self.shared.synced.is_empty() {
            // it was not read since the app opened: theirs goes first, and there is nothing of ours to join
            self.load_shared();
        } else {
            self.shared.state = WsState::Ok;
            if self.shared_changed() {
                self.save_shared();
            } else {
                self.load_shared();
            }
        }
        SHARED.into_iter().filter(|k| self.data.get(*k) != before.get(*k)).collect()
    }

    /// Changes the workspace: `theirs` takes what the folder has; otherwise it is made with what there is here.
    /// `None` goes back to this computer alone.
    pub fn set_workspace(&mut self, folder: Option<&Path>, theirs: bool) -> Result<(), String> {
        let text = folder.map(|f| f.display().to_string()).unwrap_or_default();
        self.data.insert("workspace".into(), text.into());
        self.shared = Shared::default();
        if folder.is_some() {
            if theirs {
                self.load_shared();
            } else {
                self.shared.state = WsState::New;
            }
        }
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Place(PathBuf);

    impl Place {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("openlibrary-workspace-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn person(&self, name: &str) -> Settings {
            Settings::load(&self.0.join(name).join("settings.json"))
        }

        fn ws(&self) -> PathBuf {
            self.0.join("nas").join("OpenLibrary")
        }

        fn file(&self) -> Value {
            serde_json::from_slice(&fs::read(self.ws().join(WORKSPACE_FILE)).unwrap()).unwrap()
        }
    }

    impl Drop for Place {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn set(settings: &mut Settings, key: &str, value: Value) {
        settings.data.insert(key.into(), value);
    }

    #[test]
    fn a_team_shares_and_nobody_undoes_the_others() {
        let place = Place::new("team");
        let mut ana = place.person("ana");
        set(&mut ana, "libraries", json!([{"name": "Models", "kind": "models", "paths": ["\\\\NAS\\3D"]}]));
        set(&mut ana, "thumb_size", json!(220));
        ana.set_workspace(Some(&place.ws()), false).unwrap();
        assert_eq!(ana.ws_state(), WsState::Ok);
        let mut ben = place.person("ben");
        ben.set_workspace(Some(&place.ws()), true).unwrap();
        assert_eq!(ben.library_paths(0), [PathBuf::from("\\\\NAS\\3D")]);
        assert_ne!(ben.data["thumb_size"], json!(220)); // personal
        // both write sizes of other materials at once
        set(&mut ana, "surface_sizes", json!({"a": [1]}));
        set(&mut ben, "surface_sizes", json!({"b": [2]}));
        ana.save().unwrap();
        ben.save().unwrap();
        assert_eq!(place.file()["surface_sizes"], json!({"a": [1], "b": [2]}));
        assert!(place.file().get("thumb_size").is_none());
        assert_eq!(ana.refresh_shared(), ["surface_sizes"]);
        assert_eq!(ana.data["surface_sizes"], json!({"a": [1], "b": [2]}));
        // one removes a size and the other adds a tab
        set(&mut ben, "surface_sizes", json!({"a": [1]}));
        ben.save().unwrap();
        let mut tabs = ana.data["libraries"].clone();
        tabs.as_array_mut().unwrap().push(json!({"name": "Wood", "kind": "materials", "paths": []}));
        set(&mut ana, "libraries", tabs);
        ana.save().unwrap();
        ben.refresh_shared();
        assert_eq!((ben.libraries().len(), &ben.data["surface_sizes"]), (2, &json!({"a": [1]})));
    }

    #[test]
    fn a_key_that_is_new_for_one_arrives_and_stays() {
        let place = Place::new("new");
        let mut ana = place.person("ana");
        ana.set_workspace(Some(&place.ws()), false).unwrap();
        let mut ben = place.person("ben");
        ben.set_workspace(Some(&place.ws()), true).unwrap();
        set(&mut ben, "surface_mapping", json!({"m": "uvw"}));
        ben.save().unwrap();
        assert_eq!(ana.refresh_shared(), ["surface_mapping"]);
        ana.save().unwrap();
        ben.refresh_shared();
        assert_eq!(ben.data["surface_mapping"], json!({"m": "uvw"}));
    }

    #[test]
    fn without_the_folder_the_last_copy_is_used_and_joined_when_it_is_back() {
        let place = Place::new("offline");
        let mut ana = place.person("ana");
        set(&mut ana, "libraries", json!([{"name": "NAS", "kind": "models", "paths": ["N:\\X"]}]));
        ana.set_workspace(Some(&place.ws()), false).unwrap();
        let (nas, away) = (place.0.join("nas"), place.0.join("away"));
        fs::rename(&nas, &away).unwrap();
        let mut again = place.person("ana");
        assert_eq!((again.ws_state(), again.libraries()[0]["name"].as_str()), (WsState::Offline, Some("NAS")));
        set(&mut again, "thumb_size", json!(300));
        again.save().unwrap();
        assert_eq!(again.ws_state(), WsState::Offline);
        // the one whose app was open loses the connection, writes a size, and it comes back
        set(&mut ana, "surface_sizes", json!({"a": [1]}));
        ana.save().unwrap();
        assert_eq!(ana.ws_state(), WsState::Offline);
        fs::rename(&away, &nas).unwrap();
        ana.refresh_shared();
        assert_eq!(place.file()["surface_sizes"], json!({"a": [1]}));
        again.refresh_shared();
        assert_eq!(again.ws_state(), WsState::Ok);
    }

    #[test]
    fn a_broken_shared_file_is_not_written_over() {
        let place = Place::new("broken");
        let mut ana = place.person("ana");
        ana.set_workspace(Some(&place.ws()), false).unwrap();
        fs::write(place.ws().join(WORKSPACE_FILE), b"{half").unwrap();
        set(&mut ana, "surface_sizes", json!({"a": [1]}));
        ana.save().unwrap();
        assert_eq!(ana.ws_state(), WsState::Offline);
        assert_eq!(fs::read(place.ws().join(WORKSPACE_FILE)).unwrap(), b"{half");
    }

    #[test]
    fn what_the_file_has_and_this_version_does_not_know_stays() {
        let place = Place::new("unknown");
        let mut ana = place.person("ana");
        ana.set_workspace(Some(&place.ws()), false).unwrap();
        let mut file = place.file();
        file["from_the_future"] = json!(7);
        fs::write(place.ws().join(WORKSPACE_FILE), serde_json::to_vec(&file).unwrap()).unwrap();
        set(&mut ana, "surface_sizes", json!({"a": [1]}));
        ana.save().unwrap();
        assert_eq!((&place.file()["from_the_future"], &place.file()["surface_sizes"]), (&json!(7), &json!({"a": [1]})));
    }

    #[test]
    fn the_chosen_folder_is_the_workspace_or_holds_it() {
        let place = Place::new("dir");
        assert_eq!(workspace_dir(&place.0), place.0.join(WORKSPACE_DIR));
        assert_eq!(workspace_dir(&place.0.join("_openlibrary")), place.0.join("_openlibrary"));
        fs::write(place.0.join(WORKSPACE_FILE), b"{}").unwrap();
        assert_eq!(workspace_dir(&place.0), place.0);
        prepare_workspace(&place.0.join("x")).unwrap();
        assert!(place.0.join("x").join("medidas").is_dir() && place.0.join("x").join("README.txt").is_file());
    }
}
