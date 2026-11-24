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
    Continue => "Continue",
        "Continuar";
    Cancel => "Cancel",
        "Cancelar";
    Close => "Close",
        "Cerrar";
    Undo => "Undo",
        "Deshacer";
    Cancelled => "Cancelled.",
        "Cancelado.";
    CopyList => "Copy the list",
        "Copiar la lista";
    NoMaxBatch => "3dsmaxbatch.exe, which comes with 3ds Max, was not found.",
        "No se encuentra 3dsmaxbatch.exe, que viene con 3ds Max.";
    BatchBusy => "A 3ds Max is already working in the background.",
        "Ya hay un 3ds Max trabajando en segundo plano.";
    BatchStarting => "Starting a 3ds Max in the background…",
        "Arrancando un 3ds Max en segundo plano…";
    BatchProgress => "In the background: {done} of {count}…",
        "En segundo plano: {done} de {count}…";
    BatchDone => "Done: {ok} · could not be done: {failed}",
        "Hechos: {ok} · no se pudieron hacer: {failed}";
    BatchFailed => "The 3ds Max in the background did not work ({error}).",
        "El 3ds Max en segundo plano no funcionó ({error}).";
    Previews => "Previews",
        "Previews";
    PreviewOfSelected => "Of the selected model, in the background…",
        "Del modelo seleccionado, en segundo plano…";
    PreviewOfMissing => "Of all those without one here ({count}), in the background…",
        "De todos los que no tienen aquí ({count}), en segundo plano…";
    PreviewAsk => "A 3ds Max in the background is going to make {count} previews. It does not touch the scene you have open, and you can go on working. Each image is saved beside its model, in the library.",
        "Un 3ds Max en segundo plano va a hacer {count} previews. No toca la escena que tengas abierta y puedes seguir trabajando. Cada imagen se guarda junto a su modelo, en la biblioteca.";
    PreviewAskReplace => "{count} of them have a preview already, and it will be replaced.",
        "{count} de ellos ya tienen preview y se sustituirá.";
    MenuPreview => "Make its preview in the background…",
        "Hacer su preview en segundo plano…";
    MenuData => "Read its faces and vertices",
        "Leer sus caras y vértices";
    ModelData => "Model data",
        "Datos del modelo";
    DataOfSelected => "Read those of the selected model",
        "Leer los del modelo seleccionado";
    DataOfAll => "Read those of all the models here ({count})",
        "Leer los de todos los modelos de aquí ({count})";
    StatsAllRead => "The data of these models is already read.",
        "Los datos de estos modelos ya están leídos.";
    StatsRead => "Read faces and vertices",
        "Leer caras y vértices";
    StatsUnreadable => "3ds Max could not read it in the background.",
        "3ds Max no pudo leerlo en segundo plano.";
    StatsLine => "{polys} faces · {verts} vertices · objects: {objects}",
        "{polys} caras · {verts} vértices · objetos: {objects}";
    StatsDisplacement => "With displacement",
        "Con desplazamiento";
    StatsAnimation => "Animated, frames {start} to {end}",
        "Animado, fotogramas {start} a {end}";
    MissingOpen => "See the missing files…",
        "Ver los archivos que faltan…";
    MissingTitle => "Missing files · {name}",
        "Archivos que faltan · {name}";
    MissingNothing => "Nothing is missing: every external file of this model is in its place.",
        "No falta nada: todos los archivos externos de este modelo están en su sitio.";
    MissingHead => "{count} files not found. Select rows (Ctrl, Shift) to relink or remove them; nothing changes until you save.",
        "{count} archivos sin encontrar. Selecciona filas (Ctrl, Shift) para re-enlazarlas o quitarlas; no cambia nada hasta que guardes.";
    Missing => "Missing",
        "Falta";
    MissingNearby => "Missing, but it is in the model's folder",
        "Falta, pero está en la carpeta del modelo";
    MissingToRemove => "Removed from the .max",
        "Se quita del .max";
    MissingRelink => "Relink by hand…",
        "Re-enlazar a mano…";
    MissingSearch => "Search in a folder…",
        "Buscar en una carpeta…";
    MissingUseNearby => "Use those in the model's folder ({count})",
        "Usar los de la carpeta del modelo ({count})";
    MissingRemove => "Remove from the .max",
        "Quitar del .max";
    MissingRemoveJunk => "Remove .cube, .hdr and checkers ({count})",
        "Quitar .cube, .hdr y checker ({count})";
    MissingSearching => "Searching in {folder}…",
        "Buscando en {folder}…";
    MissingFound => "Found {found} of {count}.",
        "Encontrados {found} de {count}.";
    MissingSave => "Save in the .max ({count})",
        "Guardar en el .max ({count})";
    MissingSaving => "Saving with a 3ds Max in the background (about half a minute)…",
        "Guardando con un 3ds Max en segundo plano (medio minuto)…";
    MissingSaved => "Saved: {count} paths changed.",
        "Guardado: {count} rutas cambiadas.";
    MissingNotSaved => "These could not be written (open in 3ds Max, or read-only?): {files}",
        "No se pudo escribir en estos (¿abiertos en 3ds Max o de solo lectura?): {files}";
    MissingAskSave => ".max files to write again: {files}. Paths relinked: {linked}. References removed: {removed}, which cannot be brought back from here: those maps are left without an image. Go on?",
        "Archivos .max que se reescriben: {files}. Rutas re-enlazadas: {linked}. Referencias quitadas: {removed}, que no se pueden recuperar desde aquí: esos mapas se quedan sin imagen. ¿Seguir?";
    MissingAskClose => "There are changes not saved in the .max. Close without saving them?",
        "Hay cambios sin guardar en el .max. ¿Cerrar sin guardarlos?";
    EngineUnknown => "  ·  renderer not known",
        "  ·  motor sin identificar";
    ReasonOpen => "3ds Max could not open it.",
        "3ds Max no pudo abrirlo.";
    ReasonSave => "It could not be saved.",
        "No se pudo guardar.";
    ReasonConverter => "The Corona Converter was not found (is Corona installed?).",
        "No se encuentra el Corona Converter (¿está Corona instalado?).";
    ReasonVray => "V-Ray must be installed to convert its materials.",
        "Hace falta V-Ray instalado para convertir sus materiales.";
    ReasonStuck => "The 3ds Max in the background closed, or got stuck, on it.",
        "El 3ds Max en segundo plano se cerró o se quedó atascado con él.";
    Catalog => "Catalogue…",
        "Catalogar…";
    CatalogHint => "Choose a download, or drag it to this window.",
        "Elige una descarga o arrástrala a esta ventana.";
    CatalogOpenArchive => "Open archive…",
        "Abrir comprimido…";
    CatalogOpenFolder => "Open folder…",
        "Abrir carpeta…";
    CatalogPaste => "Paste preview",
        "Pegar preview";
    CatalogPasteHint => "In the browser, right-click an image and choose to copy it; then press here.",
        "En el navegador, clic derecho en una imagen y copiarla; luego pulsa aquí.";
    CatalogNoImage => "There is no image in the clipboard.",
        "No hay ninguna imagen en el portapapeles.";
    CatalogOpening => "Opening {name}…",
        "Abriendo {name}…";
    CatalogOpened => "{name} · {count} files, {size}",
        "{name} · {count} archivos, {size}";
    CatalogOpenedNoMax => "{name} · {count} files, {size} · there is no .max in it",
        "{name} · {count} archivos, {size} · no hay ningún .max dentro";
    CatalogTooMany => "That folder has more than {count} files: it looks like a whole library. Choose the folder of one model.",
        "Esa carpeta tiene más de {count} archivos: parece una biblioteca entera. Elige la carpeta de un modelo.";
    CatalogParts => "Parts of this download are missing; put them in the same folder: {parts}",
        "Faltan partes de esta descarga; ponlas en la misma carpeta: {parts}";
    CatalogNoTool => "7-Zip (7-zip.org) is needed to open this file. Install it and try again.",
        "Para abrir este archivo hace falta 7-Zip (7-zip.org). Instálalo y vuelve a probar.";
    CatalogIncomplete => "The file is incomplete: a part is missing, or the download was cut short.",
        "El archivo está incompleto: falta alguna parte o la descarga se cortó.";
    CatalogOpenFailed => "It could not be opened: {error}",
        "No se pudo abrir: {error}";
    CatalogPassword => "The file has a password (it is usually on the download page or in a .txt):",
        "El archivo tiene contraseña (suele venir en la web de descarga o en un .txt):";
    CatalogWrongPassword => "That is not the password. Try again:",
        "Esa contraseña no es. Prueba otra vez:";
    CatalogSkip => "Not copied",
        "No se copia";
    CatalogMax => "1 · .max files",
        "1 · Archivos .max";
    CatalogPreview => "2 · Preview",
        "2 · Preview";
    CatalogMaps => "3 · Textures and the rest",
        "3 · Texturas y otros";
    CatalogPreviewHint => "Drag an image here from a folder",
        "Arrastra aquí una imagen desde una carpeta";
    CatalogToMax => "Move to the .max files",
        "Mover a «Archivos .max»";
    CatalogToPreview => "Move to the preview",
        "Mover a «Preview»";
    CatalogToMaps => "Move to the textures",
        "Mover a «Texturas y otros»";
    CatalogToSkip => "Do not copy",
        "No copiar";
    CatalogFirst => "Use this one as the thumbnail",
        "Usar esta de miniatura";
    CatalogName => "Name",
        "Nombre";
    CatalogWhere => "Folder of the library",
        "Carpeta de la biblioteca";
    CatalogFindFolder => "Find a folder…",
        "Buscar carpeta…";
    CatalogNewFolder => "New folder…",
        "Nueva carpeta…";
    CatalogAskFolder => "Name of the new folder, inside {folder}:",
        "Nombre de la carpeta nueva, dentro de {folder}:";
    CatalogMapsIn => "Textures in",
        "Texturas en";
    CatalogToCorona => "Make the Corona version of the V-Ray .max too",
        "Crear también la versión Corona del .max de V-Ray";
    CatalogToCoronaHint => "With the Corona Converter, when no Corona .max came. Corona and V-Ray must be installed.",
        "Con el Corona Converter, si no viene ningún .max de Corona. Hace falta tener Corona y V-Ray instalados.";
    CatalogCleanLayers => "Everything to layer 0, and delete the other layers",
        "Todo a la capa 0 y borrar las demás capas";
    CatalogMade => "made from the V-Ray one",
        "se crea a partir del de V-Ray";
    CatalogMapsCount => "{count} files ({skipped} repeated left out)",
        "{count} archivos ({skipped} repetidos fuera)";
    CatalogNoPreview => "Without a preview: drag an image to list 2, or make it later.",
        "Sin preview: arrastra una imagen a la lista 2 o hazlo después.";
    CatalogNeedName => "Write the model's name.",
        "Escribe el nombre del modelo.";
    CatalogNeedFolder => "Choose in the tree the folder to save it in.",
        "Elige en el árbol la carpeta donde guardarlo.";
    CatalogNeedFiles => "Drag to lists 1, 2 and 3 what you want to save.",
        "Arrastra a las listas 1, 2 y 3 lo que quieras guardar.";
    CatalogExists => "Some of these files exist already.",
        "Algunos de estos archivos ya existen.";
    CatalogGo => "Catalogue",
        "Catalogar";
    CatalogAskNoMax => "There is no .max in list 1, so it will not show as a model in the library. Save anyway?",
        "No hay ningún .max en la lista 1, así que no saldrá como modelo en la biblioteca. ¿Guardar igualmente?";
    CatalogAskOverwrite => "{count} files exist already:\n\n{files}\n\nReplace them?",
        "Ya existen {count} archivos:\n\n{files}\n\n¿Sustituirlos?";
    CatalogSaving => "Saving {name}…",
        "Guardando {name}…";
    CatalogSaved => "{name} saved in {folder} ({count} files).",
        "{name} guardado en {folder} ({count} archivos).";
    CatalogCancelled => "Cancelled: {count} files of {name} were saved in {folder}.",
        "Cancelado: se guardaron {count} archivos de {name} en {folder}.";
    CatalogSaveFailed => "It could not be saved: {error}",
        "No se pudo guardar: {error}";
    CatalogPreparing => "A 3ds Max in the background is preparing the .max files (a couple of minutes)…",
        "Un 3ds Max en segundo plano está preparando los .max (un par de minutos)…";
    CatalogPrepareCancelled => "The 3ds Max in the background was stopped: look the .max files over.",
        "Se paró el 3ds Max en segundo plano: revisa los .max.";
    SurfacesReading => "Reading the folders for the first time: {count} so far…",
        "Leyendo las carpetas por primera vez: {count} por ahora…";
    SurfacesShown => "{count} here",
        "{count} aquí";
    NoSurfaces => "Nothing here.",
        "Aquí no hay nada.";
    BridgeTooOld => "The bridge in this 3ds Max is older than this function: update it and restart 3ds Max.",
        "El puente de este 3ds Max es anterior a esta función: actualízalo y reinicia 3ds Max.";
    Sending => "{name}: sending to 3ds Max…",
        "{name}: enviando a 3ds Max…";
    SendFailed => "3ds Max could not do it: {error}",
        "3ds Max no pudo hacerlo: {error}";
    MaterialExists => "{name} is in the scene already. Use that one, or bring in a new one (called {name}_01)?",
        "{name} ya está en la escena. ¿Le pongo el que ya hay o meto uno nuevo (se llamará {name}_01)?";
    UseExisting => "Use the scene's",
        "Usar el de la escena";
    MakeNew => "Bring in a new one",
        "Meter uno nuevo";
    OptEngine => "Renderer",
        "Motor";
    EngineScene => "The scene's",
        "El de la escena";
    OptResolution => "Resolution",
        "Resolución";
    ResHighest => "Highest",
        "La más alta";
    ResLowest => "Lowest",
        "La más baja";
    OptDisplacement => "Displacement on",
        "Desplazamiento activado";
    OptBalls => "Show as balls",
        "Ver como bolas";
    OptOnlyLibraries => "Only .mat libraries",
        "Solo bibliotecas .mat";
    OptOnlySized => "Only with a known size",
        "Solo con medida conocida";
    OptRecent => "Newest first",
        "Lo más nuevo primero";
    OptRole => "Load as",
        "Cargar como";
    RoleAuto => "By its name",
        "Según el nombre";
    RoleColor => "Colour (sRGB)",
        "Color (sRGB)";
    RoleData => "Data (linear)",
        "Datos (lineal)";
    OptMapping => "Mapping",
        "Mapeado";
    MapUv => "The object's UV",
        "UV del objeto";
    OptLight => "Light",
        "Luz";
    LightDay => "Day",
        "Día";
    LightNight => "Night",
        "Noche";
    OptDepth => "Depth",
        "Fondo";
    DepthAuto => "Automatic",
        "Automático";
    HintMaterials => "Drag a material onto an object in 3ds Max to put it there, with the mapping and the size it has here, or onto the Slate editor. Select several (Ctrl, Shift) to take them to Slate at once.",
        "Arrastra un material sobre un objeto de 3ds Max para ponérselo, con el mapeado y las medidas que tenga aquí, o sobre el editor Slate. Selecciona varios (Ctrl, Shift) para llevarlos a Slate a la vez.";
    HintTextures => "Drag one or several textures onto the Slate editor to have them as maps, or onto an object to make it a quick material.",
        "Arrastra una o varias texturas sobre el editor Slate para tenerlas como mapas, o sobre un objeto para crearle un material rápido.";
    HintInteriors => "Drag a room onto a window (a plane or a box) in 3ds Max. Dropped where there is nothing, the window's plane is made with its size.",
        "Arrastra una habitación sobre una ventana (plano o caja) de 3ds Max. Si sueltas en el vacío, se crea el plano de la ventana con su tamaño.";
    LibraryHint => "A .mat library: to Slate go all its materials, to objects the first one, as they were saved.",
        "Biblioteca .mat: a Slate van todos sus materiales y a los objetos el primero, tal como están guardados.";
    TextureRole => "Its name says: {role}",
        "Por el nombre: {role}";
    RoomWindow => "Window of {width}×{height} cm",
        "Ventana de {width}×{height} cm";
    InteriorNoShader => "The wParallax shader (wParallax_SMP.osl) is not in the folders of this tab: add the folder that has it.",
        "El shader de wParallax (wParallax_SMP.osl) no está en las carpetas de esta pestaña: añade la carpeta que lo trae.";
    SizeSaved => "Size kept for this material",
        "Medidas guardadas para este material";
    SizeFromName => "Size told by its name or its maker",
        "Medidas sacadas del nombre o del fabricante";
    SizeUnknown => "Size not known: write it above and it is kept for this material",
        "Medidas desconocidas: escríbelas arriba y se quedan guardadas para este material";
    SizeForget => "Forget the size kept",
        "Olvidar las medidas guardadas";
    SizeToFolder => "Use this size in the folder (those without one)",
        "Usar estas medidas en la carpeta (los que no tienen)";
    SizeToFolderDone => "{size} given to {count} materials.",
        "{size} puesto a {count} materiales.";
    MapsToLoad => "Maps it would load",
        "Mapas que se cargarían";
    NotWired => "(not wired)",
        "(no se conecta)";
    Planks => "{count} loose planks: the first one is used.",
        "{count} lamas sueltas: se usa la primera.";
    MenuToSelection => "Apply to the selection in 3ds Max",
        "Aplicar a la selección de 3ds Max";
    MenuQuickMaterial => "Quick material on the selection in 3ds Max",
        "Material rápido en la selección de 3ds Max";
    MenuToSlate => "Put in the Slate editor",
        "Poner en el editor Slate";
    MenuWindow => "Make the window in 3ds Max",
        "Crear la ventana en 3ds Max";
    PutObjects => "{name} put on {count} objects.",
        "{name} puesto a {count} objetos.";
    PutAlsoSlate => "It is in the active Slate view too.",
        "También está en la vista activa de Slate.";
    PutLibrary => "The library has {count} materials: the others, with the Slate editor.",
        "La biblioteca trae {count} materiales: los demás, con el editor Slate.";
    PutWindow => "{name}: window made in 3ds Max (selected, to move it to its place).",
        "{name}: ventana creada en 3ds Max (seleccionada para moverla a su sitio).";
    PutSlate => "Nodes in the active Slate view: {count}.",
        "Nodos en la vista activa de Slate: {count}.";
    PutSlateFound => "In Slate. Already in the scene, and selected: {count}.",
        "En Slate. Ya estaban en la escena y quedan seleccionados: {count}.";
    PutNoSlate => "The Slate editor could not be opened in 3ds Max.",
        "No se pudo abrir el editor Slate en 3ds Max.";
    PutNothing => "No objects are selected in 3ds Max: select them, or use the Slate editor.",
        "No hay objetos seleccionados en 3ds Max: selecciónalos o usa el editor Slate.";
    PutNoWindow => "No objects are selected in 3ds Max: select the window, or make it.",
        "No hay objetos seleccionados en 3ds Max: selecciona la ventana o créala.";
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
