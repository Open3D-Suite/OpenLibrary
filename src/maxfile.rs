// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a `.max` file without 3ds Max.
//!
//! A `.max` file is a Compound File Binary (OLE). Only the few streams that matter are read, so it is fast
//! even on a network drive and with files of hundreds of MB:
//! - `ClassDirectory3` (and `DllDirectory`): the classes the scene uses, which tell its renderer;
//! - `\x05SummaryInformation`: the viewport capture 3ds Max saves with the file;
//! - `FileAssetMetaData3`: the external files (textures, IES, proxies) and their paths.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const FREESECT: u32 = 0xFFFF_FFFF;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const MAXREGSECT: u32 = 0xFFFF_FFFA;
/// Longest chain followed, in sectors: a corrupt file never makes the reader loop for ever.
const MAX_CHAIN: usize = 10_000_000;

/// What went wrong while reading a compound file.
#[derive(Debug)]
pub enum CfbError {
    Io(std::io::Error),
    NotCompound,
    Corrupt(&'static str),
}

impl From<std::io::Error> for CfbError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// One entry of the directory.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub kind: u8,
    pub start: u32,
    pub size: u64,
}

/// A minimal, lazy reader of compound files: it reads only the sectors it needs.
pub struct CompoundFile<R: Read + Seek> {
    reader: R,
    sector_size: usize,
    mini_size: usize,
    mini_cutoff: u64,
    first_dir: u32,
    first_minifat: u32,
    n_minifat: u32,
    fat_sectors: Vec<u32>,
    fat_cache: Vec<Option<Vec<u32>>>,
    minifat: Option<Vec<u32>>,
    entries: Option<Vec<Option<Entry>>>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i32_at(data: &[u8], at: usize) -> Option<i32> {
    u32_at(data, at).map(|v| v as i32)
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    data.get(at..at + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
}

fn words(data: &[u8]) -> Vec<u32> {
    data.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect()
}

impl CompoundFile<File> {
    pub fn open(path: &Path) -> Result<Self, CfbError> {
        Self::new(File::open(path)?)
    }
}

impl<R: Read + Seek> CompoundFile<R> {
    pub fn new(mut reader: R) -> Result<Self, CfbError> {
        let mut header = [0u8; 512];
        reader.seek(SeekFrom::Start(0))?;
        if reader.read(&mut header)? < 512 || header[..8] != SIGNATURE {
            return Err(CfbError::NotCompound);
        }
        let shift = u16_at(&header, 0x1E).unwrap_or(0);
        let mini_shift = u16_at(&header, 0x20).unwrap_or(0);
        if shift != 9 && shift != 12 {
            return Err(CfbError::Corrupt("sector size"));
        }
        if mini_shift > 12 {
            return Err(CfbError::Corrupt("mini sector size"));
        }
        let field = |at| u32_at(&header, at).unwrap_or(0);
        let mut cf = Self {
            reader,
            sector_size: 1 << shift,
            mini_size: 1 << mini_shift,
            mini_cutoff: u64::from(field(0x38)),
            first_dir: field(0x30),
            first_minifat: field(0x3C),
            n_minifat: field(0x40),
            fat_sectors: Vec::new(),
            fat_cache: Vec::new(),
            minifat: None,
            entries: None,
        };
        let n_fat = field(0x2C) as usize;
        let mut fat_sectors: Vec<u32> = (0..109).map(|i| field(0x4C + i * 4)).collect();
        let mut sect = field(0x44);
        let mut guard = 0;
        while sect != ENDOFCHAIN && sect != FREESECT && sect <= MAXREGSECT {
            let values = words(&cf.sector(sect)?);
            let (last, body) = values.split_last().ok_or(CfbError::Corrupt("DIFAT"))?;
            fat_sectors.extend_from_slice(body);
            sect = *last;
            guard += 1;
            if guard > 100_000 {
                return Err(CfbError::Corrupt("DIFAT chain"));
            }
        }
        fat_sectors.truncate(n_fat);
        fat_sectors.retain(|s| *s <= MAXREGSECT);
        cf.fat_cache = vec![None; fat_sectors.len()];
        cf.fat_sectors = fat_sectors;
        Ok(cf)
    }

    fn sector(&mut self, n: u32) -> Result<Vec<u8>, CfbError> {
        let mut data = vec![0u8; self.sector_size];
        self.reader.seek(SeekFrom::Start((u64::from(n) + 1) * self.sector_size as u64))?;
        let mut got = 0;
        while got < data.len() {
            let read = self.reader.read(&mut data[got..])?;
            if read == 0 {
                break; // a short last sector reads as zeros
            }
            got += read;
        }
        Ok(data)
    }

    fn fat(&mut self, n: u32) -> Result<u32, CfbError> {
        let per_sector = self.sector_size / 4;
        let (index, offset) = (n as usize / per_sector, n as usize % per_sector);
        if index >= self.fat_sectors.len() {
            return Err(CfbError::Corrupt("FAT index"));
        }
        if self.fat_cache[index].is_none() {
            let table = words(&self.sector(self.fat_sectors[index])?);
            self.fat_cache[index] = Some(table);
        }
        Ok(self.fat_cache[index].as_ref().map_or(ENDOFCHAIN, |t| t[offset]))
    }

    fn chain(&mut self, start: u32, limit: Option<u64>) -> Result<Vec<u8>, CfbError> {
        let mut out = Vec::new();
        let mut sect = start;
        let mut steps = 0;
        while sect <= MAXREGSECT {
            out.extend_from_slice(&self.sector(sect)?);
            if limit.is_some_and(|limit| out.len() as u64 >= limit) {
                break;
            }
            sect = self.fat(sect)?;
            steps += 1;
            if steps > MAX_CHAIN {
                return Err(CfbError::Corrupt("FAT chain"));
            }
        }
        Ok(out)
    }

    /// Every entry of the directory, `None` for the unused ones.
    pub fn entries(&mut self) -> Result<&[Option<Entry>], CfbError> {
        if self.entries.is_none() {
            let raw = self.chain(self.first_dir, None)?;
            let small = self.sector_size == 512;
            let entries = raw
                .as_chunks::<128>()
                .0
                .iter()
                .map(|e| {
                    let name_len = u16_at(e, 64).unwrap_or(0) as usize;
                    let kind = e[66];
                    if kind == 0 || !(2..=64).contains(&name_len) {
                        return None;
                    }
                    let units: Vec<u16> = e[..name_len - 2].as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
                    let size = u64_at(e, 120).unwrap_or(0);
                    Some(Entry {
                        name: String::from_utf16_lossy(&units),
                        kind,
                        start: u32_at(e, 116).unwrap_or(ENDOFCHAIN),
                        size: if small { size & 0xFFFF_FFFF } else { size },
                    })
                })
                .collect();
            self.entries = Some(entries);
        }
        Ok(self.entries.as_deref().unwrap_or(&[]))
    }

    fn mini_stream(&mut self, start: u32, size: u64) -> Result<Vec<u8>, CfbError> {
        let root_start = self.entries()?.first().and_then(|e| e.as_ref()).map(|e| e.start).ok_or(CfbError::Corrupt("root"))?;
        if self.minifat.is_none() {
            let raw = if self.n_minifat > 0 { self.chain(self.first_minifat, None)? } else { Vec::new() };
            self.minifat = Some(words(&raw));
        }
        let per_big = self.sector_size / self.mini_size;
        // The root's chain holds the mini stream: walk it once.
        let mut root_chain = Vec::new();
        let mut out = Vec::new();
        let mut sect = start;
        while sect <= MAXREGSECT && (out.len() as u64) < size {
            let (big, offset) = (sect as usize / per_big, sect as usize % per_big);
            while root_chain.len() <= big {
                let next = match root_chain.last() {
                    None => root_start,
                    Some(prev) => self.fat(*prev)?,
                };
                if next > MAXREGSECT || root_chain.len() > MAX_CHAIN {
                    return Err(CfbError::Corrupt("mini stream"));
                }
                root_chain.push(next);
            }
            let data = self.sector(root_chain[big])?;
            out.extend_from_slice(&data[offset * self.mini_size..(offset + 1) * self.mini_size]);
            sect = self.minifat.as_ref().and_then(|m| m.get(sect as usize).copied()).unwrap_or(ENDOFCHAIN);
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// The bytes of a stream, up to `max_bytes`; `None` when the file has no stream of that name.
    pub fn read_stream(&mut self, name: &str, max_bytes: u64) -> Result<Option<Vec<u8>>, CfbError> {
        let found = self.entries()?.iter().flatten().find(|e| e.kind == 2 && e.name == name).cloned();
        let Some(entry) = found else { return Ok(None) };
        let size = entry.size.min(max_bytes);
        let mut data = if entry.size < self.mini_cutoff { self.mini_stream(entry.start, size)? } else { self.chain(entry.start, Some(size))? };
        data.truncate(size as usize);
        Ok(Some(data))
    }
}

// ----------------------------------------------------------------------------------------------------
// The renderer a scene was made for
// ----------------------------------------------------------------------------------------------------

/// The renderer of a `.max` file, as far as its classes tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Corona,
    VRay,
    FStorm,
    Other,
}

impl Engine {
    /// The name used in settings files and caches.
    pub fn key(self) -> &'static str {
        match self {
            Self::Corona => "corona",
            Self::VRay => "vray",
            Self::FStorm => "fstorm",
            Self::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [Self::Corona, Self::VRay, Self::FStorm, Self::Other].into_iter().find(|e| e.key() == key)
    }
}

/// The renderers recognised, in the order that wins a tie, with the words that name each one.
const ENGINE_KEYS: [(Engine, &[&str]); 3] =
    [(Engine::Corona, &["corona"]), (Engine::VRay, &["vray", "v-ray", "v_ray"]), (Engine::FStorm, &["fstorm"])];

fn score(text: &str) -> [usize; 3] {
    let text = text.to_lowercase();
    ENGINE_KEYS.map(|(_, keys)| keys.iter().map(|k| text.matches(k).count()).sum())
}

fn winner(score: [usize; 3]) -> Option<Engine> {
    let best = *score.iter().max()?;
    if best == 0 {
        return None;
    }
    ENGINE_KEYS.iter().zip(score).find(|(_, n)| *n == best).map(|((engine, _), _)| *engine)
}

fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).filter(|t| t.chars().count() > 2)
}

fn utf16(data: &[u8]) -> String {
    let units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
    String::from_utf16_lossy(&units)
}

/// The renderer from the class list and the plug-in list of a scene. What the scene *uses* (materials,
/// lights, proxies) weighs more than the active renderer; renderers are only counted when nothing else tells.
pub fn engine_from_classes(classes: &str, dlls: &str) -> Engine {
    let mut usage = [0usize; 3];
    for token in tokens(classes) {
        let lower = token.to_lowercase();
        if lower.contains("render") && !lower.contains("element") {
            continue;
        }
        for (total, n) in usage.iter_mut().zip(score(&lower)) {
            *total += n;
        }
    }
    winner(usage).or_else(|| winner(score(&format!("{classes}{dlls}")))).unwrap_or(Engine::Other)
}

/// The renderer a `.max` file was made for; `Other` when it cannot be read.
pub fn detect_engine(path: &Path) -> Engine {
    let read = || -> Result<(Vec<u8>, Vec<u8>), CfbError> {
        let mut cf = CompoundFile::open(path)?;
        let classes = match cf.read_stream("ClassDirectory3", 8 << 20)? {
            Some(c) => c,
            None => cf.read_stream("ClassDirectory2", 8 << 20)?.unwrap_or_default(),
        };
        let dlls = cf.read_stream("DllDirectory", 8 << 20)?.unwrap_or_default();
        Ok((classes, dlls))
    };
    match read() {
        Ok((classes, dlls)) => engine_from_classes(&utf16(&classes), &utf16(&dlls)),
        Err(_) => Engine::Other,
    }
}

// ----------------------------------------------------------------------------------------------------
// The viewport capture 3ds Max saves with the file
// ----------------------------------------------------------------------------------------------------

const PID_THUMBNAIL: u32 = 17;
const VT_CF: u32 = 0x47;
const CF_DIB: u32 = 8;
const CF_METAFILEPICT: u32 = 3;

/// A DIB (BITMAPINFOHEADER, palette, pixels) as a whole `.bmp` file.
pub fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let header_size = u32_at(dib, 0)?;
    let width = i32_at(dib, 4)?;
    let height = i32_at(dib, 8)?;
    let bpp = u16_at(dib, 14)?;
    let compression = u32_at(dib, 16)?;
    if header_size < 40 || width <= 0 || height == 0 || ![1, 4, 8, 16, 24, 32].contains(&bpp) {
        return None;
    }
    let used = u32_at(dib, 32)?;
    let colors = if used > 0 {
        used
    } else if bpp <= 8 {
        1 << bpp
    } else {
        0
    };
    let masks = if compression == 3 && header_size == 40 { 12 } else { 0 };
    let offset = 14 + header_size + masks + colors * 4;
    let size = u32::try_from(14 + dib.len()).ok()?;
    let mut bmp = Vec::with_capacity(dib.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&size.to_le_bytes());
    bmp.extend_from_slice(&[0, 0, 0, 0]);
    bmp.extend_from_slice(&offset.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

/// The first DIB inside a Windows metafile (WMF).
fn dib_in_wmf(wmf: &[u8]) -> Option<&[u8]> {
    let mut pos = usize::from(u16_at(wmf, 2)?) * 2; // the header's size, in 16-bit words
    while pos + 6 <= wmf.len() {
        let size = u32_at(wmf, pos)? as usize;
        let function = u16_at(wmf, pos + 4)?;
        if size < 3 {
            break;
        }
        // The records that carry a DIB, and the bytes of parameters before it.
        let skip = match function {
            0x0F43 => Some(22), // StretchDIBits
            0x0B41 => Some(20), // DIBStretchBlt
            0x0940 => Some(16), // DIBBitBlt
            _ => None,
        };
        if let Some(skip) = skip {
            return wmf.get(pos + 6 + skip..(pos + size * 2).min(wmf.len()));
        }
        pos = pos.checked_add(size * 2)?;
    }
    None
}

/// The thumbnail property of a SummaryInformation stream, as a `.bmp` file.
pub fn thumbnail_from_summary(si: &[u8]) -> Option<Vec<u8>> {
    let section = u32_at(si, 44)? as usize;
    let count = u32_at(si, section + 4)?;
    for i in 0..count.min(256) as usize {
        let pid = u32_at(si, section + 8 + i * 8)?;
        if pid != PID_THUMBNAIL {
            continue;
        }
        let at = section + u32_at(si, section + 12 + i * 8)? as usize;
        let (vt, cb, format) = (u32_at(si, at)?, u32_at(si, at + 4)? as usize, i32_at(si, at + 8)?);
        if vt != VT_CF || format != -1 {
            return None; // -1: a Windows clipboard format follows
        }
        let clip = u32_at(si, at + 12)?;
        let data = si.get(at + 16..(at + 8 + cb).min(si.len()))?;
        return match clip {
            CF_DIB => dib_to_bmp(data),
            CF_METAFILEPICT => dib_to_bmp(dib_in_wmf(data.get(8..)?)?), // METAFILEPICT, then the WMF
            _ => None,
        };
    }
    None
}

/// The viewport capture 3ds Max saved inside a `.max` file, as a `.bmp` file; `None` if it has none.
pub fn embedded_thumbnail(path: &Path) -> Option<Vec<u8>> {
    let mut cf = CompoundFile::open(path).ok()?;
    let si = cf.read_stream("\u{5}SummaryInformation", 4 << 20).ok()??;
    thumbnail_from_summary(&si)
}

// ----------------------------------------------------------------------------------------------------
// External files saved in the scene
// ----------------------------------------------------------------------------------------------------

/// One external file of a scene: its kind (`Bitmap`, `Photometric`...), the path as saved and the path
/// 3ds Max had found when it saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub kind: String,
    pub stored: String,
    pub resolved: String,
}

fn read_counted(data: &[u8], at: usize) -> Option<(String, usize)> {
    let n = u32_at(data, at)? as usize;
    let start = at + 4;
    if n > 32_768 || start + 2 * n > data.len() {
        return None;
    }
    Some((utf16(&data[start..start + 2 * n]), start + 2 * n + 2))
}

/// The assets of a `FileAssetMetaData3` stream: for each, a GUID (16 bytes) and three UTF-16 strings with
/// their length (u32) before them and a null after.
pub fn assets_from_metadata(data: &[u8]) -> Vec<Asset> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 20 <= data.len() {
        let Some((kind, next)) = read_counted(data, at + 16) else { break };
        let Some((stored, next)) = read_counted(data, next) else { break };
        let Some((resolved, next)) = read_counted(data, next) else { break };
        if !stored.is_empty() {
            out.push(Asset { kind, stored, resolved });
        }
        at = next;
    }
    out
}

/// The external files of a `.max` file; empty when it cannot be read.
pub fn read_assets(path: &Path) -> Vec<Asset> {
    CompoundFile::open(path)
        .and_then(|mut cf| cf.read_stream("FileAssetMetaData3", 16 << 20))
        .ok()
        .flatten()
        .map(|data| assets_from_metadata(&data))
        .unwrap_or_default()
}

/// The renderer of each `.max` read so far, by its path, date and size, so that a network drive is not read
/// again. It lives in the app's data folder.
pub struct EngineCache {
    path: std::path::PathBuf,
    data: std::sync::Mutex<std::collections::HashMap<String, (u64, u64, Engine)>>,
    dirty: std::sync::atomic::AtomicBool,
}

fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let secs = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((secs, meta.len()))
}

impl EngineCache {
    pub fn load(path: &Path) -> Self {
        let data = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path: path.to_path_buf(), data: std::sync::Mutex::new(data), dirty: false.into() }
    }

    /// The renderer known for a file, if it has not changed since it was read.
    pub fn get(&self, file: &Path) -> Option<Engine> {
        let (secs, size) = file_stamp(file)?;
        let data = self.data.lock().ok()?;
        data.get(&crate::library::norm_key(file)).filter(|r| r.0 == secs && r.1 == size).map(|r| r.2)
    }

    /// The renderer of a file: known, or read now and kept.
    pub fn detect(&self, file: &Path) -> Engine {
        if let Some(engine) = self.get(file) {
            return engine;
        }
        let Some((secs, size)) = file_stamp(file) else { return Engine::Other };
        let engine = detect_engine(file);
        if let Ok(mut data) = self.data.lock() {
            data.insert(crate::library::norm_key(file), (secs, size, engine));
            self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        engine
    }

    /// Writes the cache if it learnt something: whole into a new file that then takes the old one's place.
    pub fn save(&self) -> std::io::Result<()> {
        if !self.dirty.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let bytes = {
            let data = self.data.lock().map_err(|_| std::io::Error::other("engine cache lock"))?;
            serde_json::to_vec(&*data).map_err(std::io::Error::other)?
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A writer of small compound files, so that the reader is tested without real scenes.

    const SECTOR: usize = 512;
    const MINI: usize = 64;

    fn put_u32(buf: &mut [u8], at: usize, v: u32) {
        buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn dir_entry(name: &str, kind: u8, start: u32, size: u32) -> [u8; 128] {
        let mut e = [0u8; 128];
        let units: Vec<u16> = name.encode_utf16().collect();
        for (i, u) in units.iter().enumerate() {
            e[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        e[64..66].copy_from_slice(&((units.len() as u16 + 1) * 2).to_le_bytes());
        e[66] = kind;
        put_u32(&mut e, 68, 0xFFFF_FFFF);
        put_u32(&mut e, 72, 0xFFFF_FFFF);
        put_u32(&mut e, 76, 0xFFFF_FFFF);
        put_u32(&mut e, 116, start);
        put_u32(&mut e, 120, size);
        e
    }

    /// A compound file with these streams: those under 4096 bytes go in the mini stream, as 3ds Max does.
    pub fn compound(streams: &[(&str, &[u8])]) -> Vec<u8> {
        let mut sectors: Vec<Vec<u8>> = Vec::new(); // after the header: FAT, directory, minifat, data
        let mut fat: Vec<u32> = Vec::new();
        let chain = |sectors: &mut Vec<Vec<u8>>, fat: &mut Vec<u32>, data: &[u8]| -> u32 {
            let first = sectors.len() as u32;
            let n = data.len().div_ceil(SECTOR).max(1);
            for i in 0..n {
                let mut s = vec![0u8; SECTOR];
                let end = ((i + 1) * SECTOR).min(data.len());
                if i * SECTOR < end {
                    s[..end - i * SECTOR].copy_from_slice(&data[i * SECTOR..end]);
                }
                sectors.push(s);
                fat.push(if i + 1 == n { 0xFFFF_FFFE } else { first + i as u32 + 1 });
            }
            first
        };
        sectors.push(vec![0u8; SECTOR]); // the FAT, written last
        fat.push(0xFFFF_FFFD);
        let mut mini_data = Vec::new();
        let mut minifat: Vec<u32> = Vec::new();
        let mut entries = vec![];
        let mut regular = vec![];
        for (name, data) in streams {
            if data.len() < 4096 {
                let first = (mini_data.len() / MINI) as u32;
                let n = data.len().div_ceil(MINI).max(1);
                for i in 0..n {
                    minifat.push(if i + 1 == n { 0xFFFF_FFFE } else { first + i as u32 + 1 });
                }
                mini_data.extend_from_slice(data);
                mini_data.resize((first as usize + n) * MINI, 0);
                entries.push((name.to_string(), first, data.len()));
            } else {
                regular.push((name.to_string(), data.to_vec()));
            }
        }
        let dir_first = sectors.len() as u32;
        let n_entries = 1 + entries.len() + regular.len();
        let dir_sectors = (n_entries * 128).div_ceil(SECTOR);
        for i in 0..dir_sectors {
            sectors.push(vec![0u8; SECTOR]);
            fat.push(if i + 1 == dir_sectors { 0xFFFF_FFFE } else { dir_first + i as u32 + 1 });
        }
        let mut minifat_bytes = Vec::new();
        for v in &minifat {
            minifat_bytes.extend_from_slice(&v.to_le_bytes());
        }
        let minifat_first = if minifat.is_empty() { 0xFFFF_FFFE } else { chain(&mut sectors, &mut fat, &minifat_bytes) };
        let mini_first = if mini_data.is_empty() { 0xFFFF_FFFE } else { chain(&mut sectors, &mut fat, &mini_data) };
        let mut dir = Vec::new();
        dir.extend_from_slice(&dir_entry("Root Entry", 5, mini_first, mini_data.len() as u32));
        for (name, first, size) in &entries {
            dir.extend_from_slice(&dir_entry(name, 2, *first, *size as u32));
        }
        for (name, data) in &regular {
            let first = chain(&mut sectors, &mut fat, data);
            dir.extend_from_slice(&dir_entry(name, 2, first, data.len() as u32));
        }
        for (i, chunk) in dir.chunks(SECTOR).enumerate() {
            sectors[dir_first as usize + i][..chunk.len()].copy_from_slice(chunk);
        }
        assert!(fat.len() <= SECTOR / 4, "the test writer keeps to one FAT sector");
        fat.resize(SECTOR / 4, 0xFFFF_FFFF);
        for (i, v) in fat.iter().enumerate() {
            put_u32(&mut sectors[0], i * 4, *v);
        }
        let mut header = vec![0u8; SECTOR];
        header[..8].copy_from_slice(&super::SIGNATURE);
        header[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
        header[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
        header[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
        header[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        put_u32(&mut header, 0x2C, 1);
        put_u32(&mut header, 0x30, dir_first);
        put_u32(&mut header, 0x38, 4096);
        put_u32(&mut header, 0x3C, minifat_first);
        put_u32(&mut header, 0x40, minifat_bytes.len().div_ceil(SECTOR) as u32);
        put_u32(&mut header, 0x44, 0xFFFF_FFFE);
        put_u32(&mut header, 0x4C, 0);
        for i in 1..109 {
            put_u32(&mut header, 0x4C + i * 4, 0xFFFF_FFFF);
        }
        let mut out = header;
        for s in sectors {
            out.extend_from_slice(&s);
        }
        out
    }

    pub fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{compound, utf16};
    use super::*;
    use std::io::Cursor;

    fn open(bytes: Vec<u8>) -> CompoundFile<Cursor<Vec<u8>>> {
        CompoundFile::new(Cursor::new(bytes)).expect("a valid compound file")
    }

    #[test]
    fn small_and_large_streams_read_back() {
        let small: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
        let large: Vec<u8> = (0..9000u32).map(|i| (i % 253) as u8).collect();
        let mut cf = open(compound(&[("Small", &small), ("Large", &large)]));
        assert_eq!(cf.read_stream("Small", u64::MAX).unwrap(), Some(small));
        assert_eq!(cf.read_stream("Large", u64::MAX).unwrap(), Some(large.clone()));
        assert_eq!(cf.read_stream("Large", 100).unwrap(), Some(large[..100].to_vec()));
        assert_eq!(cf.read_stream("Missing", u64::MAX).unwrap(), None);
    }

    #[test]
    fn a_file_that_is_not_compound_is_refused() {
        assert!(matches!(CompoundFile::new(Cursor::new(b"not a max file".to_vec())), Err(CfbError::NotCompound)));
        assert!(matches!(CompoundFile::new(Cursor::new(vec![0u8; 600])), Err(CfbError::NotCompound)));
    }

    #[test]
    fn the_engine_is_what_the_scene_uses() {
        assert_eq!(engine_from_classes("CoronaPhysicalMtl CoronaBitmap VRayRenderer", ""), Engine::Corona);
        assert_eq!(engine_from_classes("VRayMtl VRayBitmap CoronaRenderer", ""), Engine::VRay);
        assert_eq!(engine_from_classes("FStormMtl", ""), Engine::FStorm);
        // only a renderer: it decides
        assert_eq!(engine_from_classes("CoronaRenderer Standard", ""), Engine::Corona);
        // a tie: Corona first
        assert_eq!(engine_from_classes("CoronaMtl VRayMtl", ""), Engine::Corona);
        assert_eq!(engine_from_classes("Standard Box", "maxscrpt.dlx"), Engine::Other);
        // the plug-in list counts when the classes say nothing
        assert_eq!(engine_from_classes("Box", "V-Ray 7 for 3ds Max"), Engine::VRay);
    }

    #[test]
    fn the_engine_is_read_from_a_file() {
        let dir = std::env::temp_dir().join(format!("openlibrary-maxfile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chair.max");
        let classes = utf16("VRayMtl\0Bitmaptexture\0CoronaRenderer\0");
        std::fs::write(&path, compound(&[("ClassDirectory3", &classes), ("DllDirectory", &[])])).unwrap();
        assert_eq!(detect_engine(&path), Engine::VRay);
        std::fs::write(&path, b"broken").unwrap();
        assert_eq!(detect_engine(&path), Engine::Other);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn dib(width: i32, height: i32) -> Vec<u8> {
        let row = (width as usize * 3).div_ceil(4) * 4;
        let mut d = 40u32.to_le_bytes().to_vec();
        d.extend_from_slice(&width.to_le_bytes());
        d.extend_from_slice(&height.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        for v in [0u32, (row * height as usize) as u32, 2835, 2835, 0, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.resize(40 + row * height as usize, 0x80);
        d
    }

    fn wmf(dib: &[u8]) -> Vec<u8> {
        let mut params = Vec::new();
        params.extend_from_slice(&0x00CC_0020u32.to_le_bytes());
        params.extend_from_slice(&[0u8; 18]); // usage, sizes and positions: 22 bytes in all
        let record_words = 3 + (params.len() + dib.len()) / 2;
        let mut w = Vec::new();
        for v in [1u16, 9, 0x300] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        w.extend_from_slice(&0u32.to_le_bytes());
        w.extend_from_slice(&0u16.to_le_bytes());
        w.extend_from_slice(&(record_words as u32).to_le_bytes());
        w.extend_from_slice(&0u16.to_le_bytes());
        w.extend_from_slice(&(record_words as u32).to_le_bytes());
        w.extend_from_slice(&0x0F43u16.to_le_bytes());
        w.extend_from_slice(&params);
        w.extend_from_slice(dib);
        w.extend_from_slice(&3u32.to_le_bytes());
        w.extend_from_slice(&0u16.to_le_bytes());
        w
    }

    /// A SummaryInformation stream with one property, the thumbnail (VT_CF).
    fn summary(clip_format: u32, clip: &[u8]) -> Vec<u8> {
        let mut value = Vec::new();
        value.extend_from_slice(&VT_CF.to_le_bytes());
        value.extend_from_slice(&((8 + clip.len()) as u32).to_le_bytes());
        value.extend_from_slice(&(-1i32).to_le_bytes());
        value.extend_from_slice(&clip_format.to_le_bytes());
        value.extend_from_slice(clip);
        let mut si = vec![0u8; 48];
        si[..2].copy_from_slice(&0xFFFEu16.to_le_bytes());
        si[44..48].copy_from_slice(&48u32.to_le_bytes());
        for v in [(16 + value.len()) as u32, 1, PID_THUMBNAIL, 16] {
            si.extend_from_slice(&v.to_le_bytes());
        }
        si.extend_from_slice(&value);
        si
    }

    fn check_bmp(bmp: &[u8], width: i32, height: i32) {
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32_at(bmp, 2), Some(bmp.len() as u32));
        assert_eq!(u32_at(bmp, 10), Some(14 + 40)); // 24 bits: no palette
        assert_eq!((i32_at(bmp, 18), i32_at(bmp, 22)), (Some(width), Some(height)));
    }

    #[test]
    fn the_thumbnail_is_found_in_a_metafile_or_a_dib() {
        let mut clip = vec![1, 0, 0, 2, 0x24, 1, 0, 0]; // METAFILEPICT
        clip.extend_from_slice(&wmf(&dib(2, 2)));
        check_bmp(&thumbnail_from_summary(&summary(CF_METAFILEPICT, &clip)).unwrap(), 2, 2);
        check_bmp(&thumbnail_from_summary(&summary(CF_DIB, &dib(3, 1))).unwrap(), 3, 1);
    }

    #[test]
    fn no_thumbnail_is_none() {
        assert_eq!(thumbnail_from_summary(&[]), None);
        assert_eq!(thumbnail_from_summary(&summary(2, &[0u8; 40])), None); // a format that is not an image
        assert_eq!(thumbnail_from_summary(&summary(CF_DIB, &[0u8; 10])), None); // too short
    }

    #[test]
    fn the_thumbnail_is_read_from_a_file() {
        let dir = std::env::temp_dir().join(format!("openlibrary-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lamp.max");
        std::fs::write(&path, compound(&[("\u{5}SummaryInformation", &summary(CF_DIB, &dib(4, 3)))])).unwrap();
        check_bmp(&embedded_thumbnail(&path).unwrap(), 4, 3);
        std::fs::write(&path, compound(&[("Scene", &[1, 2, 3])])).unwrap();
        assert_eq!(embedded_thumbnail(&path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn counted(text: &str) -> Vec<u8> {
        let units = utf16(text);
        let mut out = ((units.len() / 2) as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&units);
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn external_files_are_listed() {
        let mut data = Vec::new();
        for (kind, stored, resolved) in [("Bitmap", "maps\\wood.jpg", "X:\\lib\\maps\\wood.jpg"), ("Bitmap", "", ""), ("Photometric", "a.ies", "")] {
            data.extend_from_slice(&[7u8; 16]);
            data.extend_from_slice(&counted(kind));
            data.extend_from_slice(&counted(stored));
            data.extend_from_slice(&counted(resolved));
        }
        data.extend_from_slice(&[1, 2, 3]); // a broken tail is ignored
        let assets = assets_from_metadata(&data);
        assert_eq!(
            assets,
            vec![
                Asset { kind: "Bitmap".into(), stored: "maps\\wood.jpg".into(), resolved: "X:\\lib\\maps\\wood.jpg".into() },
                Asset { kind: "Photometric".into(), stored: "a.ies".into(), resolved: String::new() },
            ]
        );
    }

    #[test]
    fn the_engine_cache_remembers_until_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("openlibrary-enginecache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sofa.max");
        std::fs::write(&file, compound(&[("ClassDirectory3", &utf16("CoronaPhysicalMtl"))])).unwrap();
        let cache_path = dir.join("engines.json");
        let cache = EngineCache::load(&cache_path);
        assert_eq!(cache.get(&file), None);
        assert_eq!(cache.detect(&file), Engine::Corona);
        cache.save().unwrap();
        let again = EngineCache::load(&cache_path);
        assert_eq!(again.get(&file), Some(Engine::Corona));
        std::fs::write(&file, compound(&[("ClassDirectory3", &utf16("VRayMtl")), ("Scene", &[0u8; 5000])])).unwrap();
        assert_eq!(again.get(&file), None); // another size: read again
        assert_eq!(again.detect(&file), Engine::VRay);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_engine_keys_round_trip() {
        for engine in [Engine::Corona, Engine::VRay, Engine::FStorm, Engine::Other] {
            assert_eq!(Engine::from_key(engine.key()), Some(engine));
        }
        assert_eq!(Engine::from_key("arnold"), None);
    }
}
