// SPDX-License-Identifier: GPL-3.0-or-later
//! Fake interiors for windows (wParallax, ShadersBOX): which rooms the folders hold.
//!
//! A room is an EXR with the atlas of its walls and furniture, which an OSL map (`wParallax_SMP.osl`,
//! `jiWindowBox`) reads in 3ds Max on a plane. Here its EXR files are put together: day and night, resolutions,
//! the size of the window (S, M, L) and its preview.

use crate::library::norm_key;
use crate::surfaces::{Listing, Res};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// The height of the windows of the packs, in cm.
pub const HEIGHT: f64 = 350.0;
/// The shaders, the preferred one first.
const SHADERS: [&str; 3] = ["wparallax_smp.osl", "wparallax_adv.osl", "jiwindowbox_all_v1_10.osl"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Light {
    Day,
    Night,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSize {
    S,
    M,
    L,
}

impl WindowSize {
    pub fn code(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::M => "M",
            Self::L => "L",
        }
    }

    /// In cm.
    pub fn width(self) -> f64 {
        match self {
            Self::S => 350.0,
            Self::M => 700.0,
            Self::L => 1050.0,
        }
    }
}

struct Parsed {
    key: String,
    name: String,
    light: Option<Light>,
    res: u32,
    size: Option<WindowSize>,
}

/// The name of an EXR, or of a preview.
fn parse_room(stem: &str, folder: &Path) -> Parsed {
    let mut out = Parsed { key: String::new(), name: String::new(), light: None, res: 0, size: None };
    let mut words: Vec<&str> = Vec::new();
    for word in stem.split(|c: char| c == '_' || c == '-' || c.is_whitespace()).filter(|w| !w.is_empty()) {
        let low = word.to_lowercase();
        let res = low.strip_suffix('k').filter(|d| (1..=2).contains(&d.len())).and_then(|d| d.parse().ok());
        match (low.as_str(), word) {
            ("day", _) => out.light = Some(Light::Day),
            ("night", _) => out.light = Some(Light::Night),
            (_, "S") | ("single", _) => out.size = Some(WindowSize::S),
            (_, "M") | ("double", _) => out.size = Some(WindowSize::M),
            (_, "L") | ("triple", _) => out.size = Some(WindowSize::L),
            ("wp" | "mp", _) if words.is_empty() => {} // the maker's prefix
            _ => match res {
                Some(res) => out.res = res,
                None => words.push(word),
            },
        }
    }
    let folder = folder.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if out.res == 0 {
        out.res = if folder.contains("low_res") {
            1
        } else if folder.contains("high_res") {
            4
        } else {
            0
        };
    }
    out.name = words.join(" ");
    out.key = out.name.to_lowercase();
    out
}

/// A room: its EXR by light and resolution, and its previews.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub name: String,
    /// The folder of its pack.
    pub folder: PathBuf,
    pub key: String,
    pub size: WindowSize,
    pub exrs: BTreeMap<Light, BTreeMap<u32, PathBuf>>,
    pub previews: BTreeMap<Light, PathBuf>,
    /// The shader that reads it, if the folders have one.
    pub osl: Option<PathBuf>,
    pub mtime: u64,
}

impl Room {
    pub fn top_res(&self) -> u32 {
        self.exrs.values().flat_map(|by| by.keys()).max().copied().unwrap_or(0)
    }

    /// The EXR of a light (the other one when it lacks it) and a resolution (or the nearest).
    pub fn pick(&self, light: Light, res: Res) -> Option<&PathBuf> {
        let by = self.exrs.get(&light).or_else(|| self.exrs.values().next())?;
        match res {
            Res::Highest => by.values().next_back(),
            Res::Lowest => by.values().next(),
            Res::K(res) => by.iter().min_by_key(|(r, _)| (r.abs_diff(res), std::cmp::Reverse(**r))).map(|(_, p)| p),
        }
    }

    pub fn thumb_source(&self, light: Light) -> Option<&PathBuf> {
        self.previews.get(&light).or_else(|| self.previews.values().next())
    }
}

/// The first folder under the root: the pack.
fn pack_of(folder: &Path, roots: &[PathBuf]) -> PathBuf {
    for root in roots {
        let (key, root_key) = (norm_key(folder), norm_key(root));
        if key == root_key {
            return folder.to_path_buf();
        }
        if key.starts_with(&format!("{}\\", root_key.trim_end_matches('\\'))) {
            let depth = root.components().count() + 1;
            return folder.components().take(depth).collect();
        }
    }
    folder.to_path_buf()
}

/// A name whose numbers sort as numbers.
fn natural(text: &str) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = vec![(0, String::new())];
    let mut in_number = false;
    for c in text.to_lowercase().chars() {
        let digit = c.to_digit(10).map(u64::from);
        match (digit, in_number) {
            (Some(d), true) => {
                if let Some(last) = out.last_mut() {
                    last.0 = last.0.saturating_mul(10).saturating_add(d);
                }
            }
            (Some(d), false) => out.push((d, String::new())),
            (None, _) => {
                if let Some(last) = out.last_mut() {
                    last.1.push(c);
                }
            }
        }
        in_number = digit.is_some();
    }
    out
}

/// Every room of a listing, by pack and name.
pub fn build_interiors(listing: &Listing, roots: &[PathBuf]) -> Vec<Room> {
    let mut rooms: BTreeMap<(String, String), Room> = BTreeMap::new();
    let mut previews: HashMap<(String, String), BTreeMap<Light, PathBuf>> = HashMap::new();
    let mut shaders: HashMap<String, PathBuf> = HashMap::new();
    for (folder, listed) in listing {
        let pack = pack_of(folder, roots);
        for (file, mtime) in listed.files.iter().filter(|(f, _)| !f.starts_with("._")) {
            let path = folder.join(file);
            let low = file.to_lowercase();
            let Some((stem, ext)) = file.rsplit_once('.') else { continue };
            match ext.to_lowercase().as_str() {
                "osl" => {
                    shaders.entry(low).or_insert(path);
                }
                "jpg" | "jpeg" | "png" => {
                    let p = parse_room(stem, folder);
                    let by = previews.entry((norm_key(&pack), p.key)).or_default();
                    by.entry(p.light.unwrap_or(Light::Day)).or_insert(path);
                }
                "exr" => {
                    let p = parse_room(stem, folder);
                    if p.key.is_empty() {
                        continue;
                    }
                    let key = (norm_key(&pack), p.key.clone());
                    let room = rooms.entry(key).or_insert_with(|| Room {
                        name: p.name.clone(),
                        folder: pack.clone(),
                        key: format!("{}|{}", norm_key(&pack), p.key),
                        size: WindowSize::S,
                        exrs: BTreeMap::new(),
                        previews: BTreeMap::new(),
                        osl: None,
                        mtime: 0,
                    });
                    room.size = p.size.unwrap_or(room.size);
                    // ShadersBOX: "..._Day.exr", and without a surname the night one
                    room.exrs.entry(p.light.unwrap_or(Light::Night)).or_default().insert(p.res, path);
                    room.mtime = room.mtime.max(*mtime);
                }
                _ => {}
            }
        }
    }
    // the newest shader of the library reads every EXR: the atlas is the same
    let shader = SHADERS.iter().find_map(|s| shaders.get(*s)).cloned();
    let mut out: Vec<Room> = rooms
        .into_iter()
        .map(|(key, mut room)| {
            room.previews = previews.remove(&key).unwrap_or_default();
            room.osl = shader.clone();
            room
        })
        .collect();
    out.sort_by_cached_key(|r| (r.folder.to_string_lossy().to_lowercase(), natural(&r.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::testing::listing;

    #[test]
    fn the_names_of_the_packs_are_understood() {
        let parse = |stem: &str, folder: &str| {
            let p = parse_room(stem, Path::new(folder));
            (p.key, p.light, p.res, p.size)
        };
        assert_eq!(parse("wP_Lobby_v1_LB07_M_day_2K", ""), ("lobby v1 lb07".into(), Some(Light::Day), 2, Some(WindowSize::M)));
        assert_eq!(parse("mP_Rooms_v1_R01_L_night_1k", ""), ("rooms v1 r01".into(), Some(Light::Night), 1, Some(WindowSize::L)));
        let high = parse("Commerce_V10_Triple_Day", r"X\maps_high_res");
        assert_eq!(high, ("commerce v10".into(), Some(Light::Day), 4, Some(WindowSize::L)));
        // a preview without the size gives the same key
        assert_eq!(parse("wp_Bedroom_01_day", "").0, parse("wP_Bedroom_01_S_night_3K", "").0);
    }

    #[test]
    fn the_rooms_are_put_together() {
        let rooms = build_interiors(
            &listing(&[
                (r"L:\WP\lobby\OSL_Shaders", &["jiWindowBox_All_v1_10.osl", "wParallax_SMP.osl"]),
                (r"L:\WP\lobby\wP_Lobby_01\EXR\2K", &["wP_Lobby_v1_LB07_M_day_2K.exr", "wP_Lobby_v1_LB07_M_night_2K.exr"]),
                (r"L:\WP\lobby\wP_Lobby_01\EXR\4K", &["wP_Lobby_v1_LB07_M_day_4K.exr"]),
                (r"L:\WP\lobby\wP_Lobby_01\Preview", &["wP_Lobby_v1_LB07_M_day_4K.jpg", "wP_Lobby_v1_LB07_M_night_4K.JPG"]),
                (r"L:\WP\sbox\maps_low_res", &["Rooms-Bedroom_V1_Single.exr", "Rooms-Bedroom_V1_Single_Day.exr"]),
                (r"L:\WP\sbox\maps_high_res", &["Rooms-Bedroom_V1_Single.exr"]),
            ]),
            &[PathBuf::from(r"L:\WP")],
        );
        assert_eq!(rooms.len(), 2);
        let (lobby, sbox) = (&rooms[0], &rooms[1]);
        assert_eq!((lobby.name.as_str(), lobby.folder.clone(), lobby.size), ("Lobby v1 LB07", PathBuf::from(r"L:\WP\lobby"), WindowSize::M));
        let ends = |p: Option<&PathBuf>, end: &str| assert!(p.is_some_and(|p| p.to_string_lossy().ends_with(end)), "{p:?} {end}");
        ends(lobby.pick(Light::Day, Res::Highest), "_day_4K.exr");
        ends(lobby.pick(Light::Night, Res::K(4)), "_night_2K.exr");
        ends(lobby.pick(Light::Day, Res::Lowest), "_day_2K.exr");
        ends(lobby.thumb_source(Light::Night), "night_4K.JPG");
        ends(lobby.osl.as_ref(), "wParallax_SMP.osl");
        assert_eq!((sbox.name.as_str(), sbox.top_res(), sbox.size), ("Rooms Bedroom V1", 4, WindowSize::S));
        ends(sbox.pick(Light::Day, Res::Highest), "Single_Day.exr");
        assert_eq!(sbox.thumb_source(Light::Day), None);
    }

    #[test]
    fn numbers_sort_as_numbers() {
        let mut names = vec!["Room 10", "Room 9", "Room 1b"];
        names.sort_by_cached_key(|n| natural(n));
        assert_eq!(names, ["Room 1b", "Room 9", "Room 10"]);
    }
}
