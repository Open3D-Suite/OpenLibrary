// SPDX-License-Identifier: GPL-3.0-or-later
//! The external files a `.max` does not find, and changing or removing their paths inside it. The `.max` is not
//! opened: a 3ds Max in the background writes again its table of external files (`olib_batchPaths`).

use crate::library::IMAGE_EXTS;
use crate::maxbatch::Job;
use crate::maxfile;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Found,
    Missing,
    /// Not where the file says, but in the model's folder.
    Nearby,
}

/// One external file of a `.max`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub kind: String,
    pub stored: String,
    pub state: State,
}

/// The name at the end of a path as a `.max` stores it.
pub fn base_name(stored: &str) -> &str {
    stored.rsplit(['\\', '/']).next().unwrap_or(stored)
}

/// The folder of a path as a `.max` stores it; empty when it is only a name.
pub fn folder_name(stored: &str) -> &str {
    stored.rfind(['\\', '/']).map_or("", |end| &stored[..end])
}

fn extension(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// The files under a folder by lower-case name: three levels down, and up to 20 000 of them.
fn files_under(folder: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let mut pending = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, level)) = pending.pop() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if level < 3 && !name.starts_with('.') && !name.starts_with("_proxies") {
                        pending.push((entry.path(), level + 1));
                    }
                }
                Ok(_) => {
                    out.entry(name.to_lowercase()).or_insert_with(|| entry.path());
                }
                Err(_) => {}
            }
        }
        if out.len() > 20_000 {
            break;
        }
    }
    out
}

/// The external files of a `.max`, one per stored path.
pub fn externals(max: &Path) -> Vec<External> {
    let base = max.parent().unwrap_or(Path::new(""));
    externals_from(base, maxfile::read_assets(max))
}

fn externals_from(base: &Path, assets: Vec<maxfile::Asset>) -> Vec<External> {
    let mut seen = HashSet::new();
    let mut nearby: Option<HashMap<String, PathBuf>> = None;
    let mut out = Vec::new();
    for asset in assets {
        if Path::new(&asset.stored).extension().is_none() || !seen.insert(asset.stored.to_lowercase()) {
            continue;
        }
        let stored = Path::new(&asset.stored);
        let found = (stored.is_absolute() && stored.is_file())
            || (!stored.is_absolute() && base.join(stored).is_file())
            || (!asset.resolved.is_empty() && Path::new(&asset.resolved).is_file());
        let state = if found {
            State::Found
        } else if nearby.get_or_insert_with(|| files_under(base)).contains_key(&base_name(&asset.stored).to_lowercase()) {
            State::Nearby
        } else {
            State::Missing
        };
        out.push(External { kind: asset.kind, stored: asset.stored, state });
    }
    out
}

/// What is almost never needed and can go at once when it does not turn up: Corona LUTs (.cube), environment
/// HDRs (.hdr) and the "checker" images for unwrapping.
pub fn is_disposable(kind: &str, stored: &str) -> bool {
    let name = base_name(stored).to_lowercase();
    let ext = extension(&name);
    if ext == "cube" || ext == "hdr" {
        return true;
    }
    let image = IMAGE_EXTS.contains(&ext.as_str()) || ["tga", "gif", "exr", "psd"].contains(&ext.as_str());
    name.contains("checker") && (image || kind.trim_start_matches('#').eq_ignore_ascii_case("bitmap"))
}

/// Looks for files by name in a folder and its subfolders: lower-case name, and where the first one is.
pub fn find_by_name(folder: &Path, names: &HashSet<String>, cancelled: &dyn Fn() -> bool) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    let mut seen = 0usize;
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            // a link to a folder is not followed, so that a circle of links ends
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if !name.starts_with('.') && name != "$recycle.bin" {
                    pending.push(entry.path());
                }
                continue;
            }
            seen += 1;
            if names.contains(&name) {
                found.entry(name).or_insert_with(|| entry.path());
            }
        }
        if found.len() == names.len() || seen > 300_000 || cancelled() {
            break;
        }
    }
    found
}

/// What is to be done with a missing file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    To(PathBuf),
    Remove,
}

/// A change of a path: the `.max`, the path it stores and what becomes of it.
pub type Edit = (PathBuf, String, Change);

/// The job that writes the changes, and the `.max` of each of its lines.
pub fn job(edits: &[Edit]) -> (Job, Vec<PathBuf>) {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut lines: Vec<Vec<String>> = Vec::new();
    for (file, stored, change) in edits {
        let at = files.iter().position(|f| f == file).unwrap_or_else(|| {
            files.push(file.clone());
            lines.push(vec![file.display().to_string()]);
            files.len() - 1
        });
        let new = match change {
            Change::To(path) => path.display().to_string(),
            Change::Remove => String::new(),
        };
        if let Some(line) = lines.get_mut(at) {
            line.extend([stored.clone(), new]);
        }
    }
    (Job { function: "olib_batchPaths", arguments: Vec::new(), lines, stall: Duration::from_secs(150) }, files)
}

/// The file of this name in the folder of a `.max` (three levels down), for what is [`State::Nearby`].
pub fn nearby(max: &Path, stored: &str) -> Option<PathBuf> {
    files_under(max.parent()?).remove(&base_name(stored).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openlibrary-missing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn asset(stored: &str) -> maxfile::Asset {
        maxfile::Asset { kind: "Bitmap".into(), stored: stored.into(), resolved: String::new() }
    }

    #[test]
    fn each_file_is_found_nearby_or_missing_and_told_once() {
        let dir = folder("states");
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("here.jpg"), b"x").unwrap();
        std::fs::write(dir.join("maps").join("Oak.JPG"), b"x").unwrap();
        let list = ["here.jpg", r"Q:\old\oak.jpg", r"Q:\old\OAK.jpg", r"Q:\old\gone.png", "noextension"];
        let out = externals_from(&dir, list.into_iter().map(asset).collect());
        let states: Vec<_> = out.iter().map(|e| (base_name(&e.stored), e.state)).collect();
        assert_eq!(states, [("here.jpg", State::Found), ("oak.jpg", State::Nearby), ("gone.png", State::Missing)]);
        assert_eq!(nearby(&dir.join("a.max"), r"Q:\old\oak.jpg"), Some(dir.join("maps").join("Oak.JPG")));
    }

    #[test]
    fn luts_hdrs_and_checkers_are_disposable() {
        assert!(is_disposable("Photometric", r"C:\x\Film.CUBE") && is_disposable("Bitmap", "sky.hdr"));
        assert!(is_disposable("Bitmap", "UV_Checker.png") && is_disposable("#bitmap", "checker.xyz"));
        assert!(!is_disposable("Bitmap", "oak.png") && !is_disposable("XRef", "checker.max"));
    }

    #[test]
    fn files_are_found_by_name_in_subfolders() {
        let dir = folder("find");
        std::fs::create_dir_all(dir.join("a").join("b")).unwrap();
        std::fs::write(dir.join("a").join("b").join("Oak.jpg"), b"x").unwrap();
        let names = HashSet::from(["oak.jpg".to_string(), "pine.jpg".to_string()]);
        let found = find_by_name(&dir, &names, &|| false);
        assert_eq!(found, HashMap::from([("oak.jpg".to_string(), dir.join("a").join("b").join("Oak.jpg"))]));
    }

    #[test]
    fn the_changes_of_a_file_go_in_one_line() {
        let (a, b) = (PathBuf::from(r"S:\a.max"), PathBuf::from(r"S:\b.max"));
        let edits = [
            (a.clone(), "x.jpg".to_string(), Change::To(PathBuf::from(r"S:\x.jpg"))),
            (b.clone(), "y.cube".to_string(), Change::Remove),
            (a.clone(), "z.hdr".to_string(), Change::Remove),
        ];
        let (job, files) = job(&edits);
        assert_eq!(files, [a, b]);
        assert_eq!(job.lines, [vec![r"S:\a.max", "x.jpg", r"S:\x.jpg", "z.hdr", ""], vec![r"S:\b.max", "y.cube", ""]]);
    }

    #[test]
    fn a_stored_path_splits_into_folder_and_name() {
        assert_eq!((base_name(r"C:\a/b.jpg"), folder_name(r"C:\a/b.jpg")), ("b.jpg", r"C:\a"));
        assert_eq!((base_name("b.jpg"), folder_name("b.jpg")), ("b.jpg", ""));
    }
}
