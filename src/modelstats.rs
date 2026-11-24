// SPDX-License-Identifier: GPL-3.0-or-later
//! What a model has: faces, vertices, objects, displacement and animation. 3ds Max reads it in the background
//! (`olib_sceneStats`), and it is kept in `model_stats.json` of the data folder by each file's date and size.

use crate::library::norm_key;
use crate::maxbatch::Job;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub polys: u64,
    pub verts: u64,
    pub objects: u64,
    pub disp: bool,
    pub anim: bool,
    pub start: i64,
    pub end: i64,
}

/// The fields `olib_sceneStats` writes; `None` when they are not that.
pub fn parse(fields: &[String]) -> Option<Stats> {
    let [polys, verts, objects, disp, anim, start, end, ..] = fields else { return None };
    Some(Stats {
        polys: polys.parse().ok()?,
        verts: verts.parse().ok()?,
        objects: objects.parse().ok()?,
        disp: disp == "1",
        anim: anim == "1",
        start: start.parse().ok()?,
        end: end.parse().ok()?,
    })
}

/// A number with a thin space every three digits: 1 234 567.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

/// The job that reads these files.
pub fn job(files: &[PathBuf]) -> Job {
    Job {
        function: "olib_batchStats",
        arguments: Vec::new(),
        lines: files.iter().map(|f| vec![f.display().to_string()]).collect(),
        stall: Duration::from_secs(150),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    date: u64,
    size: u64,
    /// `None` when 3ds Max could not read it.
    stats: Option<Stats>,
}

fn stamp(file: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(file).ok()?;
    let date = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((date, meta.len()))
}

/// The stats read so far.
pub struct StatsCache {
    path: PathBuf,
    data: HashMap<String, Record>,
}

impl StatsCache {
    pub fn load(path: &Path) -> Self {
        let data = std::fs::read(path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        Self { path: path.to_path_buf(), data }
    }

    /// The stats last read, without asking the disk whether the file has changed.
    pub fn peek(&self, file: &Path) -> Option<Option<&Stats>> {
        self.data.get(&norm_key(file)).map(|r| r.stats.as_ref())
    }

    /// `None`: not read since the file changed. `Some(None)`: 3ds Max could not read it.
    pub fn get(&self, file: &Path) -> Option<Option<&Stats>> {
        let record = self.data.get(&norm_key(file))?;
        (stamp(file)? == (record.date, record.size)).then_some(record.stats.as_ref())
    }

    pub fn put(&mut self, file: &Path, stats: Option<Stats>) {
        if let Some((date, size)) = stamp(file) {
            self.data.insert(norm_key(file), Record { date, size, stats });
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(&self.data).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str) -> Vec<String> {
        text.split('\t').map(str::to_string).collect()
    }

    #[test]
    fn the_fields_of_3ds_max_are_read() {
        let stats = parse(&fields("1200\t640\t3\t1\t0\t0\t100")).unwrap();
        assert_eq!((stats.polys, stats.verts, stats.objects, stats.disp, stats.anim, stats.end), (1200, 640, 3, true, false, 100));
        assert_eq!(parse(&fields("1200\t640")), None);
        assert_eq!(parse(&fields("a\tb\tc\t1\t0\t0\t100")), None);
    }

    #[test]
    fn numbers_are_easy_to_read() {
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(1_234_567), "1\u{202f}234\u{202f}567");
    }

    #[test]
    fn a_changed_file_is_read_again() {
        let dir = std::env::temp_dir().join(format!("openlibrary-stats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (file, cache) = (dir.join("a.max"), dir.join("model_stats.json"));
        std::fs::write(&file, b"one").unwrap();
        let mut stats = StatsCache::load(&cache);
        assert_eq!(stats.get(&file), None);
        stats.put(&file, parse(&fields("1\t2\t3\t0\t0\t0\t1")));
        stats.save().unwrap();
        let mut stats = StatsCache::load(&cache);
        assert_eq!(stats.get(&file).unwrap().unwrap().verts, 2);
        std::fs::write(&file, b"longer").unwrap();
        assert_eq!(stats.get(&file), None);
        stats.put(&file, None);
        assert_eq!(stats.get(&file), Some(None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
