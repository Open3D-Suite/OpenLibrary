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

    /// Draws it; `false` when it is closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        if let Some(image) = self.task.as_ref().and_then(|t| t.poll().pop()) {
            self.task = None;
            self.failed = image.is_none();
            self.texture = image.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture("quick-look", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), TextureOptions::LINEAR)
            });
        }
        let room = ctx.content_rect().size() - Vec2::splat(120.0);
        let response = egui::Modal::new(egui::Id::unique("openlibrary-quick-look")).show(ctx, |ui| {
            if let Some((name, _)) = self.items.get(self.at) {
                ui.heading(name);
            }
            match &self.texture {
                Some(texture) => {
                    let size = texture.size_vec2();
                    let scale = (room.x / size.x).min((room.y - 40.0) / size.y);
                    ui.image((texture.id(), size * scale));
                }
                None => {
                    ui.allocate_ui(Vec2::new(room.x.min(600.0), room.y.min(400.0)), |ui| {
                        let text = if self.failed { Text::NoPreview } else { Text::Loading };
                        ui.centered_and_justified(|ui| ui.label(text.get(self.lang)));
                    });
                }
            }
        });
        let (left, right, space) =
            ctx.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::Space)));
        if left {
            self.step(-1);
        }
        if right {
            self.step(1);
        }
        !(response.should_close() || space)
    }
}
