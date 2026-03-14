use eframe::egui::{self, ColorImage, Response, TextureHandle, TextureOptions, Ui, Vec2};

use crate::charting::scene::RenderedFrame;

pub fn color_image(frame: &RenderedFrame) -> ColorImage {
    ColorImage::from_rgb(
        [frame.width_px as usize, frame.height_px as usize],
        &frame.rgb,
    )
}

#[derive(Default)]
pub struct RetainedChartTexture {
    texture: Option<TextureHandle>,
}

impl RetainedChartTexture {
    pub fn clear(&mut self) {
        self.texture = None;
    }

    pub fn update(&mut self, ctx: &egui::Context, id: &str, frame: &RenderedFrame) {
        let image = color_image(frame);
        let texture = self.texture.get_or_insert_with(|| {
            ctx.load_texture(id.to_string(), image.clone(), TextureOptions::LINEAR)
        });
        texture.set(image, TextureOptions::LINEAR);
    }

    pub fn show(&self, ui: &mut Ui, size: Vec2) -> Option<Response> {
        self.texture
            .as_ref()
            .map(|texture| ui.add(egui::Image::new(texture).fit_to_exact_size(size)))
    }
}
