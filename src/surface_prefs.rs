// SPDX-License-Identifier: GPL-3.0-or-later
//! What is remembered of each material (its real size and its mapping), the options of the surface tabs, and the
//! requests to 3ds Max that come out of them.

use crate::interiors::{self, Light, Room};
use crate::library::norm_key;
use crate::settings::Settings;
use crate::surfaces::{self, Aspect, Channel, Material, Res, Texture};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const SIZES: &str = "surface_sizes";
pub const MAPPINGS: &str = "surface_mapping";
const OPTIONS: &str = "surfaces";
/// The size of what does not tell it, in cm.
const DEFAULT_SIZE: f64 = 100.0;

/// How a texture is laid on an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mapping {
    /// The object's own coordinates.
    Uv,
    RealWorld,
    /// A box of X, Y, Z.
    Box,
    Triplanar,
}

impl Mapping {
    pub const ALL: [Self; 4] = [Self::Uv, Self::RealWorld, Self::Box, Self::Triplanar];

    pub fn key(self) -> &'static str {
        match self {
            Self::Uv => "uv",
            Self::RealWorld => "realworld",
            Self::Box => "uvw",
            Self::Triplanar => "triplanar",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// How many of X, Y, Z it uses.
    pub fn axes(self) -> usize {
        match self {
            Self::Uv => 0,
            Self::Triplanar => 1,
            Self::RealWorld => 2,
            Self::Box => 3,
        }
    }
}

/// The key a material is remembered by: relative to its root folder ("02_poliigon/bricks/oak|pbr"), so that it
/// is the same with the library open as a drive or as a server.
pub fn stored_key(roots: &[PathBuf], material: &Material) -> String {
    let full = norm_key(&material.folder.join(&material.name));
    let suffix = if material.is_library() { "|mat" } else { "|pbr" };
    for root in roots {
        let root = norm_key(root).trim_end_matches('\\').to_string();
        let base = root.rsplit('\\').next().unwrap_or_default();
        if full == root {
            return format!("{base}/.{suffix}");
        }
        if let Some(rel) = full.strip_prefix(&format!("{root}\\")) {
            return format!("{base}/{}{suffix}", rel.replace('\\', "/"));
        }
    }
    material.key()
}

/// What is remembered of a material; the Python version's oldest keys, the whole path, are read too.
fn stored<'a>(settings: &'a Settings, store: &str, roots: &[PathBuf], material: &Material) -> Option<&'a Value> {
    let list = settings.data.get(store)?;
    list.get(stored_key(roots, material)).or_else(|| list.get(material.key()))
}

/// Remembers a value of a material, or forgets it.
pub fn remember(settings: &mut Settings, store: &str, roots: &[PathBuf], material: &Material, value: Option<Value>) {
    let list = settings.data.entry(store).or_insert_with(|| json!({}));
    if !list.is_object() {
        *list = json!({});
    }
    if let Some(list) = list.as_object_mut() {
        list.remove(&material.key());
        match value {
            Some(value) => list.insert(stored_key(roots, material), value),
            None => list.remove(&stored_key(roots, material)),
        };
    }
}

/// X, Y, Z in cm; a size of two repeats the last.
fn xyz(values: &[f64]) -> Option<[f64; 3]> {
    let last = *values.last()?;
    let at = |i: usize| values.get(i).copied().unwrap_or(last);
    Some([at(0), at(1), at(2)])
}

/// The size remembered for a material.
pub fn saved_size(settings: &Settings, roots: &[PathBuf], material: &Material) -> Option<[f64; 3]> {
    let list = stored(settings, SIZES, roots, material)?.as_array()?;
    let values: Vec<f64> = list.iter().filter_map(Value::as_f64).filter(|v| *v > 0.0).collect();
    xyz(&values)
}

/// The size of a material that is known: remembered, or told by its name.
pub fn known_size(settings: &Settings, roots: &[PathBuf], material: &Material) -> Option<[f64; 3]> {
    saved_size(settings, roots, material).or_else(|| material.size.and_then(|(w, h)| xyz(&[w, h])))
}

pub fn size_of(settings: &Settings, roots: &[PathBuf], material: &Material) -> [f64; 3] {
    known_size(settings, roots, material).unwrap_or([DEFAULT_SIZE; 3])
}

/// Whether a texture is square, by its real size or else by its pixels.
pub fn shape(size: Option<[f64; 3]>, pixels: Option<(u32, u32)>) -> Option<(Aspect, f64)> {
    let (w, h) = size.map(|s| (s[0], s[1])).or_else(|| pixels.map(|(w, h)| (f64::from(w), f64::from(h))))?;
    Some((surfaces::aspect(w, h)?, w / h))
}

/// The mapping of a material: the one remembered, or the object's. Triplanar is only for square ones.
pub fn mapping_of(settings: &Settings, roots: &[PathBuf], material: &Material, aspect: Option<Aspect>) -> Mapping {
    let saved = stored(settings, MAPPINGS, roots, material).and_then(Value::as_str).and_then(Mapping::from_key);
    match saved.unwrap_or(Mapping::Uv) {
        Mapping::Triplanar if aspect.is_some_and(|a| a != Aspect::Square) => Mapping::RealWorld,
        mapping => mapping,
    }
}

/// An option of the surface tabs.
pub fn option<'a>(settings: &'a Settings, key: &str) -> Option<&'a Value> {
    settings.data.get(OPTIONS)?.get(key)
}

pub fn set_option(settings: &mut Settings, key: &str, value: Value) {
    let options = settings.data.entry(OPTIONS).or_insert_with(|| json!({}));
    if !options.is_object() {
        *options = json!({});
    }
    if let Some(options) = options.as_object_mut() {
        options.insert(key.to_string(), value);
    }
}

pub fn resolution(settings: &Settings) -> Res {
    match option(settings, "res") {
        Some(Value::Number(n)) => n.as_u64().and_then(|n| u32::try_from(n).ok()).map_or(Res::Highest, Res::K),
        Some(Value::String(s)) if s == "min" => Res::Lowest,
        _ => Res::Highest,
    }
}

pub fn set_resolution(settings: &mut Settings, res: Res) {
    let value = match res {
        Res::Highest => json!("max"),
        Res::Lowest => json!("min"),
        Res::K(k) => json!(k),
    };
    set_option(settings, "res", value);
}

pub fn light(settings: &Settings) -> Light {
    if option(settings, "light").and_then(Value::as_str) == Some("night") { Light::Night } else { Light::Day }
}

fn engine(settings: &Settings) -> &str {
    option(settings, "engine").and_then(Value::as_str).unwrap_or("auto")
}

fn displacement(settings: &Settings) -> bool {
    option(settings, "displacement").and_then(Value::as_bool).unwrap_or(false)
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

/// A material for the bridge: its name, maps, mapping and size.
pub fn material_args(settings: &Settings, roots: &[PathBuf], material: &Material, pixels: Option<(u32, u32)>) -> Value {
    let mut maps = serde_json::Map::new();
    for (channel, file) in material.pick(resolution(settings)) {
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let dx = channel == Channel::Normal && surfaces::parse_name(&name).dx;
        maps.insert(if dx { "normaldx" } else { channel.key() }.to_string(), text(&file).into());
    }
    let aspect = shape(known_size(settings, roots, material), pixels).map(|(a, _)| a);
    json!({
        "name": material.name,
        "maps": maps,
        "mapping": mapping_of(settings, roots, material, aspect).key(),
        "size": size_of(settings, roots, material),
    })
}

/// `apply_material` from [`material_args`].
pub fn apply_material(settings: &Settings, material: &Value, target: &str, existing: &str) -> Value {
    let mut args = material.clone();
    args["engine"] = engine(settings).into();
    args["displacement"] = displacement(settings).into();
    args["target"] = target.into();
    args["existing"] = existing.into();
    args["routed"] = true.into();
    args
}

pub fn apply_materials(settings: &Settings, items: Vec<Value>) -> Value {
    json!({"items": items, "engine": engine(settings), "displacement": displacement(settings), "existing": "use"})
}

pub fn add_textures(settings: &Settings, textures: &[Texture], size: [f64; 3], target: &str) -> Value {
    let role = option(settings, "role").and_then(Value::as_str).unwrap_or("auto");
    let files: Vec<String> = textures.iter().map(|t| text(&t.path)).collect();
    let roles: Vec<&str> =
        files.iter().map(|f| if role == "auto" { surfaces::guess_role(f.rsplit('\\').next().unwrap_or(f)) } else { role }).collect();
    let mapping = option(settings, "mapping").and_then(Value::as_str).unwrap_or("uv");
    json!({"files": files, "roles": roles, "engine": engine(settings), "mapping": mapping, "size": size, "target": target})
}

/// `None` when the folders have no shader that reads it.
pub fn apply_interior(settings: &Settings, room: &Room, target: &str) -> Option<Value> {
    let light = light(settings);
    let depth = option(settings, "depth").and_then(Value::as_i64).unwrap_or(-1);
    Some(json!({
        "name": room.name,
        "exr": text(room.pick(light, resolution(settings))?),
        "osl": text(room.osl.as_deref()?),
        "preview": room.thumb_source(light).map(|p| text(p)).unwrap_or_default(),
        "size_code": room.size.code(),
        "width": room.size.width(),
        "height": interiors::HEIGHT,
        "engine": engine(settings),
        "depth": depth,
        "target": target,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::{build_materials, testing::listing};

    fn oak() -> Material {
        let names: &[&str] = &["Oak_basecolor.png", "Oak_normal_directx.png"];
        build_materials(&listing(&[(r"B:\Lib\Vendor\Wood", names)])).remove(0)
    }

    fn settings() -> Settings {
        Settings::load(Path::new("no-such-folder/settings.json"))
    }

    #[test]
    fn a_material_is_remembered_the_same_from_a_drive_and_from_a_server() {
        let mut material = oak();
        assert_eq!(stored_key(&[r"B:\Lib\Vendor".into()], &material), "vendor/wood/oak|pbr");
        material.folder = r"\\SERVER\Lib\Vendor\Wood".into();
        assert_eq!(stored_key(&[r"\\server\lib\vendor\".into()], &material), "vendor/wood/oak|pbr");
        assert_eq!(stored_key(&[r"C:\Other".into()], &material), material.key());
    }

    #[test]
    fn sizes_are_remembered_and_forgotten_and_old_keys_read() {
        let (mut s, material, roots) = (settings(), oak(), [PathBuf::from(r"B:\Lib\Vendor")]);
        assert_eq!((known_size(&s, &roots, &material), size_of(&s, &roots, &material)), (None, [100.0; 3]));
        s.data.insert(SIZES.into(), json!({material.key(): [30.0, 60.0]}));
        assert_eq!(saved_size(&s, &roots, &material), Some([30.0, 60.0, 60.0]));
        remember(&mut s, SIZES, &roots, &material, Some(json!([1.0, 2.0, 3.0])));
        assert_eq!(s.data[SIZES], json!({"vendor/wood/oak|pbr": [1.0, 2.0, 3.0]}));
        remember(&mut s, SIZES, &roots, &material, None);
        assert_eq!(s.data[SIZES], json!({}));
    }

    #[test]
    fn triplanar_is_only_for_square_textures() {
        let (mut s, material, roots) = (settings(), oak(), [PathBuf::from(r"B:\Lib\Vendor")]);
        remember(&mut s, MAPPINGS, &roots, &material, Some("triplanar".into()));
        assert_eq!(mapping_of(&s, &roots, &material, Some(Aspect::Square)), Mapping::Triplanar);
        assert_eq!(mapping_of(&s, &roots, &material, Some(Aspect::Wide)), Mapping::RealWorld);
        assert_eq!(shape(None, Some((4096, 2048))), Some((Aspect::Wide, 2.0)));
        assert_eq!(shape(Some([60.0, 60.0, 60.0]), Some((4096, 2048))), Some((Aspect::Square, 1.0)));
    }

    #[test]
    fn a_request_carries_the_maps_and_the_options() {
        let (mut s, material, roots) = (settings(), oak(), [PathBuf::from(r"B:\Lib\Vendor")]);
        set_option(&mut s, "displacement", true.into());
        let args = apply_material(&s, &material_args(&s, &roots, &material, None), "cursor", "new");
        assert_eq!(args["maps"]["color"], json!(r"B:\Lib\Vendor\Wood\Oak_basecolor.png"));
        assert_eq!(args["maps"]["normaldx"], json!(r"B:\Lib\Vendor\Wood\Oak_normal_directx.png"));
        assert_eq!((&args["engine"], &args["displacement"], &args["routed"]), (&json!("auto"), &json!(true), &json!(true)));
        let texture = Texture { name: "a_roughness".into(), folder: r"L:\".into(), path: r"L:\a_roughness.jpg".into(), mtime: 0 };
        let args = add_textures(&s, &[texture], [1.0; 3], "slate");
        assert_eq!((&args["roles"], &args["mapping"]), (&json!(["data"]), &json!("uv")));
        set_resolution(&mut s, Res::K(4));
        assert_eq!(resolution(&s), Res::K(4));
        set_resolution(&mut s, Res::Lowest);
        assert_eq!(resolution(&s), Res::Lowest);
    }
}
