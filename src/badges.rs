// SPDX-License-Identifier: GPL-3.0-or-later
//! What is drawn over the images of the cards: the renderers, the warning, the maps of a material, its shape, its
//! resolution, and the material ball.

use crate::kit;
use crate::maxfile::Engine;
use crate::surfaces::{Aspect, Channel};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, TextureHandle, Vec2};

/// Behind the white texts.
const DARK: Color32 = Color32::from_rgba_premultiplied(17, 20, 29, 215);
const EDGE: Color32 = Color32::from_black_alpha(70);

/// How large they are on an image of this side.
pub fn size(side: f32) -> f32 {
    (side / 7.0).clamp(20.0, 28.0)
}

fn logo_bytes(engine: Engine) -> Option<&'static [u8]> {
    match engine {
        Engine::Corona => Some(include_bytes!("../assets/engine_corona.png")),
        Engine::VRay => Some(include_bytes!("../assets/engine_vray.png")),
        Engine::FStorm => Some(include_bytes!("../assets/engine_fstorm.png")),
        Engine::Other => None,
    }
}

/// The logo of a renderer. The logos are of their owners: they only say which renderer a file is for.
fn logo(ctx: &egui::Context, engine: Engine) -> Option<TextureHandle> {
    let id = egui::Id::unique(("engine-logo", engine.key()));
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let image = image::load_from_memory(logo_bytes(engine)?).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let options = egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
    let texture = ctx.load_texture(engine.key(), egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), options);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}

fn corner(value: f32) -> CornerRadius {
    CornerRadius::same(value.round().clamp(0.0, 255.0) as u8)
}

/// A renderer: its logo, faded when it is not that of the scene; a sign for another one or one not read yet.
pub fn engine(painter: &egui::Painter, rect: Rect, engine: Option<Engine>, faded: bool) {
    let round = corner(rect.width() * 0.24);
    painter.rect_filled(rect, round, Color32::WHITE);
    match engine.and_then(|e| logo(painter.ctx(), e)) {
        Some(texture) => {
            let tint = if faded { Color32::from_white_alpha(97) } else { Color32::WHITE };
            let uv = Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0));
            painter.add(egui::epaint::RectShape::filled(rect, round, tint).with_texture(texture.id(), uv));
            painter.rect_stroke(rect, round, Stroke::new(1.0, Color32::from_white_alpha(150)), StrokeKind::Inside);
        }
        None => {
            painter.rect_stroke(rect, round, Stroke::new(1.0, Color32::from_black_alpha(45)), StrokeKind::Inside);
            let text = if engine.is_some() { "?" } else { "…" };
            let font = kit::strong(rect.height() * 0.58);
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, font, Color32::from_gray(0x7A));
        }
    }
}

/// The yellow triangle of the files that are not found.
pub fn warning(painter: &egui::Painter, rect: Rect) {
    let dark = Color32::from_rgb(0x22, 0x22, 0x22);
    let triangle = vec![rect.center_top(), rect.right_bottom(), rect.left_bottom()];
    painter.add(egui::Shape::convex_polygon(triangle, kit::palette(true).warn, Stroke::new(1.5, Color32::from_black_alpha(150))));
    let at = rect.center() + Vec2::new(0.0, rect.height() * 0.13);
    painter.text(at, egui::Align2::CENTER_CENTER, "!", kit::strong(rect.height() * 0.62), dark);
}

/// A text in a shape with round ends; where it ends.
pub fn pill(painter: &egui::Painter, at: Pos2, size: f32, text: &str, (back, ink): (Color32, Color32)) -> f32 {
    let galley = painter.layout_no_wrap(text.to_string(), kit::strong(size * 0.48), ink);
    let rect = Rect::from_min_size(at, Vec2::new((galley.size().x + size * 0.7).max(size * 1.25), size));
    painter.rect_filled(rect, corner(size / 2.0), back);
    painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
    rect.right()
}

/// The highest resolution; where it ends.
pub fn resolution(painter: &egui::Painter, at: Pos2, size: f32, top: u32) -> f32 {
    pill(painter, at, size, &format!("{top}K"), (DARK, Color32::WHITE))
}

/// A map of a material: a circle of its colour with its letters.
pub fn channel(painter: &egui::Painter, rect: Rect, channel: Channel) {
    let (letters, [r, g, b]) = channel.badge();
    painter.circle(rect.center(), rect.width() / 2.0, Color32::from_rgb(r, g, b), Stroke::new(1.0, EDGE));
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, letters, kit::strong(rect.height() * 0.48), Color32::WHITE);
}

/// The shape of the texture, filled (white when it is square, in the colour of the app when it is not), and its
/// proportion; `at` is its top right corner.
pub fn shape(painter: &egui::Painter, at: Pos2, size: f32, aspect: Aspect, text: &str) {
    let galley = painter.layout_no_wrap(text.to_string(), kit::strong(size * 0.48), Color32::WHITE);
    let unit = size * 0.5;
    let (w, h) = match aspect {
        Aspect::Square => (unit, unit),
        Aspect::Wide => (unit * 1.6, unit * 0.8),
        Aspect::Tall => (unit * 0.6, unit),
    };
    let width = size * 0.45 + w + 5.0 + galley.size().x + size * 0.4;
    let rect = Rect::from_min_size(egui::pos2(at.x - width, at.y), Vec2::new(width, size));
    painter.rect_filled(rect, corner(size / 2.0), DARK);
    let middle = egui::pos2(rect.left() + size * 0.45 + w / 2.0, rect.center().y);
    let color = if aspect == Aspect::Square { Color32::WHITE } else { kit::palette(true).accent };
    let drawn = Rect::from_center_size(middle, Vec2::new(w, h));
    painter.rect_filled(drawn, corner(1.5), color);
    painter.galley(egui::pos2(drawn.right() + 5.0, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
}

/// The shades of the ball from its light to its edge: where, how white or black, and how much.
const SHADES: [(f32, u8, f32); 5] = [(0.0, 255, 75.0), (0.22, 255, 12.0), (0.55, 0, 25.0), (0.85, 0, 120.0), (1.0, 0, 205.0)];

fn shade(t: f32) -> Color32 {
    let next = SHADES.iter().position(|(at, ..)| *at >= t).unwrap_or(SHADES.len() - 1).max(1);
    let ((a, from, alpha_a), (b, to, alpha_b)) = (SHADES[next - 1], SHADES[next]);
    let part = ((t - a) / (b - a)).clamp(0.0, 1.0);
    let grey = f32::from(from) + (f32::from(to) - f32::from(from)) * part;
    let alpha = alpha_a + (alpha_b - alpha_a) * part;
    Color32::from_rgba_unmultiplied(grey as u8, grey as u8, grey as u8, alpha as u8)
}

/// How far a point of the unit circle is from the light, as a part of the way to the edge.
fn from_light(point: Vec2, light: Vec2) -> f32 {
    let way = point - light;
    let a = way.length_sq();
    if a < 1e-9 {
        return 0.0;
    }
    let b = light.dot(way);
    let reach = (-b + (b * b + a * (1.0 - light.length_sq())).max(0.0).sqrt()) / a;
    (1.0 / reach.max(1e-6)).min(1.0)
}

fn disc(mesh: &mut egui::Mesh, rings: u32, mut vertex: impl FnMut(Vec2) -> egui::epaint::Vertex) {
    const SIDES: u32 = 48;
    for ring in 0..=rings {
        let r = ring as f32 / rings as f32;
        for side in 0..=SIDES {
            let angle = side as f32 / SIDES as f32 * std::f32::consts::TAU;
            mesh.vertices.push(vertex(Vec2::angled(angle) * r));
        }
    }
    for ring in 0..rings {
        for side in 0..SIDES {
            let a = ring * (SIDES + 1) + side;
            let b = a + SIDES + 1;
            mesh.add_triangle(a, b, a + 1);
            mesh.add_triangle(a + 1, b, b + 1);
        }
    }
}

/// A material ball: the texture in a circle, with light from the upper left and a shadow below. Not a render.
pub fn ball(painter: &egui::Painter, area: Rect, texture: &TextureHandle) {
    let d = area.width().min(area.height()) * 0.84;
    let middle = area.center() - Vec2::new(0.0, d * 0.02);
    let plain = |pos: Pos2, color: Color32| egui::epaint::Vertex { pos, uv: egui::epaint::WHITE_UV, color };
    let mut shadow = egui::Mesh::default();
    let below = egui::pos2(middle.x, middle.y + d * 0.48);
    disc(&mut shadow, 6, |p| {
        let alpha = 110.0 * (1.0 - p.length());
        plain(below + Vec2::new(p.x * d * 0.5, p.y * d * 0.1), Color32::from_black_alpha(alpha as u8))
    });
    painter.add(egui::Shape::mesh(shadow));
    let pixels = texture.size_vec2();
    let crop = pixels.min_elem() / 2.0;
    let mut skin = egui::Mesh::with_texture(texture.id());
    disc(&mut skin, 1, |p| egui::epaint::Vertex {
        pos: middle + p * d / 2.0,
        uv: egui::pos2(0.5 + p.x * crop / pixels.x, 0.5 + p.y * crop / pixels.y),
        color: Color32::WHITE,
    });
    painter.add(egui::Shape::mesh(skin));
    let mut light = egui::Mesh::default();
    disc(&mut light, 24, |p| plain(middle + p * d / 2.0, shade(from_light(p, Vec2::new(-0.32, -0.44)))));
    painter.add(egui::Shape::mesh(light));
    painter.circle_stroke(middle, d / 2.0, Stroke::new(1.0, Color32::from_black_alpha(160)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_logo_can_be_read() {
        for engine in [Engine::Corona, Engine::VRay, Engine::FStorm] {
            assert!(image::load_from_memory(logo_bytes(engine).unwrap()).is_ok());
        }
        assert!(logo_bytes(Engine::Other).is_none());
    }

    #[test]
    fn the_light_of_the_ball_fades_to_its_edge() {
        let light = Vec2::new(-0.32, -0.44);
        assert!(from_light(light, light) < 0.01);
        assert!((from_light(Vec2::new(1.0, 0.0), light) - 1.0).abs() < 0.01);
        assert!(shade(0.0).r() > 60 && shade(1.0).a() > 200 && shade(1.0).r() == 0);
    }
}
