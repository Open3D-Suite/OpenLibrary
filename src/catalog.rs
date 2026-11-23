// SPDX-License-Identifier: GPL-3.0-or-later
//! Cataloguing a download: sharing out its files and leaving them in the library under its name.
//!
//! ```text
//! Furniture\Sofa Bolzano.max          the model (with _corona / _vray when there are several versions)
//! Furniture\Sofa Bolzano.jpg          the preview, with the same name
//! Furniture\z-maps\Sofa Bolzano\...   textures and all the rest, with their own names
//! ```

use crate::library::{IMAGE_EXTS, MAX_EXT};
use crate::maxbatch::Job;
use crate::maxfile::Engine;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAPS_FOLDER: &str = "z-maps";

/// Where a file of the download goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bucket {
    Skip,
    Max,
    Preview,
    Maps,
}

/// A file of the download; `rel` is the folder it came in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub rel: String,
    pub ext: String,
    pub size: u64,
}

impl Item {
    pub fn new(path: &Path, rel: &str) -> Self {
        Self {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            rel: rel.to_string(),
            ext: extension(path),
            size: std::fs::metadata(path).map_or(0, |m| m.len()),
            path: path.to_path_buf(),
        }
    }

    pub fn is_image(&self) -> bool {
        IMAGE_EXTS.contains(&self.ext.as_str())
    }

    pub fn is_max(&self) -> bool {
        self.ext == MAX_EXT
    }
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// Every file under a folder, up to `limit`: the `.max` first, then by name. Links are left out: a download
/// has no business pointing at other folders of the computer.
pub fn list_files(folder: &Path, limit: usize) -> Vec<Item> {
    let mut out = Vec::new();
    let mut pending = vec![(folder.to_path_buf(), String::new())];
    while let Some((dir, rel)) = pending.pop() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                if !name.eq_ignore_ascii_case("__macosx") {
                    pending.push((entry.path(), if rel.is_empty() { name } else { format!("{rel}\\{name}") }));
                }
            } else if kind.is_file() {
                out.push(Item::new(&entry.path(), &rel));
            }
        }
        if out.len() > limit {
            break;
        }
    }
    out.sort_by_key(|i| (!i.is_max(), i.name.to_lowercase()));
    out
}

/// Whether a folder's name says it holds maps.
fn is_maps_folder(rel: &str) -> bool {
    const WORDS: [&str; 13] =
        ["map", "maps", "texture", "textures", "textura", "texturas", "tex", "bitmap", "bitmaps", "image", "images", "material", "materials"];
    rel.to_lowercase().split(['\\', '/', ' ', '_', '.', '-']).any(|w| WORDS.contains(&w))
}

/// The proposed share: the `.max` to their place, the rest to the maps, and as preview one image, only if it is
/// clear: it is called like the `.max` or the download, or says so, or is the only one outside a maps folder.
pub fn guess(items: &[Item], source: &str) -> Vec<Bucket> {
    let mut out: Vec<Bucket> = items
        .iter()
        .map(|item| {
            let low = item.name.to_lowercase();
            let junk = ["thumbs.db", "desktop.ini", ".ds_store"].contains(&low.as_str()) || ["url", "lnk", "webloc"].contains(&item.ext.as_str());
            if item.is_max() {
                Bucket::Max
            } else if junk || low.starts_with("._") {
                Bucket::Skip
            } else {
                Bucket::Maps
            }
        })
        .collect();
    let mut stems: HashSet<String> = items.iter().filter(|i| i.is_max()).map(|i| stem(&i.name).to_lowercase()).collect();
    if !source.is_empty() {
        stems.insert(source.to_lowercase());
    }
    let images: Vec<usize> = (0..items.len()).filter(|&i| items[i].is_image() && out[i] == Bucket::Maps).collect();
    let loose: Vec<usize> = images.iter().copied().filter(|&i| !is_maps_folder(&items[i].rel)).collect();
    let mut best = (0, None);
    for &i in &images {
        let name = stem(&items[i].name).to_lowercase();
        let outside = loose.contains(&i);
        let score = if ["preview", "prev", "render", "thumb", "cover", "beauty"].iter().any(|w| name.contains(w)) {
            5
        } else if stems.contains(&name) && outside {
            4
        } else if outside && loose.len() == 1 && images.len() > 1 {
            3
        } else if outside && images.len() == 1 {
            2
        } else {
            0
        };
        if score > best.0 {
            best = (score, Some(i));
        }
    }
    if let Some(bucket) = best.1.and_then(|i| out.get_mut(i)) {
        *bucket = Bucket::Preview;
    }
    out
}

/// The engine of a `.max`: the one read from it, or else the one its name tells.
pub fn engine_of(name: &str, detected: Engine) -> Engine {
    if detected != Engine::Other {
        return detected;
    }
    let low = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| low.contains(w));
    if has(&["corona", "_cr.", "_cr_"]) {
        Engine::Corona
    } else if has(&["vray", "v-ray", "v_ray", "_vr.", "_vr_"]) {
        Engine::VRay
    } else if has(&["fstorm"]) {
        Engine::FStorm
    } else {
        Engine::Other
    }
}

/// A name Windows takes for a file and for a folder.
pub fn clean_name(text: &str) -> String {
    let spaced: String = text.chars().map(|c| if c.is_control() || r#"\/:*?"<>|"#.contains(c) { ' ' } else { c }).collect();
    let name = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = name.trim_matches([' ', '.']);
    let device = name.split('.').next().unwrap_or("").trim().to_uppercase();
    let numbered = (device.starts_with("COM") || device.starts_with("LPT")) && device.len() == 4 && device.ends_with(|c: char| c.is_ascii_digit());
    if numbered || ["CON", "PRN", "AUX", "NUL"].contains(&device.as_str()) { format!("_{name}") } else { name.to_string() }
}

/// A year from 2000 to 2039 in a name, apart from other digits.
fn year_in(name: &str) -> Option<&str> {
    let bytes = name.as_bytes();
    (0..bytes.len().saturating_sub(3)).find_map(|i| {
        let digits = name.get(i..i + 4)?;
        let alone = !(i > 0 && bytes[i - 1].is_ascii_digit()) && !bytes.get(i + 4).is_some_and(u8::is_ascii_digit);
        let year: u32 = digits.parse().ok()?;
        (alone && (2000..2040).contains(&year)).then_some(digits)
    })
}

/// The final name of each `.max`, in order. One alone: `Name.max`. Several: `Name_corona.max`...; when an
/// engine repeats, with the year of the original's name, or a number.
pub fn max_names(name: &str, maxes: &[(PathBuf, Engine)]) -> Vec<String> {
    if maxes.len() == 1 {
        return vec![format!("{name}.{MAX_EXT}")];
    }
    let mut used = HashSet::new();
    maxes
        .iter()
        .map(|(path, engine)| {
            let suffix = if *engine == Engine::Other { String::new() } else { format!("_{}", engine.key()) };
            let mut candidate = format!("{name}{suffix}");
            if used.contains(&candidate.to_lowercase()) {
                let original = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(year) = year_in(&original) {
                    candidate = format!("{name} {year}{suffix}");
                }
                let mut n = 2;
                while used.contains(&candidate.to_lowercase()) {
                    candidate = format!("{name} {n}{suffix}");
                    n += 1;
                }
            }
            used.insert(candidate.to_lowercase());
            format!("{candidate}.{MAX_EXT}")
        })
        .collect()
}

/// Which file goes where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// (from, to)
    pub ops: Vec<(PathBuf, PathBuf)>,
    /// The folder of the maps, when something goes there.
    pub maps_dir: Option<PathBuf>,
    /// Maps that came twice (same name and size).
    pub skipped: usize,
    /// (the V-Ray `.max` already copied, the Corona one made from it)
    pub corona: Vec<(PathBuf, PathBuf)>,
    pub clean_layers: bool,
    /// The `.max` copied.
    pub max_dsts: Vec<PathBuf>,
}

/// What is to be catalogued.
pub struct Request<'a> {
    pub dest: &'a Path,
    pub name: &'a str,
    pub maxes: &'a [(PathBuf, Engine)],
    pub previews: &'a [PathBuf],
    pub others: &'a [PathBuf],
    pub maps_folder: &'a str,
    /// From each V-Ray `.max`, its Corona version, if none came.
    pub to_corona: bool,
    /// Everything to layer 0.
    pub clean_layers: bool,
}

/// Whether making the Corona version makes sense: there is a V-Ray `.max` and no Corona one.
pub fn can_make_corona(maxes: &[(PathBuf, Engine)]) -> bool {
    let has = |engine| maxes.iter().any(|(_, e)| *e == engine);
    has(Engine::VRay) && !has(Engine::Corona)
}

impl Plan {
    pub fn new(request: &Request<'_>) -> Self {
        let Request { dest, name, maxes, .. } = *request;
        let mut plan = Self { clean_layers: request.clean_layers && !maxes.is_empty(), ..Self::default() };
        let mut all = maxes.to_vec();
        if request.to_corona && can_make_corona(maxes) {
            all.extend(maxes.iter().filter(|(_, e)| *e == Engine::VRay).map(|(p, _)| (p.clone(), Engine::Corona)));
        }
        let names = max_names(name, &all);
        for (i, ((src, _), file)) in all.iter().zip(&names).enumerate() {
            let dst = dest.join(file);
            if i < maxes.len() {
                plan.ops.push((src.clone(), dst.clone()));
                plan.max_dsts.push(dst);
            } else if let Some((_, from)) = plan.ops.iter().find(|(s, _)| s == src) {
                plan.corona.push((from.clone(), dst));
            }
        }
        for (i, src) in request.previews.iter().enumerate() {
            let number = if i == 0 { String::new() } else { format!("_{}", i + 1) };
            plan.ops.push((src.clone(), dest.join(format!("{name}{number}.{}", extension(src)))));
        }
        plan.add_maps(request);
        plan
    }

    fn add_maps(&mut self, request: &Request<'_>) {
        if request.others.is_empty() {
            return;
        }
        let folder = match clean_name(request.maps_folder) {
            folder if folder.is_empty() => MAPS_FOLDER.to_string(),
            folder => folder,
        };
        let dir = request.dest.join(folder).join(request.name);
        let mut seen: HashMap<String, HashSet<u64>> = HashMap::new();
        for src in request.others {
            let mut name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let size = std::fs::metadata(src).map_or(u64::MAX, |m| m.len());
            if let Some(sizes) = seen.get(&name.to_lowercase()) {
                if sizes.contains(&size) {
                    self.skipped += 1;
                    continue;
                }
                let original = name.clone();
                let (base, ext) = original.rsplit_once('.').map_or((original.as_str(), String::new()), |(b, e)| (b, format!(".{e}")));
                let mut n = 2;
                while seen.contains_key(&name.to_lowercase()) {
                    name = format!("{base} ({n}){ext}");
                    n += 1;
                }
                seen.entry(original.to_lowercase()).or_default().insert(size);
            }
            seen.entry(name.to_lowercase()).or_default().insert(size);
            self.ops.push((src.clone(), dir.join(name)));
        }
        self.maps_dir = Some(dir);
    }

    /// The destinations that exist already.
    pub fn conflicts(&self) -> Vec<PathBuf> {
        self.ops.iter().chain(&self.corona).map(|(_, dst)| dst).filter(|d| d.exists()).cloned().collect()
    }

    /// What a 3ds Max in the background does once the files are copied, if anything.
    pub fn job(&self) -> Option<Job> {
        let lines: Vec<Vec<String>> = self
            .max_dsts
            .iter()
            .filter_map(|max| {
                let corona = self.corona.iter().find(|(from, _)| from == max).map(|(_, to)| to.display().to_string());
                (self.clean_layers || corona.is_some())
                    .then(|| vec![max.display().to_string(), u8::from(self.clean_layers).to_string(), corona.unwrap_or_default()])
            })
            .collect();
        (!lines.is_empty()).then(|| Job { function: "olib_batchPrepare", arguments: Vec::new(), lines, stall: Duration::from_secs(1800) })
    }

    /// Copies (or moves) the files; how many it left. A file is written whole beside its place and then takes
    /// it, so that what was there is lost only once the new one is safe.
    pub fn apply(&self, moving: bool, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(usize)) -> Result<usize, (PathBuf, std::io::Error)> {
        for (done, (src, dst)) in self.ops.iter().enumerate() {
            if cancelled() {
                return Ok(done);
            }
            place(src, dst, moving).map_err(|e| (dst.clone(), e))?;
            progress(done + 1);
        }
        Ok(self.ops.len())
    }
}

fn place(src: &Path, dst: &Path, moving: bool) -> std::io::Result<()> {
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if moving && std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let mut tmp = dst.as_os_str().to_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let result = std::fs::copy(src, &tmp).and_then(|_| std::fs::rename(&tmp, dst));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    } else if moving {
        let _ = std::fs::remove_file(src); // it is in the app's own folder, which goes when the window closes
    }
    result
}

/// 1.4 MB, 820 KB...
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut n = bytes as f64;
    let mut unit = 0;
    while n >= 1024.0 && unit < 3 {
        n /= 1024.0;
        unit += 1;
    }
    let label = UNITS[unit];
    if unit == 0 { format!("{bytes} {label}") } else { format!("{n:.1} {label}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, rel: &str) -> Item {
        Item { path: PathBuf::from(rel).join(name), name: name.into(), rel: rel.into(), ext: extension(Path::new(name)), size: 1 }
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openlibrary-catalog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn max(name: &str, engine: Engine) -> (PathBuf, Engine) {
        (PathBuf::from(name), engine)
    }

    fn request<'a>(dest: &'a Path, maxes: &'a [(PathBuf, Engine)], others: &'a [PathBuf]) -> Request<'a> {
        Request { dest, name: "Sofa", maxes, previews: &[], others, maps_folder: "z-maps", to_corona: true, clean_layers: true }
    }

    #[test]
    fn the_files_are_shared_out() {
        let items = [item("sofa.max", ""), item("sofa.jpg", ""), item("oak.jpg", "maps"), item("site.url", ""), item("._x.jpg", "")];
        assert_eq!(guess(&items, "Sofa"), [Bucket::Max, Bucket::Preview, Bucket::Maps, Bucket::Skip, Bucket::Skip]);
    }

    #[test]
    fn a_preview_is_proposed_only_when_it_is_clear() {
        let two = [item("a.max", ""), item("oak.jpg", ""), item("pine.jpg", "")];
        assert!(!guess(&two, "x").contains(&Bucket::Preview));
        let one_loose = [item("a.max", ""), item("shot.jpg", ""), item("oak.jpg", "My Textures")];
        assert_eq!(guess(&one_loose, "x")[1], Bucket::Preview);
        let in_maps = [item("oak.max", ""), item("oak.jpg", "textures")];
        assert_eq!(guess(&in_maps, "x")[1], Bucket::Maps);
        let says_so = [item("a.max", ""), item("oak.jpg", ""), item("final_render.png", "maps")];
        assert_eq!(guess(&says_so, "x")[2], Bucket::Preview);
    }

    #[test]
    fn names_windows_refuses_are_cleaned() {
        for (text, name) in
            [("a/b:  c?", "a b c"), (" x. ", "x"), ("CON", "_CON"), ("com1", "_com1"), ("nul.x", "_nul.x"), ("..\\..", ""), ("Cono", "Cono")]
        {
            assert_eq!(clean_name(text), name);
        }
    }

    #[test]
    fn several_max_are_told_apart() {
        assert_eq!(max_names("Sofa", &[max("x.max", Engine::VRay)]), ["Sofa.max"]);
        let list = [max("a 2016.max", Engine::VRay), max("b.max", Engine::Corona), max("c_2019.max", Engine::VRay), max("d.max", Engine::VRay)];
        assert_eq!(max_names("Sofa", &list), ["Sofa_vray.max", "Sofa_corona.max", "Sofa 2019_vray.max", "Sofa 2_vray.max"]);
    }

    #[test]
    fn the_corona_version_is_made_only_when_none_came() {
        let dest = PathBuf::from(r"S:\Lib");
        let vray = [max("x.max", Engine::VRay)];
        let plan = Plan::new(&request(&dest, &vray, &[]));
        assert_eq!(plan.ops, [(PathBuf::from("x.max"), dest.join("Sofa_vray.max"))]);
        assert_eq!(plan.corona, [(dest.join("Sofa_vray.max"), dest.join("Sofa_corona.max"))]);
        let job = plan.job().unwrap();
        assert_eq!(job.lines, [vec![r"S:\Lib\Sofa_vray.max", "1", r"S:\Lib\Sofa_corona.max"]]);
        let both = [max("x.max", Engine::VRay), max("y.max", Engine::Corona)];
        let plan = Plan::new(&request(&dest, &both, &[]));
        assert!(plan.corona.is_empty());
        assert_eq!(plan.job().unwrap().lines.len(), 2);
        let plan = Plan::new(&Request { clean_layers: false, ..request(&dest, &both, &[]) });
        assert_eq!(plan.job(), None);
    }

    #[test]
    fn maps_of_one_name_are_kept_unless_they_are_the_same() {
        let dir = folder("maps");
        for (sub, data) in [("a", "one"), ("b", "one"), ("c", "different")] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
            std::fs::write(dir.join(sub).join("oak.jpg"), data).unwrap();
        }
        let others: Vec<PathBuf> = ["a", "b", "c"].iter().map(|s| dir.join(s).join("oak.jpg")).collect();
        let plan = Plan::new(&request(&dir, &[], &others));
        let maps = dir.join("z-maps").join("Sofa");
        assert_eq!(plan.ops, [(others[0].clone(), maps.join("oak.jpg")), (others[2].clone(), maps.join("oak (2).jpg"))]);
        assert_eq!((plan.skipped, plan.maps_dir), (1, Some(maps)));
    }

    #[test]
    fn copying_leaves_the_download_and_replaces_whole_files() {
        let dir = folder("apply");
        let (src, dst) = (dir.join("in.max"), dir.join("lib").join("Sofa.max"));
        std::fs::write(&src, "new").unwrap();
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(&dst, "old").unwrap();
        let plan = Plan { ops: vec![(src.clone(), dst.clone())], ..Plan::default() };
        assert_eq!(plan.conflicts(), std::slice::from_ref(&dst));
        assert_eq!(plan.apply(false, &|| false, &mut |_| {}).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new");
        assert!(src.exists());
        assert_eq!(plan.apply(true, &|| false, &mut |_| {}).unwrap(), 1);
        assert!(!src.exists() && !dir.join("lib").join("Sofa.max.part").exists());
        assert!(plan.apply(false, &|| false, &mut |_| {}).is_err());
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new");
        assert_eq!(plan.apply(false, &|| true, &mut |_| {}).unwrap(), 0);
    }

    #[test]
    fn the_engine_is_read_or_told_by_the_name() {
        assert_eq!(engine_of("sofa_vray.max", Engine::Corona), Engine::Corona);
        assert_eq!(engine_of("Sofa V-Ray 2016.max", Engine::Other), Engine::VRay);
        assert_eq!(engine_of("sofa_cr.max", Engine::Other), Engine::Corona);
        assert_eq!(engine_of("sofa.max", Engine::Other), Engine::Other);
    }

    #[test]
    fn sizes_are_easy_to_read() {
        assert_eq!((human_size(12), human_size(1536), human_size(5 << 30)), ("12 B".into(), "1.5 KB".into(), "5.0 GB".into()));
    }
}
