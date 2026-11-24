// SPDX-License-Identifier: GPL-3.0-or-later
//! The index of a library of models. There is no database: the folder is the truth.
//!
//! ```text
//! Chairs/
//!     Eames_corona.max     the Corona version
//!     Eames_vray.max       the V-Ray version
//!     Eames.jpg            its preview (also .png, Eames_preview.jpg, or in a "previews" folder)
//!     maps/ ...            its textures
//!     _proxies/ ...        proxies made by the app: never indexed
//! ```
//!
//! A `.max` without an engine suffix is an asset too; its renderer is read from the file
//! ([`crate::maxfile::detect_engine`]).

use crate::maxfile::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const MAX_EXT: &str = "max";
pub const IMAGE_EXTS: [&str; 7] = ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"];
/// Folders that are never indexed (the app's own, and those NAS boxes and Windows add).
const SKIP_DIRS: [&str; 6] = ["_proxies", ".openlibrary", "$recycle.bin", "@eadir", "#recycle", "autobackup"];
const PREVIEW_DIRS: [&str; 4] = ["previews", "preview", "_previews", "thumbs"];
/// The engine suffixes of a file name, and the engine each one means.
const SUFFIXES: [(&str, Engine); 6] = [
    ("corona", Engine::Corona),
    ("cr", Engine::Corona),
    ("vray", Engine::VRay),
    ("v-ray", Engine::VRay),
    ("vr", Engine::VRay),
    ("fstorm", Engine::FStorm),
];
const CACHE_VERSION: u32 = 1;

/// A path as Windows compares it: backslashes and lower case. Used as a key, never shown.
pub fn norm_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '_' | '-' | '.')
}

/// `"Chair_corona"` is `("Chair", Some(Corona))`; `"Chair"` is `("Chair", None)`.
pub fn split_engine_suffix(stem: &str) -> (&str, Option<Engine>) {
    for (suffix, engine) in SUFFIXES {
        let Some(cut) = stem.len().checked_sub(suffix.len()) else { continue };
        if !stem.is_char_boundary(cut) || !stem[cut..].eq_ignore_ascii_case(suffix) {
            continue;
        }
        let base = stem[..cut].trim_end_matches(is_separator);
        if base.len() < cut && !base.is_empty() {
            return (base, Some(engine));
        }
    }
    (stem, None)
}

/// One model: the `.max` files of one name in one folder, and its preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    #[serde(rename = "n")]
    pub name: String,
    #[serde(skip)]
    pub folder: PathBuf,
    /// The files named after their engine (`Chair_corona.max`).
    #[serde(rename = "v")]
    pub versions: BTreeMap<Engine, PathBuf>,
    /// The files without a suffix: their engine is read from the file.
    #[serde(rename = "a")]
    pub auto_files: Vec<PathBuf>,
    #[serde(rename = "p")]
    pub preview: Option<PathBuf>,
    /// The newest modification time of its files, in seconds.
    #[serde(rename = "m")]
    pub mtime: f64,
}

impl Asset {
    fn new(name: &str, folder: &Path) -> Self {
        Self { name: name.to_string(), folder: folder.to_path_buf(), versions: BTreeMap::new(), auto_files: Vec::new(), preview: None, mtime: 0.0 }
    }

    /// What identifies the asset: its folder and its name, as Windows compares them.
    pub fn key(&self) -> String {
        norm_key(&self.folder.join(&self.name))
    }

    pub fn all_files(&self) -> Vec<&Path> {
        self.versions.values().chain(&self.auto_files).map(PathBuf::as_path).collect()
    }

    pub fn any_file(&self) -> Option<&Path> {
        self.all_files().into_iter().next()
    }

    /// Where its thumbnail comes from: the preview, or else the `.max` (its viewport capture).
    pub fn thumb_source(&self) -> Option<&Path> {
        self.preview.as_deref().or_else(|| self.any_file())
    }

    /// The file for an engine: by its suffix, or else the first unsuffixed file `detect` says is of it.
    pub fn file_for_engine(&self, engine: Engine, detect: impl Fn(&Path) -> Engine) -> Option<&Path> {
        self.versions.get(&engine).map(PathBuf::as_path).or_else(|| self.auto_files.iter().map(PathBuf::as_path).find(|f| detect(f) == engine))
    }

    /// Whether every word is in its name or in the name of its folder (words in lower case).
    pub fn matches(&self, words: &[String]) -> bool {
        let folder = self.folder.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        let hay = format!("{} {}", self.name, folder).to_lowercase();
        words.iter().all(|w| hay.contains(w.as_str()))
    }
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn mtime_of(meta: &fs::Metadata) -> f64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64())
}

fn find_preview(name: &str, stems: &[String], images: &HashMap<String, PathBuf>) -> Option<PathBuf> {
    for candidate in std::iter::once(name).chain(stems.iter().map(String::as_str)) {
        let s = candidate.to_lowercase();
        for key in [s.clone(), format!("{s}_preview"), format!("{s}-preview"), format!("{s} preview"), format!("{s}_thumb")] {
            if let Some(path) = images.get(&key) {
                return Some(path.clone());
            }
        }
    }
    None
}

struct Group {
    name: String,
    versions: BTreeMap<Engine, PathBuf>,
    autos: Vec<PathBuf>,
    stems: Vec<String>,
    mtime: f64,
}

/// The assets of one folder (not its subfolders), from its entries.
pub fn scan_folder(folder: &Path, entries: &[fs::DirEntry]) -> Vec<Asset> {
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut images: HashMap<String, PathBuf> = HashMap::new();
    let mut sub_previews: HashMap<String, PathBuf> = HashMap::new();
    for entry in entries {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_file() {
            let ext = extension(&path);
            let file_stem = stem(&path);
            if ext == MAX_EXT {
                let (base, engine) = split_engine_suffix(&file_stem);
                let at = *index.entry(base.to_lowercase()).or_insert_with(|| {
                    groups.push(Group { name: base.to_string(), versions: BTreeMap::new(), autos: Vec::new(), stems: Vec::new(), mtime: 0.0 });
                    groups.len() - 1
                });
                let group = &mut groups[at];
                match engine {
                    Some(engine) if !group.versions.contains_key(&engine) => {
                        group.versions.insert(engine, path.clone());
                    }
                    _ => group.autos.push(path.clone()),
                }
                group.stems.push(file_stem);
                if let Ok(meta) = entry.metadata() {
                    group.mtime = group.mtime.max(mtime_of(&meta));
                }
            } else if IMAGE_EXTS.contains(&ext.as_str()) {
                images.insert(file_stem.to_lowercase(), path);
            }
        } else if kind.is_dir() && PREVIEW_DIRS.contains(&entry.file_name().to_string_lossy().to_lowercase().as_str()) {
            for sub in fs::read_dir(&path).into_iter().flatten().flatten() {
                let sub_path = sub.path();
                if IMAGE_EXTS.contains(&extension(&sub_path).as_str()) {
                    sub_previews.insert(stem(&sub_path).to_lowercase(), sub_path);
                }
            }
        }
    }
    for (k, v) in sub_previews {
        images.entry(k).or_insert(v);
    }
    let single = groups.len() == 1;
    let folder_name = folder.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut assets: Vec<Asset> = groups
        .into_iter()
        .map(|g| {
            let mut asset = Asset::new(&g.name, folder);
            asset.preview = find_preview(&g.name, &g.stems, &images);
            if asset.preview.is_none() && single {
                // A folder of one asset, with "preview.jpg" or an image named as the folder
                asset.preview = ["preview", "thumb", "thumbnail", folder_name.as_str()].iter().find_map(|k| images.get(*k).cloned());
            }
            asset.versions = g.versions;
            asset.auto_files = g.autos;
            asset.mtime = g.mtime;
            asset
        })
        .collect();
    assets.sort_by_key(|a| a.name.to_lowercase());
    assets
}

/// What the index knows of one folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    path: PathBuf,
    mtime: f64,
    subdirs: Vec<PathBuf>,
    assets: Vec<Asset>,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    roots: Vec<String>,
    records: Vec<Record>,
}

/// How much of the last scan was read from disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub listed: usize,
    pub reused: usize,
}

/// The index of one tab (several root folders), with its cache on disk.
///
/// The first time every folder is listed. Afterwards the cache loads at once and a scan lists again only
/// the folders whose modification time changed (Windows and NAS boxes change it when something inside is
/// created, deleted or renamed); for the rest one `stat` is enough.
#[derive(Clone, Default)]
pub struct LibraryIndex {
    pub roots: Vec<PathBuf>,
    records: HashMap<String, Record>,
    pub folders: Vec<PathBuf>,
    by_folder: HashMap<String, Vec<Asset>>,
    pub stats: ScanStats,
}

fn list_folder(folder: &Path) -> std::io::Result<(Vec<PathBuf>, Vec<Asset>)> {
    let entries: Vec<fs::DirEntry> = fs::read_dir(folder)?.flatten().collect();
    let mut subdirs: Vec<PathBuf> = entries
        .iter()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str())
        })
        .map(|e| e.path())
        .collect();
    subdirs.sort_by_key(|p| norm_key(p));
    Ok((subdirs, scan_folder(folder, &entries)))
}

impl LibraryIndex {
    pub fn new(roots: &[PathBuf]) -> Self {
        Self { roots: roots.iter().filter(|r| !r.as_os_str().is_empty()).cloned().collect(), ..Self::default() }
    }

    fn rebuild(&mut self) {
        let mut folders: Vec<PathBuf> = self.records.values().map(|r| r.path.clone()).collect();
        folders.sort_by_key(|p| norm_key(p));
        self.folders = folders;
        self.by_folder = self.records.iter().filter(|(_, r)| !r.assets.is_empty()).map(|(k, r)| (k.clone(), r.assets.clone())).collect();
    }

    /// Reads the folders again: only those that changed when `incremental`. `cancelled` is asked between
    /// folders; a cancelled scan keeps what the index had.
    pub fn scan(&mut self, incremental: bool, cancelled: &dyn Fn() -> bool) -> &mut Self {
        self.scan_reporting(incremental, cancelled, &mut |_, _| {})
    }

    /// [`Self::scan`], telling `progress` every 25 folders how many folders and models it has seen.
    pub fn scan_reporting(&mut self, incremental: bool, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(usize, usize)) -> &mut Self {
        let mut models = 0;
        let old = if incremental { std::mem::take(&mut self.records) } else { HashMap::new() };
        let mut new: HashMap<String, Record> = HashMap::new();
        self.stats = ScanStats::default();
        let mut was_cancelled = false;
        for root in &self.roots {
            let mut stack = vec![root.clone()];
            while let Some(folder) = stack.pop() {
                if cancelled() {
                    was_cancelled = true;
                    break;
                }
                let key = norm_key(&folder);
                let Ok(meta) = fs::metadata(&folder) else { continue };
                let mtime = mtime_of(&meta);
                let record = match old.get(&key) {
                    Some(r) if r.mtime == mtime => {
                        self.stats.reused += 1;
                        r.clone()
                    }
                    _ => {
                        let Ok((subdirs, assets)) = list_folder(&folder) else { continue };
                        self.stats.listed += 1;
                        Record { path: folder.clone(), mtime, subdirs, assets }
                    }
                };
                stack.extend(record.subdirs.iter().rev().cloned());
                models += record.assets.len();
                new.insert(key, record);
                if new.len().is_multiple_of(25) {
                    progress(new.len(), models);
                }
            }
        }
        if was_cancelled {
            self.records = old;
        } else {
            self.records = new;
            self.rebuild();
        }
        self
    }

    /// Lists one folder again, after the app changed it.
    pub fn rescan_folder(&mut self, folder: &Path) -> Vec<Asset> {
        let (Ok((subdirs, assets)), Ok(meta)) = (list_folder(folder), fs::metadata(folder)) else { return Vec::new() };
        let record = Record { path: folder.to_path_buf(), mtime: mtime_of(&meta), subdirs, assets: assets.clone() };
        self.records.insert(norm_key(folder), record);
        self.rebuild();
        assets
    }

    fn root_keys(&self) -> Vec<String> {
        self.roots.iter().map(|r| norm_key(r)).collect()
    }

    /// Writes the cache: whole into a new file, which then takes the place of the old one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let cache = CacheFile { version: CACHE_VERSION, roots: self.root_keys(), records: self.records.values().cloned().collect() };
        let data = serde_json::to_vec(&cache).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, data)?;
        fs::rename(&tmp, path)
    }

    /// Loads the cache; `false` when there is none, it is of another version or of other roots.
    pub fn load(&mut self, path: &Path) -> bool {
        let Ok(data) = fs::read(path) else { return false };
        let Ok(cache) = serde_json::from_slice::<CacheFile>(&data) else { return false };
        if cache.version != CACHE_VERSION || cache.roots != self.root_keys() {
            return false;
        }
        self.records = cache
            .records
            .into_iter()
            .map(|mut r| {
                for asset in &mut r.assets {
                    asset.folder = r.path.clone();
                }
                (norm_key(&r.path), r)
            })
            .collect();
        self.rebuild();
        true
    }

    /// Every asset, by name.
    pub fn assets(&self) -> Vec<&Asset> {
        let mut out: Vec<&Asset> = self.by_folder.values().flatten().collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    /// The assets in a folder, and in its subfolders when `recursive`; every asset for `None`.
    pub fn assets_in(&self, folder: Option<&Path>, recursive: bool) -> Vec<&Asset> {
        let Some(folder) = folder else { return self.assets() };
        let key = norm_key(folder);
        let prefix = format!("{key}\\");
        let mut out: Vec<&Asset> =
            self.by_folder.iter().filter(|(k, _)| **k == key || (recursive && k.starts_with(&prefix))).flat_map(|(_, list)| list).collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    /// The assets whose name or folder holds every word of `text`.
    pub fn search(&self, text: &str, folder: Option<&Path>, recursive: bool) -> Vec<&Asset> {
        let words: Vec<String> = text.to_lowercase().split_whitespace().map(str::to_string).collect();
        self.assets_in(folder, recursive).into_iter().filter(|a| a.matches(&words)).collect()
    }
}

/// Whether `name` matches a pattern with `*`, `?` and `[...]`, as a whole, case and all.
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    /// How many letters of the pattern match one letter of the name, if they do.
    fn one(p: &[char], c: char) -> Option<usize> {
        match *p.first()? {
            '?' => Some(1),
            '[' => {
                let Some(close) = p.iter().skip(2).position(|c| *c == ']').map(|i| i + 2) else { return (c == '[').then_some(1) };
                let set = &p[1..close];
                let (negate, set) = if set.first() == Some(&'!') { (true, &set[1..]) } else { (false, set) };
                let mut hit = false;
                let mut i = 0;
                while i < set.len() {
                    if i + 2 < set.len() && set[i + 1] == '-' {
                        hit |= set[i] <= c && c <= set[i + 2];
                        i += 3;
                    } else {
                        hit |= set[i] == c;
                        i += 1;
                    }
                }
                (hit != negate).then_some(close + 1)
            }
            letter => (letter == c).then_some(1),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut i, mut j) = (0, 0);
    // only the last star is tried again, so that a pattern of many stars takes no longer
    let mut star: Option<(usize, usize)> = None;
    while j < n.len() {
        if p.get(i) == Some(&'*') {
            star = Some((i + 1, j));
            i += 1;
        } else if let Some(used) = p.get(i..).and_then(|rest| one(rest, n[j])) {
            i += used;
            j += 1;
        } else if let Some((after, from)) = star {
            star = Some((after, from + 1));
            (i, j) = (after, from + 1);
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}

/// The folders (as keys) that are not shown: those named like one of `names` (wildcards allowed, such as
/// `"z-*"`), those in `paths`, and everything under them. A root folder is never hidden.
pub fn hidden_folders(folders: &[PathBuf], roots: &[PathBuf], names: &[String], paths: &[PathBuf]) -> HashSet<String> {
    let names: Vec<String> = names.iter().map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect();
    let paths: HashSet<String> = paths.iter().map(|p| norm_key(p)).collect();
    let keep: HashSet<String> = roots.iter().map(|r| norm_key(r)).collect();
    let mut hidden = HashSet::new();
    if names.is_empty() && paths.is_empty() {
        return hidden;
    }
    let mut keys: Vec<String> = folders.iter().map(|f| norm_key(f)).collect();
    keys.sort(); // parents before their children
    for key in keys {
        if keep.contains(&key) {
            continue;
        }
        let (parent, base) = key.rsplit_once('\\').unwrap_or(("", key.as_str()));
        if hidden.contains(parent) || paths.contains(&key) || names.iter().any(|n| wildcard_match(n, base)) {
            hidden.insert(key);
        }
    }
    hidden
}

/// The name of the cache file of a set of roots: the same roots, in any order, give the same name.
pub fn cache_name(roots: &[PathBuf]) -> String {
    let mut keys: Vec<String> = roots.iter().filter(|r| !r.as_os_str().is_empty()).map(|r| norm_key(r)).collect();
    keys.sort();
    // FNV-1a, so that the name stays the same from one build to the next.
    let hash = keys.join("|").bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_of_many_stars_ends() {
        let pattern = "*a".repeat(40) + "b";
        assert!(!wildcard_match(&pattern, &"a".repeat(200)));
        assert!(wildcard_match("z-*", "z-maps") && wildcard_match("*[0-9]?", "abc1x") && !wildcard_match("[!a]*", "abc"));
        assert!(wildcard_match("a*b*c", "axxbyyc") && !wildcard_match("a*b", "axxbyy") && wildcard_match("**", ""));
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("openlibrary-{name}-{}", std::process::id()));
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

    fn library(name: &str) -> Scratch {
        let s = Scratch::new(name);
        let r = &s.0;
        for f in [
            "Chairs/Eames_corona.max",
            "Chairs/Eames_vray.max",
            "Chairs/Eames.jpg",
            "Chairs/Tulip.max",
            "Chairs/previews/Tulip.png",
            "Chairs/_proxies/x/Eames.cgeo",
            "Chairs/_proxies/x/Fake.max",
            "Buildings/Tower/Tower_vray.max",
            "Buildings/Tower/preview.jpg",
            "Empty/readme.txt",
        ] {
            touch(&r.join(f));
        }
        // Windows may update the date of a folder just made a moment later: fix them all, so that a second
        // scan sees exactly what the first one saw.
        let fixed = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let mut stack = vec![r.clone()];
        while let Some(dir) = stack.pop() {
            stack.extend(fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_type().unwrap().is_dir()).map(|e| e.path()));
            touch_folder(&dir, fixed);
        }
        s
    }

    /// Sets a folder's modification time, as Windows does when something inside changes.
    fn touch_folder(folder: &Path, time: std::time::SystemTime) {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000; // the flag Windows needs to open a folder
        let dir = fs::OpenOptions::new().write(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(folder).unwrap();
        dir.set_modified(time).unwrap();
    }

    fn names(assets: &[&Asset]) -> Vec<String> {
        assets.iter().map(|a| a.name.clone()).collect()
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn engine_suffixes() {
        assert_eq!(split_engine_suffix("Chair_corona"), ("Chair", Some(Engine::Corona)));
        assert_eq!(split_engine_suffix("Chair-VRay"), ("Chair", Some(Engine::VRay)));
        assert_eq!(split_engine_suffix("Chair v-ray"), ("Chair", Some(Engine::VRay)));
        assert_eq!(split_engine_suffix("Table_CR"), ("Table", Some(Engine::Corona)));
        assert_eq!(split_engine_suffix("Table_vr"), ("Table", Some(Engine::VRay)));
        assert_eq!(split_engine_suffix("Chair_fstorm"), ("Chair", Some(Engine::FStorm)));
        assert_eq!(split_engine_suffix("Corona"), ("Corona", None));
        assert_eq!(split_engine_suffix("Tree"), ("Tree", None));
        assert_eq!(split_engine_suffix("Lámpara_corona"), ("Lámpara", Some(Engine::Corona)));
    }

    #[test]
    fn a_scan_finds_the_assets_and_their_previews() {
        let lib = library("scan");
        let mut index = LibraryIndex::new(std::slice::from_ref(&lib.0));
        index.scan(true, &never);
        let assets = index.assets();
        assert_eq!(names(&assets), ["Eames", "Tower", "Tulip"]); // _proxies is left out
        let eames = assets.iter().find(|a| a.name == "Eames").unwrap();
        assert_eq!(eames.versions.keys().copied().collect::<Vec<_>>(), [Engine::Corona, Engine::VRay]);
        assert!(eames.preview.as_ref().unwrap().ends_with("Eames.jpg"));
        let tulip = assets.iter().find(|a| a.name == "Tulip").unwrap();
        assert_eq!(tulip.auto_files.len(), 1);
        assert!(tulip.preview.as_ref().unwrap().ends_with("previews/Tulip.png"));
        let tower = assets.iter().find(|a| a.name == "Tower").unwrap();
        assert!(tower.preview.as_ref().unwrap().ends_with("preview.jpg"));
        assert!(index.folders.contains(&lib.0.join("Empty")));
    }

    #[test]
    fn folders_and_search() {
        let lib = library("search");
        let mut index = LibraryIndex::new(std::slice::from_ref(&lib.0));
        index.scan(true, &never);
        let buildings = lib.0.join("Buildings");
        assert_eq!(names(&index.assets_in(Some(&buildings), true)), ["Tower"]);
        assert!(index.assets_in(Some(&buildings), false).is_empty());
        assert_eq!(names(&index.search("eam", None, true)), ["Eames"]);
        assert_eq!(names(&index.search("chairs tul", None, true)), ["Tulip"]); // the folder's name counts
        assert!(index.search("nothing", None, true).is_empty());
    }

    #[test]
    fn the_file_for_an_engine() {
        let lib = library("engine");
        let mut index = LibraryIndex::new(std::slice::from_ref(&lib.0));
        index.scan(true, &never);
        let assets = index.assets();
        let eames = assets.iter().find(|a| a.name == "Eames").unwrap();
        assert!(eames.file_for_engine(Engine::VRay, |_| Engine::Other).unwrap().ends_with("Eames_vray.max"));
        assert_eq!(eames.file_for_engine(Engine::Other, |_| Engine::Corona), None);
        let tulip = assets.iter().find(|a| a.name == "Tulip").unwrap();
        assert!(tulip.file_for_engine(Engine::Corona, |_| Engine::Corona).is_some());
        assert_eq!(tulip.thumb_source(), tulip.preview.as_deref());
    }

    #[test]
    fn the_cache_and_an_incremental_scan() {
        let lib = library("cache");
        let cache_dir = Scratch::new("cache-dir");
        let cache = cache_dir.0.join("cache.json");
        let mut index = LibraryIndex::new(std::slice::from_ref(&lib.0));
        index.scan(true, &never);
        assert!(index.stats.listed > 0);
        index.save(&cache).unwrap();

        let mut again = LibraryIndex::new(std::slice::from_ref(&lib.0));
        assert!(again.load(&cache));
        assert_eq!(names(&again.assets()), ["Eames", "Tower", "Tulip"]);
        assert_eq!(again.assets()[0].folder, lib.0.join("Chairs"));
        again.scan(true, &never);
        assert_eq!(again.stats.listed, 0); // nothing changed: everything reused

        let chairs = lib.0.join("Chairs");
        touch(&chairs.join("New_corona.max"));
        let later = fs::metadata(&chairs).unwrap().modified().unwrap() + std::time::Duration::from_secs(5);
        touch_folder(&chairs, later);
        again.scan(true, &never);
        assert_eq!(again.stats.listed, 1); // only the folder that changed
        assert!(names(&again.assets()).contains(&"New".to_string()));

        assert!(!LibraryIndex::new(&[chairs]).load(&cache)); // other roots: another cache
    }

    #[test]
    fn a_scan_tells_its_progress() {
        let lib = library("progress");
        for i in 0..30 {
            touch(&lib.0.join(format!("Many/F{i}/M{i}.max")));
        }
        let mut seen = Vec::new();
        LibraryIndex::new(std::slice::from_ref(&lib.0)).scan_reporting(true, &never, &mut |folders, models| seen.push((folders, models)));
        assert_eq!(seen.len(), 1); // 40 folders: told once, at 25
        assert_eq!(seen[0].0, 25);
    }

    #[test]
    fn a_cancelled_scan_keeps_the_index() {
        let lib = library("cancel");
        let mut index = LibraryIndex::new(std::slice::from_ref(&lib.0));
        index.scan(true, &never);
        index.scan(false, &|| true);
        assert_eq!(names(&index.assets()), ["Eames", "Tower", "Tulip"]);
    }

    #[test]
    fn several_roots() {
        let lib = library("roots");
        let mut index = LibraryIndex::new(&[lib.0.join("Chairs"), lib.0.join("Buildings")]);
        index.scan(true, &never);
        assert_eq!(names(&index.assets_in(None, true)), ["Eames", "Tower", "Tulip"]);
    }

    #[test]
    fn hidden_folders_and_their_children() {
        let root = PathBuf::from(r"X:\Lib");
        let folders: Vec<PathBuf> =
            [r"X:\Lib", r"X:\Lib\Chairs", r"X:\Lib\Chairs\z-maps", r"X:\Lib\Chairs\z-maps\Eames", r"X:\Lib\Old", r"X:\Lib\Tables"]
                .iter()
                .map(PathBuf::from)
                .collect();
        let hidden = hidden_folders(&folders, std::slice::from_ref(&root), &["Z-*".into()], &[PathBuf::from(r"x:/lib/old")]);
        let mut got: Vec<&str> = hidden.iter().map(String::as_str).collect();
        got.sort();
        assert_eq!(got, [r"x:\lib\chairs\z-maps", r"x:\lib\chairs\z-maps\eames", r"x:\lib\old"]);
        assert!(hidden_folders(&folders, std::slice::from_ref(&root), &["*".into()], &[]).iter().all(|k| k != r"x:\lib"));
        assert!(hidden_folders(&folders, &[root], &[], &[]).is_empty());
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("z-*", "z-maps"));
        assert!(wildcard_match("ma?s", "maps"));
        assert!(wildcard_match("[mt]aps", "taps"));
        assert!(wildcard_match("[!a-c]x", "dx"));
        assert!(!wildcard_match("[!a-c]x", "bx"));
        assert!(!wildcard_match("z-*", "maps"));
        assert!(wildcard_match("*", ""));
    }

    #[test]
    fn the_cache_name_ignores_order_and_case() {
        let a = cache_name(&[PathBuf::from(r"X:\Lib"), PathBuf::from(r"Y:\Other")]);
        let b = cache_name(&[PathBuf::from(r"y:/other"), PathBuf::from(r"x:\lib")]);
        assert_eq!(a, b);
        assert_ne!(a, cache_name(&[PathBuf::from(r"X:\Lib")]));
    }
}
