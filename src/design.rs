use eframe::egui::{self, Color32, FontId, Pos2, Rect, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(230, 185, 157);
pub const FOCUS: Color32 = Color32::from_rgb(169, 211, 255);
pub const BG: Color32 = Color32::from_rgb(9, 10, 12);
pub const SURFACE: Color32 = Color32::from_rgb(25, 30, 38);
pub const MUTED: Color32 = Color32::from_rgb(151, 160, 174);
pub const TEXT: Color32 = Color32::from_rgb(238, 240, 244);
pub const GAP: f32 = 20.0;

/// Every card, title and progress bar shares this geometry. The height budget
/// prevents the first row from being taller than the usable viewport.
#[derive(Clone, Copy, Debug)]
pub struct CatalogGeometry {
    pub columns: usize,
    pub width: f32,
    pub poster_height: f32,
    pub card_height: f32,
    pub gap: f32,
}
impl CatalogGeometry {
    pub fn new(width: f32, height: f32) -> Self {
        Self::with_target(width, height, 218.0, GAP)
    }
    pub fn library(width: f32, height: f32) -> Self {
        Self::with_target(width, height, 184.0, 14.0)
    }
    fn with_target(width: f32, height: f32, target: f32, gap: f32) -> Self {
        let target = ((height - 70.0) / 1.5).clamp(140.0, target);
        let columns = (((width + gap) / (target + gap)).ceil() as usize).clamp(2, 24);
        let width = ((width - (columns - 1) as f32 * gap) / columns as f32).max(1.0);
        Self {
            columns,
            width,
            poster_height: width * 1.5,
            card_height: width * 1.5 + 68.0,
            gap,
        }
    }
    pub fn episodes(width: f32) -> Self {
        let gap = 20.0;
        let columns = (((width + gap) / 320.0).ceil() as usize).clamp(2, 12);
        let width = (width - (columns - 1) as f32 * gap) / columns as f32;
        Self {
            columns,
            width,
            poster_height: width * 9.0 / 16.0,
            card_height: width * 9.0 / 16.0 + 68.0,
            gap,
        }
    }
    pub fn card(&self, origin: Pos2, index: usize) -> Rect {
        Rect::from_min_size(
            origin
                + Vec2::new(
                    (index % self.columns) as f32 * (self.width + self.gap),
                    (index / self.columns) as f32 * (self.card_height + self.gap),
                ),
            Vec2::new(self.width, self.card_height),
        )
    }
    pub fn height(&self, count: usize) -> f32 {
        if count == 0 {
            0.0
        } else {
            count.div_ceil(self.columns) as f32 * (self.card_height + self.gap) - self.gap
        }
    }
}

pub fn cover_uv(source: Vec2, target: Vec2) -> Rect {
    if source.x <= 0.0 || source.y <= 0.0 || target.x <= 0.0 || target.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let source_aspect = source.x / source.y;
    let target_aspect = target.x / target.y;
    let fraction = if source_aspect > target_aspect {
        Vec2::new(target_aspect / source_aspect, 1.0)
    } else {
        Vec2::new(1.0, source_aspect / target_aspect)
    };
    Rect::from_center_size(Pos2::new(0.5, 0.5), fraction)
}

/// Paint a photo with object-fit: cover. Texture dimensions, rather than the
/// destination rectangle, determine the UV crop, so faces are never stretched.
pub fn cover(ui: &egui::Ui, image: egui::Image<'_>, rect: Rect, base_uv: Rect, radius: u8) -> bool {
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ui.ctx(), rect.size())
    else {
        return false;
    };
    let crop = cover_uv(texture.size * base_uv.size(), rect.size());
    let uv = Rect::from_min_max(
        base_uv.min + base_uv.size() * crop.min.to_vec2(),
        base_uv.min + base_uv.size() * crop.max.to_vec2(),
    );
    egui::Image::new(texture)
        .uv(uv)
        .corner_radius(radius)
        .paint_at(ui, rect);
    true
}

/// Text never paints outside its assigned box, even for long translated titles.
pub fn text(ui: &egui::Ui, rect: Rect, value: &str, size: f32, color: Color32, rows: usize) -> f32 {
    let mut job = egui::text::LayoutJob::simple(
        value.to_string(),
        FontId::proportional(size),
        color,
        rect.width(),
    );
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = rows == 1;
    let galley = ui.painter().layout_job(job);
    let height = galley.size().y;
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(rect))
        .galley(rect.min, galley, color);
    height
}

pub fn fade(ui: &egui::Ui, rect: Rect, stops: &[(f32, Color32)], vertical: bool) {
    let mut mesh = egui::Mesh::default();
    for pair in stops.windows(2) {
        let [(a, ca), (b, cb)] = pair else { continue };
        let r = if vertical {
            Rect::from_min_max(
                Pos2::new(rect.left(), rect.top() + rect.height() * a),
                Pos2::new(rect.right(), rect.top() + rect.height() * b),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(rect.left() + rect.width() * a, rect.top()),
                Pos2::new(rect.left() + rect.width() * b, rect.bottom()),
            )
        };
        let colors = if vertical {
            [*ca, *ca, *cb, *cb]
        } else {
            [*ca, *cb, *cb, *ca]
        };
        let start = mesh.vertices.len() as u32;
        for (pos, color) in [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ]
        .into_iter()
        .zip(colors)
        {
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: egui::epaint::WHITE_UV,
                color,
            });
        }
        mesh.indices
            .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

pub fn header_logo(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::click());
    ui.put(
        rect.shrink(1.0),
        egui::Image::new(egui::include_image!("../assets/aster-mark.png"))
            .fit_to_exact_size(Vec2::splat(30.0))
            .tint(egui::Color32::WHITE),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Aster home"));
    response.on_hover_text("Aster · Home")
}

pub fn keycap(ui: &mut egui::Ui, key: &str, label: &str) {
    let key_text = ui
        .painter()
        .layout_no_wrap(key.into(), FontId::proportional(10.5), ACCENT);
    let label_text = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(12.0), MUTED);
    let single = key.chars().count() == 1;
    let key_width = if single {
        16.0
    } else {
        key_text.size().x + 4.0
    };
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(key_width + 6.0 + label_text.size().x, 18.0),
        egui::Sense::hover(),
    );
    let key_rect = Rect::from_min_size(rect.min + Vec2::new(0.0, 1.0), Vec2::new(key_width, 16.0));
    if single {
        ui.painter().circle_stroke(
            key_rect.center(),
            7.5,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(64, 65, 70)),
        );
    }
    ui.painter()
        .galley(key_rect.center() - key_text.size() / 2.0, key_text, ACCENT);
    ui.painter().galley(
        rect.min + Vec2::new(key_width + 6.0, (18.0 - label_text.size().y) / 2.0),
        label_text,
        MUTED,
    );
}

/// Short ease-in/out rings retain a stable hit target while focus settles.
pub fn focus_ring(ui: &egui::Ui, rect: Rect, id: egui::Id, active: bool, radius: f32) {
    let t = ui.ctx().animate_bool_with_time(id, active, 0.16);
    if t <= 0.0 {
        return;
    }
    let t = t * t * (3.0 - 2.0 * t);
    let color = FOCUS.gamma_multiply(t);
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        radius,
        egui::Stroke::new(2.0_f32, color),
        egui::StrokeKind::Inside,
    );
}

pub fn progress_bar(ui: &egui::Ui, artwork: Rect, progress: f32) {
    if progress <= 0.0 {
        return;
    }
    let track = Rect::from_min_size(
        egui::pos2(artwork.left() + 11.0, artwork.bottom() - 11.0),
        Vec2::new((artwork.width() - 22.0).max(0.0), 3.0),
    );
    ui.painter()
        .rect_filled(track, 2.0, Color32::from_black_alpha(155));
    ui.painter().rect_filled(
        Rect::from_min_size(
            track.min,
            Vec2::new(track.width() * progress.clamp(0.0, 1.0), track.height()),
        ),
        2.0,
        ACCENT,
    );
}

/// Reveal the nearest clipped edge without recentering an already visible card.
pub fn reveal_horizontal(ui: &egui::Ui, rect: Rect) {
    let clip = ui.clip_rect().shrink2(Vec2::new(6.0, 0.0));
    let dx = if rect.left() < clip.left() {
        clip.left() - rect.left()
    } else if rect.right() > clip.right() {
        clip.right() - rect.right()
    } else {
        0.0
    };
    if dx != 0.0 {
        ui.scroll_with_delta(Vec2::new(dx, 0.0));
    }
}
/// Keep nested horizontal shelves out of a vertical focus-scroll request.
pub fn reveal_vertical(ui: &egui::Ui, rect: Rect) {
    ui.scroll_to_rect(
        Rect::from_x_y_ranges(ui.clip_rect().x_range(), rect.y_range()),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_preserves_geometry_and_never_stretches() {
        for (source, target) in [
            (Vec2::new(1920.0, 1080.0), Vec2::new(1300.0, 280.0)),
            (Vec2::new(400.0, 900.0), Vec2::new(210.0, 315.0)),
        ] {
            let uv = cover_uv(source, target);
            let visible = source * uv.size();
            assert!((visible.x / visible.y - target.x / target.y).abs() < 0.001);
            assert!(uv.min.x >= 0.0 && uv.min.y >= 0.0 && uv.max.x <= 1.0 && uv.max.y <= 1.0);
        }
    }
    #[test]
    fn cards_stay_inside_desktop_and_tv_widths() {
        for (width, height) in [
            (540.0, 260.0),
            (930.0, 300.0),
            (1340.0, 420.0),
            (1840.0, 600.0),
            (3760.0, 1400.0),
        ] {
            let g = CatalogGeometry::new(width, height);
            let last = g.card(Pos2::ZERO, g.columns - 1);
            assert!(last.right() <= width + 0.1);
            assert!((g.poster_height / g.width - 1.5).abs() < 0.001);
            assert_eq!(g.card(Pos2::ZERO, g.columns).top(), g.card_height + GAP);
        }
    }
}
