// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenLibrary's own settings: `settings.json` in its settings folder
//! (`%APPDATA%\Open3D\OpenLibrary`, from [`crate::open3d_settings::app_settings_dir`]).
//!
//! The file is kept as JSON values, so that keys this version does not know (written by a newer one, or by the
//! Python version it replaces) are kept as they are. The defaults fill in what the file lacks.

use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const FILE_NAME: &str = "settings.json";
/// The kinds of tab: models, and the three kinds of surface.
pub const KINDS: [&str; 4] = ["models", "materials", "textures", "interiors"];
/// What a tab shows when it opens: favourites and the rest, favourites only, or everything.
pub const SHOW_MODES: [&str; 3] = ["both", "favorites", "all"];

/// The defaults; a file's own values go on top of them.
pub fn defaults() -> Map<String, Value> {
    let value = json!({
        "libraries": [
            {"name": "Models", "kind": "models", "paths": []},
            {"name": "Materials", "kind": "materials", "paths": []},
            {"name": "Textures", "kind": "textures", "paths": []},
        ],
        "current_library": 0,
        "show_info_bar": true,
        "always_on_top": false,
        "thumb_size": 160,
        "include_subfolders": true,
        "import": {
            "group": true,
            "layer_mode": "asset",
            "object_dups": "autorename",
            "material_dups": "scene",
            "import_lights": false,
            "import_cameras": false,
            "place_on_surface": true,
            "place_multiple": true,
            "proxy_mode": "single",
            "proxy_location": "asset",
            "proxy_custom_dir": "",
            "proxy_display": "preview",
            "missing_engine": "ask",
            "relink_on_import": true,
            "relink_dirs": [],
        },
        "catalog": {"maps_folder": "z-maps", "last_dir": ""},
        "preview": {"size": 512, "method": "render", "margin": 1.15, "elevation": 20.0, "azimuth": 35.0},
    });
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// `data` on top of `base`; objects are merged key by key, anything else is replaced.
fn merge(mut base: Map<String, Value>, data: Map<String, Value>) -> Map<String, Value> {
    for (key, value) in data {
        match (base.get_mut(&key), value) {
            (Some(Value::Object(mine)), Value::Object(theirs)) => {
                let merged = merge(std::mem::take(mine), theirs);
                *mine = merged;
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
    base
}

/// Tabs from any version of the file: the old `[{"name", "path"}]` becomes folders of a models tab.
pub fn migrate_libraries(libraries: Option<&Value>) -> Value {
    let default = || defaults().remove("libraries").unwrap_or(Value::Array(Vec::new()));
    let Some(Value::Array(list)) = libraries else { return default() };
    let mut out: Vec<Map<String, Value>> = Vec::new();
    let mut loose: Vec<Value> = Vec::new();
    for lib in list.iter().filter_map(Value::as_object) {
        if let Some(paths) = lib.get("paths") {
            let mut entry = Map::new();
            let name = lib.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).unwrap_or("Library");
            entry.insert("name".into(), name.into());
            let kind = lib.get("kind").and_then(Value::as_str).filter(|k| KINDS.contains(k)).unwrap_or("models");
            entry.insert("kind".into(), kind.into());
            let paths: Vec<Value> =
                paths.as_array().map(|p| p.iter().filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).cloned().collect()).unwrap_or_default();
            entry.insert("paths".into(), Value::Array(paths));
            if let Some(show) = lib.get("show").and_then(Value::as_str).filter(|s| SHOW_MODES.contains(s)) {
                entry.insert("show".into(), show.into());
            }
            for key in ["tree_fav", "tree_all"] {
                if let Some(flag) = lib.get(key).and_then(Value::as_bool) {
                    entry.insert(key.into(), flag.into());
                }
            }
            out.push(entry);
        } else if let Some(path) = lib.get("path").filter(|p| p.as_str().is_some_and(|s| !s.is_empty())) {
            loose.push(path.clone());
        }
    }
    if !loose.is_empty() {
        if !out.iter().any(|l| l.get("kind").and_then(Value::as_str) == Some("models")) {
            if out.is_empty() {
                out = default().as_array().into_iter().flatten().filter_map(|v| v.as_object().cloned()).collect();
            }
            if !out.iter().any(|l| l.get("kind").and_then(Value::as_str) == Some("models")) {
                let mut models = Map::new();
                models.insert("name".into(), "Models".into());
                models.insert("kind".into(), "models".into());
                models.insert("paths".into(), Value::Array(Vec::new()));
                out.insert(0, models);
            }
        }
        let models = out.iter_mut().find(|l| l.get("kind").and_then(Value::as_str) == Some("models"));
        if let Some(Value::Array(paths)) = models.and_then(|m| m.get_mut("paths")) {
            for path in loose {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    Value::Array(out.into_iter().map(Value::Object).collect())
}

fn read(path: &Path) -> Result<Map<String, Value>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let Value::Object(data) = serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())? else {
        return Err("not a JSON object".into());
    };
    let mut data = merge(defaults(), data);
    let libraries = migrate_libraries(data.get("libraries"));
    data.insert("libraries".into(), libraries);
    Ok(data)
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// The settings, as read from their file.
pub struct Settings {
    pub path: PathBuf,
    pub data: Map<String, Value>,
    /// What the file held when it was last read or written, to tell what this copy changed.
    synced: Map<String, Value>,
    disk: Option<(SystemTime, u64)>,
}

impl Settings {
    /// Reads the settings. A file that cannot be read gives way to the copy of the save before
    /// (`settings.json.bak`); with neither, the defaults.
    pub fn load(path: &Path) -> Self {
        let tries = if path.exists() { vec![path.to_path_buf(), sibling(path, ".bak")] } else { vec![path.to_path_buf()] };
        let data = tries.iter().find_map(|p| read(p).ok()).unwrap_or_else(|| {
            let mut data = defaults();
            let libraries = migrate_libraries(data.get("libraries"));
            data.insert("libraries".into(), libraries);
            data
        });
        Self { path: path.to_path_buf(), synced: data.clone(), data, disk: stamp(path) }
    }

    /// Writes the settings without undoing what another OpenLibrary saved meanwhile: if the file changed
    /// since it was read, this copy starts from it and writes on top only the keys it changed itself. The
    /// file is written whole into a new file that then takes its place, and the old one stays as `.bak`.
    pub fn save(&mut self) -> Result<(), String> {
        let disk = read(&self.path).ok();
        if let Some(mut theirs) = disk.clone()
            && stamp(&self.path) != self.disk
        {
            let keys: Vec<String> = self.data.keys().chain(self.synced.keys()).cloned().collect();
            for key in keys {
                let mine = self.data.get(&key);
                if mine != self.synced.get(&key) {
                    match mine {
                        Some(value) => {
                            theirs.insert(key, value.clone());
                        }
                        None => {
                            theirs.remove(&key);
                        }
                    }
                }
            }
            self.data = theirs;
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_vec_pretty(&self.data).map_err(|e| e.to_string())?;
        let tmp = sibling(&self.path, ".tmp");
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        if disk.is_some() {
            let _ = fs::copy(&self.path, sibling(&self.path, ".bak")); // a missing backup only loses the backup
        }
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        self.synced = self.data.clone();
        self.disk = stamp(&self.path);
        Ok(())
    }

    /// The tabs, each a JSON object with `name`, `kind` and `paths`.
    pub fn libraries(&self) -> &[Value] {
        self.data.get("libraries").and_then(Value::as_array).map_or(&[], Vec::as_slice)
    }

    /// The index of the tab shown, within the tabs there are.
    pub fn current_library(&self) -> usize {
        let count = self.libraries().len();
        let index = self.data.get("current_library").and_then(Value::as_u64).unwrap_or(0) as usize;
        index.min(count.saturating_sub(1))
    }

    /// The folders of a tab.
    pub fn library_paths(&self, index: usize) -> Vec<PathBuf> {
        self.libraries()
            .get(index)
            .and_then(|l| l.get("paths"))
            .and_then(Value::as_array)
            .map(|p| p.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
            .unwrap_or_default()
    }

    /// The hidden folders of the tree: names (with wildcards) and paths.
    pub fn hidden(&self) -> (Vec<String>, Vec<PathBuf>) {
        let hidden = self.data.get("hidden_folders");
        let list = |key: &str| -> Vec<String> {
            hidden
                .and_then(|h| h.get(key))
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default()
        };
        (list("names"), list("paths").into_iter().map(PathBuf::from).collect())
    }
}

/// Brings the settings of the Python version over, once: when there is no `settings.json` in `new_dir` yet and
/// `old_dir` has one, it is copied (whole, into a new file that then takes its name). The old folder is never
/// changed. Returns whether a file was copied.
pub fn carry_over_old_settings(old_dir: &Path, new_dir: &Path) -> Result<bool, String> {
    let old = old_dir.join(FILE_NAME);
    let new = new_dir.join(FILE_NAME);
    if new.exists() || !old.is_file() {
        return Ok(false);
    }
    fs::create_dir_all(new_dir).map_err(|e| e.to_string())?;
    let tmp = sibling(&new, ".tmp");
    fs::copy(&old, &tmp).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &new).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("openlibrary-settings-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn old_libraries_become_a_models_tab() {
        let out = migrate_libraries(Some(&json!([{"name": "Studio", "path": "N:\\Lib"}, {"name": "Home", "path": "D:\\Lib"}])));
        let models = out.as_array().unwrap().iter().find(|l| l["kind"] == "models").unwrap();
        assert_eq!(models["paths"], json!(["N:\\Lib", "D:\\Lib"]));
        let kept = migrate_libraries(Some(
            &json!([{"name": "Wood", "kind": "materials", "paths": ["", "M:\\Wood"], "show": "favorites", "tree_fav": true}]),
        ));
        assert_eq!(kept, json!([{"name": "Wood", "kind": "materials", "paths": ["M:\\Wood"], "show": "favorites", "tree_fav": true}]));
        let odd = migrate_libraries(Some(&json!([{"name": "", "kind": "rockets", "paths": [], "show": "nope"}])));
        assert_eq!(odd, json!([{"name": "Library", "kind": "models", "paths": []}]));
        assert_eq!(migrate_libraries(Some(&json!("broken"))).as_array().unwrap().len(), 3);
    }

    #[test]
    fn settings_round_trip_and_keep_unknown_keys() {
        let dir = Scratch::new("roundtrip");
        let path = dir.0.join(FILE_NAME);
        fs::write(&path, r#"{"thumb_size": 200, "from_python": {"x": 1}, "import": {"group": false}}"#).unwrap();
        let mut settings = Settings::load(&path);
        assert_eq!(settings.data["thumb_size"], 200);
        assert_eq!(settings.data["import"]["group"], false);
        assert_eq!(settings.data["import"]["layer_mode"], "asset"); // a default fills what the file lacks
        settings.data.insert("always_on_top".into(), true.into());
        settings.save().unwrap();
        let again = Settings::load(&path);
        assert_eq!(again.data["always_on_top"], true);
        assert_eq!(again.data["from_python"], json!({"x": 1}));
    }

    #[test]
    fn two_copies_do_not_undo_each_other() {
        let dir = Scratch::new("two");
        let path = dir.0.join(FILE_NAME);
        let mut first = Settings::load(&path);
        first.save().unwrap();
        let mut second = Settings::load(&path);
        let mut a = Settings::load(&path);
        a.data["libraries"][0]["paths"] = json!(["X:\\Models"]);
        a.save().unwrap();
        // the second copy still has the old tabs in memory; it changes something else
        std::thread::sleep(std::time::Duration::from_millis(20));
        second.data.insert("thumb_size".into(), 222.into());
        second.save().unwrap();
        let disk = Settings::load(&path);
        assert_eq!(disk.data["libraries"][0]["paths"], json!(["X:\\Models"]));
        assert_eq!(disk.data["thumb_size"], 222);
        assert_eq!(second.data["libraries"][0]["paths"], json!(["X:\\Models"]));
    }

    #[test]
    fn a_broken_file_gives_way_to_its_backup() {
        let dir = Scratch::new("backup");
        let path = dir.0.join(FILE_NAME);
        let mut settings = Settings::load(&path);
        settings.data.insert("thumb_size".into(), 180.into());
        settings.save().unwrap();
        settings.data.insert("thumb_size".into(), 190.into());
        settings.save().unwrap(); // the .bak holds 180
        fs::write(&path, b"{ broken").unwrap();
        assert_eq!(Settings::load(&path).data["thumb_size"], 180);
        fs::remove_file(sibling(&path, ".bak")).unwrap();
        assert_eq!(Settings::load(&path).data["thumb_size"], 160); // neither: the defaults
    }

    #[test]
    fn the_current_tab_and_its_folders() {
        let dir = Scratch::new("tabs");
        let path = dir.0.join(FILE_NAME);
        fs::write(
            &path,
            r#"{"current_library": 9, "libraries": [{"name": "A", "kind": "models", "paths": ["X:\\A"]}],
            "hidden_folders": {"names": ["z-*"], "paths": ["X:\\A\\old"]}}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.current_library(), 0);
        assert_eq!(settings.library_paths(0), [PathBuf::from("X:\\A")]);
        assert!(settings.library_paths(3).is_empty());
        assert_eq!(settings.hidden(), (vec!["z-*".to_string()], vec![PathBuf::from("X:\\A\\old")]));
    }

    #[test]
    fn the_old_settings_are_carried_over_once() {
        let dir = Scratch::new("carry");
        let (old, new) = (dir.0.join("old"), dir.0.join("new"));
        assert_eq!(carry_over_old_settings(&old, &new), Ok(false)); // nothing to carry
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join(FILE_NAME), r#"{"thumb_size": 140}"#).unwrap();
        assert_eq!(carry_over_old_settings(&old, &new), Ok(true));
        assert_eq!(Settings::load(&new.join(FILE_NAME)).data["thumb_size"], 140);
        fs::write(old.join(FILE_NAME), r#"{"thumb_size": 120}"#).unwrap();
        assert_eq!(carry_over_old_settings(&old, &new), Ok(false)); // the new file is never touched
        assert_eq!(Settings::load(&new.join(FILE_NAME)).data["thumb_size"], 140);
        assert!(old.join(FILE_NAME).is_file()); // the old one stays
    }
}
