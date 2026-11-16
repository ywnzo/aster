//! Authenticated Jellyfin trickplay tiles, with private libmpv frame snapshots
//! for libraries that have not generated trickplay images. Work stays off the UI.
use crate::jellyfin::Api;
use eframe::egui::{self, Color32, Rect, Vec2};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub thumbnail_count: u32,
    pub interval: u32,
    #[serde(skip)]
    pub media_source: String,
}
impl Grid {
    pub fn select(value: &Value, item: &str) -> Option<Self> {
        let sources = value.get("Trickplay")?.as_object()?;
        let (source, sizes) = sources
            .get_key_value(item)
            .or_else(|| sources.iter().next())?;
        let mut grids: Vec<Self> = sizes
            .as_object()?
            .values()
            .filter_map(|v| serde_json::from_value::<Self>(v.clone()).ok())
            .filter(|g| {
                g.width > 0
                    && g.height > 0
                    && g.tile_width > 0
                    && g.tile_height > 0
                    && g.thumbnail_count > 0
                    && g.interval > 0
                    && g.width.saturating_mul(g.tile_width) <= 8192
                    && g.height.saturating_mul(g.tile_height) <= 8192
            })
            .collect();
        grids.sort_by_key(|g| g.width.abs_diff(320));
        let mut grid = grids.into_iter().next()?;
        grid.media_source = source.clone();
        Some(grid)
    }
    fn location(&self, seconds: f64) -> (u32, u32, u32) {
        let frame = ((seconds.max(0.0) * 1000.0 / self.interval as f64).floor() as u32)
            .min(self.thumbnail_count - 1);
        let per_sheet = self.tile_width.saturating_mul(self.tile_height).max(1);
        let within = frame % per_sheet;
        (
            frame / per_sheet,
            (within % self.tile_width) * self.width,
            (within / self.tile_width) * self.height,
        )
    }
}
/// Jellyfin chapter positions use 100 ns ticks; libmpv uses seconds.
fn chapter_entries(metadata: &Value) -> Vec<Value> {
    let Some(chapters) = metadata["Chapters"].as_array() else {
        return Vec::new();
    };
    let mut entries: Vec<_> = chapters
        .iter()
        .filter_map(|chapter| {
            let ticks = chapter["StartPositionTicks"].as_u64()?;
            let title = chapter["Name"].as_str().unwrap_or("").trim().to_owned();
            Some((ticks, title))
        })
        .collect();
    entries.sort_by_key(|(ticks, _)| *ticks);
    entries.dedup_by_key(|(ticks, _)| *ticks);
    entries.into_iter().enumerate().map(|(index,(ticks,title))|json!({"time":ticks as f64/10_000_000.0,"title":if title.is_empty() {format!("Chapter {}",index+1)}else{title}})).collect()
}
struct Frame {
    image: egui::ColorImage,
    requested: f64,
    actual: f64,
    source: &'static str,
}
struct SnapshotFile(PathBuf);
impl Drop for SnapshotFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn ipc(socket: &Path, args: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "{}", json!({"command":args,"request_id":912})).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    while reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        let reply: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if reply["request_id"] == 912 {
            return if reply["error"] == "success" {
                Ok(reply["data"].clone())
            } else {
                Err("Preview frame is not ready.".into())
            };
        }
        line.clear();
    }
    Err("Preview player closed.".into())
}
fn snapshot(socket: &Path, path: &Path, target: f64) -> Result<Frame, String> {
    if ipc(socket, json!(["get_property", "seeking"]))? == true {
        return Err("Seek frame is still decoding.".into());
    }
    let actual = ipc(socket, json!(["get_property", "time-pos"]))?
        .as_f64()
        .ok_or("No video frame yet.")?;
    if (actual - target).abs() > 1.5 {
        return Err("Hover preview requires trickplay data.".into());
    }
    ipc(socket, json!(["screenshot-to-file", path, "video"]))?;
    let image = image::open(path)
        .map_err(|e| e.to_string())?
        .thumbnail(320, 180)
        .into_rgba8();
    Ok(Frame {
        image: egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        requested: target,
        actual,
        source: "decoded-frame",
    })
}

pub struct SeekPreviews {
    requests: Sender<f64>,
    results: Receiver<Option<Frame>>,
    texture: Option<egui::TextureHandle>,
    requested: f64,
    actual: f64,
    last_sent: Option<(f64, Instant)>,
    source: Option<&'static str>,
}
impl SeekPreviews {
    pub fn new(api: Api, socket: PathBuf, item: String, ctx: egui::Context) -> Self {
        let (requests, receive) = mpsc::channel::<f64>();
        let (send, results) = mpsc::channel();
        std::thread::spawn(move || {
            let snapshot_file = SnapshotFile(
                socket
                    .parent()
                    .unwrap()
                    .join(format!("seek-preview-{}.jpg", uuid::Uuid::new_v4())),
            );
            let metadata = api.seek_metadata(&item).unwrap_or(Value::Null);
            let mut grid = Grid::select(&metadata, &item);
            let chapters = chapter_entries(&metadata);
            // Lua checks the active file and existing chapters atomically when
            // this message is handled, so a slow metadata response cannot
            // populate the following episode with the previous file's chapters.
            if !chapters.is_empty() {
                let _ = ipc(
                    &socket,
                    json!([
                        "script-message",
                        "aster-chapters",
                        item,
                        serde_json::to_string(&chapters).unwrap_or_default()
                    ]),
                );
            }
            let mut sheets = VecDeque::<(u32, image::RgbaImage)>::new();
            while let Ok(mut target) = receive.recv() {
                // Debounce fast seek repeats and discard obsolete requested positions.
                std::thread::sleep(Duration::from_millis(80));
                for newer in receive.try_iter() {
                    target = newer;
                }
                let mut frame = None;
                if let Some(g) = &grid {
                    let (index, x, y) = g.location(target);
                    if !sheets.iter().any(|(i, _)| *i == index)
                        && let Ok(bytes) = api.seek_sheet(&item, g, index)
                        && let Ok(image) = image::load_from_memory(&bytes)
                    {
                        if sheets.len() == 3 {
                            sheets.pop_front();
                        }
                        sheets.push_back((index, image.into_rgba8()));
                    }
                    if let Some((_, sheet)) = sheets.iter().find(|(i, _)| *i == index)
                        && x.saturating_add(g.width) <= sheet.width()
                        && y.saturating_add(g.height) <= sheet.height()
                    {
                        let image =
                            image::imageops::crop_imm(sheet, x, y, g.width, g.height).to_image();
                        frame = Some(Frame {
                            image: egui::ColorImage::from_rgba_unmultiplied(
                                [g.width as usize, g.height as usize],
                                image.as_raw(),
                            ),
                            requested: target,
                            actual: target,
                            source: "trickplay",
                        });
                    } else {
                        grid = None;
                    }
                }
                if frame.is_none() {
                    frame = snapshot(&socket, &snapshot_file.0, target).ok();
                }
                if send.send(frame).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            requests,
            results,
            texture: None,
            requested: -1.0,
            actual: 0.0,
            last_sent: None,
            source: None,
        }
    }
    pub fn source(&self) -> Option<&'static str> {
        self.source
    }
    pub fn timestamp(&self) -> Option<f64> {
        self.source.map(|_| self.actual)
    }
    pub fn paint(&mut self, ui: &egui::Ui, video: Rect, target: f64, duration: f64) {
        for result in self.results.try_iter() {
            if let Some(frame) = result {
                self.requested = frame.requested;
                self.actual = frame.actual;
                self.source = Some(frame.source);
                if let Some(texture) = &mut self.texture {
                    texture.set(frame.image, egui::TextureOptions::LINEAR);
                } else {
                    self.texture = Some(ui.ctx().load_texture(
                        "seek-thumbnail",
                        frame.image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            } else {
                self.requested = -1.0;
                self.source = None;
            }
        }
        if self.last_sent.is_none_or(|(old, when)| {
            ((old - target).abs() >= 0.45 || self.source.is_none())
                && when.elapsed() >= Duration::from_millis(250)
        }) {
            let _ = self.requests.send(target);
            self.last_sent = Some((target, Instant::now()));
        }
        if (self.requested - target).abs() > 1.5 {
            return;
        }
        let Some(texture) = &self.texture else { return };
        let width = (video.width() * 0.19).clamp(160.0, 260.0);
        let image_height = width * 9.0 / 16.0;
        let scale = video.height() / 720.0;
        let left = video.left() + 64.0 * scale;
        let right = video.right() - 70.0 * scale;
        let center = egui::lerp(
            left..=right,
            (target / duration.max(0.01)).clamp(0.0, 1.0) as f32,
        )
        .clamp(
            video.left() + width / 2.0 + 12.0,
            video.right() - width / 2.0 - 12.0,
        );
        let bottom = video.bottom() - 115.0 * scale;
        let card = Rect::from_min_size(
            egui::pos2(center - width / 2.0, bottom - image_height - 27.0),
            Vec2::new(width, image_height + 27.0),
        );
        ui.painter()
            .rect_filled(card.expand(3.0), 7.0, Color32::from_rgb(13, 17, 23));
        ui.painter().rect_stroke(
            card.expand(3.0),
            7.0,
            egui::Stroke::new(1.0_f32, crate::design::FOCUS),
            egui::StrokeKind::Inside,
        );
        let image_rect = Rect::from_min_size(card.min, Vec2::new(width, image_height));
        egui::Image::new((texture.id(), texture.size_vec2()))
            .corner_radius(5)
            .paint_at(ui, image_rect);
        let seconds = self.actual.max(0.0) as u64;
        ui.painter().text(
            egui::pos2(center, card.bottom() - 13.0),
            egui::Align2::CENTER_CENTER,
            format!("{}:{:02}", seconds / 60, seconds % 60),
            egui::FontId::proportional(12.0),
            crate::design::TEXT,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_chapters_are_sorted_deduplicated_and_converted_to_seconds() {
        let entries = chapter_entries(
            &json!({"Chapters":[{"StartPositionTicks":100000000,"Name":"Main story"},{"StartPositionTicks":0,"Name":" "},{"StartPositionTicks":100000000,"Name":"Duplicate"},{"StartPositionTicks":-10,"Name":"Invalid"}]}),
        );
        assert_eq!(
            entries,
            json!([{"time":0.0,"title":"Chapter 1"},{"time":10.0,"title":"Main story"}])
                .as_array()
                .unwrap()
                .clone()
        );
        assert!(chapter_entries(&Value::Null).is_empty());
    }
    #[test]
    fn sprite_coordinates_cross_rows_sheets_and_clamp_to_last_thumbnail() {
        let grid = Grid {
            width: 160,
            height: 90,
            tile_width: 2,
            tile_height: 2,
            thumbnail_count: 7,
            interval: 1000,
            ..Default::default()
        };
        assert_eq!(grid.location(0.0), (0, 0, 0));
        assert_eq!(grid.location(2.0), (0, 0, 90));
        assert_eq!(grid.location(4.0), (1, 0, 0));
        assert_eq!(grid.location(999.0), (1, 0, 90));
    }
    #[test]
    fn metadata_rejects_invalid_grids_and_prefers_the_playing_media_source() {
        assert!(Grid::select(&json!({"Trickplay":{"id":{"320":{"Width":320}}}}), "id").is_none());
        let data = json!({"Trickplay":{"other":{"160":{"Width":160,"Height":90,"TileWidth":2,"TileHeight":2,"ThumbnailCount":4,"Interval":1000}},"id":{"320":{"Width":320,"Height":180,"TileWidth":2,"TileHeight":2,"ThumbnailCount":4,"Interval":1000}}}});
        let grid = Grid::select(&data, "id").unwrap();
        assert_eq!(grid.width, 320);
        assert_eq!(grid.media_source, "id");
    }
}
