// SPDX-License-Identifier: GPL-3.0-or-later
//! The quick look: the image of a card, large, over the window; the arrows go to the cards beside it.

use crate::open3d_settings::Language;
use crate::task::{Task, Wake};
use crate::texts::Text;
use crate::thumbs;
use eframe::egui::{self, TextureHandle, TextureOptions, Vec2};
use image::RgbaImage;
use std::path::PathBuf;

/// The longest side of the image, in pixels.
const LARGE: u32 = 2048;

/// A card: its name and where its image is.
pub type Look = (String, Option<PathBuf>);

pub struct QuickLook {
    items: Vec<Look>,
    at: usize,
    task: Option<Task<Option<RgbaImage>>>,
    texture: Option<TextureHandle>,
    failed: bool,
    lang: Language,
    wake: Wake,
}

impl QuickLook {
    pub fn new(items: Vec<Look>, at: usize, lang: Language, wake: Wake) -> Self {
        let mut look = Self { items, at, task: None, texture: None, failed: false, lang, wake };
        look.load();
        look
    }

    fn load(&mut self) {
        (self.task, self.texture, self.failed) = (None, None, false);
        match self.items.get(self.at).and_then(|(_, src)| src.clone()) {
            Some(src) => self.task = Some(Task::start(self.wake.clone(), move |link| link.send(thumbs::load_large(&src, LARGE)))),
            None => self.failed = true,
        }
    }

    fn step(&mut self, by: isize) {
        let to = self.at.saturating_add_signed(by).min(self.items.len().saturating_sub(1));
        if to != self.at {
            self.at = to;
            self.load();
        }
    }

    /// The card shown.
    pub fn at(&self) -> usize {
        self.at
    }

    /// Draws it over the whole window; `false` when it is closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        if let Some(image) = self.task.as_ref().and_then(|t| t.poll().pop()) {
            self.task = None;
            self.failed = image.is_none();
            self.texture = image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture("quick-look", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
        }
        let whole = ctx.content_rect();
        let layer = egui::Area::new(egui::Id::unique("openlibrary-quick-look")).order(egui::Order::Foreground).fixed_pos(whole.min);
        let clicked = layer.show(ctx, |ui| self.paint(ui, whole)).inner;
        let (by, closed) = ctx.input(|i| {
            let pressed = |keys: &[egui::Key]| keys.iter().any(|k| i.key_pressed(*k));
            let wheel: f32 = i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum();
            let next = pressed(&[egui::Key::ArrowRight, egui::Key::ArrowDown]) || wheel < 0.0;
            let back = pressed(&[egui::Key::ArrowLeft, egui::Key::ArrowUp]) || wheel > 0.0;
            (isize::from(next) - isize::from(back), pressed(&[egui::Key::Space, egui::Key::Escape, egui::Key::Enter]))
        });
        if by != 0 {
            self.step(by);
        }
        !(clicked || closed)
    }

    /// `true` when it was clicked.
    fn paint(&self, ui: &mut egui::Ui, whole: egui::Rect) -> bool {
        let response = ui.allocate_rect(whole, egui::Sense::click());
        let painter = ui.painter_at(whole);
        painter.rect_filled(whole, 0.0, egui::Color32::from_black_alpha(242));
        let (light, dim) = (egui::Color32::from_gray(238), egui::Color32::from_gray(150));
        let room = egui::Rect::from_min_max(whole.min + Vec2::new(40.0, 30.0), whole.max - Vec2::new(40.0, 84.0));
        match &self.texture {
            Some(texture) => {
                let size = texture.size_vec2();
                let scale = (room.width() / size.x).min(room.height() / size.y).min(2.0);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(texture.id(), egui::Rect::from_center_size(room.center(), size * scale), uv, egui::Color32::WHITE);
            }
            None => {
                let text = if self.failed { Text::NoPreview } else { Text::Loading };
                painter.text(room.center(), egui::Align2::CENTER_CENTER, text.get(self.lang), egui::FontId::proportional(14.0), dim);
            }
        }
        let name = self.items.get(self.at).map_or("", |(name, _)| name.as_str());
        let at = egui::pos2(whole.center().x, whole.bottom() - 54.0);
        painter.text(at, egui::Align2::CENTER_CENTER, name, crate::kit::strong(17.0), light);
        let note = format!("{} / {}  ·  {}", self.at + 1, self.items.len(), Text::LookKeys.get(self.lang));
        painter.text(at + Vec2::new(0.0, 24.0), egui::Align2::CENTER_CENTER, note, egui::FontId::proportional(12.0), dim);
        response.clicked()
    }
}
