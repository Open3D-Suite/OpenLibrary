// SPDX-License-Identifier: GPL-3.0-or-later
//! The shortcuts of Windows that open OpenLibrary: on the desktop, and in the Start menu of this user when the one
//! the installer makes for everyone is not there. Windows lets no program pin itself to the taskbar.
//!
//! Windows' own PowerShell makes them (`WScript.Shell`); the paths go in environment variables, never in its script.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The name of the shortcut, as the installer calls it.
pub const NAME: &str = "OpenLibrary.lnk";

/// Where a shortcut goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Desktop,
    StartMenu,
}

/// The folders of this user where shortcuts go, as Windows has them (the desktop may be in OneDrive).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Folders {
    pub desktop: Option<PathBuf>,
    pub start_menu: Option<PathBuf>,
}

impl Folders {
    pub fn of(&self, place: Place) -> Option<&Path> {
        match place {
            Place::Desktop => self.desktop.as_deref(),
            Place::StartMenu => self.start_menu.as_deref(),
        }
    }

    /// Whether that place has the shortcut already; in the Start menu, also the installer's, for everyone.
    pub fn has(&self, place: Place) -> bool {
        let mine = self.of(place).is_some_and(|f| f.join(NAME).is_file());
        mine || (place == Place::StartMenu && everyones_start_menu().is_some_and(|f| f.join(NAME).is_file()))
    }
}

/// The Start menu of every user of the computer, where the installer puts its shortcut.
pub fn everyones_start_menu() -> Option<PathBuf> {
    std::env::var_os("ProgramData").map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs"))
}

fn powershell() -> Command {
    let mut command = Command::new(crate::platform::system_program(r"WindowsPowerShell\v1.0\powershell.exe"));
    command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    command
}

/// Asks Windows for the folders. It takes a moment: not for the window's thread.
pub fn folders() -> Folders {
    let script = "[Environment]::GetFolderPath('Desktop'); [Environment]::GetFolderPath('Programs')";
    let Ok(out) = powershell().arg(script).output() else { return Folders::default() };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).map(|l| (!l.is_empty()).then(|| PathBuf::from(l)));
    Folders { desktop: lines.next().flatten(), start_menu: lines.next().flatten() }
}

/// Makes the shortcut `link` to `exe`, with the icon of the exe. Not for the window's thread.
pub fn create(link: &Path, exe: &Path) -> Result<(), String> {
    let script = "$s = (New-Object -ComObject WScript.Shell).CreateShortcut($env:OLIB_LINK); \
                  $s.TargetPath = $env:OLIB_EXE; $s.WorkingDirectory = $env:OLIB_DIR; \
                  $s.IconLocation = $env:OLIB_EXE + ',0'; $s.Description = 'OpenLibrary'; $s.Save()";
    let dir = exe.parent().unwrap_or(Path::new(""));
    let out = powershell().arg(script).env("OLIB_LINK", link).env("OLIB_EXE", exe).env("OLIB_DIR", dir).output().map_err(|e| e.to_string())?;
    if out.status.success() && link.is_file() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if said.is_empty() { format!("{}: not made", link.display()) } else { said })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_has_its_shortcut_when_the_file_is_there() {
        let dir = std::env::temp_dir().join(format!("openlibrary-shortcuts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let folders = Folders { desktop: Some(dir.clone()), start_menu: None };
        assert!(!folders.has(Place::Desktop));
        std::fs::write(dir.join(NAME), b"x").unwrap();
        assert!(folders.has(Place::Desktop));
        assert_eq!(folders.of(Place::StartMenu), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_shortcut_is_made_with_a_path_that_has_quotes_and_signs() {
        let dir = std::env::temp_dir().join(format!("openlibrary-link-{}", std::process::id())).join("it's $(here) · 'x'");
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join(NAME);
        create(&link, &std::env::current_exe().unwrap()).unwrap();
        assert!(link.is_file());
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
