// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenLibrary: a library of 3D models, materials and textures for 3ds Max.
//!
//! What the program does lives here, where the tests reach it; `main.rs` only reads the arguments.

pub mod library;
pub mod maxfile;
pub mod open3d_settings;
pub mod platform;
pub mod settings;
pub mod texts;

use open3d_settings::Language;
use std::path::Path;

/// The app's name, as the Open3D launcher shows it and as its folders are called.
pub const APP: &str = "OpenLibrary";
/// The version, from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `OpenLibrary --version` answers; the packager checks it (`version_answer` in tools/repo.toml).
pub fn version_line() -> String {
    format!("{APP} {VERSION}")
}

/// Shows, in the language every Open3D app shares, where OpenLibrary keeps its files.
pub fn run() {
    let language = open3d_settings::language(open3d_settings::settings_dir().as_deref());
    let settings = open3d_settings::app_settings_dir(APP);
    let data = open3d_settings::app_data_dir(APP);
    platform::message_box(&version_line(), &describe(language, settings.as_deref(), data.as_deref()));
}

/// The text the window shows: the language and the two folders of the app. Nothing is created.
pub fn describe(language: Language, settings: Option<&Path>, data: Option<&Path>) -> String {
    let (language_label, language_name, settings_label, data_label, none) = match language {
        Language::English => ("Language", "English", "Settings", "Other files", "no folder"),
        Language::Spanish => ("Idioma", "español", "Ajustes", "Otros archivos", "ninguna carpeta"),
    };
    let shown = |folder: Option<&Path>| folder.map_or_else(|| format!("({none})"), |folder| folder.display().to_string());
    format!("{language_label}: {language_name}\n{settings_label}: {}\n{data_label}: {}", shown(settings), shown(data))
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
    fn the_text_names_both_folders_in_the_language_shown() {
        let settings = PathBuf::from(r"S:\Open3D\OpenLibrary");
        let data = PathBuf::from(r"D:\Open3D\OpenLibrary");
        let english = describe(Language::English, Some(&settings), Some(&data));
        assert_eq!(english, "Language: English\nSettings: S:\\Open3D\\OpenLibrary\nOther files: D:\\Open3D\\OpenLibrary");
        let spanish = describe(Language::Spanish, Some(&settings), Some(&data));
        assert_eq!(spanish, "Idioma: español\nAjustes: S:\\Open3D\\OpenLibrary\nOtros archivos: D:\\Open3D\\OpenLibrary");
    }

    #[test]
    fn a_missing_folder_is_said() {
        assert!(describe(Language::English, None, None).ends_with("Other files: (no folder)"));
        assert!(describe(Language::Spanish, None, None).ends_with("Otros archivos: (ninguna carpeta)"));
    }

    #[test]
    fn the_folders_are_named_after_the_app() {
        let dir = open3d_settings::settings_dir_from(Some("S:\\Open3D".into()), None).map(|dir| dir.join(APP));
        assert_eq!(dir, Some(PathBuf::from(r"S:\Open3D\OpenLibrary")));
    }
}
