// SPDX-License-Identifier: GPL-3.0-or-later
//! Thumbnails, loaded in the background, with a cache on this computer.
//!
//! The previews live in the library, often on a network drive; a small copy of each goes to the cache
//! (`thumbs` in the app's data folder), so that browsing is instant the next time. A model without a preview
//! shows the viewport capture 3ds Max saved in its `.max`. The last thumbnail asked for is loaded first: it is
//! what is on screen now, not what was scrolled past.

use crate::maxfile;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::UNIX_EPOCH;

/// The longest side of a thumbnail, in pixels.
pub const THUMB_PX: u32 = 320;
/// How many thumbnails a tab keeps in memory.
pub const IN_MEMORY: usize = 1500;
/// The grey behind a preview with transparency (a material ball cut out in a PNG).
const BACKDROP: Rgba<u8> = Rgba([58, 61, 68, 255]);

/// The cache file of a source: it changes when the source changes (its date or its size).
pub fn cache_file(cache_dir: &Path, src: &Path) -> PathBuf {
    let stamp = fs::metadata(src).ok().map(|m| {
        let secs = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        format!("|{secs}|{}", m.len())
    });
    let key = format!("{}{}", crate::library::norm_key(src), stamp.unwrap_or_default());
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    cache_dir.join(format!("{hash:016x}.jpg"))
}

/// Decodes an image; one that breaks its decoder has no thumbnail, and the threads go on.
fn decode(src: &Path) -> Option<DynamicImage> {
    std::panic::catch_unwind(|| decode_file(src)).ok().flatten()
}

fn decode_file(src: &Path) -> Option<DynamicImage> {
    let is_max = src.extension().is_some_and(|e| e.eq_ignore_ascii_case(crate::library::MAX_EXT));
    if is_max {
        let bmp = maxfile::embedded_thumbnail(src)?;
        return image::load_from_memory_with_format(&bmp, ImageFormat::Bmp).ok();
    }
    image::ImageReader::open(src).ok()?.with_guessed_format().ok()?.decode().ok()
}

/// A thumbnail of `src`: from the cache when it is there, else made and written to the cache. `None` when the
/// source cannot be read, or is a `.max` without a viewport capture.
pub fn load(src: &Path, cache_dir: &Path) -> Option<RgbaImage> {
    let cached = cache_file(cache_dir, src);
    if let Ok(img) = image::open(&cached) {
        return Some(img.to_rgba8());
    }
    let img = decode(src)?;
    let img = if img.width().max(img.height()) > THUMB_PX { img.thumbnail(THUMB_PX, THUMB_PX) } else { img };
    let mut rgba = img.to_rgba8();
    if rgba.pixels().any(|p| p.0[3] < 255) {
        let mut flat = RgbaImage::from_pixel(rgba.width(), rgba.height(), BACKDROP);
        image::imageops::overlay(&mut flat, &rgba, 0, 0);
        rgba = flat;
    }
    if fs::create_dir_all(cache_dir).is_ok() {
        // the cache is a convenience: when it cannot be written, the thumbnail is still shown
        let _ = DynamicImage::ImageRgba8(rgba.clone()).to_rgb8().save_with_format(&cached, ImageFormat::Jpeg);
    }
    Some(rgba)
}

/// The image of a source, no larger than `side`; not cached.
pub fn load_large(src: &Path, side: u32) -> Option<RgbaImage> {
    let img = decode(src)?;
    Some(if img.width().max(img.height()) > side { img.thumbnail(side, side) } else { img }.to_rgba8())
}

/// A thumbnail made by the loader: `None` when there is none to show.
pub struct Loaded {
    pub key: String,
    pub image: Option<RgbaImage>,
}

struct Queue {
    jobs: Vec<(String, PathBuf)>,
    open: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

/// Loads thumbnails on a few threads of its own; the window collects them with [`ThumbLoader::poll`].
pub struct ThumbLoader {
    queue: Shared,
    results: Receiver<Loaded>,
    pending: HashSet<String>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl ThumbLoader {
    /// `wake` is called from a loader thread after each thumbnail, so that the window draws it.
    pub fn new(cache_dir: PathBuf, threads: usize, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let queue: Shared = Arc::new((Mutex::new(Queue { jobs: Vec::new(), open: true }), Condvar::new()));
        let (send, results) = channel::<Loaded>();
        let threads = (0..threads.max(1))
            .map(|_| {
                let (queue, send, cache_dir, wake) = (queue.clone(), send.clone(), cache_dir.clone(), wake.clone());
                std::thread::spawn(move || work(&queue, &send, &cache_dir, wake.as_ref()))
            })
            .collect();
        Self { queue, results, pending: HashSet::new(), threads }
    }

    /// Asks for a thumbnail; it comes later through [`Self::poll`]. A key already asked for is not asked again.
    pub fn request(&mut self, key: &str, src: &Path) {
        if !self.pending.insert(key.to_string()) {
            return;
        }
        let (lock, wake) = &*self.queue;
        if let Ok(mut queue) = lock.lock() {
            queue.jobs.push((key.to_string(), src.to_path_buf()));
            wake.notify_one();
        }
    }

    /// Forgets what was asked and not yet started (after a change of folder, say).
    pub fn clear_queue(&mut self) {
        if let Ok(mut queue) = self.queue.0.lock() {
            for (key, _) in queue.jobs.drain(..) {
                self.pending.remove(&key);
            }
        }
    }

    /// The thumbnails made since the last call.
    pub fn poll(&mut self) -> Vec<Loaded> {
        let out: Vec<Loaded> = self.results.try_iter().collect();
        for loaded in &out {
            self.pending.remove(&loaded.key);
        }
        out
    }
}

impl Drop for ThumbLoader {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.queue.0.lock() {
            queue.open = false;
            queue.jobs.clear();
        }
        self.queue.1.notify_all();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn work(queue: &Shared, send: &Sender<Loaded>, cache_dir: &Path, wake: &(dyn Fn() + Send + Sync)) {
    let (lock, ready) = &**queue;
    loop {
        let job = {
            let Ok(mut q) = lock.lock() else { return };
            while q.open && q.jobs.is_empty() {
                q = match ready.wait(q) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
            if !q.open {
                return;
            }
            q.jobs.pop() // the last asked, first
        };
        let Some((key, src)) = job else { continue };
        let image = load(&src, cache_dir);
        if send.send(Loaded { key, image }).is_err() {
            return;
        }
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("openlibrary-thumbs-{name}-{}", std::process::id()));
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
    fn a_preview_is_reduced_and_cached() {
        let dir = Scratch::new("preview");
        let src = dir.0.join("Chair.png");
        RgbaImage::from_pixel(800, 400, Rgba([200, 10, 10, 255])).save(&src).unwrap();
        let cache = dir.0.join("cache");
        let thumb = load(&src, &cache).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (320, 160));
        assert!(cache_file(&cache, &src).is_file());
        assert_eq!(load(&src, &cache).unwrap().dimensions(), (320, 160)); // from the cache
    }

    #[test]
    fn transparency_goes_on_grey() {
        let dir = Scratch::new("alpha");
        let src = dir.0.join("Ball.png");
        RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0])).save(&src).unwrap();
        let thumb = load(&src, &dir.0.join("cache")).unwrap();
        assert_eq!(thumb.get_pixel(5, 5), &BACKDROP);
    }

    #[test]
    fn a_max_without_preview_gives_its_viewport_capture() {
        let dir = Scratch::new("max");
        // a 2x2 BMP made by the image crate, its DIB put in a SummaryInformation as 3ds Max does
        let mut bmp = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([0, 200, 0])))
            .write_to(&mut std::io::Cursor::new(&mut bmp), ImageFormat::Bmp)
            .unwrap();
        let dib = &bmp[14..];
        let mut si = vec![0u8; 48];
        si[44..48].copy_from_slice(&48u32.to_le_bytes());
        for v in [(32 + dib.len()) as u32, 1, 17, 16, 0x47, (8 + dib.len()) as u32, u32::MAX, 8] {
            si.extend_from_slice(&v.to_le_bytes());
        }
        si.extend_from_slice(dib);
        let src = dir.0.join("Lamp.max");
        fs::write(&src, maxfile::testing::compound(&[("\u{5}SummaryInformation", &si)])).unwrap();
        let thumb = load(&src, &dir.0.join("cache")).unwrap();
        assert_eq!(thumb.dimensions(), (2, 2));
        assert!(thumb.get_pixel(0, 0).0[1] > 150);
        let bare = dir.0.join("Bare.max");
        fs::write(&bare, maxfile::testing::compound(&[("Scene", &[1])])).unwrap();
        assert!(load(&bare, &dir.0.join("cache")).is_none());
    }

    #[test]
    fn the_cache_file_changes_with_the_source() {
        let dir = Scratch::new("stamp");
        let src = dir.0.join("a.jpg");
        fs::write(&src, b"1").unwrap();
        let first = cache_file(&dir.0, &src);
        fs::write(&src, b"22").unwrap();
        assert_ne!(first, cache_file(&dir.0, &src));
    }

    #[test]
    fn the_loader_sends_every_thumbnail_once() {
        let dir = Scratch::new("loader");
        for name in ["a", "b", "c"] {
            RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255])).save(dir.0.join(format!("{name}.png"))).unwrap();
        }
        let mut loader = ThumbLoader::new(dir.0.join("cache"), 2, Arc::new(|| {}));
        for name in ["a", "b", "c", "a"] {
            loader.request(name, &dir.0.join(format!("{name}.png")));
        }
        loader.request("missing", &dir.0.join("missing.png"));
        let mut got = Vec::new();
        let start = Instant::now();
        while got.len() < 4 && start.elapsed() < Duration::from_secs(10) {
            got.extend(loader.poll().into_iter().map(|l| (l.key, l.image.is_some())));
            std::thread::sleep(Duration::from_millis(10));
        }
        got.sort();
        assert_eq!(got, [("a".into(), true), ("b".into(), true), ("c".into(), true), ("missing".into(), false)]);
    }
}
