// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenLibrary: a library of 3D models, materials and textures for 3ds Max.
//!
//! What the program does lives here, where the tests reach it; `main.rs` only reads the arguments.

pub mod app;
pub mod bridge_install;
pub mod library;
pub mod maxfile;
pub mod maxlink;
pub mod open3d_settings;
pub mod platform;
pub mod settings;
pub mod texts;
pub mod thumbs;

/// The app's name, as the Open3D launcher shows it and as its folders are called.
pub const APP: &str = "OpenLibrary";
/// The version, from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `OpenLibrary --version` answers; the packager checks it (`version_answer` in tools/repo.toml).
pub fn version_line() -> String {
    format!("{APP} {VERSION}")
}

/// Opens the window; a failure to open it (no graphics, say) is told in a message box.
pub fn run() {
    if let Err(error) = app::run() {
        platform::message_box(&version_line(), &format!("OpenLibrary could not open its window: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_version_line_is_the_name_and_the_version_of_cargo_toml() {
        assert_eq!(version_line(), format!("OpenLibrary {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn the_folders_are_named_after_the_app() {
        let dir = open3d_settings::settings_dir_from(Some("S:\\Open3D".into()), None).map(|dir| dir.join(APP));
        assert_eq!(dir, Some(PathBuf::from(r"S:\Open3D\OpenLibrary")));
    }
}
