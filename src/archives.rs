// SPDX-License-Identifier: GPL-3.0-or-later
//! Opens a download (zip, rar, 7z...) with what the computer has: 7-Zip or NanaZip, WinRAR's UnRAR, or the `tar`
//! of Windows, which reads zip, and rar and 7z without a password. No unpacker travels with the app.
//!
//! The programs are run by their whole path, with their arguments apart (no command line is built), and they
//! extract into a new folder of the app. All of them refuse, by themselves, a name that leaves that folder.

use crate::maxbatch::quiet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The extensions of the file dialog.
pub const EXTENSIONS: [&str; 11] = ["zip", "rar", "7z", "001", "r00", "z01", "tar", "gz", "tgz", "bz2", "xz"];
/// What a download may take once extracted: more is no model, and would fill the disk.
const MAX_BYTES: u64 = 60 << 30;
/// 7-Zip asks on the keyboard when it is given no password.
const NO_PASSWORD: &str = "-no-password-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveError {
    /// It has a password; `wrong` when one was tried.
    Password {
        wrong: bool,
    },
    /// 7-Zip (or WinRAR) is needed for this file.
    NoTool,
    /// A part is missing, or the download was cut short.
    Incomplete,
    Cancelled,
    /// It grows past [`MAX_BYTES`].
    TooLarge,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    SevenZip,
    UnRar,
    Tar,
}

/// A download in parts: the one to open, all of them, and the names of those missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volumes {
    pub first: PathBuf,
    pub parts: Vec<PathBuf>,
    pub missing: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scheme {
    /// `name.part01.rar`
    Part,
    /// `name.7z.001`
    Numbered,
    /// `name.rar`, `name.r00`...: the `.rar` first
    OldRar,
    /// `name.z01`... and `name.zip`, the one to open
    OldZip,
}

/// The base of a name in a scheme, and its number with the digits it has (`None` for the `.rar` or `.zip` of the old ones).
fn split(scheme: Scheme, name: &str) -> Option<(&str, Option<(u32, usize)>)> {
    let (rest, ext) = name.rsplit_once('.')?;
    let number = |digits: &str| -> Option<(u32, usize)> {
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then(|| Some((digits.parse().ok()?, digits.len())))?
    };
    let low = ext.to_ascii_lowercase();
    match scheme {
        Scheme::Part => {
            let (base, part) = rest.rsplit_once('.')?;
            let digits = part.get(..4).filter(|p| p.eq_ignore_ascii_case("part")).and_then(|_| part.get(4..))?;
            (low == "rar").then_some(())?;
            Some((base, Some(number(digits)?)))
        }
        Scheme::Numbered => {
            let kind = rest.rsplit_once('.')?.1.to_ascii_lowercase();
            (ext.len() >= 3 && ["7z", "zip", "rar", "tar"].contains(&kind.as_str())).then_some(())?;
            Some((rest, Some(number(ext)?)))
        }
        Scheme::OldRar | Scheme::OldZip => {
            let (whole, letter) = if scheme == Scheme::OldRar { ("rar", 'r') } else { ("zip", 'z') };
            if low == whole {
                return Some((rest, None));
            }
            let digits = low.strip_prefix(letter).filter(|d| d.len() == 2)?;
            Some((rest, Some(number(digits)?)))
        }
    }
}

fn part_name(scheme: Scheme, base: &str, n: u32, width: usize) -> String {
    match scheme {
        Scheme::Part => format!("{base}.part{n:0width$}.rar"),
        Scheme::Numbered => format!("{base}.{n:0width$}"),
        Scheme::OldRar => format!("{base}.r{n:02}"),
        Scheme::OldZip => format!("{base}.z{n:02}"),
    }
}

/// The parts of a download, whichever of them was chosen; `names` is what its folder holds. A file that is
/// whole gives itself.
pub fn volumes(path: &Path, names: &[String]) -> Volumes {
    let folder = path.parent().unwrap_or(Path::new(""));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    for scheme in [Scheme::Part, Scheme::Numbered, Scheme::OldRar, Scheme::OldZip] {
        let Some((base, number)) = split(scheme, &name) else { continue };
        let width = number.map_or(2, |(_, w)| w);
        let mut found: Vec<(u32, &String)> = names
            .iter()
            .filter_map(|n| {
                let (b, number) = split(scheme, n)?;
                (b.to_lowercase() == base.to_lowercase()).then_some((number?.0, n))
            })
            .collect();
        found.sort();
        found.dedup_by_key(|(n, _)| *n);
        let old = matches!(scheme, Scheme::OldRar | Scheme::OldZip);
        if found.is_empty() && old {
            continue; // a .rar or a .zip that is whole
        }
        let start = u32::from(scheme != Scheme::OldRar);
        let last = found.last().map_or(0, |(n, _)| *n);
        let mut missing: Vec<String> =
            (start..=last).filter(|n| !found.iter().any(|(f, _)| f == n)).map(|n| part_name(scheme, base, n, width)).collect();
        let mut parts: Vec<PathBuf> = found.iter().map(|(_, n)| folder.join(n)).collect();
        let first = if old {
            let whole = format!("{base}.{}", if scheme == Scheme::OldRar { "rar" } else { "zip" });
            match names.iter().find(|n| n.to_lowercase() == whole.to_lowercase()) {
                Some(n) if scheme == Scheme::OldRar => parts.insert(0, folder.join(n)),
                Some(n) => parts.push(folder.join(n)),
                None => missing.insert(0, whole.clone()),
            }
            folder.join(names.iter().find(|n| n.to_lowercase() == whole.to_lowercase()).unwrap_or(&whole))
        } else {
            match found.first() {
                Some((n, name)) if *n == start => folder.join(name),
                _ => folder.join(part_name(scheme, base, start, width)),
            }
        };
        return Volumes { first, parts, missing };
    }
    Volumes { first: path.to_path_buf(), parts: vec![path.to_path_buf()], missing: Vec::new() }
}

/// The name of a download without its extension nor its part, to propose as the model's.
pub fn display_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some((base, _)) = split(Scheme::Part, &name) {
        return base.to_string();
    }
    let name = split(Scheme::Numbered, &name).map_or(name.as_str(), |(base, _)| base);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    stem.strip_suffix(".tar").unwrap_or(stem).to_string()
}

/// The best unpacker there is.
pub fn find_tool() -> Option<(Kind, PathBuf)> {
    let dirs: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    let mut seven: Vec<PathBuf> = Vec::new();
    for dir in &dirs {
        for rel in [r"7-Zip\7z.exe", r"7-Zip-Zstandard\7z.exe", r"NanaZip\NanaZipC.exe", r"PeaZip\res\bin\7z\7z.exe"] {
            seven.push(dir.join(rel));
        }
    }
    // NanaZip from the Microsoft Store runs through an alias, which is not a plain file
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        seven.push(PathBuf::from(local).join(r"Microsoft\WindowsApps\NanaZipC.exe"));
    }
    if let Some(exe) = seven.into_iter().find(|p| std::fs::symlink_metadata(p).is_ok()) {
        return Some((Kind::SevenZip, exe));
    }
    if let Some(exe) = dirs.iter().map(|d| d.join(r"WinRAR\UnRAR.exe")).find(|p| p.is_file()) {
        return Some((Kind::UnRar, exe));
    }
    let tar = crate::platform::system_program("tar.exe");
    tar.is_file().then_some((Kind::Tar, tar))
}

fn arguments(kind: Kind, path: &Path, dest: &Path, password: Option<&str>) -> Vec<std::ffi::OsString> {
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    match kind {
        Kind::SevenZip => {
            out.extend(["x", "-y", "-bd", "-sccUTF-8"].map(Into::into));
            let mut to = std::ffi::OsString::from("-o");
            to.push(dest);
            out.push(to);
            out.push(format!("-p{}", password.unwrap_or(NO_PASSWORD)).into());
            out.push("--".into());
            out.push(path.into());
        }
        Kind::UnRar => {
            out.extend(["x", "-y", "-o+"].map(Into::into));
            out.push(format!("-p{}", password.unwrap_or("-")).into());
            out.push("--".into());
            out.push(path.into());
            let mut to = dest.as_os_str().to_os_string();
            to.push("\\");
            out.push(to);
        }
        Kind::Tar => {
            out.push("-xf".into());
            out.push(path.into());
            out.push("-C".into());
            out.push(dest.into());
        }
    }
    out
}

fn is_password(text: &str) -> bool {
    let low = text.to_lowercase();
    ["wrong password", "encrypted", "password is incorrect", "incorrect password", "passphrase"].iter().any(|w| low.contains(w))
}

/// What an unpacker's answer means.
fn verdict(kind: Kind, code: Option<i32>, output: &str, password: bool) -> Result<(), ArchiveError> {
    let wrong = ArchiveError::Password { wrong: password };
    let low = output.to_lowercase();
    match (kind, code) {
        (_, Some(0)) | (Kind::SevenZip, Some(1)) => Ok(()), // 7-Zip's 1 is a warning
        (Kind::UnRar, Some(11)) => Err(wrong),
        // the tar of Windows cannot be given a password
        (Kind::Tar, _) => Err(ArchiveError::NoTool),
        _ if is_password(output) => Err(wrong),
        _ if low.contains("missing volume") || low.contains("unexpected end") || low.contains("cannot find volume") => Err(ArchiveError::Incomplete),
        _ => Err(ArchiveError::Failed(last_errors(output))),
    }
}

fn last_errors(output: &str) -> String {
    let lines: Vec<&str> = output.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let errors: Vec<&str> = lines.iter().copied().filter(|l| ["error", "cannot", "can not"].iter().any(|w| l.to_lowercase().contains(w))).collect();
    let chosen = if errors.is_empty() { lines } else { errors };
    chosen[chosen.len().saturating_sub(4)..].join("\n")
}

/// Whether the files under a folder take more than `limit`.
fn larger_than(folder: &Path, limit: u64) -> bool {
    let (mut total, mut pending) = (0u64, vec![folder.to_path_buf()]);
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => pending.push(entry.path()),
                Ok(meta) => total = total.saturating_add(meta.len()),
                Err(_) => {}
            }
        }
    }
    total > limit
}

/// Extracts everything into `dest`, a new folder. WinRAR only opens rar: for the rest, the tar of Windows.
pub fn extract(path: &Path, dest: &Path, password: Option<&str>, cancelled: &dyn Fn() -> bool) -> Result<(), ArchiveError> {
    let Some((mut kind, mut exe)) = find_tool() else { return Err(ArchiveError::NoTool) };
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let rar = split(Scheme::OldRar, &name).is_some();
    if kind == Kind::UnRar && !rar {
        (kind, exe) = (Kind::Tar, crate::platform::system_program("tar.exe"));
    }
    let failed = |e: std::io::Error| ArchiveError::Failed(e.to_string());
    std::fs::create_dir_all(dest).map_err(failed)?;
    // the answer goes to a file: a pipe nobody reads would stop the unpacker
    let log = dest.with_extension("log");
    let out = std::fs::File::create(&log).map_err(failed)?;
    let err = out.try_clone().map_err(failed)?;
    let mut command = Command::new(exe);
    quiet(&mut command).args(arguments(kind, path, dest, password)).stdout(Stdio::from(out)).stderr(Stdio::from(err));
    let mut child = command.spawn().map_err(failed)?;
    let mut tick = 0u32;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {
                std::thread::sleep(Duration::from_millis(100));
                tick += 1;
                let stop = if cancelled() {
                    Some(ArchiveError::Cancelled)
                } else {
                    (tick.is_multiple_of(30) && larger_than(dest, MAX_BYTES)).then_some(ArchiveError::TooLarge)
                };
                if let Some(error) = stop {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&log);
                    return Err(error);
                }
            }
            Err(e) => return Err(failed(e)),
        }
    };
    let output = std::fs::read(&log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    verdict(kind, code, &output, password.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volumes_of(name: &str, names: &[&str]) -> (String, usize, Vec<String>) {
        let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        let v = volumes(Path::new(name), &names);
        (v.first.display().to_string(), v.parts.len(), v.missing)
    }

    #[test]
    fn a_whole_file_is_itself() {
        assert_eq!(volumes_of("sofa.zip", &["sofa.zip", "x.zip"]), ("sofa.zip".into(), 1, vec![]));
        assert_eq!(volumes_of("sofa.rar", &["sofa.rar"]), ("sofa.rar".into(), 1, vec![]));
    }

    #[test]
    fn any_part_opens_the_first_and_the_gaps_are_told() {
        let names = ["S.part1.rar", "s.part2.rar", "s.part4.rar", "other.part3.rar"];
        assert_eq!(volumes_of("s.part4.rar", &names), ("S.part1.rar".into(), 3, vec!["s.part3.rar".into()]));
        assert_eq!(volumes_of("s.part02.rar", &["s.part02.rar"]), ("s.part01.rar".into(), 1, vec!["s.part01.rar".into()]));
        assert_eq!(volumes_of("s.7z.002", &["s.7z.001", "s.7z.002"]), ("s.7z.001".into(), 2, vec![]));
    }

    #[test]
    fn the_old_schemes_open_the_rar_or_the_zip() {
        assert_eq!(volumes_of("s.r01", &["s.rar", "s.r00", "s.r01"]), ("s.rar".into(), 3, vec![]));
        assert_eq!(volumes_of("s.z01", &["s.z01", "s.z02"]), ("s.zip".into(), 2, vec!["s.zip".into()]));
        assert_eq!(volumes_of("s.zip", &["s.zip", "s.z02"]), ("s.zip".into(), 2, vec!["s.z01".into()]));
    }

    #[test]
    fn the_proposed_name_has_no_part_nor_extension() {
        let names = [("Sofa.part1.rar", "Sofa"), ("Sofa v1.2.7z.001", "Sofa v1.2"), ("Sofa.tar.gz", "Sofa"), ("Sofa v1.2.zip", "Sofa v1.2")];
        for (name, expected) in names {
            assert_eq!(display_name(Path::new(name)), expected);
        }
    }

    #[test]
    fn the_answers_of_the_unpackers_are_understood() {
        assert_eq!(verdict(Kind::SevenZip, Some(1), "", false), Ok(()));
        let wrong = "ERROR: Wrong password : a.max";
        assert_eq!(verdict(Kind::SevenZip, Some(2), wrong, false), Err(ArchiveError::Password { wrong: false }));
        assert_eq!(verdict(Kind::SevenZip, Some(2), wrong, true), Err(ArchiveError::Password { wrong: true }));
        assert_eq!(verdict(Kind::UnRar, Some(11), "", true), Err(ArchiveError::Password { wrong: true }));
        assert_eq!(verdict(Kind::SevenZip, Some(2), "ERROR: Missing volume", false), Err(ArchiveError::Incomplete));
        assert_eq!(verdict(Kind::Tar, Some(1), "Encrypted file is unsupported", false), Err(ArchiveError::NoTool));
        assert_eq!(verdict(Kind::SevenZip, Some(2), "Scanning\nERROR: bad\n", false), Err(ArchiveError::Failed("ERROR: bad".into())));
    }

    #[test]
    fn what_is_extracted_is_measured() {
        let dir = std::env::temp_dir().join(format!("openlibrary-archives-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("a").join("b"), [0; 100]).unwrap();
        assert!(larger_than(&dir, 99) && !larger_than(&dir, 100));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_or_a_password_cannot_become_an_option() {
        let args = arguments(Kind::SevenZip, Path::new("-x.zip"), Path::new(r"S:\d"), Some("a b"));
        let args: Vec<String> = args.into_iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["x", "-y", "-bd", "-sccUTF-8", r"-oS:\d", "-pa b", "--", "-x.zip"]);
    }
}
