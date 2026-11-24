// SPDX-License-Identifier: GPL-3.0-or-later
//! Materials and textures: what the folders hold, and which maps make up each material.
//!
//! The folder is the only truth. A MATERIAL is a group of images of one folder called the same but for their
//! channel (`Wood_basecolor.png`, `Wood_normal.png`...), with its resolutions (1K, 4K, 8K) together, or a `.mat`
//! library. A TEXTURE is any image. The channel is told by the last word of a name; the real size, by the name of
//! the material, of its folder or of a `.txt` beside it ("2.5x2.5 meters", "300x188cm", "_130cm").

use crate::library::norm_key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const IMAGES: [&str; 10] = ["jpg", "jpeg", "png", "tif", "tiff", "exr", "tga", "bmp", "webp", "hdr"];
const SKIP_DIRS: [&str; 5] = ["$recycle.bin", "@eadir", "#recycle", ".openlibrary", "__macosx"];
/// Sizes written down by hand for the materials of a maker, in its folder.
const MEASURES_FILE: &str = "openlibrary_medidas.json";

/// What a map is for, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    Color,
    Rough,
    Gloss,
    Refl,
    Metal,
    Normal,
    Bump,
    Disp,
    Ao,
    Opacity,
    Emission,
    Transl,
}

impl Channel {
    /// As the bridge calls it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Rough => "rough",
            Self::Gloss => "gloss",
            Self::Refl => "refl",
            Self::Metal => "metal",
            Self::Normal => "normal",
            Self::Bump => "bump",
            Self::Disp => "disp",
            Self::Ao => "ao",
            Self::Opacity => "opacity",
            Self::Emission => "emission",
            Self::Transl => "transl",
        }
    }

    /// The two letters and the colour of its badge.
    pub fn badge(self) -> (&'static str, [u8; 3]) {
        match self {
            Self::Color => ("BC", [0xC4, 0x55, 0x3A]),
            Self::Rough => ("Rg", [0x3F, 0x8F, 0x5F]),
            Self::Gloss => ("Gl", [0x3F, 0x8F, 0x5F]),
            Self::Refl => ("Rf", [0x6C, 0x7A, 0x89]),
            Self::Metal => ("Mt", [0xB0, 0x8A, 0x2E]),
            Self::Normal => ("Nr", [0x8E, 0x7C, 0xD8]),
            Self::Bump => ("Bm", [0x6F, 0x63, 0xA8]),
            Self::Disp => ("DS", [0xC2, 0x21, 0x8B]),
            Self::Ao => ("AO", [0x1E, 0x9B, 0xD7]),
            Self::Opacity => ("Op", [0xD9, 0x79, 0x2B]),
            Self::Emission => ("Em", [0xB8, 0xA2, 0x20]),
            Self::Transl => ("Tr", [0x3F, 0xA7, 0xA0]),
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "col" | "color" | "colour" | "albedo" | "alb" | "basecolor" | "basecolour" | "diffuse" | "diff" | "dif" | "difuse" => Self::Color,
            "roughness" | "rough" | "rgh" => Self::Rough,
            "gloss" | "glossiness" | "glossy" => Self::Gloss,
            "refl" | "reflection" | "reflect" | "specular" | "spec" => Self::Refl,
            "metallic" | "metalness" | "metal" | "mtl" => Self::Metal,
            "normal" | "nrm" | "nor" | "norm" | "normalmap" | "nrml" => Self::Normal,
            "bump" | "bmp" => Self::Bump,
            "disp" | "displacement" | "displace" | "height" | "dsp" | "depth" | "heightlacement" => Self::Disp,
            "ao" | "ambientocclusion" | "occlusion" | "occ" => Self::Ao,
            "opacity" | "alpha" | "opc" | "cutout" | "alphamasked" => Self::Opacity,
            "emissive" | "emission" | "emit" | "selfillum" => Self::Emission,
            "translucency" | "translucent" | "sss" => Self::Transl,
            _ => return None,
        })
    }
}

/// Words at the end of a name that say nothing of the material.
fn is_noise(word: &str) -> bool {
    const NOISE: [&str; 20] = [
        "seamless", "tile", "tileable", "opengl", "ogl", "gl", "directx", "dx", "hires", "srgb", "raw", "linear", "jpg", "jpeg", "png", "tif",
        "tiff", "exr", "bits", "bit",
    ];
    NOISE.contains(&word)
}

fn digits(text: &str, count: std::ops::RangeInclusive<usize>) -> Option<u32> {
    (count.contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok())?
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// "4k" is 4, "6563px" is 6, anything else is 0.
fn resolution(word: &str) -> u32 {
    if let Some(k) = word.strip_suffix('k').and_then(|d| digits(d, 1..=2)) {
        return k;
    }
    word.strip_suffix("px").and_then(|d| digits(d, 3..=5)).map_or(0, |px| ((f64::from(px) / 1024.0).round_ties_even() as u32).max(1))
}

/// The words of a name: apart by `_`, `-`, spaces, and a dot that no digit follows.
fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![String::new()];
    for (i, &c) in chars.iter().enumerate() {
        let dot = c == '.' && !chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c == '_' || c == '-' || c.is_whitespace() || dot {
            out.push(String::new());
        } else if let Some(last) = out.last_mut() {
            last.push(c);
        }
    }
    out.retain(|w| !w.is_empty());
    out
}

fn split_ext(name: &str) -> (&str, String) {
    match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], name[at + 1..].to_lowercase()),
        _ => (name, String::new()),
    }
}

/// What the name of an image tells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    /// The name without channel nor resolution.
    pub base: String,
    pub channel: Option<Channel>,
    /// In K; 0 when it does not say.
    pub res: u32,
    /// 16 or 32 bits.
    pub deep: bool,
    /// Which colour variant; 0 when there are none.
    pub variant: u32,
    /// A DirectX normal map.
    pub dx: bool,
}

/// The channel that ends at word `i`: it, how many words it takes, and whether it says 16 or 32 bits.
fn channel_at(low: &[String], i: usize) -> Option<(Channel, usize, bool)> {
    let mut word = low.get(i)?.as_str();
    let before = i.checked_sub(1).and_then(|b| low.get(b)).map(String::as_str);
    let mut deep = false;
    if let Some(stem) = word.strip_suffix("16").or_else(|| word.strip_suffix("32"))
        && stem.bytes().all(|b| b.is_ascii_lowercase())
        && Channel::from_word(stem).is_some()
    {
        (word, deep) = (stem, true);
    }
    if let Some(channel) = Channel::from_word(word) {
        let two = (matches!(word, "color" | "colour") && before == Some("base")) || (word == "occlusion" && before == Some("ambient"));
        return Some((channel, 1 + usize::from(two), deep));
    }
    if word == "map"
        && let Some(channel) = before.and_then(Channel::from_word)
    {
        return Some((channel, 2, deep));
    }
    word.strip_suffix("map").and_then(Channel::from_word).map(|c| (c, 1, deep))
}

/// Takes the name of an image apart.
pub fn parse_name(file: &str) -> Parsed {
    let (stem, ext) = split_ext(file);
    let tokens = words(stem);
    let low: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let mut out = Parsed { base: String::new(), channel: None, res: 0, deep: ext == "exr" || ext == "hdr", variant: 0, dx: false };
    let mut cut = tokens.len(); // from here on: channel, resolution and noise
    let mut i = tokens.len();
    while i > 1 {
        i -= 1;
        let word = low[i].as_str();
        let res = resolution(word);
        let bits = is_digits(word) && low.get(i + 1).is_some_and(|n| n == "bits" || n == "bit");
        let variant = word.strip_prefix("var").or_else(|| word.strip_prefix('v')).and_then(|d| digits(d, 1..=2));
        if res > 0 {
            out.res = if out.res > 0 { out.res } else { res };
        } else if word == "directx" || word == "dx" {
            out.dx = true;
        } else if is_noise(word) || bits {
            out.deep |= word == "16" || word == "32";
        } else if let (Some(n), None) = (variant, out.channel) {
            out.variant = n;
        } else if out.channel.is_none() {
            let Some((channel, used, deep)) = channel_at(&low, i) else { break };
            out.channel = Some(channel);
            out.deep |= deep;
            i -= used - 1;
        } else {
            break;
        }
        cut = i;
    }
    let whole = out.channel.is_none() && out.res == 0;
    out.base = if whole { tokens.join(" ") } else { tokens[..cut].join(" ") };
    out
}

// ----------------------------------------------------------------------------------------------------
// The real size
// ----------------------------------------------------------------------------------------------------

/// Centimetres of a unit; the longer names first, as they are tried.
const UNITS: [(&str, f64); 8] =
    [("cm", 1.0), ("mm", 0.1), ("meters", 100.0), ("meter", 100.0), ("metros", 100.0), ("mts", 100.0), ("mt", 100.0), ("m", 100.0)];

fn starts_with(chars: &[char], at: usize, text: &str) -> bool {
    let rest = chars.get(at..).unwrap_or_default();
    rest.len() >= text.len() && text.chars().zip(rest).all(|(a, b)| a == *b)
}

/// The ends a number that starts at `at` may have, the longest first: digits, and a fraction after `.` or `,`.
fn number_ends(chars: &[char], at: usize) -> Vec<usize> {
    let run = |from: usize| chars.get(from..).unwrap_or_default().iter().take_while(|c| c.is_ascii_digit()).count();
    let whole = run(at);
    if whole == 0 {
        return Vec::new();
    }
    let dot = at + whole;
    let fraction = if matches!(chars.get(dot), Some('.' | ',')) { run(dot + 1) } else { 0 };
    (1..=fraction).rev().map(|n| dot + 1 + n).chain((1..=whole).rev().map(|n| at + n)).collect()
}

fn spaces(chars: &[char], at: usize) -> usize {
    chars.get(at..).unwrap_or_default().iter().take_while(|c| c.is_whitespace()).count()
}

fn number(chars: &[char]) -> f64 {
    chars.iter().map(|c| if *c == ',' { '.' } else { *c }).collect::<String>().parse().unwrap_or(0.0)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// "2.5 x 2.5 meters" at `at`: the two numbers, the unit's centimetres if it says it, and where it ends.
fn pair_at(chars: &[char], at: usize) -> Option<(f64, f64, Option<f64>, usize)> {
    let alnum = |i: usize| chars.get(i).is_some_and(char::is_ascii_alphanumeric);
    for end in number_ends(chars, at) {
        let cross = end + spaces(chars, end);
        if !matches!(chars.get(cross), Some('x' | 'X' | '×')) {
            continue;
        }
        let second = cross + 1 + spaces(chars, cross + 1);
        for stop in number_ends(chars, second) {
            let (a, b) = (number(&chars[at..end]), number(&chars[second..stop]));
            for gap in (0..=spaces(chars, stop)).rev() {
                for (unit, cm) in UNITS {
                    if starts_with(chars, stop + gap, unit) && !alnum(stop + gap + unit.len()) {
                        return Some((a, b, Some(cm), stop + gap + unit.len()));
                    }
                }
                if !alnum(stop + gap) {
                    return Some((a, b, None, stop + gap));
                }
            }
        }
    }
    None
}

/// "329cm" at `at`, not in the middle of a word.
fn single_at(chars: &[char], at: usize) -> Option<f64> {
    if at > 0 && chars.get(at - 1).is_some_and(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == ',') {
        return None;
    }
    for end in number_ends(chars, at) {
        let from = end + spaces(chars, end);
        for (unit, cm) in UNITS.iter().filter(|(u, _)| ["cm", "meters", "meter", "metros", "mts"].contains(u)) {
            if starts_with(chars, from, unit) && !chars.get(from + unit.len()).is_some_and(char::is_ascii_alphabetic) {
                return Some(number(&chars[at..end]) * cm);
            }
        }
    }
    None
}

/// The real size in cm (width, height) from the first text that tells it.
pub fn real_size<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<(f64, f64)> {
    for text in texts {
        let chars: Vec<char> = text.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            let Some((a, b, unit, end)) = pair_at(&chars, at) else {
                at += 1;
                continue;
            };
            at = end;
            if a <= 0.0 || b <= 0.0 || (unit.is_none() && a.max(b) > 1000.0) {
                continue; // 1920x1080 are pixels
            }
            // without a unit, "2x2" are metres and "190x24" centimetres
            let cm = unit.unwrap_or(if a.max(b) <= 20.0 { 100.0 } else { 1.0 });
            return Some((round2(a * cm), round2(b * cm)));
        }
        if let Some(size) = (0..chars.len()).find_map(|i| single_at(&chars, i))
            && size > 0.0
        {
            return Some((round2(size), round2(size)));
        }
    }
    None
}

// ----------------------------------------------------------------------------------------------------
// Materials and textures
// ----------------------------------------------------------------------------------------------------

/// What a folder holds, as the index lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    #[serde(rename = "m")]
    pub mtime: u64,
    #[serde(rename = "d")]
    pub dirs: Vec<String>,
    #[serde(rename = "f")]
    pub files: Vec<(String, u64)>,
}

/// Folder to what it holds.
pub type Listing = BTreeMap<PathBuf, Listed>;

fn key_of(path: &Path) -> String {
    norm_key(path)
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

type Ranked = BTreeMap<u32, BTreeMap<Channel, Vec<(PathBuf, bool)>>>;
type Maps = BTreeMap<Channel, Vec<PathBuf>>;

/// A material: its maps by resolution, or a `.mat` library.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub folder: PathBuf,
    /// Resolution (in K; 0 when it does not say) to the files of each channel, the preferred one first.
    pub maps: BTreeMap<u32, Maps>,
    /// The maker's sample image.
    pub preview: Option<PathBuf>,
    /// In cm.
    pub size: Option<(f64, f64)>,
    /// How many loose planks or tiles it is made of; 0 for an ordinary one.
    pub tiles: usize,
    /// The library of 3ds Max, when it is one.
    pub mat_file: Option<PathBuf>,
    pub mtime: u64,
}

/// The resolutions, the nearest to `res` first, and of two as near the higher one.
fn nearest(list: impl Iterator<Item = u32>, res: u32) -> Vec<u32> {
    let mut out: Vec<u32> = list.collect();
    out.sort_by_key(|r| (r.abs_diff(res), std::cmp::Reverse(*r)));
    out
}

/// Which resolution to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Highest,
    Lowest,
    K(u32),
}

impl Material {
    fn new(name: &str, folder: &Path) -> Self {
        Self {
            name: name.to_string(),
            folder: folder.to_path_buf(),
            maps: BTreeMap::new(),
            preview: None,
            size: None,
            tiles: 0,
            mat_file: None,
            mtime: 0,
        }
    }

    pub fn is_library(&self) -> bool {
        self.mat_file.is_some()
    }

    pub fn key(&self) -> String {
        format!("{}|{}", key_of(&self.folder.join(&self.name)), if self.is_library() { "mat" } else { "pbr" })
    }

    pub fn channels(&self) -> Vec<Channel> {
        self.maps.values().flat_map(|m| m.keys().copied()).collect::<BTreeSet<_>>().into_iter().collect()
    }

    /// The highest resolution; 0 when no file says.
    pub fn top_res(&self) -> u32 {
        self.maps.keys().next_back().copied().unwrap_or(0)
    }

    /// The file of each channel at a resolution; a channel that lacks it comes from the nearest one.
    pub fn pick(&self, res: Res) -> BTreeMap<Channel, PathBuf> {
        let (Some(low), Some(high)) = (self.maps.keys().next(), self.maps.keys().next_back()) else { return BTreeMap::new() };
        let res = match res {
            Res::Highest => *high,
            Res::Lowest => *low,
            Res::K(res) => nearest(self.maps.keys().copied(), res).first().copied().unwrap_or(*high),
        };
        let mut out = BTreeMap::new();
        for r in nearest(self.maps.keys().copied(), res) {
            for (channel, files) in self.maps.get(&r).into_iter().flatten() {
                if let Some(file) = files.first() {
                    out.entry(*channel).or_insert_with(|| file.clone());
                }
            }
        }
        out
    }

    /// The image for the thumbnail: the maker's sample, or the smallest colour map.
    pub fn thumb_source(&self) -> Option<PathBuf> {
        if self.preview.is_some() {
            return self.preview.clone();
        }
        let maps = self.pick(Res::Lowest);
        [Channel::Color, Channel::Rough, Channel::Gloss].iter().find_map(|c| maps.get(c)).or_else(|| maps.values().next()).cloned()
    }

    /// The map whose pixels tell its shape.
    fn shape_map(&self) -> Option<PathBuf> {
        let maps = self.pick(Res::Highest);
        maps.get(&Channel::Color).or_else(|| maps.values().next()).cloned()
    }
}

/// A loose image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub name: String,
    pub folder: PathBuf,
    pub path: PathBuf,
    pub mtime: u64,
}

/// A folder that only tells resolutions or formats apart: "4K", "8k_JPG", "...-1K".
fn is_res_folder(name: &str) -> bool {
    let low = words(&name.to_lowercase());
    let Some(last) = low.last() else { return false };
    low.iter().all(|t| resolution(t) > 0 || is_noise(t) || is_digits(t))
        || resolution(last) > 0
        || (is_noise(last) && low.len() >= 2 && resolution(&low[low.len() - 2]) > 0)
}

struct Group {
    name: String,
    folder: PathBuf,
    files: Vec<(PathBuf, Parsed)>,
    mtime: u64,
    sources: HashSet<PathBuf>,
    tiles: usize,
}

/// The folder of a group and its name, as Windows compares them.
type GroupKey = (String, String);
/// The images without a channel: by folder, lower-case stem to path.
type Loose = HashMap<String, BTreeMap<String, PathBuf>>;

/// The name without a number at its end ("oak 12" is "oak" and 12); `None` when there is none, or nothing else.
fn plank(name: &str) -> Option<(&str, u32)> {
    let count = name.bytes().rev().take(3).take_while(u8::is_ascii_digit).count();
    let (stem, number) = name.split_at(name.len() - count);
    let stem = stem.trim_end_matches(|c: char| c.is_whitespace() || c == '_' || c == '-');
    (!stem.is_empty()).then(|| Some((stem, number.parse().ok()?)))?
}

/// The loose planks of one floor (`..._1-Albedo`, `..._2-Albedo`...) are one material.
fn merge_planks(groups: &mut BTreeMap<GroupKey, Group>) {
    let mut stems: BTreeMap<GroupKey, Vec<(u32, GroupKey)>> = BTreeMap::new();
    for key in groups.keys() {
        if let Some((stem, n)) = plank(&key.1) {
            stems.entry((key.0.clone(), stem.to_string())).or_default().push((n, key.clone()));
        }
    }
    for ((folder, stem), mut members) in stems {
        if members.len() < 4 {
            continue;
        }
        members.sort();
        let mtime = members.iter().filter_map(|(_, k)| groups.get(k)).map(|g| g.mtime).max().unwrap_or(0);
        let mut first: Option<Group> = None;
        for (_, key) in &members {
            let group = groups.remove(key);
            first = first.or(group);
        }
        if let Some(mut group) = first {
            group.name = plank(&group.name).map_or_else(|| group.name.clone(), |(s, _)| s.to_string());
            (group.mtime, group.tiles) = (mtime, members.len());
            groups.insert((folder, format!("{stem}|tiles")), group);
        }
    }
}

/// Whether `a` (path, deep) is better than `b` for a channel: 16 bits for displacement, 8 for the rest.
fn better(channel: Channel, a: &(PathBuf, bool), b: &(PathBuf, bool)) -> bool {
    if a.1 != b.1 {
        return if channel == Channel::Disp { a.1 } else { !a.1 };
    }
    key_of(&a.0) < key_of(&b.0)
}

/// The files of a group by resolution and channel, without the colour variants after the first.
fn sort_maps(files: &[(PathBuf, Parsed)]) -> BTreeMap<u32, Maps> {
    let mut found: Ranked = BTreeMap::new();
    for (path, p) in files.iter().filter(|(_, p)| p.variant <= 1) {
        if let Some(channel) = p.channel {
            let slot = found.entry(p.res).or_default().entry(channel).or_default();
            let new = (path.clone(), p.deep);
            let at = slot.iter().position(|old| better(channel, &new, old)).unwrap_or(slot.len());
            slot.insert(at, new);
        }
    }
    found.into_iter().map(|(res, by)| (res, by.into_iter().map(|(c, list)| (c, list.into_iter().map(|(p, _)| p).collect())).collect())).collect()
}

/// Every material of a listing, by name.
pub fn build_materials(listing: &Listing) -> Vec<Material> {
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    let mut libraries = Vec::new();
    let mut loose: Loose = HashMap::new();
    let mut plain: HashMap<(String, String), Vec<(PathBuf, Parsed)>> = HashMap::new();
    for (folder, listed) in listing {
        let group_folder = if is_res_folder(&name_of(folder)) { folder.parent().unwrap_or(folder) } else { folder };
        for (file, mtime) in listed.files.iter().filter(|(f, _)| !f.starts_with("._")) {
            let (stem, ext) = split_ext(file);
            if ext == "mat" {
                libraries.push((folder, file, *mtime));
            }
            if !IMAGES.contains(&ext.as_str()) {
                continue;
            }
            let parsed = parse_name(file);
            let path = folder.join(file);
            let key = (key_of(group_folder), parsed.base.to_lowercase());
            if parsed.channel.is_none() {
                loose.entry(key_of(folder)).or_default().insert(stem.to_lowercase(), path.clone());
                plain.entry(key).or_default().push((path, parsed));
                continue;
            }
            let group = groups.entry(key).or_insert_with(|| Group {
                name: parsed.base.clone(),
                folder: group_folder.to_path_buf(),
                files: Vec::new(),
                mtime: 0,
                sources: HashSet::new(),
                tiles: 0,
            });
            group.files.push((path, parsed));
            group.mtime = group.mtime.max(*mtime);
            group.sources.insert(folder.clone());
        }
    }
    merge_planks(&mut groups);
    let mut out = Vec::new();
    for (key, mut group) in groups {
        let mut channels: HashSet<Channel> = group.files.iter().filter_map(|(_, p)| p.channel).collect();
        if group.tiles == 0 && !channels.contains(&Channel::Color) {
            // "Oak.jpg" beside "Oak_BUMP.jpg" and "Oak_GLOSS.jpg" is the colour
            for (path, parsed) in plain.get(&key).into_iter().flatten() {
                group.files.push((path.clone(), Parsed { channel: Some(Channel::Color), ..parsed.clone() }));
                channels.insert(Channel::Color);
            }
        }
        if channels.len() < 2 {
            continue; // one image that says a channel is not a material
        }
        let mut material = Material::new(&group.name, &group.folder);
        (material.mtime, material.tiles, material.maps) = (group.mtime, group.tiles, sort_maps(&group.files));
        group.sources.insert(group.folder.clone());
        let folder_name = name_of(&group.folder);
        let notes: Vec<&str> = group
            .sources
            .iter()
            .filter_map(|f| listing.get(f))
            .flat_map(|l| l.files.iter().map(|(n, _)| n.as_str()))
            .filter(|n| n.to_lowercase().ends_with(".txt") && !n.starts_with("._"))
            .collect();
        material.size = real_size([group.name.as_str(), folder_name.as_str()].into_iter().chain(notes));
        material.preview = find_preview(&material, &loose, listing);
        out.push(material);
    }
    for (folder, file, mtime) in libraries {
        let stem = split_ext(file).0;
        let mut material = Material::new(stem, folder);
        (material.mat_file, material.mtime) = (Some(folder.join(file)), mtime);
        material.size = real_size([stem]);
        material.preview = loose.get(&key_of(folder)).and_then(|images| library_preview(stem, images));
        out.push(material);
    }
    out.sort_by_cached_key(|m| (m.name.to_lowercase(), m.folder.to_string_lossy().to_lowercase()));
    out
}

fn squash(text: &str) -> String {
    text.to_lowercase().chars().filter(char::is_ascii_alphanumeric).collect()
}

/// The maker's sample: in the material's folder, or with its name beside it.
fn find_preview(material: &Material, loose: &Loose, listing: &Listing) -> Option<PathBuf> {
    const WORDS: [&str; 9] = ["sphere", "preview", "render", "thumb", "ball", "shaderball", "promo", "cube", "flat"];
    let bare = |text: &str| text.replace([' ', '_', '-'], "");
    let low = bare(&material.name.to_lowercase());
    let empty = BTreeMap::new();
    let here = loose.get(&key_of(&material.folder)).unwrap_or(&empty);
    let sample = here
        .iter()
        .filter_map(|(stem, path)| Some((WORDS.iter().position(|w| stem.contains(w))?, stem, path)))
        .filter(|(_, stem, _)| here.len() <= 6 || bare(stem).contains(&low)) // among many, it has to name it
        .min_by_key(|(rank, _, _)| *rank);
    if let Some((_, _, path)) = sample {
        return Some(path.clone());
    }
    let parent = material.folder.parent()?;
    let folder = name_of(&material.folder);
    let side = loose.get(&key_of(parent)).unwrap_or(&empty);
    let wanted = [squash(&folder), squash(&material.name)];
    let hit = side.get(&folder.to_lowercase()).or_else(|| side.iter().find(|(stem, _)| wanted.contains(&squash(stem))).map(|(_, p)| p)).cloned();
    let hit = hit.or_else(|| {
        // an image beside it called as the folder, though its name ends like a channel ("..._Copper_Metal.png")
        let files = &listing.get(parent)?.files;
        let file = files.iter().find(|(n, _)| {
            let (stem, ext) = split_ext(n);
            ["jpg", "jpeg", "png", "webp", "bmp"].contains(&ext.as_str()) && squash(stem) == wanted[0]
        });
        file.map(|(n, _)| parent.join(n))
    });
    hit.filter(|p| !["exr", "hdr"].contains(&split_ext(&name_of(p)).1.as_str()))
}

fn library_preview(stem: &str, images: &BTreeMap<String, PathBuf>) -> Option<PathBuf> {
    let s = stem.to_lowercase();
    let ends = ["", "_sphere", "-01", "_01", " 01", "_preview", "_cube", "_flat"];
    if let Some(path) = ends.iter().find_map(|e| images.get(&format!("{s}{e}"))) {
        return Some(path.clone());
    }
    // "Name-3K.mat" with "Name_Sphere.jpg"
    let short = s.strip_suffix('k')?;
    let short = short.trim_end_matches(|c: char| c.is_ascii_digit());
    let count = s.len() - 1 - short.len();
    let short = short.trim_end_matches(|c: char| c.is_whitespace() || c == '_' || c == '-');
    if !(1..=2).contains(&count) {
        return None;
    }
    ["_sphere", "", "_cube", "_flat"].iter().find_map(|e| images.get(&format!("{short}{e}"))).cloned()
}

/// Every image, by folder and name.
pub fn build_textures(listing: &Listing) -> Vec<Texture> {
    let mut out: Vec<Texture> = listing
        .iter()
        .flat_map(|(folder, listed)| listed.files.iter().map(move |(f, m)| (folder, f, *m)))
        .filter(|(_, f, _)| !f.starts_with("._") && IMAGES.contains(&split_ext(f).1.as_str()))
        .map(|(folder, f, mtime)| Texture { name: split_ext(f).0.to_string(), folder: folder.clone(), path: folder.join(f), mtime })
        .collect();
    out.sort_by_cached_key(|t| (t.folder.to_string_lossy().to_lowercase(), t.name.to_lowercase()));
    out
}

/// How a loose texture is loaded: "color", "normal", or "data" (linear).
pub fn guess_role(file: &str) -> &'static str {
    match parse_name(file).channel {
        Some(Channel::Normal) => "normal",
        Some(Channel::Color | Channel::Emission | Channel::Transl) | None => "color",
        Some(_) => "data",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Square,
    Wide,
    Tall,
}

pub fn aspect(width: f64, height: f64) -> Option<Aspect> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let ratio = width / height;
    Some(if (0.95..=1.05).contains(&ratio) {
        Aspect::Square
    } else if ratio > 1.0 {
        Aspect::Wide
    } else {
        Aspect::Tall
    })
}

/// The pixels of an image (width, height), from its header alone: JPEG, PNG, TIFF.
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = [0u8; 26];
    let read = file.read(&mut head).ok()?;
    if read >= 24 && head.starts_with(b"\x89PNG\r\n\x1a\n") {
        let at = |i: usize| u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
        return Some((at(16), at(20)));
    }
    if head.starts_with(&[0xFF, 0xD8]) {
        return jpeg_size(&mut file);
    }
    if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        return tiff_size(&mut file, head[0] == b'I');
    }
    None
}

fn take<const N: usize>(file: &mut impl Read) -> Option<[u8; N]> {
    let mut out = [0u8; N];
    file.read_exact(&mut out).ok()?;
    Some(out)
}

fn jpeg_size(file: &mut fs::File) -> Option<(u32, u32)> {
    file.seek(SeekFrom::Start(2)).ok()?;
    let mut file = std::io::BufReader::new(file);
    for _ in 0..10_000 {
        while take::<1>(&mut file)?[0] != 0xFF {}
        let mut marker = 0xFF;
        while marker == 0xFF {
            marker = take::<1>(&mut file)?[0];
        }
        if matches!(marker, 0xD0..=0xD8 | 0x01) {
            continue;
        }
        let length = u16::from_be_bytes(take(&mut file)?);
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let [_, h1, h2, w1, w2] = take(&mut file)?;
            return Some((u16::from_be_bytes([w1, w2]).into(), u16::from_be_bytes([h1, h2]).into()));
        }
        file.seek_relative(i64::from(length) - 2).ok()?;
    }
    None
}

fn tiff_size(file: &mut fs::File, little: bool) -> Option<(u32, u32)> {
    let u16_of = |b: [u8; 2]| if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) };
    let u32_of = |b: [u8; 4]| if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
    file.seek(SeekFrom::Start(4)).ok()?;
    let start = u32_of(take(file)?);
    file.seek(SeekFrom::Start(start.into())).ok()?;
    let count = u16_of(take(file)?).min(64);
    let (mut width, mut height) = (None, None);
    for _ in 0..count {
        let tag = u16_of(take(file)?);
        let kind = u16_of(take(file)?);
        take::<4>(file)?;
        let raw: [u8; 4] = take(file)?;
        let value = if kind == 3 { u16_of([raw[0], raw[1]]).into() } else { u32_of(raw) };
        match tag {
            256 => width = Some(value),
            257 => height = Some(value),
            _ => {}
        }
    }
    Some((width?, height?))
}

// ----------------------------------------------------------------------------------------------------
// Sizes written down
// ----------------------------------------------------------------------------------------------------

type Measures = HashMap<String, (f64, f64)>;

/// Adds the sizes of a file: `{"materiales": {"subfolder": [w, h], "subfolder|Name": [w, h]}}`, in cm, with the
/// subfolders under `base`.
fn add_measures(out: &mut Measures, file: &Path, base: &Path) {
    let Some(data) = fs::read(file).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { return };
    let Some(list) = data.get("materiales").and_then(|m| m.as_object()) else { return };
    for (rel, size) in list {
        let at = |i: usize| size.get(i).and_then(serde_json::Value::as_f64).filter(|v| *v > 0.0);
        if let (Some(w), Some(h)) = (at(0), at(1)) {
            let (rel, name) = rel.split_once('|').unwrap_or((rel, ""));
            let key = key_of(&base.join(rel.replace('/', "\\")));
            let key = key.trim_end_matches(['\\', '.']).to_string();
            out.insert(if name.is_empty() { key } else { format!("{key}|{}", name.to_lowercase()) }, (w, h));
        }
    }
}

/// The sizes in the library itself, and in the folders of sizes (one file per root, named after it).
fn load_measures(listing: &Listing, roots: &[PathBuf], dirs: &[PathBuf]) -> Measures {
    let mut out = Measures::new();
    for (folder, listed) in listing {
        if listed.files.iter().any(|(n, _)| n.eq_ignore_ascii_case(MEASURES_FILE)) {
            add_measures(&mut out, &folder.join(MEASURES_FILE), folder);
        }
    }
    for dir in dirs {
        for root in roots {
            let name = Some(name_of(root)).filter(|n| !n.is_empty()).unwrap_or_else(|| "raiz".to_string());
            add_measures(&mut out, &dir.join(format!("{name}.json")), root);
        }
    }
    out
}

// ----------------------------------------------------------------------------------------------------
// The index
// ----------------------------------------------------------------------------------------------------

const CACHE_VERSION: u32 = 1;

/// The pixels of a map (0, 0 when it could not be read), and the date it had.
type Dims = HashMap<PathBuf, (u32, u32, u64)>;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    listing: Listing,
    dims: Dims,
}

/// The folders of a tab of materials, textures or interiors, with its cache: only those that changed are
/// listed again.
#[derive(Clone, Default)]
pub struct SurfaceIndex {
    pub roots: Vec<PathBuf>,
    pub listing: Listing,
    pub dims: Dims,
    /// Where the sizes written down are, besides the library.
    pub measures_dirs: Vec<PathBuf>,
}

fn date(meta: &fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn list(folder: &Path, mtime: u64) -> Option<Listed> {
    let mut out = Listed { mtime, ..Listed::default() };
    for entry in fs::read_dir(folder).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if !name.starts_with('.') && !SKIP_DIRS.contains(&name.to_lowercase().as_str()) {
                out.dirs.push(name);
            }
        } else {
            let ext = split_ext(&name).1;
            if IMAGES.contains(&ext.as_str()) || ["mat", "txt", "osl"].contains(&ext.as_str()) || name.eq_ignore_ascii_case(MEASURES_FILE) {
                out.files.push((name, date(&meta)));
            }
        }
    }
    out.dirs.sort_by_cached_key(|d| d.to_lowercase());
    out.files.sort_by_cached_key(|(f, _)| f.to_lowercase());
    Some(out)
}

impl SurfaceIndex {
    pub fn new(roots: &[PathBuf], measures_dirs: &[PathBuf]) -> Self {
        let roots = roots.iter().filter(|r| !r.as_os_str().is_empty()).cloned().collect();
        Self { roots, measures_dirs: measures_dirs.to_vec(), ..Self::default() }
    }

    pub fn folders(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.listing.keys().cloned().collect();
        out.sort_by_cached_key(|f| norm_key(f));
        out
    }

    /// Reads the folders again; `false` when it was cancelled, and the index keeps what it had. How many it has
    /// seen is told every 25. A root that is not reachable keeps what was last read of it.
    pub fn scan(&mut self, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(usize)) -> bool {
        let mut new = Listing::new();
        let (online, offline): (Vec<PathBuf>, Vec<PathBuf>) = self.roots.iter().cloned().partition(|r| r.is_dir());
        let mut pending = online;
        while let Some(folder) = pending.pop() {
            if cancelled() {
                return false;
            }
            let Ok(meta) = fs::metadata(&folder) else { continue };
            let mtime = date(&meta);
            let listed = match self.listing.get(&folder) {
                Some(old) if old.mtime == mtime => old.clone(),
                _ => match list(&folder, mtime) {
                    Some(listed) => listed,
                    None => continue,
                },
            };
            pending.extend(listed.dirs.iter().map(|d| folder.join(d)));
            new.insert(folder, listed);
            if new.len().is_multiple_of(25) {
                progress(new.len());
            }
        }
        for (folder, listed) in &self.listing {
            if offline.iter().any(|r| folder.starts_with(r)) {
                new.insert(folder.clone(), listed.clone());
            }
        }
        self.listing = new;
        true
    }

    pub fn materials(&self) -> Vec<Material> {
        let mut out = build_materials(&self.listing);
        let measures = load_measures(&self.listing, &self.roots, &self.measures_dirs);
        for m in out.iter_mut().filter(|m| m.size.is_none() && !m.is_library()) {
            let key = key_of(&m.folder);
            m.size = measures.get(&format!("{key}|{}", m.name.to_lowercase())).or_else(|| measures.get(&key)).copied();
        }
        out
    }

    /// The pixels of a material's colour map, if they were read.
    pub fn dims_of(&self, material: &Material) -> Option<(u32, u32)> {
        self.dims.get(&material.shape_map()?).filter(|d| d.0 > 0 && d.1 > 0).map(|d| (d.0, d.1))
    }

    /// Reads the header of the colour maps not known yet, or changed. Returns whether it read any.
    pub fn fill_dims(&mut self, materials: &[Material], cancelled: &dyn Fn() -> bool) -> bool {
        let mut any = false;
        for path in materials.iter().filter_map(Material::shape_map) {
            if cancelled() {
                break;
            }
            let listed = path.parent().and_then(|d| self.listing.get(d));
            let file = name_of(&path);
            let mtime = listed.and_then(|l| l.files.iter().find(|(n, _)| *n == file)).map_or(0, |(_, m)| *m);
            if self.dims.get(&path).is_some_and(|d| d.2 == mtime) {
                continue;
            }
            let (w, h) = image_size(&path).unwrap_or((0, 0));
            self.dims.insert(path, (w, h, mtime));
            any = true;
        }
        any
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let cache = CacheFile { version: CACHE_VERSION, listing: self.listing.clone(), dims: self.dims.clone() };
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(&cache).map_err(std::io::Error::other)?)?;
        fs::rename(&tmp, path)
    }

    pub fn load(&mut self, path: &Path) -> bool {
        match fs::read(path).ok().and_then(|b| serde_json::from_slice::<CacheFile>(&b).ok()) {
            Some(cache) if cache.version == CACHE_VERSION => {
                (self.listing, self.dims) = (cache.listing, cache.dims);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::{Listed, Listing};
    use std::path::PathBuf;

    /// A listing like the index's, from folders and names.
    pub fn listing(tree: &[(&str, &[&str])]) -> Listing {
        tree.iter()
            .map(|(folder, names)| {
                let files = names.iter().map(|n| (n.to_string(), 0)).collect();
                (PathBuf::from(folder), Listed { mtime: 0, dirs: Vec::new(), files })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::listing;
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openlibrary-surfaces-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ends(path: Option<&PathBuf>, end: &str) -> bool {
        path.is_some_and(|p| p.to_string_lossy().ends_with(end))
    }

    #[test]
    fn channels_and_resolutions_are_told_by_the_name() {
        let cases = [
            ("WoodFlooring065_COL_4K.jpg", Channel::Color, 4),
            ("WoodFlooring065_DISP16_4K.tif", Channel::Disp, 4),
            ("Mahogany_Veneer_Wood_basecolor.png", Channel::Color, 0),
            ("Brick_Stack_Bond_ambientocclusion.png", Channel::Ao, 0),
            ("MOTW_239_Maple_8k_Albedo.tiff", Channel::Color, 8),
            ("TextureSupply_Concrete001_Raw_329cm_8K-Normal.jpg", Channel::Normal, 8),
            ("txyz_silk_05_dsp_2k.tif", Channel::Disp, 2),
            ("3DC_Beech_Wood_NormalMap_OpenGL_Seamless_16K.jpg", Channel::Normal, 16),
            ("Poliigon_Facade_10956_heightlacement.tiff", Channel::Disp, 0),
            ("Metal_Plate_Base_Color.png", Channel::Color, 0),
            ("LARGE_STAINS_08_NORM_8_bits.tiff", Channel::Normal, 0),
        ];
        for (name, channel, res) in cases {
            let p = parse_name(name);
            assert_eq!((p.channel, p.res), (Some(channel), res), "{name}");
        }
        assert_eq!(parse_name("Metal_Plate_Base_Color.png").base, "Metal Plate");
    }

    #[test]
    fn a_material_s_word_is_not_a_channel() {
        let p = parse_name("Brushed_Metal_Plate_roughness.png");
        assert_eq!((p.channel, p.base.as_str()), (Some(Channel::Rough), "Brushed Metal Plate"));
        assert_eq!(parse_name("Oak Classic 123.jpg").channel, None);
    }

    #[test]
    fn depth_variant_and_directx_are_told() {
        assert!(parse_name("Wood_NRM16_4K.tif").deep);
        assert_eq!(parse_name("Wood_COL_VAR2_4K.jpg").variant, 2);
        assert!(parse_name("Wood_Normal_DirectX.png").dx);
    }

    #[test]
    fn the_real_size_is_read() {
        let cases = [
            ("3D Scanned Bunker Concrete - 2.5x2.5 meters", Some((250.0, 250.0))),
            ("Real World Size=300x188cm.txt", Some((300.0, 188.0))),
            ("Oak_MultiTexture_40,8x6,8cm", Some((40.8, 6.8))),
            ("Asphalt_Base5_4x4_4K-4x4m", Some((400.0, 400.0))),
            ("WF_01_190x24", Some((190.0, 24.0))),
            ("Concrete001_Raw_329cm_8K", Some((329.0, 329.0))),
            ("Concrete 329cm", Some((329.0, 329.0))),
            ("2x2 wood", Some((200.0, 200.0))),
            ("Render_1920x1080", None),
            ("Oak Classic", None),
            ("4x4K", None),
        ];
        for (text, size) in cases {
            assert_eq!(real_size([text]), size, "{text}");
        }
        assert_eq!(real_size(["Oak", "Oak 30x60cm"]), Some((30.0, 60.0)));
    }

    #[test]
    fn a_loose_texture_is_loaded_by_its_name() {
        assert_eq!(guess_role("x_normal.png"), "normal");
        assert_eq!(guess_role("x_roughness.png"), "data");
        assert_eq!(guess_role("fingerprints_004.jpg"), "color");
    }

    #[test]
    fn resolutions_are_one_material() {
        let mats = build_materials(&listing(&[
            (r"L:\Wood\Oak 01\2K", &["Oak01_COL_2K.jpg", "Oak01_NRM_2K.jpg", "Oak01_GLOSS_2K.jpg"]),
            (r"L:\Wood\Oak 01\4K", &["Oak01_COL_4K.jpg", "Oak01_NRM_4K.jpg", "Oak01_NRM16_4K.tif", "Oak01_DISP_4K.jpg", "Oak01_DISP16_4K.tif"]),
            (r"L:\Wood", &["Oak 01.jpg"]),
        ]));
        assert_eq!(mats.len(), 1);
        let m = &mats[0];
        assert_eq!(m.folder, PathBuf::from(r"L:\Wood\Oak 01"));
        assert_eq!(m.channels(), [Channel::Color, Channel::Gloss, Channel::Normal, Channel::Disp]);
        let top = m.pick(Res::Highest);
        assert!(ends(top.get(&Channel::Normal), "Oak01_NRM_4K.jpg"));
        assert!(ends(top.get(&Channel::Disp), "Oak01_DISP16_4K.tif"));
        assert!(ends(top.get(&Channel::Gloss), "Oak01_GLOSS_2K.jpg"));
        assert!(ends(m.pick(Res::Lowest).get(&Channel::Color), "_2K.jpg"));
        assert!(ends(m.pick(Res::K(8)).get(&Channel::Color), "_4K.jpg"));
        assert_eq!(m.thumb_source(), Some(PathBuf::from(r"L:\Wood\Oak 01.jpg")));
    }

    #[test]
    fn an_image_without_a_channel_is_the_colour() {
        let mats = build_materials(&listing(&[(r"L:\D\PBR maps", &["HeartOak-350mm.jpg", "HeartOak-350mm_BUMP.jpg", "HeartOak-350mm_GLOSS.jpg"])]));
        assert_eq!(mats[0].channels(), [Channel::Color, Channel::Gloss, Channel::Bump]);
    }

    #[test]
    fn one_map_is_not_a_material() {
        let tree = listing(&[(r"L:\Dirt", &["Smudge_01_roughness.jpg", "Leak.jpg", "notes.txt"])]);
        assert!(build_materials(&tree).is_empty());
        assert_eq!(build_textures(&tree).len(), 2);
    }

    #[test]
    fn loose_planks_are_one_material() {
        let names: Vec<String> = (1..7).flat_map(|i| [format!("Oak_40,8x6,8cm_{i}-Albedo.jpg"), format!("Oak_40,8x6,8cm_{i}-Normal.jpg")]).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mats = build_materials(&listing(&[(r"L:\Planks", &names)]));
        assert_eq!((mats.len(), mats[0].tiles, mats[0].size), (1, 6, Some((40.8, 6.8))));
        assert!(ends(mats[0].pick(Res::Highest).get(&Channel::Color), "_1-Albedo.jpg"));
    }

    #[test]
    fn a_text_tells_the_size_and_a_mat_is_a_library() {
        let mats = build_materials(&listing(&[
            (r"L:\MOTW\Elm\Elm_8k_JPG", &["Elm_Albedo_8k.jpg", "Elm_Roughness_8k.jpg", "Real World Size=300x188cm.txt"]),
            (r"L:\Libs", &["Asphalt Road.mat", "Asphalt Road.jpg", "Card-3K.mat", "Card_Sphere.jpg"]),
        ]));
        let by = |name: &str| mats.iter().find(|m| m.name == name).unwrap();
        assert_eq!((by("Elm").size, by("Elm").folder.clone()), (Some((300.0, 188.0)), PathBuf::from(r"L:\MOTW\Elm")));
        assert!(by("Asphalt Road").is_library());
        assert_eq!(by("Asphalt Road").preview, Some(PathBuf::from(r"L:\Libs\Asphalt Road.jpg")));
        assert_eq!(by("Card-3K").preview, Some(PathBuf::from(r"L:\Libs\Card_Sphere.jpg")));
    }

    #[test]
    fn a_sample_beside_the_folder_is_found_whatever_its_separators() {
        let mats = build_materials(&listing(&[
            (r"L:\Bricks", &["Bricks_001.jpg"]),
            (r"L:\Bricks\Bricks 001", &["Bricks01_COL_VAR1_6K.jpg", "Bricks01_NRM_6K.jpg"]),
        ]));
        assert_eq!(mats[0].preview, Some(PathBuf::from(r"L:\Bricks\Bricks_001.jpg")));
        let mats = build_materials(&listing(&[
            (r"L:\Metal", &["Aged_Cast_Copper_Metal.png"]),
            (r"L:\Metal\Aged_Cast_Copper_Metal", &["Aged_Cast_Copper_Metal_basecolor.png", "Aged_Cast_Copper_Metal_metallic.png"]),
        ]));
        assert_eq!(mats[0].preview, Some(PathBuf::from(r"L:\Metal\Aged_Cast_Copper_Metal.png")));
    }

    #[test]
    fn the_shape_comes_from_the_header() {
        assert_eq!(aspect(4096.0, 4096.0), Some(Aspect::Square));
        assert_eq!(aspect(4096.0, 2048.0), Some(Aspect::Wide));
        assert_eq!(aspect(300.0, 600.0), Some(Aspect::Tall));
        let dir = folder("header");
        for ext in ["png", "jpg", "tiff"] {
            let path = dir.join(format!("a.{ext}"));
            image::RgbImage::new(64, 32).save(&path).unwrap();
            assert_eq!(image_size(&path), Some((64, 32)), "{ext}");
        }
        fs::write(dir.join("bad.jpg"), [0xFF, 0xD8, 0xFF]).unwrap();
        assert_eq!(image_size(&dir.join("bad.jpg")), None);
    }

    #[test]
    fn the_index_lists_again_only_what_changed_and_reads_the_sizes() {
        let dir = folder("index");
        let brick = dir.join("Vendor").join("Brick A");
        fs::create_dir_all(&brick).unwrap();
        for name in ["BrickA_basecolor.png", "BrickA_normal.png"] {
            image::RgbImage::new(8, 4).save(brick.join(name)).unwrap();
        }
        fs::write(brick.join("loose.jpg"), b"x").unwrap();
        fs::write(dir.join("Vendor").join(MEASURES_FILE), r#"{"materiales": {"Brick A": [200, 100]}}"#).unwrap();
        let mut index = SurfaceIndex::new(&[dir.join("Vendor")], &[]);
        assert!(index.scan(&|| false, &mut |_| {}));
        let mats = index.materials();
        assert_eq!((mats.len(), mats[0].size), (1, Some((200.0, 100.0))));
        assert_eq!(build_textures(&index.listing).len(), 3);
        assert!(index.fill_dims(&mats, &|| false) && !index.fill_dims(&mats, &|| false));
        assert_eq!(index.dims_of(&mats[0]), Some((8, 4)));
        let cache = dir.join("cache.json");
        index.save(&cache).unwrap();
        let mut again = SurfaceIndex::new(&[dir.join("Vendor")], &[]);
        assert!(again.load(&cache));
        assert_eq!((again.listing == index.listing, again.dims == index.dims), (true, true));
        assert!(!again.scan(&|| true, &mut |_| {}));
        assert_eq!(again.listing, index.listing);
        // the folder of sizes, with a file per root
        fs::remove_file(dir.join("Vendor").join(MEASURES_FILE)).unwrap();
        fs::write(dir.join("Vendor.json"), r#"{"materiales": {"Brick A|brickA": [30, 60]}}"#).unwrap();
        let mut sized = SurfaceIndex::new(&[dir.join("Vendor")], std::slice::from_ref(&dir));
        sized.scan(&|| false, &mut |_| {});
        assert_eq!(sized.materials()[0].size, Some((30.0, 60.0)));
    }
}
