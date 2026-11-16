#[cfg(test)]
mod autoplay_tests;
mod browse;
mod config;
mod design;
mod dev_control;
mod embedded;
mod input;
mod jellyfin;
mod keyboard;
mod player;
mod preview;
mod seek_preview;
mod series;
mod ui;
mod vault;
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let mut fullscreen = None;
    let mut server = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--windowed" => fullscreen = Some(false),
            "--fullscreen" => fullscreen = Some(true),
            "--server" => {
                server = Some(args.next().unwrap_or_else(|| {
                    eprintln!("--server requires a URL");
                    std::process::exit(2);
                }))
            }
            "--version" | "-V" => {
                println!("Aster {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "Aster — a controller-first Jellyfin client\n\nUsage: aster [--windowed | --fullscreen] [--server URL]\n       aster --version\n\nD-pad / left stick: navigate | A: select | B: back\nX: settings | Y: search | LB/RB: sections\nKeyboard: arrows, Enter, Escape, F1 settings, F3 search, F11 fullscreen"
                );
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {arg}. Use --help for usage.");
                std::process::exit(2);
            }
        }
    }
    let (mut settings, warning) = match config::Settings::load() {
        Ok(s) => (s, None),
        Err(e) => (
            config::Settings::default(),
            Some(format!(
                "Settings could not be read: {e}. Your existing file has not been deleted."
            )),
        ),
    };
    if let Some(server) = server {
        settings.server = jellyfin::normalize_server(&server)
            .map(|u| u.to_string())
            .unwrap_or(server);
    }
    if let Some(value) = fullscreen {
        settings.fullscreen = value;
    }
    let size = std::env::var("ASTER_WINDOW_SIZE")
        .ok()
        .and_then(|s| {
            s.split_once('x')
                .and_then(|(w, h)| Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?]))
        })
        .unwrap_or([1440.0, 900.0]);
    let title = std::env::var("ASTER_WINDOW_TITLE").unwrap_or_else(|_| "Aster".into());
    eframe::run_native(
        &title,
        eframe::NativeOptions {
            // Mesa/Wayland swap waits can block the UI event loop while a
            // window is occluded. The compositor still presents at vblank;
            // Aster and libmpv already schedule frames at display cadence.
            vsync: std::env::var_os("WAYLAND_DISPLAY").is_none(),
            viewport: eframe::egui::ViewportBuilder::default()
                .with_icon({
                    let image = image::load_from_memory(include_bytes!("../assets/aster.png"))
                        .expect("bundled Aster icon")
                        .into_rgba8();
                    egui_icon(image)
                })
                .with_app_id("aster")
                .with_active(std::env::var_os("ASTER_WINDOW_INACTIVE").is_none())
                .with_transparent(true)
                .with_inner_size(size)
                .with_min_inner_size([960.0, 640.0])
                .with_fullscreen(fullscreen.unwrap_or(settings.fullscreen)),
            ..Default::default()
        },
        Box::new(|cc| {
            let embedded = embedded::Embedded::new(cc, &settings);
            let mut app = ui::Aster::new(&cc.egui_ctx, settings);
            match embedded {
                Ok(player) => app.set_embedded(player),
                Err(e) => app.startup_warning(e),
            }
            if let Some(w) = warning {
                app.startup_warning(w);
            }
            Ok(Box::new(app))
        }),
    )
}

fn egui_icon(image: image::RgbaImage) -> eframe::egui::IconData {
    let (width, height) = image.dimensions();
    eframe::egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
