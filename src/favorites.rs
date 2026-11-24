// SPDX-License-Identifier: GPL-3.0-or-later
//! The favourites of each person, and what a tab shows of them. Their keys are relative to the root folder, so
//! that they are the same with the library open as a drive or as a server; the Python version's are the same.

use crate::library::norm_key;
use crate::settings::Settings;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const FAVORITES: &str = "favorites";

/// "root/sub/name" and the suffix; the whole path when it is under no root.
pub fn rel_key(roots: &[PathBuf], full: &str, suffix: &str) -> String {
    for root in roots {
        let root = norm_key(root).trim_end_matches('\\').to_string();
        let base = root.rsplit('\\').next().unwrap_or_default();
        if full == root {
            return format!("{base}/.{suffix}");
        }
        if let Some(rel) = full.strip_prefix(&format!("{root}\\")) {
            return format!("{base}/{}{suffix}", rel.replace('\\', "/"));
        }
    }
    format!("{full}{suffix}")
}

/// The key of a model.
pub fn model_key(roots: &[PathBuf], asset_key: &str) -> String {
    rel_key(roots, asset_key, "|model")
}

/// The key of a surface, from its own key and its folder and name.
pub fn surface_key(roots: &[PathBuf], key: &str, folder: &Path, name: &str) -> String {
    let full = norm_key(&folder.join(name));
    let suffix = key.strip_prefix(&full).unwrap_or_default();
    rel_key(roots, &full, suffix)
}

pub fn is_favorite(settings: &Settings, key: &str) -> bool {
    settings.data.get(FAVORITES).and_then(|f| f.get(key)).is_some_and(|v| v.as_i64() != Some(0) && !v.is_null() && v != &json!(false))
}

pub fn any(settings: &Settings) -> bool {
    settings.data.get(FAVORITES).and_then(Value::as_object).is_some_and(|f| !f.is_empty())
}

/// Marks these as favourites, or takes the mark off when all of them have it.
pub fn toggle(settings: &mut Settings, keys: &[String]) {
    let on = !keys.iter().all(|k| is_favorite(settings, k));
    let list = settings.data.entry(FAVORITES).or_insert_with(|| json!({}));
    if !list.is_object() {
        *list = json!({});
    }
    if let Some(list) = list.as_object_mut() {
        for key in keys {
            if on {
                list.insert(key.clone(), json!(1));
            } else {
                list.remove(key);
            }
        }
    }
}

/// What a tab shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabView {
    /// The rows above the tree.
    pub favorites: bool,
    pub all: bool,
    /// It opens on the favourites.
    pub opens_favorites: bool,
    /// With everything, the favourites go first.
    pub favorites_first: bool,
}

pub fn tab_view(library: Option<&Value>) -> TabView {
    let flag = |key: &str| library.and_then(|l| l.get(key)).and_then(Value::as_bool).unwrap_or(true);
    let show = library.and_then(|l| l.get("show")).and_then(Value::as_str).unwrap_or("both");
    let (favorites, all) = (flag("tree_fav"), flag("tree_all"));
    TabView { favorites, all, opens_favorites: favorites && (show == "favorites" || !all), favorites_first: show == "both" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_those_of_the_python_version() {
        let roots = [PathBuf::from(r"B:\01_3D MODELS")];
        assert_eq!(model_key(&roots, r"b:\01_3d models\chairs\oak"), "01_3d models/chairs/oak|model");
        let folder = Path::new(r"B:\01_3D MODELS\Tex");
        assert_eq!(surface_key(&roots, r"b:\01_3d models\tex\oak.jpg", folder, "Oak"), "01_3d models/tex/oak.jpg");
        assert_eq!(surface_key(&roots, r"b:\01_3d models\tex|room 1", folder, "Room 1"), "01_3d models/tex/room 1");
    }

    #[test]
    fn a_favourite_is_marked_and_unmarked() {
        let mut settings = Settings::load(Path::new("no-such-folder/settings.json"));
        let keys = ["a".to_string(), "b".to_string()];
        toggle(&mut settings, &keys[..1]);
        toggle(&mut settings, &keys); // one of them lacked it: both have it now
        assert!(is_favorite(&settings, "a") && is_favorite(&settings, "b"));
        toggle(&mut settings, &keys);
        assert!(!any(&settings));
    }

    #[test]
    fn a_tab_opens_as_its_settings_say() {
        let view = tab_view(None);
        assert!(view.favorites && view.all && !view.opens_favorites && view.favorites_first);
        assert!(tab_view(Some(&json!({"show": "favorites"}))).opens_favorites);
        assert!(tab_view(Some(&json!({"tree_all": false}))).opens_favorites);
        assert!(!tab_view(Some(&json!({"show": "favorites", "tree_fav": false}))).opens_favorites);
    }
}
