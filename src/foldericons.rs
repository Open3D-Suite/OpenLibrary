// SPDX-License-Identifier: GPL-3.0-or-later
//! The icon and the colour of a folder in the tree: the icon comes from the folder's name (trees, sofas,
//! buildings...), and the person may choose another, and a colour, from the folder's menu. What they choose is
//! in `folder_marks` of the settings.

use crate::library::norm_key;
use crate::settings::Settings;
use crate::texts::Text;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::LazyLock;

const MARKS: &str = "folder_marks";

/// Name, text and drawing (an SVG path in a box of 24).
pub const ICONS: [(&str, Text, &str); 18] = [
    (
        "folder",
        Text::IconFolder,
        "M3 6.500A1.500 1.500 0 0 1 4.500 5h4.300l2 2.200h8.700A1.500 1.500 0 0 1 21 8.700v9.800a1.500 1.500 0 0 1-1.500 1.500h-15A1.500 1.500 0 0 1 3 18.500z",
    ),
    (
        "tree",
        Text::IconTree,
        "M12 21v-7M9.500 21h5M12 3a4.500 4.500 0 0 1 4.300 3.200A4 4 0 0 1 15.500 14h-7a4 4 0 0 1-.800-7.800A4.500 4.500 0 0 1 12 3z",
    ),
    (
        "plant",
        Text::IconPlant,
        "M8 15h8l-1 6H9zM12 15V9M12 10c0-3 2-5 5.500-5 0 3-2 5-5.500 5zM12 12.500c0-2.500-2-4.500-5.500-4.500 0 3 2 4.500 5.500 4.500z",
    ),
    (
        "sofa",
        Text::IconSofa,
        "M6 11V8a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v3M3 13a1.800 1.800 0 0 1 3.600 0v1.500h10.800V13A1.800 1.800 0 0 1 21 13v5H3zM6 18v2M18 18v2",
    ),
    ("chair", Text::IconChair, "M7 3v9M17 3v9M7 6.500h10M6 12h12v3H6zM7 15v6M17 15v6"),
    ("table", Text::IconTable, "M3 7h18v2.500H3zM5.500 9.500V19M18.500 9.500V19M5.500 14h13"),
    ("bed", Text::IconBed, "M3 19v-9M21 19v-5a2 2 0 0 0-2-2H3M3 15.500h18V17H3zM6 12V9.500a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1V12"),
    ("lamp", Text::IconLamp, "M8.500 3h7l3 8h-13zM12 11v9M8.500 20h7"),
    ("building", Text::IconBuilding, "M5 21V4h9v17M14 9h5v12M3 21h18M8 8h3M8 12h3M8 16h3"),
    ("door", Text::IconDoor, "M6.500 21V3.500h11V21M4 21h16M14.500 12.500v1"),
    (
        "car",
        Text::IconCar,
        "M4 16.500v-4l2-5h12l2 5v4zM4.500 12.500h15M5.9 17A1.6 1.6 0 1 1 9.1 17A1.6 1.6 0 1 1 5.9 17zM14.9 17A1.6 1.6 0 1 1 18.1 17A1.6 1.6 0 1 1 14.9 17z",
    ),
    ("person", Text::IconPerson, "M8.8 7A3.2 3.2 0 1 1 15.2 7A3.2 3.2 0 1 1 8.8 7zM5.500 20.500v-1.500a6.500 6.500 0 0 1 13 0v1.500"),
    ("bath", Text::IconBath, "M3 12h18v2a5 5 0 0 1-5 5H8a5 5 0 0 1-5-5zM6 12V6a2 2 0 0 1 4 0M7 19l-1 2M17 19l1 2"),
    ("kitchen", Text::IconKitchen, "M7 3v18M5 3v4a2 2 0 0 0 4 0V3M16.500 21V3c-2.500 2-2.500 7 0 9"),
    ("decor", Text::IconDecor, "M9.500 3h5M10.500 3c0 3-4.500 5-4.500 10a6 6 0 0 0 12 0c0-5-4.500-7-4.500-10M6.500 11h11"),
    ("image", Text::IconImage, "M3.5 4.5h17v15h-17zM7.5 10A1.5 1.5 0 1 1 10.5 10A1.5 1.5 0 1 1 7.5 10zM4 17l5-4 4 3 3-2.500 4 3.500"),
    ("box", Text::IconBox, "M12 3l8 4.500v9L12 21l-8-4.500v-9zM4 7.500l8 4.500 8-4.500M12 12v9"),
    ("star", Text::IconStar, "M12 3.500l2.600 5.300 5.900.900-4.250 4.100 1 5.800L12 16.900 6.750 19.600l1-5.800L3.500 9.700l5.900-.900z"),
];

pub const COLORS: [(&str, Text); 9] = [
    ("#E5484D", Text::ColorRed),
    ("#F76B15", Text::ColorOrange),
    ("#F0A93B", Text::ColorAmber),
    ("#46A758", Text::ColorGreen),
    ("#12A594", Text::ColorTurquoise),
    ("#3E8BF0", Text::ColorBlue),
    ("#8E6CEF", Text::ColorViolet),
    ("#E667A5", Text::ColorPink),
    ("#8B93A0", Text::ColorGrey),
];

/// The words (lower case, without accents) that give a folder its icon; the first that is in its name wins.
const WORDS: [(&str, &[&str]); 15] = [
    ("image", &["z-maps", "maps", "mapas", "textur", "hdri"]),
    ("tree", &["arbol", "tree", "arbust", "bosque", "palm", "forest"]),
    ("plant", &["plant", "veget", "flor", "flores", "flower", "jardin", "garden", "macet", "cesped", "grass"]),
    (
        "building",
        &[
            "edific",
            "building",
            "arquitect",
            "ciudad",
            "city",
            "casa",
            "house",
            "equipam",
            "comercial",
            "hospital",
            "aeropuerto",
            "educacion",
            "deportiv",
            "exposicion",
        ],
    ),
    ("door", &["puerta", "ventana", "door", "window"]),
    ("person", &["person", "people", "human", "gente", "figur"]),
    ("car", &["coche", "car", "vehic", "bus", "camion", "truck", "moto", "bici", "bicicleta", "ambulanc", "transport", "avion", "barco"]),
    ("bath", &["bano", "bath", "sanitar", "aseo", "grifer"]),
    ("kitchen", &["cocina", "kitchen", "electrodom", "vajilla"]),
    ("bed", &["cama", "bed", "dormitor"]),
    ("lamp", &["lamp", "lampara", "ilumin", "light", "luz", "luces", "farola", "colgante"]),
    ("chair", &["silla", "chair", "taburete", "stool", "banco", "asiento"]),
    ("table", &["mesa", "table", "escritorio", "desk", "aparador", "armario", "estanter"]),
    ("sofa", &["sofa", "butaca", "sillon", "couch", "armchair", "mobiliario", "mueble", "furniture"]),
    ("decor", &["decor", "accesor", "escultur", "jarron", "vase", "cuadro", "cortina", "alfombra", "textil"]),
];

fn plain(text: &str) -> String {
    let bare = |c: char| match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        c => c,
    };
    text.to_lowercase().chars().map(bare).collect()
}

/// The icon of a folder by its name.
pub fn guess(folder: &str) -> &'static str {
    let name = plain(folder);
    let tokens: Vec<&str> = name.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| !t.is_empty()).collect();
    let fits = |word: &str| {
        if word.contains('-') {
            return name.contains(word);
        }
        // the whole word or its plural; a long one is enough as the beginning
        tokens.iter().any(|t| *t == word || t.strip_suffix('s') == Some(word) || (word.len() >= 5 && t.starts_with(word)))
    };
    WORDS.iter().find(|(_, words)| words.iter().any(|w| fits(w))).map_or("folder", |(icon, _)| icon)
}

/// The icon and the colour the person gave a folder.
pub fn mark<'a>(settings: &'a Settings, folder: &str) -> (Option<&'a str>, Option<&'a str>) {
    let mark = settings.data.get(MARKS).and_then(|m| m.get(folder));
    let get = |key: &str| mark.and_then(|m| m.get(key)).and_then(Value::as_str);
    (get("icon"), get("color"))
}

/// Sets or takes off a folder's `icon` or `color`.
pub fn set_mark(settings: &mut Settings, folder: &Path, what: &str, value: Option<&str>) {
    let marks = settings.data.entry(MARKS).or_insert_with(|| json!({}));
    if !marks.is_object() {
        *marks = json!({});
    }
    let Some(marks) = marks.as_object_mut() else { return };
    let key = norm_key(folder);
    let mut mark: Map<String, Value> = marks.get(&key).and_then(Value::as_object).cloned().unwrap_or_default();
    match value {
        Some(value) => mark.insert(what.into(), value.into()),
        None => mark.remove(what),
    };
    if mark.is_empty() {
        marks.remove(&key);
    } else {
        marks.insert(key, Value::Object(mark));
    }
}

/// "#E5484D" as a colour.
pub fn color(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#').filter(|h| h.len() == 6)?;
    let part = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some(Color32::from_rgb(part(0)?, part(2)?, part(4)?))
}

type Lines = Vec<Vec<Pos2>>;

static DRAWINGS: LazyLock<Vec<(&'static str, Lines)>> = LazyLock::new(|| ICONS.iter().map(|(name, _, path)| (*name, lines_of(path))).collect());

/// Draws an icon in a square.
pub fn paint(painter: &egui::Painter, rect: Rect, icon: &str, color: Color32) {
    let Some((_, lines)) = DRAWINGS.iter().find(|(n, _)| *n == icon).or_else(|| DRAWINGS.first()) else { return };
    let scale = rect.width() / 24.0;
    for line in lines {
        let points = line.iter().map(|p| rect.min + p.to_vec2() * scale).collect();
        painter.add(egui::Shape::line(points, Stroke::new((1.7 * scale).max(1.0), color)));
    }
}

/// The numbers and letters of an SVG path.
enum Token {
    Command(char),
    Number(f32),
}

fn tokens(path: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, out: &mut Vec<Token>| {
        if let Ok(n) = number.parse() {
            out.push(Token::Number(n));
        }
        number.clear();
    };
    for c in path.chars() {
        match c {
            '0'..='9' => number.push(c),
            '.' if !number.contains('.') => number.push(c),
            '.' | '-' => {
                flush(&mut number, &mut out);
                number.push(c);
            }
            c => {
                flush(&mut number, &mut out);
                if c.is_ascii_alphabetic() {
                    out.push(Token::Command(c));
                }
            }
        }
    }
    flush(&mut number, &mut out);
    out
}

/// An SVG path as lines: M, L, H, V, C, A and Z, absolute and relative.
pub(crate) fn lines_of(path: &str) -> Lines {
    let mut out: Lines = Vec::new();
    let mut at = Pos2::ZERO;
    let mut start = Pos2::ZERO;
    let mut command = 'M';
    let mut numbers: Vec<f32> = Vec::new();
    for token in tokens(path) {
        match token {
            Token::Command(c) => {
                (command, numbers) = (c, Vec::new());
                if c.eq_ignore_ascii_case(&'z') {
                    if let Some(line) = out.last_mut() {
                        line.push(start);
                    }
                    at = start;
                }
                continue;
            }
            Token::Number(n) => numbers.push(n),
        }
        let need = match command.to_ascii_lowercase() {
            'm' | 'l' => 2,
            'h' | 'v' => 1,
            'c' => 6,
            'a' => 7,
            _ => usize::MAX,
        };
        if numbers.len() < need {
            continue;
        }
        let n = std::mem::take(&mut numbers);
        let base = if command.is_ascii_lowercase() { at } else { Pos2::ZERO };
        let point = |i: usize| base + egui::vec2(n[i], n[i + 1]);
        let mut add = |to: Pos2, at: &mut Pos2| {
            if out.last().is_none_or(|l| l.last() != Some(at)) {
                out.push(vec![*at]);
            }
            if let Some(line) = out.last_mut() {
                line.push(to);
            }
            *at = to;
        };
        match command.to_ascii_lowercase() {
            'm' => {
                at = point(0);
                start = at;
                command = if command == 'm' { 'l' } else { 'L' };
            }
            'l' => add(point(0), &mut at),
            'h' => add(egui::pos2(base.x + n[0], at.y), &mut at),
            'v' => add(egui::pos2(at.x, base.y + n[0]), &mut at),
            'c' => {
                let (p0, p1, p2, p3) = (at, point(0), point(2), point(4));
                for step in 1..=8 {
                    let t = step as f32 / 8.0;
                    let u = 1.0 - t;
                    let sum =
                        p0.to_vec2() * (u * u * u) + p1.to_vec2() * (3.0 * u * u * t) + p2.to_vec2() * (3.0 * u * t * t) + p3.to_vec2() * (t * t * t);
                    add(sum.to_pos2(), &mut at);
                }
            }
            _ => {
                for to in arc(at, [n[0], n[1]], n[3] != 0.0, n[4] != 0.0, point(5)) {
                    add(to, &mut at);
                }
            }
        }
    }
    out
}

/// The points of an SVG arc without rotation.
fn arc(from: Pos2, radii: [f32; 2], large: bool, sweep: bool, to: Pos2) -> Vec<Pos2> {
    let half = (from - to) * 0.5;
    let [mut rx, mut ry] = radii.map(|r| r.abs().max(1e-3));
    let lack = (half.x / rx).powi(2) + (half.y / ry).powi(2);
    if lack > 1.0 {
        (rx, ry) = (rx * lack.sqrt(), ry * lack.sqrt());
    }
    let (a, b) = ((rx * half.y).powi(2), (ry * half.x).powi(2));
    let side = ((rx * rx * ry * ry - a - b).max(0.0) / (a + b).max(1e-6)).sqrt() * if large == sweep { -1.0 } else { 1.0 };
    let center = egui::vec2(side * rx * half.y / ry, -side * ry * half.x / rx);
    let angle = |v: egui::Vec2| (v.y / ry).atan2(v.x / rx);
    let first = angle(half - center);
    let mut turn = angle(-half - center) - first;
    if sweep && turn < 0.0 {
        turn += std::f32::consts::TAU;
    } else if !sweep && turn > 0.0 {
        turn -= std::f32::consts::TAU;
    }
    let middle = from + (to - from) * 0.5 + center;
    (1..=10)
        .map(|i| {
            let angle = first + turn * i as f32 / 10.0;
            if i == 10 { to } else { middle + egui::vec2(rx * angle.cos(), ry * angle.sin()) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_s_name_gives_its_icon() {
        let cases = [
            ("02_ÁRBOLES", "tree"),
            ("Sofás", "sofa"),
            ("z-maps", "image"),
            ("Cars", "car"),
            ("Cartas", "folder"),
            ("Baño", "bath"),
            ("X", "folder"),
        ];
        for (name, icon) in cases {
            assert_eq!(guess(name), icon, "{name}");
        }
    }

    #[test]
    fn every_drawing_stays_in_its_box() {
        for (name, lines) in DRAWINGS.iter() {
            assert!(!lines.is_empty(), "{name}");
            for p in lines.iter().flatten() {
                assert!((0.0..=24.0).contains(&p.x) && (0.0..=24.0).contains(&p.y), "{name} {p:?}");
            }
        }
        assert!(WORDS.iter().all(|(icon, _)| ICONS.iter().any(|(n, _, _)| n == icon)));
        assert!(COLORS.iter().all(|(c, _)| color(c).is_some()));
    }

    #[test]
    fn a_mark_is_set_and_taken_off() {
        let mut settings = Settings::load(Path::new("no-such-folder/settings.json"));
        let folder = Path::new(r"B:\Lib\Chairs");
        set_mark(&mut settings, folder, "icon", Some("star"));
        set_mark(&mut settings, folder, "color", Some("#E5484D"));
        assert_eq!(mark(&settings, &norm_key(folder)), (Some("star"), Some("#E5484D")));
        set_mark(&mut settings, folder, "icon", None);
        set_mark(&mut settings, folder, "color", None);
        assert_eq!(settings.data[MARKS], json!({}));
    }
}
