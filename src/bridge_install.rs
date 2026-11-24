// SPDX-License-Identifier: GPL-3.0-or-later
//! Installs OpenLibrary's bridge in each 3ds Max of this Windows user, and tells which ones lack it or have an
//! old one.
//!
//! The bridge's files travel inside the exe. They go to `%LOCALAPPDATA%\Autodesk\3dsMax\<version>\ENU`:
//! `scripts\startup\open3d_openlibrary.ms`, which starts it, and `scripts\Open3D\OpenLibrary`, with the bridge
//! and `app.json` (where the app is, whether 3ds Max shows the toolbar, and the token, encrypted with DPAPI).
//! It lives beside the Python version's bridge without touching it: other folder, ports and names.

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

/// The oldest 3ds Max the bridge supports.
pub const MIN_YEAR: u32 = 2025;
pub const STARTUP_NAME: &str = "open3d_openlibrary.ms";

/// The bridge's files: where each goes under `scripts\Open3D\OpenLibrary`, and its content.
pub const FILES: [(&str, &[u8]); 9] = [
    ("olib_bridge/__init__.py", include_bytes!("../bridge/olib_bridge/__init__.py")),
    ("olib_bridge/config.py", include_bytes!("../bridge/olib_bridge/config.py")),
    ("olib_bridge/ops.py", include_bytes!("../bridge/olib_bridge/ops.py")),
    ("olib_bridge/server.py", include_bytes!("../bridge/olib_bridge/server.py")),
    ("olib_bridge/toolbar.py", include_bytes!("../bridge/olib_bridge/toolbar.py")),
    ("olib_bridge/olib_core.ms", include_bytes!("../bridge/olib_bridge/olib_core.ms")),
    ("olib_bridge/olib_materials.ms", include_bytes!("../bridge/olib_bridge/olib_materials.ms")),
    ("olib_bridge/olib_launch.mcr", include_bytes!("../bridge/olib_bridge/olib_launch.mcr")),
    ("olib_bridge/openlibrary.svg", include_bytes!("../bridge/olib_bridge/openlibrary.svg")),
];
/// The start-up script, which goes to `scripts\startup`.
pub const STARTUP: &[u8] = include_bytes!("../bridge/open3d_openlibrary.ms");

/// One 3ds Max of this user: the folder of its settings (`...\3dsMax\2026 - 64bit\ENU`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaxTarget {
    pub enu: PathBuf,
    pub year: Option<u32>,
}

impl MaxTarget {
    pub fn new(enu: &Path) -> Self {
        let name = enu.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let year = name.split(|c: char| !c.is_ascii_digit()).find(|s| s.len() == 4).and_then(|s| s.parse().ok());
        Self { enu: enu.to_path_buf(), year }
    }

    /// "3ds Max 2026", the way 3ds Max calls itself.
    pub fn label(&self) -> String {
        self.year.map_or_else(|| "3ds Max".to_string(), |y| format!("3ds Max {y}"))
    }

    pub fn supported(&self) -> bool {
        self.year.is_some_and(|y| y >= MIN_YEAR)
    }

    pub fn folder(&self) -> PathBuf {
        self.enu.join("scripts").join("Open3D").join("OpenLibrary")
    }

    pub fn startup(&self) -> PathBuf {
        self.enu.join("scripts").join("startup").join(STARTUP_NAME)
    }

    pub fn app_json(&self) -> PathBuf {
        self.folder().join("app.json")
    }

    pub fn installed(&self) -> bool {
        self.startup().is_file() && self.folder().join("olib_bridge").join("__init__.py").is_file()
    }

    /// Installed, but some file differs from the app's (the version number alone does not change with every fix).
    pub fn outdated(&self) -> bool {
        if !self.installed() {
            return false;
        }
        let differs = |path: &Path, want: &[u8]| fs::read(path).map_or(true, |have| have != want);
        differs(&self.startup(), STARTUP) || FILES.iter().any(|(rel, want)| differs(&self.folder().join(rel), want))
    }

    fn read_app_json(&self) -> serde_json::Map<String, Value> {
        fs::read(self.app_json()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
    }

    /// Whether 3ds Max shows the OpenLibrary toolbar.
    pub fn toolbar(&self) -> bool {
        self.read_app_json().get("toolbar").and_then(Value::as_bool).unwrap_or(false)
    }

    /// Copies the bridge and writes `app.json`. `toolbar: None` keeps what was chosen before.
    pub fn install(&self, exe: &Path, sealed_token: &str, toolbar: Option<bool>) -> Result<(), String> {
        let toolbar = toolbar.unwrap_or_else(|| self.toolbar());
        let folder = self.folder();
        for (rel, data) in FILES {
            write_whole(&folder.join(rel), data)?;
        }
        write_whole(&self.startup(), STARTUP)?;
        self.write_app_json(exe, sealed_token, toolbar)
    }

    /// Takes the bridge out: its own files, and its folders when nothing else is left in them.
    pub fn uninstall(&self) -> Result<(), String> {
        let folder = self.folder();
        let files = FILES.iter().map(|(rel, _)| folder.join(rel)).chain([self.app_json(), self.startup()]);
        for file in files.filter(|f| f.exists()) {
            fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        }
        let package = folder.join("olib_bridge");
        let _ = fs::remove_dir_all(package.join("__pycache__")); // what Python compiled of them
        for dir in [Some(package.as_path()), Some(folder.as_path()), folder.parent()].into_iter().flatten() {
            if fs::remove_dir(dir).is_err() {
                break; // something of somebody else's is in it
            }
        }
        Ok(())
    }

    fn write_app_json(&self, exe: &Path, sealed_token: &str, toolbar: bool) -> Result<(), String> {
        let data = json!({"command": [exe.display().to_string()], "toolbar": toolbar, "token_dpapi": sealed_token});
        write_whole(&self.app_json(), &serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?)
    }

    /// Points an installed bridge at this app and this token, if it points elsewhere (the app moved, or it was
    /// installed by another copy). Returns whether `app.json` was written.
    pub fn fix_app_json(&self, exe: &Path, sealed_token: &str) -> Result<bool, String> {
        if !self.installed() {
            return Ok(false);
        }
        let data = self.read_app_json();
        let command = data.get("command").and_then(Value::as_array).and_then(|c| c.first()).and_then(Value::as_str);
        let same_exe = command.is_some_and(|c| crate::library::norm_key(Path::new(c)) == crate::library::norm_key(exe));
        let same_token = data.get("token_dpapi").and_then(Value::as_str) == Some(sealed_token);
        if same_exe && same_token {
            return Ok(false);
        }
        self.write_app_json(exe, sealed_token, self.toolbar())?;
        Ok(true)
    }
}

/// Writes a file whole into a new one beside it, which then takes its name.
fn write_whole(path: &Path, data: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder")?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The 3ds Max of this user, newest first, from the folders of their settings under `local_app_data`.
pub fn find_targets(local_app_data: &Path) -> Vec<MaxTarget> {
    let base = local_app_data.join("Autodesk").join("3dsMax");
    let mut out: Vec<MaxTarget> = fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("ENU"))
        .filter(|enu| enu.is_dir())
        .map(|enu| MaxTarget::new(&enu))
        .collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.year));
    out
}

/// What to tell when the app opens: the supported 3ds Max without the bridge (but those of the years in `skip`,
/// where the person chose not to be asked again), and those with an old one (always told).
pub fn pending(targets: &[MaxTarget], skip: &[u32]) -> (Vec<MaxTarget>, Vec<MaxTarget>) {
    let supported = targets.iter().filter(|t| t.supported());
    let missing = supported.clone().filter(|t| !t.installed() && !t.year.is_some_and(|y| skip.contains(&y))).cloned().collect();
    let old = supported.filter(|t| t.outdated()).cloned().collect();
    (missing, old)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("openlibrary-bridge-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            for version in ["2026 - 64bit", "2023 - 64bit"] {
                fs::create_dir_all(dir.join("Autodesk").join("3dsMax").join(version).join("ENU")).unwrap();
            }
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn targets_are_found_newest_first() {
        let s = Scratch::new("find");
        let targets = find_targets(&s.0);
        assert_eq!(targets.iter().map(|t| t.year).collect::<Vec<_>>(), [Some(2026), Some(2023)]);
        assert!(targets[0].supported());
        assert!(!targets[1].supported());
        assert_eq!(targets[0].label(), "3ds Max 2026");
    }

    #[test]
    fn install_update_and_the_toolbar_choice() {
        let s = Scratch::new("install");
        let target = find_targets(&s.0).remove(0);
        let exe = s.0.join("OpenLibrary.exe");
        assert!(!target.installed());
        assert!(!target.outdated()); // not installed: nothing to tell
        target.install(&exe, "aa11", Some(true)).unwrap();
        assert!(target.installed());
        assert!(!target.outdated());
        assert!(target.toolbar());
        assert_eq!(fs::read(target.folder().join("olib_bridge/server.py")).unwrap(), FILES[3].1);
        let mut file = fs::read(target.folder().join("olib_bridge/ops.py")).unwrap();
        file.extend_from_slice(b"\n# old\n");
        fs::write(target.folder().join("olib_bridge/ops.py"), file).unwrap();
        assert!(target.outdated()); // the same version, another file
        target.install(&exe, "aa11", None).unwrap();
        assert!(!target.outdated());
        assert!(target.toolbar()); // updating without saying keeps the choice
    }

    #[test]
    fn app_json_follows_the_app_and_its_token() {
        let s = Scratch::new("fix");
        let target = find_targets(&s.0).remove(0);
        let exe = s.0.join("OpenLibrary.exe");
        assert_eq!(target.fix_app_json(&exe, "aa11"), Ok(false)); // nothing installed
        target.install(&exe, "aa11", Some(false)).unwrap();
        assert_eq!(target.fix_app_json(&exe, "aa11"), Ok(false)); // already this app
        assert_eq!(target.fix_app_json(&s.0.join("moved").join("OpenLibrary.exe"), "aa11"), Ok(true));
        assert_eq!(target.fix_app_json(&s.0.join("moved").join("OpenLibrary.exe"), "bb22"), Ok(true));
        let data: Value = serde_json::from_slice(&fs::read(target.app_json()).unwrap()).unwrap();
        assert_eq!(data["token_dpapi"], "bb22");
        assert_eq!(data["toolbar"], false);
    }

    #[test]
    fn what_to_tell_when_the_app_opens() {
        let s = Scratch::new("pending");
        let targets = find_targets(&s.0);
        let (missing, old) = pending(&targets, &[]);
        assert_eq!((missing.len(), old.len()), (1, 0)); // 2023 is not supported
        assert_eq!(pending(&targets, &[2026]).0.len(), 0); // "do not ask again"
        targets[0].install(&s.0.join("OpenLibrary.exe"), "aa11", None).unwrap();
        assert_eq!(pending(&targets, &[]), (vec![], vec![]));
        fs::write(targets[0].startup(), b"-- old\n").unwrap();
        let (missing, old) = pending(&targets, &[2026]);
        assert_eq!((missing.len(), old.len()), (0, 1)); // an old bridge is always told
    }

    #[test]
    fn taking_the_bridge_out_leaves_what_is_not_its_own() {
        let dir = Scratch::new("uninstall");
        let target = MaxTarget::new(&dir.0.join("2026 - 64bit").join("ENU"));
        target.install(Path::new("S:\\OpenLibrary.exe"), "00", Some(false)).unwrap();
        assert!(target.installed());
        target.uninstall().unwrap();
        assert!(!target.installed() && !target.folder().exists() && !target.startup().exists());
        target.install(Path::new("S:\\OpenLibrary.exe"), "00", Some(false)).unwrap();
        let other = target.folder().join("notes.txt");
        fs::write(&other, b"mine").unwrap();
        target.uninstall().unwrap();
        assert!(other.exists() && !target.app_json().exists());
    }
}
