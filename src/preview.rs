//! Explicit development-only visual fixture. Never supplies authentication and
//! never loads without ASTER_PREVIEW_DATA. The user's atlas remains in work/.
use crate::jellyfin::Item;
use eframe::egui::{self, Pos2, Rect, Vec2};
use serde::Deserialize;
#[derive(Clone, Deserialize)]
pub struct PreviewShelf {
    pub title: String,
    pub landscape: bool,
    pub start: usize,
    pub count: usize,
}
#[derive(Deserialize)]
struct Fixture {
    items: Vec<Item>,
    atlas: String,
    posters: Vec<[f32; 4]>,
    backdrop: Option<[f32; 4]>,
    #[serde(default)]
    shelves: Vec<PreviewShelf>,
}
pub struct Preview {
    pub items: Vec<Item>,
    pub shelves: Vec<PreviewShelf>,
    texture: egui::TextureHandle,
    posters: Vec<Rect>,
    backdrop: Option<Rect>,
}
impl Preview {
    pub fn load(path: &str, ctx: &egui::Context) -> Result<Self, String> {
        let f: Fixture = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let image = image::open(f.atlas).map_err(|e| e.to_string())?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let dims = Vec2::new(size[0] as f32, size[1] as f32);
        let texture = ctx.load_texture(
            "visual-fixture",
            egui::ColorImage::from_rgba_unmultiplied(size, &image),
            egui::TextureOptions::LINEAR,
        );
        let uv = |r: [f32; 4]| {
            Rect::from_min_max(
                Pos2::new(r[0] / dims.x, r[1] / dims.y),
                Pos2::new(r[2] / dims.x, r[3] / dims.y),
            )
        };
        Ok(Self {
            items: f.items,
            shelves: f.shelves,
            texture,
            posters: f.posters.into_iter().map(uv).collect(),
            backdrop: f.backdrop.map(uv),
        })
    }
    pub fn artwork(&self, item: &Item, backdrop: bool) -> Option<(egui::Image<'static>, Rect)> {
        let index = item.id.strip_prefix("preview-")?.parse::<usize>().ok()?;
        let uv = if backdrop {
            self.backdrop?
        } else {
            *self.posters.get(index % self.posters.len())?
        };
        Some((
            egui::Image::new((self.texture.id(), self.texture.size_vec2())),
            uv,
        ))
    }
}
