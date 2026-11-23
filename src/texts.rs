// SPDX-License-Identifier: GPL-3.0-or-later
//! Every text of the interface, in English and in Spanish, side by side.
//!
//! A text is a variant of [`Text`]; [`Text::get`] gives it in a language. A text with a value in it marks the
//! place with `{}` names (`{count}`), filled with [`fill`]. The tests fail when a text lacks one of its
//! languages, or when the two do not name the same values.

use crate::open3d_settings::Language;

macro_rules! texts {
    ($($name:ident => $en:literal, $es:literal;)*) => {
        /// A text of the interface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Text {
            $($name,)*
        }

        impl Text {
            /// Every text, for the tests.
            pub const ALL: &[Text] = &[$(Text::$name,)*];

            /// The text in a language.
            pub fn get(self, language: Language) -> &'static str {
                match (self, language) {
                    $((Text::$name, Language::English) => $en,
                      (Text::$name, Language::Spanish) => $es,)*
                }
            }
        }
    };
}

texts! {
    Search => "Search the library…", "Buscar en la biblioteca…";
    AllFolders => "All", "Todo";
    Scanning => "Reading the library…", "Leyendo la biblioteca…";
    ScanProgress => "Reading the library: {folders} folders, {models} models so far…",
        "Leyendo la biblioteca: {folders} carpetas, {models} modelos por ahora…";
    Models => "{count} models", "{count} modelos";
    NoFolders => "This tab has no folders yet. Add them in the settings of the Python version for now.",
        "Esta pestaña aún no tiene carpetas. Por ahora, añádelas en los ajustes de la versión Python.";
    NoModels => "No models here.", "Aquí no hay modelos.";
    NotModelsYet => "Materials, textures and interiors come in a later version.",
        "Los materiales, las texturas y los interiores llegan en una versión posterior.";
    NoPreview => "no preview", "sin preview";
    Loading => "loading…", "cargando…";
    Versions => "Versions", "Versiones";
    Folder => "Folder", "Carpeta";
    ViewportCapture => "No preview: the image is the viewport capture 3ds Max saved.",
        "Sin preview: la imagen es la captura del visor que guardó 3ds Max.";
    EnginePending => "reading…", "leyendo…";
    EngineOther => "other renderer", "otro motor";
    ExternalFiles => "External files", "Archivos externos";
    FilesOk => "{count} files, all found", "{count} archivos, todos encontrados";
    FilesMissing => "{missing} of {count} files missing", "faltan {missing} de {count} archivos";
    OpenFolder => "Open folder", "Abrir carpeta";
    CarriedOver => "Your settings from the previous OpenLibrary were brought over.",
        "Se han traído los ajustes de la OpenLibrary anterior.";
    BridgeMissing => "{maxes} has no bridge: without it, models cannot be dragged from OpenLibrary into the scene.",
        "{maxes} no tiene el puente: sin él no se pueden arrastrar modelos de OpenLibrary a la escena.";
    BridgeOld => "The bridge of {maxes} is from an earlier OpenLibrary. Update it, so that dragging into the scene works.",
        "El puente de {maxes} es de una OpenLibrary anterior. Actualízalo para que arrastrar a la escena funcione.";
    Install => "Install", "Instalar";
    Update => "Update", "Actualizar";
    InstallUpdate => "Install and update", "Instalar y actualizar";
    Later => "Later", "Más tarde";
    DontAskAgain => "Do not ask again", "No volver a preguntar";
    BridgeDone => "Bridge ready in {maxes}. If 3ds Max is open, restart it.", "Puente listo en {maxes}. Si 3ds Max está abierto, reinícialo.";
    BridgeError => "The bridge could not be installed in {max}: {error}", "No se pudo instalar el puente en {max}: {error}";
    Connected => "Connected to {max} · {engine}", "Conectado a {max} · {engine}";
    NoMax => "No 3ds Max with the bridge is open.", "No hay ningún 3ds Max abierto con el puente.";
    DropOnMax => "Drop the model on the window of a 3ds Max with the bridge.", "Suelta el modelo sobre la ventana de un 3ds Max con el puente.";
    DragHint => "Drag to 3ds Max · Shift: as proxy · Ctrl: in its original place", "Arrastra a 3ds Max · Shift: como proxy · Ctrl: en su posición original";
    Placing => "{name}: in 3ds Max, click to place · the wheel turns 90° · right button or Esc ends",
        "{name}: en 3ds Max, clic para colocar · la rueda gira 90° · botón derecho o Esc termina";
    Merged => "{name}: {count} objects in {max}", "{name}: {count} objetos en {max}";
    MergeFailed => "{name} could not be brought in: {error}", "No se pudo traer {name}: {error}";
    OtherEngine => "{name} has no {engine} version: the {used} one came in.", "{name} no tiene versión de {engine}: ha entrado la de {used}.";
    NoFile => "{name} has no .max file.", "{name} no tiene archivo .max.";
    Busy => "3ds Max is busy with the command before.", "3ds Max está ocupado con la orden anterior.";
    MenuPlace => "Merge and place", "Merge and place";
    MenuProxy => "Merge as proxy", "Merge as proxy";
    MenuOrigin => "Merge in its original place", "Merge en su posición original";
    ProxyNeedsEngine => "Merge as proxy (needs Corona or V-Ray)", "Merge as proxy (necesita Corona o V-Ray)";
    ProxyLocal => "The proxy could not be written beside the model, so it went to this computer: {folder}",
        "No se pudo escribir el proxy junto al modelo, así que se ha guardado en este ordenador: {folder}";
    SettingsError => "The settings could not be saved: {error}", "No se pudieron guardar los ajustes: {error}";
}

/// A text with its `{name}` places filled in.
pub fn fill(text: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    values.iter().fold(text.to_string(), |out, (name, value)| out.replace(&format!("{{{name}}}"), &value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(text: &str) -> Vec<String> {
        let mut out: Vec<String> = text.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(name, _)| name.to_string())).collect();
        out.sort();
        out
    }

    #[test]
    fn every_text_has_english_and_spanish_with_the_same_places() {
        for text in Text::ALL {
            let (en, es) = (text.get(Language::English), text.get(Language::Spanish));
            assert!(!en.trim().is_empty(), "{text:?} has no English");
            assert!(!es.trim().is_empty(), "{text:?} has no Spanish");
            assert_eq!(places(en), places(es), "{text:?} names other values in Spanish");
        }
    }

    #[test]
    fn places_are_filled() {
        let text = Text::FilesMissing.get(Language::Spanish);
        assert_eq!(fill(text, &[("missing", &2), ("count", &9)]), "faltan 2 de 9 archivos");
    }
}
