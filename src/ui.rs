mod cinema;
mod inline_series;
use crate::{
    browse::{self, Filter, Sort},
    config::Settings,
    design,
    dev_control::{Connection, Control},
    input::{Action, Input},
    jellyfin::{Api, Item, LibraryPage, Session},
    keyboard::{KeyAction, Keyboard},
    player::{Player, Status},
    preview::Preview,
    series::{Route, Snapshot},
    vault,
};
use design::{ACCENT, BG, MUTED};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use serde_json::json;
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
const TABS: [&str; 6] = [
    "Discover",
    "Movies",
    "Series",
    "Continue watching",
    "Favorites",
    "Settings",
];
enum Message {
    InlineSeasons(String, Result<LibraryPage, String>),
    InlineEpisodes(String, String, bool, Result<LibraryPage, String>),
    Dashboard(u64, Result<Vec<(String, bool, Vec<Item>)>, String>),
    Details(String, Result<Box<Item>, String>),
    Login(u64, Result<(Api, Session), String>),
    Items(u64, bool, Result<LibraryPage, String>),
    Quick(u64, Result<(String, String), String>),
    Poll(u64, Result<Option<(Api, Session)>, String>),
    Note(Result<(), String>),
    Flag(String, bool, bool, Result<(), String>),
    SignedOut(Result<(), String>),
}
#[derive(Clone)]
struct UiSender {
    tx: Sender<Message>,
    ctx: egui::Context,
}
impl UiSender {
    fn send(&self, message: Message) -> Result<(), ()> {
        let result = self.tx.send(message).map_err(|_| ());
        self.ctx.request_repaint();
        result
    }
}
pub struct Aster {
    preview_context: egui::Context,
    last_frame: Instant,
    ui_frames: u64,
    uptime: Instant,
    inline: inline_series::InlineSeries,
    detail_parent: Option<Item>,
    dev_action: bool,
    page_key: String,
    page_started: Instant,
    shelves: Vec<cinema::Shelf>,
    detail_requested: String,
    detail_scroll: f32,
    detail_scroll_delta: f32,
    settings: Settings,
    api: Option<Api>,
    input: Input,
    tx: UiSender,
    rx: Receiver<Message>,
    tab: usize,
    focus: usize,
    items: Vec<Item>,
    detail: Option<Item>,
    parents: Vec<Snapshot>,
    username: String,
    password: String,
    search: String,
    busy: bool,
    error: String,
    notice: String,
    quick: Option<(String, String)>,
    polling: bool,
    last_poll: Instant,
    keyboard: Option<Keyboard>,
    player: Option<Player>,
    embedded: Option<crate::embedded::Embedded>,
    player_fullscreen: Option<bool>,
    player_overlay: serde_json::Value,
    seek_previews: Option<crate::seek_preview::SeekPreviews>,
    playing: String,
    position: f64,
    duration: f64,
    paused: bool,
    up_next: Option<(String, u64)>,
    focus_after_load: Option<String>,
    tracks: Vec<crate::player::Track>,
    generation: u64,
    auth_generation: u64,
    columns: usize,
    total: usize,
    route: Route,
    detail_focus: usize,
    focus_changed: bool,
    capture_frames: u32,
    capture_sent: bool,
    preview: Option<Preview>,
    control: Option<Control>,
    live_capture: Option<String>,
    feature_index: usize,
    shelf_offsets: Vec<(String, f32)>,
    browse_options: [browse::Options; 6],
    browse_menu: Option<(browse::Menu, usize)>,
    list_view: bool,
    search_deadline: Option<Instant>,
    search_bounds: Option<egui::Rect>,
    reset_catalog_scroll: bool,
    login_bounds: Option<egui::Rect>,
    login_fields: [Option<egui::Rect>; 3],
    login_submit_bounds: Option<egui::Rect>,
    hero_bounds: Option<egui::Rect>,
    catalog_bounds: Option<egui::Rect>,
    focus_bounds: Option<egui::Rect>,
}
impl Aster {
    pub fn set_embedded(&mut self, player: crate::embedded::Embedded) {
        self.embedded = Some(player);
    }
    pub fn startup_warning(&mut self, warning: String) {
        self.error = warning;
    }
    pub fn new(ctx: &egui::Context, settings: Settings) -> Self {
        egui_extras::install_image_loaders(ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = Color32::from_rgb(25, 29, 36);
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke = Stroke::new(1.0_f32, BG);
        visuals.extreme_bg_color = Color32::from_rgb(14, 18, 24);
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(7);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(7);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(7);
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(25, 27, 31);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(37, 40, 46);
        visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        visuals.widgets.active.bg_fill = ACCENT;
        visuals.widgets.hovered.bg_stroke = Stroke::new(2.0_f32, design::FOCUS);
        ctx.set_visuals(visuals);
        ctx.style_mut(|style| style.always_scroll_the_only_direction = false);
        ctx.set_zoom_factor(
            settings.ui_scale / ctx.native_pixels_per_point().unwrap_or(1.0).clamp(0.1, 1.0),
        );
        let (tx, rx) = mpsc::channel();
        let tx = UiSender {
            tx,
            ctx: ctx.clone(),
        };
        let input = Input::new(ctx.clone());
        let mut app = Self {
            preview_context: ctx.clone(),
            last_frame: Instant::now() - Duration::from_millis(16),
            ui_frames: 0,
            uptime: Instant::now(),
            settings,
            api: None,
            input,
            tx,
            rx,
            tab: 0,
            focus: 0,
            inline: inline_series::InlineSeries::default(),
            detail_parent: None,
            dev_action: false,
            page_key: String::new(),
            page_started: Instant::now(),
            shelves: Vec::new(),
            detail_requested: String::new(),
            detail_scroll: 0.0,
            detail_scroll_delta: 0.0,
            items: Vec::new(),
            detail: None,
            parents: Vec::new(),
            username: String::new(),
            password: String::new(),
            search: String::new(),
            busy: false,
            error: String::new(),
            notice: String::new(),
            quick: None,
            polling: false,
            last_poll: Instant::now(),
            keyboard: None,
            player: None,
            embedded: None,
            player_fullscreen: None,
            player_overlay: serde_json::Value::Null,
            seek_previews: None,
            playing: String::new(),
            position: 0.0,
            duration: 0.0,
            paused: false,
            up_next: None,
            focus_after_load: None,
            tracks: Vec::new(),
            generation: 0,
            auth_generation: 0,
            columns: 5,
            total: 0,
            route: Route::Library,
            detail_focus: 0,
            focus_changed: true,
            capture_frames: 0,
            capture_sent: false,
            preview: None,
            control: None,
            live_capture: None,
            feature_index: 0,
            shelf_offsets: Vec::new(),
            browse_options: [browse::Options::default(); 6],
            browse_menu: None,
            list_view: false,
            search_deadline: None,
            search_bounds: None,
            reset_catalog_scroll: false,
            login_bounds: None,
            login_fields: [None; 3],
            login_submit_bounds: None,
            hero_bounds: None,
            catalog_bounds: None,
            focus_bounds: None,
        };
        if app.settings.remember_session
            && !app.settings.server.is_empty()
            && std::env::var_os("ASTER_DEV_RESTORE").is_none()
            && std::env::var_os("ASTER_PREVIEW_DATA").is_none()
        {
            let server = app.settings.server.clone();
            let device = app.settings.device_id.clone();
            let private = app.settings.private_token_file;
            app.work(move || {
                Message::Login(
                    0,
                    (|| {
                        let session = vault::load(&server, private)?;
                        let mut api = Api::new(&server, &device)?;
                        api.session = Some(session.clone());
                        Ok((api, session))
                    })(),
                )
            });
        }
        if std::env::var("ASTER_CAPTURE_SCREEN").as_deref() == Ok("settings") {
            app.settings.server = "https://jellyfin.example.org/".into();
            let mut api = Api::new(&app.settings.server, &app.settings.device_id).unwrap();
            api.session = Some(Session {
                access_token: String::new(),
                user: crate::jellyfin::User {
                    id: "preview".into(),
                    name: "Preview".into(),
                },
            });
            app.api = Some(api);
            app.tab = 5;
            app.focus = 6;
            app.notice = "Settings preview · sample connection".into();
        }
        match std::env::var("ASTER_CAPTURE_SCREEN").as_deref() {
            Ok("keyboard") => app.edit(0),
            Ok("symbols") => {
                app.edit(0);
                app.keyboard.as_mut().unwrap().page();
            }
            Ok("password") => {
                app.edit(2);
                app.keyboard.as_mut().unwrap().text = "example-only".into();
            }
            _ => {}
        }
        if let Ok(path) = std::env::var("ASTER_PREVIEW_DATA") {
            match Preview::load(&path, ctx) {
                Ok(preview) => {
                    let mut api =
                        Api::new("http://preview.invalid/", &app.settings.device_id).unwrap();
                    api.session = Some(Session {
                        access_token: String::new(),
                        user: crate::jellyfin::User {
                            id: "preview".into(),
                            name: "Visual test".into(),
                        },
                    });
                    app.api = Some(api);
                    app.items = preview.items.clone();
                    app.total = app.items.len();
                    app.focus = 6;
                    app.shelves = preview
                        .shelves
                        .iter()
                        .filter(|s| s.count > 0 && s.start + s.count <= app.items.len())
                        .map(|s| cinema::Shelf {
                            title: s.title.clone(),
                            landscape: s.landscape,
                            start: s.start,
                            count: s.count,
                        })
                        .collect();
                    app.preview = Some(preview);
                    app.busy = false;
                    match std::env::var("ASTER_PREVIEW_ROUTE").as_deref() {
                        Ok("settings") => {
                            app.tab = 5;
                        }
                        Ok("detail") => app.detail = app.items.first().cloned(),
                        Ok("last") => app.focus = app.items.len() + 5,
                        _ => {}
                    }
                }
                Err(e) => app.error = e,
            }
        }
        if std::env::var_os("ASTER_PREVIEW_DATA").is_none()
            && let Some(path) = std::env::var_os("ASTER_DEV_RESTORE")
        {
            match Control::restore(std::path::Path::new(&path)) {
                Ok(Some(connection)) => {
                    if let Ok(mut api) = Api::new(&connection.server, &app.settings.device_id) {
                        api.session = Some(connection.session.clone());
                        let _ = app.tx.send(Message::Login(
                            app.auth_generation,
                            Ok((api, connection.session)),
                        ));
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("Development session restore unavailable: {e}"),
            }
        }
        if let Some(path) = std::env::var_os("ASTER_DEV_BRIDGE") {
            match Control::start(path.into()) {
                Ok(control) => {
                    control.set_context(ctx.clone());
                    app.control = Some(control);
                }
                Err(e) => app.error = format!("Development control could not start: {e}"),
            }
        }
        app
    }
    fn work(&mut self, job: impl FnOnce() -> Message + Send + 'static) {
        self.busy = true;
        self.error.clear();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(job());
        });
    }
    fn load(&mut self) {
        self.generation += 1;
        if self.is_dashboard() {
            self.shelves.clear();
        }
        if let Some(preview) = &self.preview {
            self.items = preview
                .items
                .iter()
                .filter(|item| {
                    item.name
                        .to_lowercase()
                        .contains(&self.search.to_lowercase())
                })
                .cloned()
                .collect();
            let options = self.browse_options[self.tab];
            self.items.retain(|item| match options.filter {
                Filter::All => true,
                Filter::Watched => item.user_data.played,
                Filter::Unwatched => !item.user_data.played,
                Filter::Favorites => item.user_data.is_favorite,
            });
            self.items.sort_by(|a, b| match options.sort {
                Sort::Title => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Sort::TitleDescending => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
                Sort::Newest => b.production_year.cmp(&a.production_year),
                Sort::Rating => b
                    .community_rating
                    .unwrap_or(0.0)
                    .total_cmp(&a.community_rating.unwrap_or(0.0)),
                Sort::RecentlyAdded => b.id.cmp(&a.id),
            });
            self.total = self.items.len();
            if self.is_dashboard() {
                self.shelves = preview
                    .shelves
                    .iter()
                    .filter(|s| s.count > 0 && s.start + s.count <= self.items.len())
                    .map(|s| cinema::Shelf {
                        title: s.title.clone(),
                        landscape: s.landscape,
                        start: s.start,
                        count: s.count,
                    })
                    .collect();
            }
            self.reset_catalog_scroll = true;
            self.focus = if self.items.is_empty() && !matches!(self.route, Route::Seasons { .. }) {
                self.tab
            } else {
                6
            };
            return;
        }
        if let Some(api) = self.api.clone() {
            if self.is_dashboard() {
                let generation = self.generation;
                self.work(move || Message::Dashboard(generation, api.dashboard()));
                return;
            }
            let route = self.route.clone();
            let generation = self.generation;
            let section = TABS[self.tab].to_string();
            let search = self.search.clone();
            let options = self.browse_options[self.tab];
            self.work(move || {
                Message::Items(
                    generation,
                    false,
                    route.fetch(&api, &section, &search, 0, options),
                )
            });
        }
    }
    fn load_more(&mut self) {
        if self.busy {
            return;
        }
        if let Some(api) = self.api.clone() {
            let generation = self.generation;
            let route = self.route.clone();
            let section = TABS[self.tab].to_string();
            let search = self.search.clone();
            let offset = self.items.len();
            let options = self.browse_options[self.tab];
            self.work(move || {
                Message::Items(
                    generation,
                    true,
                    route.fetch(&api, &section, &search, offset, options),
                )
            });
        }
    }
    fn push_route(&mut self, route: Route) {
        self.parents.push(Snapshot {
            route: self.route.clone(),
            items: self.items.clone(),
            focus: self.focus,
            total: self.total,
            search: self.search.clone(),
            detail: self.detail.clone(),
            list_view: self.list_view,
        });
        self.route = route;
        self.detail = None;
        self.search.clear();
        self.list_view = false;
        self.items.clear();
        self.total = 0;
        self.feature_index = 0;
        self.load();
    }
    fn open_season(&mut self, season: Option<Item>) {
        let series = match &self.route {
            Route::Seasons { series } | Route::Episodes { series, .. } => series.clone(),
            Route::Library => return,
        };
        self.push_route(Route::Episodes {
            series,
            season: season.map(Box::new),
        });
    }
    fn back_catalog(&mut self) {
        self.generation += 1;
        self.busy = false;
        if let Some(snapshot) = self.parents.pop() {
            self.route = snapshot.route;
            self.items = snapshot.items;
            self.focus = snapshot.focus;
            self.total = snapshot.total;
            self.search = snapshot.search;
            self.detail = snapshot.detail;
            self.list_view = snapshot.list_view;
            self.reset_catalog_scroll = false;
        } else {
            self.focus = self.tab;
        }
        self.focus_changed = true;
    }
    fn activate_item(&mut self, index: usize) {
        self.detail_parent = None;
        self.detail_requested.clear();
        self.focus = index + 6;
        self.feature_index = index;
        self.detail_focus = 0;
        if let Some(item) = self.items.get(index).cloned() {
            if item.kind == "Season" {
                self.open_season(Some(item));
            } else {
                self.detail = Some(item);
            }
        }
    }
    fn detail_activate(&mut self) {
        if self.dev_action && matches!(self.detail_focus, 1 | 2) {
            return;
        }
        if self.busy {
            return;
        }
        let Some(item) = self.detail.clone() else {
            return;
        };
        if self.preview.is_some() && matches!(self.detail_focus, 1 | 2) {
            let favorite = self.detail_focus == 1;
            if let Some(detail) = &mut self.detail {
                if favorite {
                    detail.user_data.is_favorite = !detail.user_data.is_favorite;
                } else {
                    detail.user_data.played = !detail.user_data.played;
                }
            }
            return;
        }
        match self.detail_focus {
            0 => {
                if item.kind == "Series" {
                    self.detail_focus = 4;
                    self.focus_changed = true;
                } else {
                    self.play();
                }
            }
            1 | 2 => {
                if let Some(api) = self.api.clone() {
                    let favorite = self.detail_focus == 1;
                    let value = if favorite {
                        !item.user_data.is_favorite
                    } else {
                        !item.user_data.played
                    };
                    self.work(move || {
                        Message::Flag(
                            item.id.clone(),
                            favorite,
                            value,
                            api.set_flag(&item, favorite, value),
                        )
                    });
                }
            }
            _ => self.return_from_detail(),
        }
    }
    fn login(&mut self, quick: bool) {
        if self.busy {
            return;
        }
        let api = match Api::new(&self.settings.server, &self.settings.device_id) {
            Ok(a) => a,
            Err(e) => {
                self.error = e;
                return;
            }
        };
        self.settings.server = api.base.to_string();
        if let Err(e) = self.settings.save() {
            self.error = e;
            return;
        }
        self.auth_generation += 1;
        let generation = self.auth_generation;
        if quick {
            self.work(move || {
                Message::Quick(
                    generation,
                    api.quick_start().and_then(|v| {
                        Ok((
                            v["Code"]
                                .as_str()
                                .ok_or("Quick Connect returned no code.")?
                                .into(),
                            v["Secret"]
                                .as_str()
                                .ok_or("Quick Connect returned no secret.")?
                                .into(),
                        ))
                    }),
                )
            });
        } else {
            let name = self.username.clone();
            let password = std::mem::take(&mut self.password);
            self.work(move || {
                let mut api = api;
                Message::Login(generation, api.login(&name, &password).map(|s| (api, s)))
            });
        }
    }
    fn messages(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Login(g, result) => {
                    if g != self.auth_generation {
                        continue;
                    }
                    self.busy = false;
                    match result {
                        Ok((api, session)) => {
                            if let Some(control) = &self.control {
                                control.publish(Some(Connection {
                                    server: api.base.to_string(),
                                    session: session.clone(),
                                }));
                            }
                            self.notice = format!("Welcome, {}", session.user.name);
                            self.quick = None;
                            self.password.clear();
                            self.keyboard = None;
                            self.api = Some(api);
                            self.focus = 6;
                            self.tab = 0;
                            if self.settings.remember_session {
                                let server = self.settings.server.clone();
                                let private = self.settings.private_token_file;
                                let tx = self.tx.clone();
                                thread::spawn(move || {
                                    let _ = tx.send(Message::Note(vault::store(
                                        &server, &session, private,
                                    )));
                                });
                            }
                            self.load();
                        }
                        Err(e) => {
                            if !e.starts_with("No saved sign-in")
                                && !e.starts_with("No private-file")
                            {
                                self.error = e;
                            }
                        }
                    }
                }
                Message::InlineSeasons(id, result) => self.receive_inline_seasons(id, result),
                Message::InlineEpisodes(id, season, append, result) => {
                    self.receive_inline_episodes(id, season, append, result)
                }
                Message::Details(id, result) => {
                    if self.detail.as_ref().is_some_and(|i| i.id == id) {
                        match result {
                            Ok(item) => self.detail = Some(*item),
                            Err(e) => self.notice = e,
                        }
                    }
                }
                Message::Dashboard(g, result) => {
                    if g != self.generation {
                        continue;
                    }
                    self.busy = false;
                    match result {
                        Ok(rows) => {
                            self.items.clear();
                            self.shelves.clear();
                            for (title, landscape, items) in rows {
                                if items.is_empty() {
                                    continue;
                                }
                                self.shelves.push(cinema::Shelf {
                                    title,
                                    landscape,
                                    start: self.items.len(),
                                    count: items.len(),
                                });
                                self.items.extend(items);
                            }
                            self.total = self.items.len();
                            self.focus = 6;
                            if let Some(id) = self.focus_after_load.take()
                                && let Some(index) = self.items.iter().position(|i| i.id == id)
                            {
                                self.focus = index + 6;
                            }
                            self.feature_index = 0;
                            self.reset_catalog_scroll = true;
                        }
                        Err(e) => self.error = e,
                    }
                }
                Message::Items(g, append, result) => {
                    if g == self.generation {
                        self.busy = false;
                        match result {
                            Ok(page) => {
                                self.total = page.total_record_count;
                                if append {
                                    self.items.extend(page.items);
                                } else {
                                    self.items = page.items;
                                    self.feature_index = 0;
                                    self.reset_catalog_scroll = true;
                                    self.focus = if self.items.is_empty()
                                        && !matches!(self.route, Route::Seasons { .. })
                                    {
                                        self.tab
                                    } else {
                                        6
                                    };
                                    if let Some(id) = self.focus_after_load.take()
                                        && let Some(index) =
                                            self.items.iter().position(|i| i.id == id)
                                    {
                                        self.focus = index + 6;
                                    }
                                }
                                self.focus_changed = true;
                            }
                            Err(e) => self.error = e,
                        }
                    }
                }
                Message::Quick(g, result) => {
                    if g != self.auth_generation {
                        continue;
                    }
                    self.busy = false;
                    match result {
                        Ok(q) => {
                            self.quick = Some(q);
                            self.last_poll = Instant::now() - Duration::from_secs(3);
                        }
                        Err(e) => self.error = e,
                    }
                }
                Message::Poll(g, result) => {
                    if g != self.auth_generation {
                        continue;
                    }
                    self.polling = false;
                    match result {
                        Ok(Some((api, session))) => {
                            let _ = self.tx.send(Message::Login(g, Ok((api, session))));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            self.quick = None;
                            self.error = e;
                        }
                    }
                }
                Message::Flag(id, favorite, value, result) => {
                    self.busy = false;
                    match result {
                        Ok(()) => {
                            for item in &mut self.items {
                                if item.id == id {
                                    if favorite {
                                        item.user_data.is_favorite = value;
                                    } else {
                                        item.user_data.played = value;
                                    }
                                }
                            }
                            if let Some(item) = &mut self.detail
                                && item.id == id
                            {
                                if favorite {
                                    item.user_data.is_favorite = value;
                                } else {
                                    item.user_data.played = value;
                                }
                            }
                        }
                        Err(e) => self.error = e,
                    }
                }
                Message::Note(result) => {
                    if let Err(e) = result {
                        self.error = e;
                    }
                }
                Message::SignedOut(result) => {
                    self.busy = false;
                    match result {
                        Ok(()) => {
                            if let Some(control) = &self.control {
                                control.publish(None);
                            }
                            self.api = None;
                            self.items.clear();
                            self.detail = None;
                            self.parents.clear();
                            self.route = Route::Library;
                            self.quick = None;
                            self.focus = 0;
                            self.notice = "Signed out".into();
                        }
                        Err(e) => self.error = e,
                    }
                }
            }
        }
        if self.quick.is_some()
            && !self.polling
            && self.last_poll.elapsed() > Duration::from_secs(3)
            && let (Ok(mut api), Some((_, secret))) = (
                Api::new(&self.settings.server, &self.settings.device_id),
                self.quick.clone(),
            )
        {
            let tx = self.tx.clone();
            let generation = self.auth_generation;
            self.polling = true;
            self.last_poll = Instant::now();
            thread::spawn(move || {
                let result = api.quick_poll(&secret).map(|s| s.map(|s| (api, s)));
                let _ = tx.send(Message::Poll(generation, result));
            });
        }
        let mut finished = false;
        let events: Vec<_> = self
            .player
            .as_ref()
            .map(|p| p.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Status::Finished => finished = true,
                Status::Fullscreen(value) => self.player_fullscreen = Some(value),
                Status::Overlay(value) => self.player_overlay = value,
                Status::Position(s) => self.position = s,
                Status::Duration(d) => self.duration = d,
                Status::Paused(p) => self.paused = p,
                Status::Tracks(tracks) => self.tracks = tracks,
                Status::Error(e) | Status::Warning(e) => self.error = e,
                Status::Ready => self.notice = "Playing".into(),
                Status::Playing(item) => self.playback_started(*item),
                Status::UpNext(title, seconds) => self.up_next = Some((title, seconds)),
                Status::Completed(id) => {
                    for item in self.items.iter_mut().chain(self.inline.episodes.iter_mut()) {
                        if item.id == id {
                            item.user_data.played = true;
                            item.user_data.playback_position_ticks = 0;
                        }
                    }
                    if let Some(item) = &mut self.detail
                        && item.id == id
                    {
                        item.user_data.played = true;
                        item.user_data.playback_position_ticks = 0;
                    }
                }
            }
        }
        if finished {
            self.seek_previews = None;
            self.player = None;
            self.up_next = None;
            self.notice = "Returned to your library".into();
            self.focus_after_load = self
                .detail_parent
                .as_ref()
                .or(self.detail.as_ref())
                .map(|i| i.id.clone());
            self.load();
        }
    }
    fn select_tab(&mut self, index: usize) {
        self.detail_parent = None;
        if self.busy && self.api.is_some() {
            self.generation += 1;
            self.busy = false;
        } else if self.busy {
            return;
        }
        self.tab = index;
        self.focus = index;
        self.detail = None;
        self.parents.clear();
        self.route = Route::Library;
        self.search.clear();
        if index != 5 {
            self.items.clear();
            self.load();
        }
    }
    fn edit(&mut self, field: usize) {
        let text = match field {
            0 => self.settings.server.clone(),
            1 => self.username.clone(),
            2 => self.password.clone(),
            _ => self.search.clone(),
        };
        self.keyboard = Some(Keyboard::new(field, text));
    }
    fn play(&mut self) {
        if self.dev_action {
            return;
        }
        if let (Some(api), Some(item)) = (self.api.clone(), self.detail.clone()) {
            self.playing = item.playback_title();
            self.up_next = None;
            self.player_overlay = serde_json::Value::Null;
            self.position = 0.0;
            self.duration = 0.0;
            self.paused = false;
            self.tracks.clear();
            if let Some(embedded) = &self.embedded {
                self.player = Some(Player::start_embedded(
                    api,
                    item,
                    self.settings.clone(),
                    embedded.core.clone(),
                ));
                if let Some(player) = &self.player {
                    player.set_context(self.preview_context.clone());
                }
            } else {
                self.error =
                    "Embedded playback is unavailable. Check the libmpv/OpenGL startup error."
                        .into();
            }
        }
    }
    fn playback_started(&mut self, item: Item) {
        if let Some(player) = &self.player {
            self.seek_previews = Some(crate::seek_preview::SeekPreviews::new(
                player.preview_api.clone(),
                player.preview_socket.clone(),
                item.id.clone(),
                self.preview_context.clone(),
            ));
        }
        self.inline_playback_started(&item);
        self.playing = item.playback_title();
        self.up_next = None;
        if let Route::Episodes { series, season } = &self.route
            && item.series_id.as_deref() == Some(series.id.as_str())
        {
            let changed = season
                .as_ref()
                .is_some_and(|s| item.season_id.as_deref() != Some(s.id.as_str()));
            if changed && let Some(id) = &item.season_id {
                let next_season = Item {
                    id: id.clone(),
                    kind: "Season".into(),
                    name: item
                        .parent_index_number
                        .map(|n| {
                            if n == 0 {
                                "Specials".into()
                            } else {
                                format!("Season {n}")
                            }
                        })
                        .unwrap_or_else(|| "Season".into()),
                    index_number: item.parent_index_number,
                    series_id: item.series_id.clone(),
                    series_name: item.series_name.clone(),
                    ..Item::default()
                };
                self.route = Route::Episodes {
                    series: series.clone(),
                    season: Some(Box::new(next_season)),
                };
                if let Some(snapshot) = self.parents.last_mut()
                    && matches!(snapshot.route, Route::Seasons { .. })
                    && let Some(index) = snapshot.items.iter().position(|s| s.id == *id)
                {
                    snapshot.focus = index + 6;
                }
                self.focus_after_load = Some(item.id.clone());
                self.load();
            }
        }
        if let Some(index) = self.items.iter().position(|i| i.id == item.id) {
            self.focus = index + 6;
        }
        self.detail = Some(item);
    }
    fn action(&mut self, a: Action, ctx: &egui::Context) {
        self.focus_changed = true;
        let a = if self.player.is_none() {
            match a {
                Action::LeftRepeat => Action::Left,
                Action::RightRepeat => Action::Right,
                Action::ReleaseHorizontal => return,
                _ => a,
            }
        } else {
            a
        };
        if let Some(k) = &mut self.keyboard {
            match a {
                Action::Left | Action::Right | Action::Up | Action::Down => k.navigate(a),
                Action::Back => self.keyboard = None,
                Action::Confirm => self.keyboard_key(),
                Action::Delete => k.delete(),
                Action::Menu => {
                    k.selected = k.done_index();
                    self.keyboard_key();
                }
                Action::Audio => k.page(),
                Action::Search => {
                    k.text.push(' ');
                    k.request_focus = true;
                }
                Action::Subtitles => k.upper = !k.upper,
                _ => {}
            }
            return;
        }
        let a = if a == Action::Delete { Action::Menu } else { a };
        if let Some((menu, selected)) = &mut self.browse_menu {
            let count = match menu {
                browse::Menu::Sort => 6,
                browse::Menu::Filter => 4,
            };
            match a {
                Action::Up => *selected = selected.saturating_sub(1),
                Action::Down => *selected = (*selected + 1).min(count - 1),
                Action::Back => self.browse_menu = None,
                Action::Confirm => self.apply_browse_menu(),
                _ => {}
            }
            return;
        }
        if let Some(player) = &self.player {
            player.command(crate::input::playback_command(a));
            return;
        }
        if a == Action::Back && !self.error.is_empty() {
            self.error.clear();
            return;
        }
        if self.busy && !matches!(a, Action::Back | Action::Menu | Action::Prev | Action::Next) {
            return;
        }
        if self.api.is_some() && self.busy && a == Action::Back {
            self.generation += 1;
            self.busy = false;
        }
        if self.api.is_none() {
            if self.quick.is_some() {
                if matches!(a, Action::Back | Action::Confirm) {
                    self.quick = None;
                    self.polling = false;
                    self.auth_generation += 1;
                    self.busy = false;
                }
                return;
            }
            match a {
                Action::Up => self.focus = self.focus.saturating_sub(1),
                Action::Down => self.focus = (self.focus + 1).min(4),
                Action::Confirm => match self.focus {
                    0..=2 => self.edit(self.focus),
                    3 => self.login(false),
                    _ => self.login(true),
                },
                Action::Back => {
                    self.auth_generation += 1;
                    self.quick = None;
                    self.polling = false;
                    self.busy = false;
                }
                _ => {}
            }
            return;
        }
        if self.detail.is_some() {
            if self.inline_action(a) {
                return;
            }
            match a {
                Action::Back => self.return_from_detail(),
                Action::Left | Action::Up => {
                    self.detail_focus = self.detail_focus.saturating_sub(1)
                }
                Action::Right | Action::Down => self.detail_focus = (self.detail_focus + 1).min(3),
                Action::Audio | Action::Subtitles => {
                    self.detail_scroll_delta = if a == Action::Audio { -280.0 } else { 280.0 };
                    self.focus_changed = false;
                }
                Action::Confirm => self.detail_activate(),
                _ => {}
            }
            return;
        }
        if !self.route.is_library() && matches!(a, Action::Audio | Action::Subtitles) {
            if a == Action::Audio {
                if self.route.is_episodes() {
                    self.back_catalog();
                } else {
                    self.open_season(None);
                }
            }
            return;
        }
        if self.is_dashboard() && matches!(a, Action::Audio | Action::Subtitles) {
            return;
        }
        if matches!(a, Action::Audio | Action::Subtitles) && self.tab != 5 {
            self.browse_menu = Some((
                if a == Action::Audio {
                    browse::Menu::Filter
                } else {
                    browse::Menu::Sort
                },
                0,
            ));
            return;
        }
        if a == Action::Menu {
            self.select_tab(5);
            return;
        }
        if a == Action::Search {
            if self.tab == 5 {
                self.tab = 0;
                self.detail = None;
                self.parents.clear();
            }
            self.route = Route::Library;
            self.parents.clear();
            self.edit(3);
            return;
        }
        if a == Action::Prev {
            self.select_tab(self.tab.saturating_sub(1));
            return;
        }
        if a == Action::Next {
            self.select_tab((self.tab + 1).min(5));
            return;
        }
        if a == Action::Back {
            self.back_catalog();
            return;
        }
        if self.focus < 6 {
            match a {
                Action::Left => self.focus = self.focus.saturating_sub(1),
                Action::Right => self.focus = (self.focus + 1).min(5),
                Action::Down => {
                    self.focus = if self.tab == 5 {
                        6
                    } else if self.items.is_empty() {
                        self.tab
                    } else {
                        6
                    }
                }
                Action::Confirm => self.select_tab(self.focus),
                _ => {}
            }
            return;
        }
        if self.tab == 5 {
            match a {
                Action::Up => self.focus = if self.focus == 6 { 5 } else { self.focus - 1 },
                Action::Down => self.focus = (self.focus + 1).min(19),
                Action::Left => self.setting_change(-1, ctx),
                Action::Right | Action::Confirm => self.setting_change(1, ctx),
                _ => {}
            }
            return;
        }
        if self.is_dashboard() && !self.shelves.is_empty() {
            self.dashboard_action(a);
            return;
        }
        match a {
            Action::Left => {
                if self.focus < 6 + self.items.len()
                    && !(self.focus - 6).is_multiple_of(self.columns)
                {
                    self.focus -= 1;
                }
            }
            Action::Right => {
                if self.focus - 6 + 1 < self.items.len()
                    && (self.focus - 6) % self.columns + 1 < self.columns
                {
                    self.focus = (self.focus + 1)
                        .min(5 + self.items.len() + usize::from(self.items.len() < self.total));
                }
            }
            Action::Up => {
                self.focus = if self.focus >= 6 + self.items.len() && !self.items.is_empty() {
                    6 + (self.items.len() - 1) / self.columns * self.columns
                } else if self.focus < 6 + self.columns {
                    self.tab
                } else {
                    self.focus - self.columns
                }
            }
            Action::Down => {
                self.focus = (self.focus + self.columns)
                    .min(5 + self.items.len() + usize::from(self.items.len() < self.total))
            }
            Action::Confirm => {
                if self.items.is_empty() && matches!(self.route, Route::Seasons { .. }) {
                    self.open_season(None);
                } else if self.focus - 6 == self.items.len() {
                    self.load_more();
                } else {
                    self.activate_item(self.focus - 6);
                }
            }
            _ => {}
        }
    }
    fn setting_change(&mut self, d: i32, ctx: &egui::Context) {
        match self.focus - 6 {
            0 => {
                self.settings.fullscreen = !self.settings.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.settings.fullscreen));
                if let Some(player) = &self.player {
                    player.command(json!([
                        "set_property",
                        "fullscreen",
                        self.settings.fullscreen
                    ]));
                }
            }
            1 => {
                self.settings.ui_scale = (self.settings.ui_scale + d as f32 * 0.1).clamp(0.8, 1.6);
                ctx.set_zoom_factor(
                    self.settings.ui_scale
                        / ctx.native_pixels_per_point().unwrap_or(1.0).clamp(0.1, 1.0),
                );
            }
            2 => self.settings.volume = (self.settings.volume as i32 + d * 5).clamp(0, 100) as u8,
            3 => self.settings.hardware_decode = !self.settings.hardware_decode,
            4 => self.settings.resume = !self.settings.resume,
            5 => self.settings.autoplay = !self.settings.autoplay,
            6 => {
                self.settings.autoplay_delay_seconds =
                    (self.settings.autoplay_delay_seconds as i32 + d).clamp(0, 15) as u8
            }
            7 => {
                self.settings.stick_deadzone =
                    (self.settings.stick_deadzone + d as f32 * 0.05).clamp(0.2, 0.9)
            }
            8 => self.settings.swap_confirm = !self.settings.swap_confirm,
            9 => {
                self.settings.remember_session = !self.settings.remember_session;
                let server = self.settings.server.clone();
                let private = self.settings.private_token_file;
                let session = self.api.as_ref().and_then(|a| a.session.clone());
                let enabled = self.settings.remember_session;
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let result = if enabled {
                        session.map_or(Ok(()), |s| vault::store(&server, &s, private))
                    } else {
                        vault::clear(&server, private)
                    };
                    let _ = tx.send(Message::Note(result));
                });
            }
            10 => {
                if self.settings.remember_session {
                    self.error =
                        "Turn off Remember sign-in before changing credential storage.".into();
                    return;
                }
                self.settings.private_token_file = !self.settings.private_token_file;
            }
            11 => {
                self.load();
                self.tab = 0;
            }
            12 => {
                let server = self.settings.server.clone();
                let private = self.settings.private_token_file;
                let remembered = self.settings.remember_session;
                self.settings.remember_session = false;
                self.work(move || {
                    Message::SignedOut(if remembered {
                        vault::clear(&server, private)
                    } else {
                        Ok(())
                    })
                });
            }
            13 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        if let Err(e) = self.settings.save() {
            self.error = format!("Could not save settings: {e}");
        }
    }
    fn keyboard_key(&mut self) {
        let done = self.keyboard.as_mut().is_some_and(Keyboard::press);
        if done {
            let k = self.keyboard.take().unwrap();
            match k.field {
                0 => self.settings.server = k.text,
                1 => self.username = k.text,
                2 => self.password = k.text,
                _ => {
                    self.search = k.text;
                    self.load();
                }
            }
        }
    }
    fn draw_header(&mut self, ctx: &egui::Context) {
        if self.api.is_none() {
            return;
        }
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(9, 10, 12, 195))
                    .inner_margin(egui::Margin::symmetric(24, 7)),
            )
            .show(ctx, |ui| {
                let compact = ui.available_width() < 850.0;
                ui.horizontal(|ui| {
                    if design::header_logo(ui).clicked() {
                        self.select_tab(0);
                    }
                    if self.api.is_some() && !compact {
                        ui.add_space(16.0);
                        self.draw_navigation(ui);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(session) = self.api.as_ref().and_then(|api| api.session.clone())
                        {
                            let (r, response) =
                                ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::click());
                            ui.painter().circle_filled(
                                r.center(),
                                14.0,
                                Color32::from_rgb(55, 46, 42),
                            );
                            ui.painter().text(
                                r.center(),
                                egui::Align2::CENTER_CENTER,
                                session
                                    .user
                                    .name
                                    .chars()
                                    .next()
                                    .unwrap_or('U')
                                    .to_uppercase()
                                    .to_string(),
                                egui::FontId::proportional(15.0),
                                ACCENT,
                            );
                            if response
                                .on_hover_text(format!("{} · Settings", session.user.name))
                                .clicked()
                            {
                                self.select_tab(5);
                            }
                            let width = (ctx.viewport_rect().width() * 0.30)
                                .clamp(220.0, 520.0)
                                .min(ui.available_width() - 12.0);
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .id_salt("library-search")
                                    .hint_text("Search your library")
                                    .font(egui::FontId::proportional(15.0))
                                    .desired_width((width - 24.0).max(160.0))
                                    .margin(Vec2::new(10.0, 6.0)),
                            );
                            self.search_bounds = Some(r.rect);
                            if r.changed() {
                                self.search_deadline =
                                    Some(Instant::now() + Duration::from_millis(350));
                            }
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                self.search_deadline = Some(Instant::now());
                            }
                        }
                    });
                });
                if self.api.is_some() && compact {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| self.draw_navigation(ui));
                }
            });
    }
    fn draw_navigation(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 5.0;
        let compact = ui.ctx().viewport_rect().width() < 1280.0;
        for (i, label) in [
            "Discover",
            "Movies",
            "Series",
            "Continue",
            "Favorites",
            "Settings",
        ]
        .iter()
        .enumerate()
        {
            let font = egui::FontId::proportional(if compact { 13.0 } else { 15.0 });
            let galley = ui.painter().layout_no_wrap(
                (*label).to_string(),
                font,
                if self.tab == i { ACCENT } else { MUTED },
            );
            let (rect, r) = ui.allocate_exact_size(
                Vec2::new(galley.size().x + if compact { 14.0 } else { 22.0 }, 32.0),
                egui::Sense::click(),
            );
            if self.focus == i {
                ui.painter()
                    .rect_filled(rect, 7.0, Color32::from_rgb(45, 39, 36));
                ui.painter().rect_stroke(
                    rect,
                    7.0,
                    Stroke::new(1.0_f32, design::FOCUS),
                    egui::StrokeKind::Inside,
                );
            } else if r.hovered() {
                ui.painter().rect_filled(rect, 7.0, design::SURFACE);
            }
            if self.tab == i {
                ui.painter().line_segment(
                    [
                        rect.left_bottom() + Vec2::new(10.0, -1.0),
                        rect.right_bottom() + Vec2::new(-10.0, -1.0),
                    ],
                    Stroke::new(2.0_f32, ACCENT),
                );
            }
            ui.painter().galley(
                rect.center() - galley.size() / 2.0,
                galley,
                if self.tab == i { ACCENT } else { design::TEXT },
            );
            if r.clicked() {
                self.select_tab(i);
            }
        }
    }
    fn draw_footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(9, 10, 12, 195))
                    .inner_margin(egui::Margin::symmetric(24, 5)),
            )
            .show(ctx, |ui| {
                let confirm = if self.settings.swap_confirm { "B" } else { "A" };
                let back = if self.settings.swap_confirm { "A" } else { "B" };
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    if self.keyboard.is_some() {
                        design::keycap(ui, confirm, "Type");
                        design::keycap(ui, back, "Cancel");
                        design::keycap(ui, "X", "Delete");
                        design::keycap(ui, "Y", "Space");
                        design::keycap(ui, "L2 / LT", "Symbols");
                        design::keycap(ui, "R2 / RT", "Shift");
                        design::keycap(ui, "Start", "Done");
                    } else if self.player.is_some() {
                        design::keycap(ui, confirm, "Pause");
                        design::keycap(ui, back, "Stop");
                        design::keycap(ui, "D-pad", "Buttons / timeline");
                        design::keycap(ui, "LT / RT", "Audio / subtitles");
                    } else if self.api.is_none() {
                        design::keycap(ui, confirm, "Select / edit");
                        design::keycap(ui, back, "Cancel");
                        design::keycap(ui, "D-pad", "Move");
                    } else if self.tab == 5 && self.detail.is_none() {
                        design::keycap(ui, "Left / right", "Adjust");
                        design::keycap(ui, confirm, "Select");
                        design::keycap(ui, back, "Back");
                        design::keycap(ui, "LB / RB", "Sections");
                    } else {
                        design::keycap(ui, confirm, "Select");
                        design::keycap(ui, back, "Back");
                        design::keycap(ui, "Y", "Search");
                        design::keycap(ui, "X", "Settings");
                        if self.detail.is_some() {
                            design::keycap(ui, "LT / RT", "Scroll page");
                        } else if self.route.is_library() && !self.is_dashboard() {
                            design::keycap(ui, "LT / RT", "Filter / sort");
                        } else if !self.route.is_library() {
                            design::keycap(
                                ui,
                                "LT",
                                if self.route.is_episodes() {
                                    "Seasons"
                                } else {
                                    "All episodes"
                                },
                            );
                        }
                        design::keycap(ui, "LB / RB", "Sections");
                    }
                });
                if let Some(error) = &self.input.error {
                    ui.label(RichText::new(error).size(12.0).color(Color32::LIGHT_RED));
                }
            });
    }
    fn draw_setup(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let card_width = available.x.min(488.0);
        let card_height = self.login_bounds.map(|r| r.height()).unwrap_or(510.0);
        let top = ((available.y - card_height) / 2.0 - 4.0).max(0.0);
        let mut submit = false;
        let mut quick = false;
        egui::ScrollArea::vertical().id_salt("sign-in").auto_shrink([false,false]).show(ui,|ui| {
            ui.add_space(top);
            ui.horizontal_top(|ui| {
                ui.add_space(((available.x-card_width)/2.0).max(0.0));
                ui.vertical(|ui| {ui.set_width(card_width);
                let card=egui::Frame::new().fill(Color32::from_rgb(23,28,36)).corner_radius(18.0).inner_margin(28).stroke(Stroke::new(1.0_f32,Color32::from_rgb(45,52,64))).show(ui,|ui| {
                    ui.set_width((card_width-58.0).max(200.0));
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("aster").size(32.0).color(ACCENT));ui.add_space(4.0);
                        ui.label(RichText::new("Your library. Your evening.").size(15.0).color(MUTED));
                    });ui.add_space(26.0);
                    if self.quick.is_none(){
                    for i in 0..3 {
                        let label=["Jellyfin server","Username","Password"][i];let hint=["http://your-server:8096","Your Jellyfin username","Your password"][i];
                        ui.label(RichText::new(label).size(12.0).color(MUTED));ui.add_space(3.0);
                        let mut open_keyboard=false;
                        ui.horizontal(|ui| {
                            let text=match i{0=>&mut self.settings.server,1=>&mut self.username,_=>&mut self.password};
                            let id=egui::Id::new(("aster-login-field",i));
                            let output=egui::TextEdit::singleline(text).id(id).password(i==2).hint_text(hint).desired_width((ui.available_width()-62.0).max(140.0)).font(egui::FontId::proportional(16.0)).margin(Vec2::new(11.0,11.0)).show(ui);
                            self.login_fields[i]=Some(output.response.rect);
                            if output.response.clicked()||output.response.gained_focus(){self.focus=i;}
                            if self.focus==i && !output.response.has_focus(){ui.painter().rect_stroke(output.response.rect.expand(1.0),5.0,Stroke::new(1.0_f32,design::FOCUS),egui::StrokeKind::Outside);}
                            if output.response.lost_focus() && ui.input(|input|input.key_pressed(egui::Key::Enter)) {
                                if i==2{submit=true;}else{ui.memory_mut(|m|m.request_focus(egui::Id::new(("aster-login-field",i+1))));self.focus=i+1;}
                            }
                            let (rect,r)=ui.allocate_exact_size(Vec2::new(30.0,37.0),egui::Sense::click());
                            ui.painter().rect_stroke(rect.shrink(3.0),4.0,Stroke::new(1.0_f32,if r.hovered(){ACCENT}else{MUTED}),egui::StrokeKind::Inside);
                            for y in 0..2 {for x in 0..3{ui.painter().rect_filled(egui::Rect::from_min_size(rect.min+Vec2::new(7.0+x as f32*6.0,10.0+y as f32*7.0),Vec2::new(3.0,3.0)),0.5,MUTED);}}
                            ui.painter().line_segment([rect.min+Vec2::new(9.0,27.0),rect.min+Vec2::new(21.0,27.0)],Stroke::new(1.0_f32,MUTED));
                            open_keyboard=r.on_hover_text("Open on-screen keyboard").clicked();
                        });
                        if open_keyboard{self.edit(i);}
                        ui.add_space(12.0);
                    }
                    ui.add_space(6.0);
                    let primary=ui.add_sized([ui.available_width(),43.0],egui::Button::new(RichText::new(if self.busy{"Connecting…"}else{"Sign in"}).size(16.0).color(BG)).fill(ACCENT).corner_radius(8.0).stroke(Stroke::new(if self.focus==3{2.0_f32}else{0.0_f32},Color32::WHITE)));
                    self.login_submit_bounds=Some(primary.rect);submit|=primary.clicked();
                    ui.add_space(10.0);
                    quick|=button(ui,"Use Quick Connect",self.focus==4,Vec2::new(ui.available_width(),40.0)).clicked();
                    ui.add_space(14.0);
                    ui.add(egui::Label::new(RichText::new("Quick Connect lets you approve this device from an already signed-in Jellyfin app.").size(12.0).color(MUTED)).wrap());
                    }else if let Some((code,_))=self.quick.clone(){
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("Approve this device").size(21.0).color(design::TEXT));ui.add_space(14.0);
                            ui.label(RichText::new(code).size(46.0).color(ACCENT));ui.add_space(14.0);
                            ui.add(egui::Label::new(RichText::new("Open Quick Connect in an already signed-in Jellyfin app and enter this code.").size(15.0).color(MUTED)).wrap());
                            ui.add_space(20.0);ui.spinner();ui.label(RichText::new("Waiting for approval").size(12.0).color(MUTED));ui.add_space(20.0);
                        });
                        if button(ui,"Cancel Quick Connect",true,Vec2::new(ui.available_width(),42.0)).clicked(){self.quick=None;self.polling=false;self.auth_generation+=1;}
                    }
                });
                self.login_bounds=Some(card.response.rect);
                });
            });
        });
        if submit {
            self.focus = 3;
            self.login(false);
        } else if quick {
            self.focus = 4;
            self.login(true);
        }
    }
    fn draw_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let width = ui.available_width().min(1000.0);
        let inset = (ui.available_width() - width) / 2.0;
        ui.horizontal_top(|ui| {
            ui.add_space(inset);
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    ui.label(RichText::new("Settings").size(34.0).color(design::TEXT));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Make Aster feel at home. Changes save automatically.")
                            .size(15.0)
                            .color(MUTED),
                    );
                    ui.add_space(26.0);
                    let on = |v| if v { "On".to_owned() } else { "Off".to_owned() };
                    let values = [
                        on(self.settings.fullscreen),
                        format!("{:.0}%", self.settings.ui_scale * 100.0),
                        format!("{}%", self.settings.volume),
                        on(self.settings.hardware_decode),
                        on(self.settings.resume),
                        on(self.settings.autoplay),
                        format!("{} s", self.settings.autoplay_delay_seconds),
                        format!("{:.0}%", self.settings.stick_deadzone * 100.0),
                        on(self.settings.swap_confirm),
                        on(self.settings.remember_session),
                        if self.settings.private_token_file {
                            "Private file".into()
                        } else {
                            "System keyring".into()
                        },
                        "Refresh".into(),
                        "Sign out".into(),
                        "Quit".into(),
                    ];
                    let labels = [
                        "Fullscreen",
                        "Interface scale",
                        "Starting volume",
                        "Hardware decoding",
                        "Resume playback",
                        "Next-episode autoplay",
                        "Up next countdown",
                        "Stick deadzone",
                        "Swap confirm and back",
                        "Remember sign-in",
                        "Credential storage",
                        "Refresh library",
                        "Sign out",
                        "Quit Aster",
                    ];
                    let descriptions = [
                        "Use the entire screen",
                        "Adjust text, cards, and controls",
                        "Volume when playback starts",
                        "Let the GPU decode supported video",
                        "Continue from your last position",
                        "Continue after an episode finishes naturally",
                        "Time to cancel or play the next episode now",
                        "Ignore small stick movements",
                        "Choose the face-button layout you prefer",
                        "Stay connected when you reopen Aster",
                        "System keyring or an unencrypted user-only file",
                        "Reload titles from your server",
                        "Forget the saved sign-in on this device",
                        "Close the application",
                    ];
                    for i in 0..labels.len() {
                        let group = match i {
                            0 => Some("DISPLAY"),
                            2 => Some("PLAYBACK"),
                            7 => Some("CONTROLLER"),
                            9 => Some("ACCOUNT"),
                            11 => Some("APPLICATION"),
                            _ => None,
                        };
                        if let Some(group) = group {
                            if i > 0 {
                                ui.add_space(24.0);
                            }
                            ui.label(RichText::new(group).size(11.0).color(ACCENT));
                            ui.add_space(9.0);
                        }
                        let focused = self.focus == i + 6;
                        let (rect, r) =
                            ui.allocate_exact_size(Vec2::new(width, 72.0), egui::Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            10.0,
                            if focused {
                                Color32::from_rgb(43, 37, 35)
                            } else {
                                design::SURFACE
                            },
                        );
                        if focused {
                            ui.painter().rect_stroke(
                                rect,
                                10.0,
                                Stroke::new(1.5_f32, design::FOCUS),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let control_width = if width < 650.0 { 165.0 } else { 230.0 };
                        let left_width = (width - control_width - 42.0).max(140.0);
                        design::text(
                            ui,
                            egui::Rect::from_min_size(
                                rect.min + Vec2::new(18.0, 12.0),
                                Vec2::new(left_width, 24.0),
                            ),
                            labels[i],
                            16.0,
                            design::TEXT,
                            1,
                        );
                        design::text(
                            ui,
                            egui::Rect::from_min_size(
                                rect.min + Vec2::new(18.0, 39.0),
                                Vec2::new(left_width, 20.0),
                            ),
                            descriptions[i],
                            12.0,
                            MUTED,
                            1,
                        );
                        let numeric = matches!(i, 1 | 2 | 6 | 7);
                        let controls = egui::Rect::from_min_size(
                            rect.right_top() - Vec2::new(control_width + 14.0, -17.0),
                            Vec2::new(control_width, 38.0),
                        );
                        let mut delta = None;
                        if numeric {
                            for (sign, d, x) in [
                                ("-", -1, controls.left()),
                                ("+", 1, controls.right() - 34.0),
                            ] {
                                let b = egui::Rect::from_min_size(
                                    egui::pos2(x, controls.top()),
                                    Vec2::new(34.0, 38.0),
                                );
                                let br = ui.interact(
                                    b,
                                    ui.id().with(("setting", i, d)),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(
                                    b,
                                    6.0,
                                    if br.hovered() {
                                        Color32::from_rgb(64, 61, 61)
                                    } else {
                                        Color32::from_rgb(39, 44, 53)
                                    },
                                );
                                ui.painter().text(
                                    b.center(),
                                    egui::Align2::CENTER_CENTER,
                                    sign,
                                    egui::FontId::proportional(18.0),
                                    design::TEXT,
                                );
                                if br.clicked() {
                                    delta = Some(d);
                                }
                            }
                        }
                        ui.painter().text(
                            controls.center(),
                            egui::Align2::CENTER_CENTER,
                            &values[i],
                            egui::FontId::proportional(15.0),
                            if focused { ACCENT } else { MUTED },
                        );
                        if focused && self.focus_changed {
                            ui.scroll_to_rect(rect.expand(8.0), None);
                        }
                        if let Some(d) = delta {
                            self.focus = i + 6;
                            self.setting_change(d, ctx);
                        } else if r.clicked() {
                            self.focus = i + 6;
                            if !numeric {
                                self.setting_change(1, ctx);
                            }
                        }
                        ui.add_space(7.0);
                    }
                    ui.add_space(26.0);
                    ui.label(
                        RichText::new(format!("Aster {}", env!("CARGO_PKG_VERSION")))
                            .size(13.0)
                            .color(MUTED),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("Connected to {}", self.settings.server))
                                .size(13.0)
                                .color(MUTED),
                        )
                        .wrap(),
                    );
                    ui.add_space(24.0);
                },
            );
        });
    }
    fn apply_browse_menu(&mut self) {
        let Some((menu, index)) = self.browse_menu.take() else {
            return;
        };
        match menu {
            browse::Menu::Sort if index == 5 => {
                self.list_view = !self.list_view;
                self.reset_catalog_scroll = true;
            }
            browse::Menu::Sort => {
                self.browse_options[self.tab].sort = Sort::ALL[index.min(4)];
                self.load();
            }
            browse::Menu::Filter => {
                self.browse_options[self.tab].filter = Filter::ALL[index.min(3)];
                self.load();
            }
        }
    }
    fn draw_library_toolbar(&mut self, ui: &mut egui::Ui) {
        let title = if !self.search.is_empty() {
            format!("Results for “{}”", self.search)
        } else if let Some(title) = self.route.title() {
            title
        } else {
            TABS[self.tab].into()
        };
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), egui::Sense::hover());
        let left_width = (rect.width() - 365.0).max(100.0);
        design::text(
            ui,
            egui::Rect::from_min_size(
                rect.min + Vec2::new(0.0, 4.0),
                Vec2::new(left_width - 80.0, 26.0),
            ),
            &title,
            21.0,
            design::TEXT,
            1,
        );
        let count = if self.busy {
            "Loading…".into()
        } else {
            format!(
                "{} {}",
                self.total.max(self.items.len()),
                if self.route.is_episodes() {
                    "episodes"
                } else if matches!(self.route, Route::Seasons { .. }) {
                    "seasons"
                } else {
                    "titles"
                }
            )
        };
        let title_width = ui
            .painter()
            .layout_no_wrap(
                title.clone(),
                egui::FontId::proportional(21.0),
                design::TEXT,
            )
            .size()
            .x
            .min(left_width - 80.0);
        design::text(
            ui,
            egui::Rect::from_min_size(
                rect.min + Vec2::new(title_width + 12.0, 11.0),
                Vec2::new(100.0, 18.0),
            ),
            &count,
            12.0,
            MUTED,
            1,
        );
        let controls = if self.route.is_library() {
            vec![
                (
                    0,
                    if self.list_view {
                        "List".to_string()
                    } else {
                        "Grid".to_string()
                    },
                    64.0,
                ),
                (1, self.browse_options[self.tab].sort.label().into(), 150.0),
                (
                    2,
                    self.browse_options[self.tab].filter.label().into(),
                    118.0,
                ),
            ]
        } else {
            vec![
                (
                    0,
                    if self.list_view {
                        "List".into()
                    } else {
                        "Grid".into()
                    },
                    64.0,
                ),
                (
                    3,
                    if self.route.is_episodes() {
                        "Choose season".into()
                    } else {
                        "All episodes".into()
                    },
                    150.0,
                ),
                (4, "Back".into(), 80.0),
            ]
        };
        let mut x = rect.right();
        for (index, label, _) in controls {
            let dropdown = matches!(index, 1 | 2);
            let width = ui
                .painter()
                .layout_no_wrap(label.clone(), egui::FontId::proportional(13.0), MUTED)
                .size()
                .x
                + if dropdown { 32.0 } else { 18.0 };
            x -= width;
            let r =
                egui::Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), Vec2::new(width, 28.0));
            let response = ui.interact(
                r,
                ui.id().with(("library-toolbar", index)),
                egui::Sense::click(),
            );
            if response.hovered() {
                ui.painter().rect_filled(r, 4.0, design::SURFACE);
            }
            ui.painter().text(
                egui::pos2(r.left() + 9.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                &label,
                egui::FontId::proportional(13.0),
                if response.hovered() {
                    design::TEXT
                } else {
                    MUTED
                },
            );
            if dropdown {
                let c = egui::pos2(r.right() - 11.0, r.center().y);
                ui.painter().line_segment(
                    [c + Vec2::new(-3.0, -1.5), c + Vec2::new(0.0, 1.5)],
                    Stroke::new(1.0_f32, MUTED),
                );
                ui.painter().line_segment(
                    [c + Vec2::new(0.0, 1.5), c + Vec2::new(3.0, -1.5)],
                    Stroke::new(1.0_f32, MUTED),
                );
            }
            if response.clicked() {
                match index {
                    0 => {
                        self.list_view = !self.list_view;
                        self.reset_catalog_scroll = true;
                    }
                    1 => self.browse_menu = Some((browse::Menu::Sort, 0)),
                    2 => self.browse_menu = Some((browse::Menu::Filter, 0)),
                    3 => {
                        if self.route.is_episodes() {
                            self.back_catalog();
                        } else {
                            self.open_season(None);
                        }
                    }
                    _ => self.back_catalog(),
                }
            }
            x -= 8.0;
        }
    }
    fn draw_browse_menu(&mut self, ctx: &egui::Context) {
        let Some((menu, selected)) = self.browse_menu else {
            return;
        };
        let title = match menu {
            browse::Menu::Sort => "Sort & layout",
            browse::Menu::Filter => "Filter titles",
        };
        let labels: Vec<String> = match menu {
            browse::Menu::Sort => Sort::ALL
                .iter()
                .map(|sort| sort.label().into())
                .chain([format!(
                    "Layout: {}",
                    if self.list_view { "List" } else { "Grid" }
                )])
                .collect(),
            browse::Menu::Filter => Filter::ALL.iter().map(|f| f.label().into()).collect(),
        };
        let mut activate = None;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("browse-menu"))
            .frame(
                egui::Frame::new()
                    .fill(design::SURFACE)
                    .corner_radius(16.0)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.set_width(350.0);
                ui.label(RichText::new(title).size(24.0));
                ui.add_space(16.0);
                for (i, label) in labels.iter().enumerate() {
                    let active = match menu {
                        browse::Menu::Sort => Sort::ALL
                            .get(i)
                            .is_some_and(|s| *s == self.browse_options[self.tab].sort),
                        browse::Menu::Filter => Filter::ALL
                            .get(i)
                            .is_some_and(|f| *f == self.browse_options[self.tab].filter),
                    };
                    let label = format!("{}{}", if active { "•  " } else { "    " }, label);
                    if button(
                        ui,
                        &label,
                        selected == i,
                        Vec2::new(ui.available_width(), 44.0),
                    )
                    .clicked()
                    {
                        activate = Some(i);
                    }
                    ui.add_space(6.0);
                }
                ui.add_space(10.0);
                if ui.link("Cancel · B").clicked() {
                    cancel = true;
                }
            });
        if let Some(i) = activate {
            self.browse_menu = Some((menu, i));
            self.apply_browse_menu();
        } else if cancel {
            self.browse_menu = None;
        }
    }
    fn artwork(&self, item: &Item, backdrop: bool) -> Option<(egui::Image<'static>, egui::Rect)> {
        if let Some(preview) = &self.preview {
            return preview.artwork(item, backdrop);
        }
        self.api.as_ref().map(|api| {
            (
                egui::Image::new(api.image(item, backdrop)),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            )
        })
    }
    fn draw_hero(&mut self, ui: &mut egui::Ui, height: f32) {
        let index = if self.focus >= 6 && self.focus - 6 < self.items.len() {
            self.feature_index = self.focus - 6;
            self.feature_index
        } else {
            self.feature_index.min(self.items.len().saturating_sub(1))
        };
        let Some(item) = self.items.get(index).cloned() else {
            return;
        };
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            egui::Sense::hover(),
        );
        self.hero_bounds = Some(rect);
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
        painter.rect_filled(rect, 16.0, Color32::from_rgb(33, 39, 48));
        if let Some((image, uv)) = self.artwork(&item, true) {
            design::cover(ui, image, rect, uv, 16);
        }
        design::fade(
            ui,
            rect,
            &[
                (0.0, Color32::from_rgb(20, 24, 30)),
                (0.30, Color32::from_rgba_unmultiplied(20, 24, 30, 245)),
                (0.64, Color32::from_rgba_unmultiplied(20, 24, 30, 160)),
                (1.0, Color32::from_black_alpha(10)),
            ],
            false,
        );
        design::fade(
            ui,
            rect,
            &[
                (0.0, Color32::TRANSPARENT),
                (0.6, Color32::TRANSPARENT),
                (1.0, Color32::from_black_alpha(120)),
            ],
            true,
        );
        let pad = if height < 200.0 { 20.0 } else { 28.0 };
        let left = rect.min + Vec2::splat(pad);
        let width = (rect.width() * 0.56)
            .clamp(280.0, 720.0)
            .min(rect.width() - pad * 2.0);
        design::text(
            ui,
            egui::Rect::from_min_size(left, Vec2::new(width, 18.0)),
            if item.kind == "Series" {
                "YOUR NEXT SERIES"
            } else {
                "FROM YOUR LIBRARY"
            },
            11.0,
            ACCENT,
            1,
        );
        design::text(
            ui,
            egui::Rect::from_min_size(left + Vec2::new(0.0, 24.0), Vec2::new(width, 52.0)),
            &item.name,
            if height < 200.0 { 28.0 } else { 38.0 },
            design::TEXT,
            1,
        );
        design::text(
            ui,
            egui::Rect::from_min_size(
                left + Vec2::new(0.0, if height < 200.0 { 61.0 } else { 72.0 }),
                Vec2::new(width, 22.0),
            ),
            &metadata(&item),
            14.0,
            Color32::from_rgb(203, 202, 202),
            1,
        );
        if height >= 235.0 {
            design::text(
                ui,
                egui::Rect::from_min_size(
                    left + Vec2::new(0.0, 104.0),
                    Vec2::new(width.min(600.0), 44.0),
                ),
                &item.overview,
                15.0,
                Color32::from_rgb(185, 190, 199),
                2,
            );
        }
        let cta = egui::Rect::from_min_size(
            egui::pos2(left.x, rect.bottom() - pad - 32.0),
            Vec2::new(140.0, 32.0),
        );
        let r = ui.interact(cta, ui.id().with("hero-details"), egui::Sense::click());
        painter.rect_filled(
            cta,
            7.0,
            if r.hovered() {
                ACCENT
            } else {
                Color32::from_rgb(230, 220, 212)
            },
        );
        painter.text(
            cta.center(),
            egui::Align2::CENTER_CENTER,
            "Explore title",
            egui::FontId::proportional(14.0),
            BG,
        );
        if r.clicked() {
            self.detail = Some(item);
            self.detail_focus = 0;
        }
    }
    fn draw_library(&mut self, ui: &mut egui::Ui) {
        if self.is_dashboard() && !self.shelves.is_empty() {
            self.draw_dashboard(ui);
            return;
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(24, 4))
            .show(ui, |ui| self.draw_library_toolbar(ui));
        let page_height = ui.available_height();
        let show_hero = !self.items.is_empty()
            && self.search.is_empty()
            && self.route.is_library()
            && page_height > 520.0;
        let hero_height = if show_hero {
            (page_height * 0.34).clamp(155.0, 285.0)
        } else {
            0.0
        };
        if self.items.is_empty() {
            if self.busy {
                let g = design::CatalogGeometry::new(ui.available_width(), ui.available_height());
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(
                        ui.available_width(),
                        g.card_height.min(ui.available_height()),
                    ),
                    egui::Sense::hover(),
                );
                for i in 0..g.columns {
                    let card = g.card(rect.min, i);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(card.min, Vec2::new(g.width, g.poster_height)),
                        12.0,
                        design::SURFACE,
                    );
                }
            } else {
                ui.add_space(40.0);
                ui.label(
                    RichText::new("Nothing here yet")
                        .size(28.0)
                        .color(design::TEXT),
                );
                if matches!(self.route, Route::Seasons { .. })
                    && button(
                        ui,
                        "Browse all episodes",
                        self.focus == 6,
                        Vec2::new(260.0, 44.0),
                    )
                    .clicked()
                {
                    self.open_season(None);
                }
                ui.label(
                    RichText::new("Try another section or search your library.")
                        .size(16.0)
                        .color(MUTED),
                );
            }
            return;
        }
        let grid_budget = page_height - if show_hero { hero_height + 22.0 } else { 0.0 };
        let mut g = if matches!(self.tab, 1 | 2) {
            design::CatalogGeometry::library((ui.available_width() - 8.0).max(100.0), grid_budget)
        } else {
            design::CatalogGeometry::new((ui.available_width() - 8.0).max(100.0), grid_budget)
        };
        if self.route.is_episodes() {
            g = design::CatalogGeometry::episodes((ui.available_width() - 8.0).max(100.0));
        }
        if self.list_view {
            g.columns = 1;
            g.width = ui.available_width() - 8.0;
            g.poster_height = 84.0;
            g.card_height = 104.0;
            g.gap = 8.0;
        }
        self.columns = g.columns;
        let has_more = self.items.len() < self.total;
        let height = g.height(self.items.len()) + 8.0 + if has_more { 76.0 } else { 0.0 };
        let mut open = None;
        let mut more = false;
        let salt = (
            "catalog-scroll",
            self.tab,
            self.route.key(),
            self.search.clone(),
        );
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink([false, false]);
        let reset = self.reset_catalog_scroll;
        if reset {
            scroll = scroll.vertical_scroll_offset(0.0);
            self.reset_catalog_scroll = false;
        }
        scroll.show_viewport(ui, |ui, _viewport| {
            self.catalog_bounds = Some(ui.clip_rect());
            if show_hero {
                self.draw_hero(ui, hero_height);
                ui.add_space(22.0);
            }
            let (bounds, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), height),
                egui::Sense::hover(),
            );
            let origin = bounds.min + Vec2::splat(4.0);
            for (i, item) in self.items.iter().enumerate() {
                let rect = g.card(origin, i);
                let focused = self.focus == i + 6;
                if focused {
                    self.focus_bounds = Some(rect);
                }
                if focused && self.focus_changed && !reset {
                    ui.scroll_to_rect(rect.expand(4.0), None);
                }
                if !ui.is_rect_visible(rect.expand(5.0)) {
                    continue;
                }
                let r = ui.interact(
                    rect,
                    ui.id().with(("title", &item.id)),
                    egui::Sense::click(),
                );
                if self.list_view {
                    ui.painter().rect_filled(
                        rect,
                        10.0,
                        if focused {
                            Color32::from_rgb(44, 37, 34)
                        } else {
                            design::SURFACE
                        },
                    );
                    if focused {
                        ui.painter().rect_stroke(
                            rect,
                            10.0,
                            Stroke::new(1.5_f32, design::FOCUS),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let poster = egui::Rect::from_min_size(
                        rect.min + Vec2::splat(10.0),
                        Vec2::new(56.0, 84.0),
                    );
                    if let Some((image, uv)) = self.artwork(item, false) {
                        design::cover(ui, image, poster, uv, 6);
                    }
                    design::text(
                        ui,
                        egui::Rect::from_min_size(
                            rect.min + Vec2::new(88.0, 22.0),
                            Vec2::new(rect.width() - 120.0, 30.0),
                        ),
                        &item.name,
                        18.0,
                        design::TEXT,
                        1,
                    );
                    design::text(
                        ui,
                        egui::Rect::from_min_size(
                            rect.min + Vec2::new(88.0, 58.0),
                            Vec2::new(rect.width() - 120.0, 22.0),
                        ),
                        &metadata(item),
                        13.0,
                        MUTED,
                        1,
                    );
                    if r.clicked() {
                        open = Some(i);
                    }
                    continue;
                }
                let poster =
                    egui::Rect::from_min_size(rect.min, Vec2::new(g.width, g.poster_height));
                ui.painter().rect_filled(poster, 11.0, design::SURFACE);
                let icon = egui::Rect::from_center_size(
                    poster.center() - Vec2::new(0.0, 10.0),
                    Vec2::new(42.0, 48.0),
                );
                ui.painter().rect_stroke(
                    icon,
                    5.0,
                    Stroke::new(2.0_f32, Color32::from_rgb(65, 76, 91)),
                    egui::StrokeKind::Inside,
                );
                for n in 0..3 {
                    for x in [icon.left() + 5.0, icon.right() - 9.0] {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(x, icon.top() + 8.0 + n as f32 * 12.0),
                                Vec2::splat(4.0),
                            ),
                            1.0,
                            Color32::from_rgb(65, 76, 91),
                        );
                    }
                }
                if let Some((image, uv)) = self.artwork(item, false) {
                    design::cover(ui, image, poster, uv, 11);
                }
                if matches!(self.tab, 1 | 2)
                    && let Some(rating) = item.community_rating
                {
                    let badge = egui::Rect::from_min_size(
                        poster.min + Vec2::splat(7.0),
                        Vec2::new(52.0, 24.0),
                    );
                    ui.painter()
                        .rect_filled(badge, 6.0, Color32::from_black_alpha(205));
                    let center = badge.min + Vec2::new(12.0, 12.0);
                    let points = (0..10)
                        .map(|n| {
                            let a =
                                n as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                            center
                                + Vec2::new(a.cos(), a.sin()) * if n % 2 == 0 { 6.0 } else { 2.8 }
                        })
                        .collect();
                    ui.painter()
                        .add(egui::Shape::convex_polygon(points, ACCENT, Stroke::NONE));
                    ui.painter().text(
                        badge.min + Vec2::new(24.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("{rating:.1}"),
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                }
                design::focus_ring(
                    ui,
                    poster,
                    ui.id().with(("catalog-glow", &item.id)),
                    focused || r.hovered(),
                    13.0,
                );
                let progress = item
                    .user_data
                    .played_percentage
                    .map(|p| p / 100.0)
                    .or_else(|| {
                        item.run_time_ticks
                            .filter(|t| *t > 0)
                            .map(|t| item.user_data.playback_position_ticks as f32 / t as f32)
                    })
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                if !item.user_data.played {
                    design::progress_bar(ui, poster, progress);
                }
                if item.user_data.played {
                    let center = poster.right_top() + Vec2::new(-17.0, 17.0);
                    ui.painter()
                        .circle_filled(center, 10.0, Color32::from_rgb(55, 45, 38));
                    ui.painter().line_segment(
                        [center + Vec2::new(-4.0, 0.0), center + Vec2::new(-1.0, 3.0)],
                        Stroke::new(1.7_f32, ACCENT),
                    );
                    ui.painter().line_segment(
                        [center + Vec2::new(-1.0, 3.0), center + Vec2::new(5.0, -4.0)],
                        Stroke::new(1.7_f32, ACCENT),
                    );
                }
                design::text(
                    ui,
                    egui::Rect::from_min_size(
                        poster.left_bottom() + Vec2::new(0.0, 10.0),
                        Vec2::new(g.width, 40.0),
                    ),
                    &if item.kind == "Season" {
                        item.season_label()
                    } else {
                        item.name.clone()
                    },
                    16.0,
                    if focused {
                        Color32::WHITE
                    } else {
                        design::TEXT
                    },
                    2,
                );
                let subtitle = if item.kind == "Episode" {
                    format!(
                        "{}{}",
                        item.episode_code(),
                        item.run_time_ticks
                            .map(|t| format!(" · {} min", t / 600_000_000))
                            .unwrap_or_default()
                    )
                } else if item.kind == "Season" {
                    item.child_count
                        .map(|n| format!("{n} episodes"))
                        .unwrap_or_else(|| "Browse episodes".into())
                } else {
                    item.production_year
                        .map(|y| y.to_string())
                        .unwrap_or_else(|| item.kind.clone())
                };
                design::text(
                    ui,
                    egui::Rect::from_min_size(
                        rect.left_bottom() - Vec2::new(0.0, 18.0),
                        Vec2::new(g.width, 18.0),
                    ),
                    &subtitle,
                    12.0,
                    MUTED,
                    1,
                );
                if r.clicked() {
                    open = Some(i);
                }
                r.on_hover_text(&item.name);
            }
            if has_more {
                let rect = egui::Rect::from_min_size(
                    origin + Vec2::new(0.0, g.height(self.items.len()) + 24.0),
                    Vec2::new(ui.available_width().min(320.0), 42.0),
                );
                let focused = self.focus == self.items.len() + 6;
                if focused && self.focus_changed {
                    ui.scroll_to_rect(rect, None);
                }
                let r = ui.interact(rect, ui.id().with("more-titles"), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 8.0, if focused { ACCENT } else { design::SURFACE });
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if self.busy {
                        "Loading…"
                    } else {
                        "Load more titles"
                    },
                    egui::FontId::proportional(15.0),
                    if focused { BG } else { design::TEXT },
                );
                more = r.clicked();
            }
        });
        if let Some(i) = open {
            self.activate_item(i);
        }
        if more {
            self.load_more();
        }
    }
    fn draw_detail(&mut self, ui: &mut egui::Ui) {
        self.draw_cinematic_detail(ui);
    }
    fn draw_playback(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        self.focus_bounds = Some(rect);
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        if let Some(embedded) = &self.embedded {
            embedded.paint(ui, rect);
        }
        let Some(player) = &self.player else { return };
        let response = ui.interact(rect, ui.id().with("embedded-video"), egui::Sense::click());
        let fixture_only =
            self.preview.is_some() && std::env::var_os("ASTER_WINDOW_INACTIVE").is_some();
        if response.hovered() && !fixture_only {
            if let Some(pos) = response.hover_pos()
                && ui.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.any_pressed())
            {
                let p = (pos - rect.min) * ui.ctx().pixels_per_point();
                player.command(json!(["mouse", p.x.round() as i32, p.y.round() as i32]));
            }
            if response.clicked()
                && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
            {
                player.command(json!(["keypress", "MBTN_LEFT"]));
            }
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                player.command(json!(["add", "volume", if scroll > 0.0 { 5 } else { -5 }]));
            }
        }
        let scale = rect.height() / 720.0;
        let track_y = rect.bottom() - 63.0 * scale;
        let hovered = ui.input(|i| i.pointer.hover_pos()).filter(|p| {
            (p.y - track_y).abs() < 12.0 * scale
                && p.x >= rect.left() + 64.0 * scale
                && p.x <= rect.right() - 70.0 * scale
        });
        let timeline =
            self.player_overlay["timeline"] == true && self.player_overlay["controls"] == true;
        if (timeline || hovered.is_some()) && self.duration > 0.0 {
            let target = hovered
                .map(|p| {
                    ((p.x - rect.left() - 64.0 * scale) / (rect.width() - 134.0 * scale))
                        .clamp(0.0, 1.0) as f64
                        * self.duration
                })
                .unwrap_or(self.position);
            if let Some(previews) = &mut self.seek_previews {
                previews.paint(ui, rect, target, self.duration);
            }
        }
    }
    fn draw_keyboard(&mut self, ctx: &egui::Context) {
        let Some(k) = &mut self.keyboard else { return };
        let mut pressed = None;
        let mut cancel = false;
        let frame = egui::Frame::new()
            .fill(Color32::from_rgb(27, 32, 41))
            .corner_radius(16.0)
            .inner_margin(24)
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(57, 64, 77)));
        egui::Modal::new(egui::Id::new("keyboard"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(185))
            .show(ctx, |ui| {
                let width = (ctx.viewport_rect().width() - 100.0).clamp(300.0, 740.0);
                ui.set_width(width);
                ui.label(
                    RichText::new(match k.field {
                        0 => "Server address",
                        1 => "Username",
                        2 => "Password",
                        _ => "Search your library",
                    })
                    .size(23.0)
                    .color(design::TEXT),
                );
                ui.add_space(14.0);
                // A steady TV keyboard cursor avoids periodic full-window redraws.
                ui.visuals_mut().text_cursor.blink=false;
                let output = egui::TextEdit::singleline(&mut k.text)
                    .id_salt("keyboard-value")
                    .password(k.field == 2)
                    .desired_width(width)
                    .font(egui::FontId::proportional(20.0))
                    .margin(Vec2::new(14.0, 12.0))
                    .show(ui);
                if k.request_focus {
                    output.response.request_focus();
                    let mut state = output.state;
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(k.text.chars().count()),
                        )));
                    state.store(ctx, output.response.id);
                    k.request_focus = false;
                }
                ui.add_space(20.0);
                let gap = 7.0;
                let unit = (width - gap * 9.0) / 10.0;
                let mut offset = 0;
                for row in k.rows() {
                    let row_width: f32 = row
                        .iter()
                        .map(|key| unit * key.units + gap * (key.units - 1.0))
                        .sum::<f32>()
                        + gap * (row.len().saturating_sub(1)) as f32;
                    let (bounds, _) =
                        ui.allocate_exact_size(Vec2::new(width, 45.0), egui::Sense::hover());
                    let mut x = bounds.left() + (width - row_width) / 2.0;
                    for (column, key) in row.iter().enumerate() {
                        let index = offset + column;
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(x, bounds.top()),
                            Vec2::new(unit * key.units + gap * (key.units - 1.0), 45.0),
                        );
                        let r =
                            ui.interact(rect, ui.id().with(("key", index)), egui::Sense::click());
                        let selected = k.selected == index;
                        let important = key.action == KeyAction::Done;
                        let bg = if selected {
                            ACCENT
                        } else if important {
                            Color32::from_rgb(71, 57, 49)
                        } else if r.hovered() {
                            Color32::from_rgb(62, 68, 81)
                        } else {
                            Color32::from_rgb(42, 49, 61)
                        };
                        ui.painter().rect_filled(rect, 7.0, bg);
                        if k.upper && key.action == KeyAction::Shift && !selected {
                            ui.painter().rect_stroke(
                                rect,
                                7.0,
                                Stroke::new(1.0_f32, design::FOCUS),
                                egui::StrokeKind::Inside,
                            );
                        }
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &key.label,
                            egui::FontId::proportional(
                                if matches!(key.action, KeyAction::Character(_)) {
                                    18.0
                                } else {
                                    14.0
                                },
                            ),
                            if selected { BG } else { design::TEXT },
                        );
                        if r.clicked() {
                            pressed = Some(index);
                        }
                        x = rect.right() + gap;
                    }
                    offset += row.len();
                    ui.add_space(gap - 2.0);
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("R2 / RT: Shift   ·   L2 / LT: Symbols   ·   Y: Space   ·   X: Delete   ·   Start / Menu: Done")
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.link(RichText::new("Cancel").color(MUTED)).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            pressed = self.keyboard.as_ref().map(Keyboard::done_index);
        }
        if let Some(index) = pressed {
            self.keyboard.as_mut().unwrap().selected = index;
            self.keyboard_key();
        }
        if cancel {
            self.keyboard = None;
        }
    }
}
impl eframe::App for Aster {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player = None;
        if let Some(embedded) = &mut self.embedded {
            embedded.close();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Wayland swap waiting stays disabled (it can deadlock when occluded).
        // Cap bursts from immediate animation requests without a perpetual timer.
        let delay = Duration::from_millis(16).saturating_sub(self.last_frame.elapsed());
        if !delay.is_zero() {
            thread::sleep(delay);
        }
        self.last_frame = Instant::now();
        self.ui_frames += 1;
        self.messages();
        if let Some(embedded) = &self.embedded {
            embedded.set_active(self.player.is_some());
        }
        if let Some(value) = self.player_fullscreen.take() {
            self.settings.fullscreen = value;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(value));
        }
        if self
            .search_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.search_deadline = None;
            self.route = Route::Library;
            self.parents.clear();
            self.detail = None;
            if self.tab == 5 {
                self.tab = 0;
            }
            self.load();
        }
        let zoom =
            self.settings.ui_scale / ctx.native_pixels_per_point().unwrap_or(1.0).clamp(0.1, 1.0);
        if (ctx.zoom_factor() - zoom).abs() > 0.001 {
            ctx.set_zoom_factor(zoom);
        }
        let commands: Vec<_> = self
            .control
            .as_ref()
            .map(|c| c.commands.try_iter().collect())
            .unwrap_or_default();
        for command in commands {
            match command["command"].as_str().unwrap_or("") {
                "capture" => {
                    if let Some(path) = command["path"].as_str() {
                        self.live_capture = Some(path.into());
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                            egui::UserData::new("live"),
                        ));
                    }
                }
                "fullscreen" => {
                    if let Some(enabled) = command["enabled"].as_bool() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
                    }
                }
                "resize" => {
                    if let (Some(w), Some(h)) =
                        (command["width"].as_f64(), command["height"].as_f64())
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                            w as f32, h as f32,
                        )));
                    }
                }
                "tab" => {
                    if let Some(index) = command["index"].as_u64().filter(|i| *i < 6) {
                        self.select_tab(index as usize);
                    }
                }
                "detail" => {
                    if let Some(index) = command["index"].as_u64() {
                        self.detail = self.items.get(index as usize).cloned();
                        self.detail_focus = 0;
                    }
                }
                "focus" => {
                    if let Some(index) = command["index"].as_u64() {
                        self.focus = (index as usize)
                            .min(5 + self.items.len() + usize::from(self.items.len() < self.total));
                        self.focus_changed = true;
                    }
                }
                "action" => {
                    let a = match command["action"].as_str() {
                        Some("left") => Some(Action::Left),
                        Some("right") => Some(Action::Right),
                        Some("left-repeat") => Some(Action::LeftRepeat),
                        Some("right-repeat") => Some(Action::RightRepeat),
                        Some("seek-release") => Some(Action::ReleaseHorizontal),
                        Some("up") => Some(Action::Up),
                        Some("down") => Some(Action::Down),
                        Some("confirm") => Some(Action::Confirm),
                        Some("menu") => Some(Action::Menu),
                        Some("delete") => Some(Action::Delete),
                        Some("search") | Some("chapters") => Some(Action::Search),
                        Some("audio") => Some(Action::Audio),
                        Some("subtitles") => Some(Action::Subtitles),
                        Some("back") => Some(Action::Back),
                        Some("previous") => Some(Action::Prev),
                        Some("next") => Some(Action::Next),
                        _ => None,
                    };
                    if let Some(a) = a {
                        self.dev_action = true;
                        self.action(a, ctx);
                        self.dev_action = false;
                    }
                }
                "search-fixture" if self.preview.is_some() => {
                    self.search = command["query"].as_str().unwrap_or("").to_owned();
                    self.load();
                }
                "play-fixture" => {
                    // Explicitly opt-in local mock playback, never the real library.
                    if self.preview.is_some()
                        && let Ok(server) = std::env::var("ASTER_DEV_FIXTURE_SERVER")
                        && let Ok(url) = url::Url::parse(&server)
                        && url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && let Some(embedded) = &self.embedded
                    {
                        let mut api = Api::new(&server, "embedded-fixture").unwrap();
                        api.session = Some(Session {
                            access_token: "fixture-token".into(),
                            user: crate::jellyfin::User {
                                id: "fixture-user".into(),
                                name: "Fixture".into(),
                            },
                        });
                        let episodes = command["episodes"] == true;
                        let item = Item {
                            id: if episodes {
                                "episode-one"
                            } else {
                                "fixture-media"
                            }
                            .into(),
                            name: "Embedded playback test".into(),
                            kind: if episodes { "Episode" } else { "Movie" }.into(),
                            series_id: episodes.then(|| "fixture-show".into()),
                            series_name: episodes.then(|| "Fixture show".into()),
                            season_id: episodes.then(|| "fixture-season".into()),
                            parent_index_number: episodes.then_some(1),
                            index_number: episodes.then_some(1),
                            user_data: crate::jellyfin::UserData {
                                playback_position_ticks: command["ticks"].as_u64().unwrap_or(0),
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        let settings = Settings {
                            volume: 0,
                            hardware_decode: command["hardware"] == true,
                            autoplay: episodes,
                            autoplay_delay_seconds: 7,
                            ..self.settings.clone()
                        };
                        self.position = 0.0;
                        self.duration = 0.0;
                        self.paused = false;
                        self.player_overlay = serde_json::Value::Null;
                        self.playing = item.name.clone();
                        self.detail = Some(item.clone());
                        self.detail_requested = item.id.clone();
                        self.player = Some(Player::start_embedded(
                            api,
                            item,
                            settings,
                            embedded.core.clone(),
                        ));
                        if let Some(player) = &self.player {
                            player.set_context(self.preview_context.clone());
                        }
                    }
                }
                "player-command" => {
                    if let Some(player) = &self.player
                        && command["args"].is_array()
                    {
                        player.command(command["args"].clone());
                    }
                }
                "close" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                _ => {}
            }
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot {
                user_data, image, ..
            } = event
                && user_data
                    .data
                    .as_ref()
                    .and_then(|s| s.downcast_ref::<&str>())
                    == Some(&"live")
                && let Some(path) = self.live_capture.take()
            {
                let pixels: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    path,
                    &pixels,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    self.error = e.to_string();
                }
            }
        }
        let fixture_only =
            self.preview.is_some() && std::env::var_os("ASTER_WINDOW_INACTIVE").is_some();
        let focused = ctx.input(|i| i.focused) && !fixture_only;
        let actions = self.input.poll(
            self.settings.stick_deadzone,
            self.settings.swap_confirm,
            focused,
            self.player.as_ref().map(|p| p.commands.clone()),
        );
        if self.input.mirrored_navigation {
            ctx.input_mut(|i| crate::input::filter_mirrored_navigation(&mut i.events));
        }
        if focused {
            for action in actions {
                if self.keyboard.is_none() {
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
                self.action(action, ctx);
            }
        }
        // Native text editing keeps arrow keys and spaces; Escape/Enter finish or cancel the keyboard.
        if self.keyboard.is_some() {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.keyboard = None;
            }
        } else if focused && !ctx.wants_keyboard_input() {
            if self.player.is_some() {
                for (key, action) in [
                    (egui::Key::Space, Action::Confirm),
                    (egui::Key::Tab, Action::Menu),
                    (egui::Key::A, Action::Audio),
                    (egui::Key::S, Action::Subtitles),
                    (egui::Key::C, Action::Search),
                    (egui::Key::PageUp, Action::Prev),
                    (egui::Key::PageDown, Action::Next),
                ] {
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                        self.action(action, ctx);
                    }
                }
            }
            if self.player.is_some()
                && ctx.input(|i| {
                    i.key_released(egui::Key::ArrowLeft) || i.key_released(egui::Key::ArrowRight)
                })
            {
                self.action(Action::ReleaseHorizontal, ctx);
            }
            for (key, action) in [
                (egui::Key::ArrowLeft, Action::Left),
                (egui::Key::ArrowRight, Action::Right),
                (egui::Key::ArrowUp, Action::Up),
                (egui::Key::ArrowDown, Action::Down),
                (egui::Key::Enter, Action::Confirm),
                (egui::Key::Escape, Action::Back),
                (egui::Key::F1, Action::Menu),
                (egui::Key::F3, Action::Search),
            ] {
                let repeated = self.player.is_some() && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: event_key, pressed: true, repeat: true, .. } if *event_key == key)));
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                    let action = if repeated {
                        match action {
                            Action::Left => Action::LeftRepeat,
                            Action::Right => Action::RightRepeat,
                            _ => action,
                        }
                    } else {
                        action
                    };
                    self.action(action, ctx);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
                self.settings.fullscreen = !self.settings.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.settings.fullscreen));
            }
        }
        if self.detail.is_none() {
            self.detail_requested.clear();
        }
        self.hero_bounds = None;
        self.catalog_bounds = None;
        self.focus_bounds = None;
        let key = format!(
            "{}:{}:{}",
            self.tab,
            self.route.key(),
            self.detail.as_ref().map(|i| i.id.as_str()).unwrap_or("")
        );
        if self.page_key != key {
            self.page_key = key;
            self.page_started = Instant::now();
        }
        let transition = (self.page_started.elapsed().as_secs_f32() / 0.20).clamp(0.0, 1.0);
        if transition < 1.0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let opacity = 0.35 + 0.65 * (1.0 - (1.0 - transition).powi(3));
        if self.player.is_none() {
            self.draw_header(ctx);
            self.draw_footer(ctx);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(
                if self.player.is_some() || (self.api.is_some() && self.tab != 5) {
                    egui::Margin::ZERO
                } else {
                    egui::Margin::symmetric(32, 20)
                },
            ))
            .show(ctx, |ui| {
                if self.player.is_none() {
                    ui.set_opacity(opacity);
                }
                if !self.error.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(Color32::LIGHT_RED, &self.error);
                        if ui.button("Dismiss").clicked() {
                            self.error.clear();
                        }
                    });
                }
                if self.busy && self.detail.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Connecting to your library…");
                    });
                }
                if self.player.is_some() {
                    self.draw_playback(ui);
                } else if self.api.is_none() {
                    self.draw_setup(ui);
                } else if self.player.is_none() && self.detail.is_none() && self.tab != 5 {
                    self.draw_library(ui);
                } else {
                    let mut scroll = egui::ScrollArea::vertical().id_salt((
                        "page",
                        self.tab,
                        self.detail.as_ref().map(|i| i.id.clone()),
                    ));
                    if self.detail.is_some() && self.detail_scroll_delta != 0.0 {
                        scroll = scroll.vertical_scroll_offset(
                            (self.detail_scroll + self.detail_scroll_delta).max(0.0),
                        );
                        self.detail_scroll_delta = 0.0;
                    }
                    let output = scroll.auto_shrink([false, false]).show(ui, |ui| {
                        if self.api.is_none() {
                            self.draw_setup(ui);
                        } else if self.player.is_some() {
                            self.draw_playback(ui);
                        } else if self.detail.is_some() {
                            self.draw_detail(ui);
                        } else {
                            self.draw_settings(ui, ctx);
                        }
                    });
                    if self.detail.is_some() {
                        self.detail_scroll = output.state.offset.y;
                    }
                }
            });
        self.draw_browse_menu(ctx);
        self.draw_keyboard(ctx);
        if self.keyboard.is_some() {
            // Our TV keyboard owns text entry; avoid opening KDE's second
            // virtual keyboard and its IME cursor-position feedback loop.
            ctx.output_mut(|output| output.ime = None);
        }
        if let Some(control) = &self.control {
            let rect =
                |r: Option<egui::Rect>| r.map(|r| [r.left(), r.top(), r.right(), r.bottom()]);
            let mut status = json!({"ready":true,"controller":self.input.controller,"mirrored_navigation":self.input.mirrored_navigation,"keyboard_open":self.keyboard.is_some(),"keyboard_selected":self.keyboard.as_ref().map(|k|k.selected),"signed_in":self.api.is_some(),"loading":self.busy,"section":TABS[self.tab],"detail":self.detail.is_some(),"detail_focus":self.detail_focus,"inline_seasons":self.inline.seasons.len(),"inline_episodes":self.inline.episodes.len(),"inline_selected":self.inline.selected,"page_transition":transition,"player_active":self.player.is_some(),"embedded":self.embedded.is_some(),"player_ui":self.player_overlay,"position":self.position,"duration":self.duration,"paused":self.paused,"autoplay":self.settings.autoplay,"up_next":self.up_next,"playing":self.playing,"route":self.route.key(),"catalog_title":self.route.title(),"item_names":self.items.iter().take(4).map(|i|i.name.clone()).collect::<Vec<_>>(),"focus":self.focus,"items":self.items.len(),"columns":self.columns,"search_width":self.search_bounds.map(|r|r.width()),"remember_signin":self.settings.remember_session,"fullscreen":ctx.input(|i|i.viewport().fullscreen.unwrap_or(self.settings.fullscreen)),"sort":self.browse_options[self.tab].sort.label(),"filter":self.browse_options[self.tab].filter.label(),"list_view":self.list_view,"viewport":rect(Some(ctx.viewport_rect())),"hero":rect(self.hero_bounds),"catalog":rect(self.catalog_bounds),"focused_card":rect(self.focus_bounds),"error":self.error});
            status["seek_preview_source"] =
                json!(self.seek_previews.as_ref().and_then(|p| p.source()));
            status["shelf_offsets"] = json!(self.shelf_offsets);
            status["seek_preview_position"] =
                json!(self.seek_previews.as_ref().and_then(|p| p.timestamp()));
            status["input_events"] = json!(ctx.input(|i| {
                i.events
                    .iter()
                    .map(|event| match event {
                        egui::Event::Key {
                            key,
                            pressed,
                            repeat,
                            ..
                        } => format!("key:{key:?}:{pressed}:{repeat}"),
                        egui::Event::Text(_) => "text".to_owned(),
                        egui::Event::PointerMoved(_) => "pointer-moved".to_owned(),
                        _ => format!("{:?}", std::mem::discriminant(event)),
                    })
                    .collect::<Vec<_>>()
            }));
            status["repaint_causes"] = json!(
                ctx.repaint_causes()
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect::<Vec<_>>()
            );
            status["performance"] = json!({"ui_frames":self.ui_frames,"uptime_seconds":self.uptime.elapsed().as_secs_f64(),"render_calls":self.embedded.as_ref().map(|e|e.render_calls()),"mpv_socket":self.embedded.as_ref().map(|e|e.core.socket.to_string_lossy())});
            control.report(status);
        }
        self.focus_changed = false;
        if let Ok(path) = std::env::var("ASTER_CAPTURE_PATH") {
            self.capture_frames += 1;
            if self.capture_frames > 60
                && !self.capture_sent
                && !self.busy
                && !ctx.has_pending_images()
            {
                self.capture_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        &path,
                        &pixels,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("Screenshot failed: {e}");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            if self.capture_frames > 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if std::env::var_os("ASTER_CAPTURE_PATH").is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if let Some(deadline) = self.search_deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        }
        if self.quick.is_some() && !self.polling {
            ctx.request_repaint_after(
                Duration::from_secs(3).saturating_sub(self.last_poll.elapsed()),
            );
        }
    }
}
fn button(ui: &mut egui::Ui, text: &str, focused: bool, size: Vec2) -> egui::Response {
    let response = ui.add_sized(
        size,
        egui::Button::new(RichText::new(text).size(15.0).color(design::TEXT))
            .fill(if focused {
                Color32::from_rgb(43, 38, 35)
            } else {
                Color32::from_rgb(25, 27, 31)
            })
            .stroke(Stroke::NONE)
            .corner_radius(7.0),
    );
    design::focus_ring(
        ui,
        response.rect,
        ui.id().with(("button-focus", text)),
        focused || response.hovered(),
        7.0,
    );
    response
}

fn metadata(i: &Item) -> String {
    let mut parts = vec![if i.kind == "Episode" {
        i.episode_code()
    } else {
        i.kind.clone()
    }];
    if let Some(year) = i.production_year {
        parts.push(year.to_string());
    }
    if let Some(r) = i.community_rating {
        parts.push(format!("Rating {r:.1}"));
    }
    if let Some(t) = i.run_time_ticks {
        parts.push(format!("{} min", t / 600_000_000));
    }
    if let Some(series) = &i.series_name {
        parts.push(series.clone());
    }
    if i.user_data.is_favorite {
        parts.push("Favorite".into());
    }
    parts.join("   ·   ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> (Aster, egui::Context) {
        let ctx = egui::Context::default();
        let app = Aster::new(&ctx, Settings::default());
        (app, ctx)
    }
    fn connect(app: &mut Aster) {
        let mut api = Api::new("https://example.org/", "test").unwrap();
        api.session = Some(Session {
            access_token: "test".into(),
            user: crate::jellyfin::User {
                id: "u".into(),
                name: "Viewer".into(),
            },
        });
        app.api = Some(api);
    }
    #[test]
    fn held_direction_events_keep_keyboard_navigation_and_release_is_inert() {
        let (mut app, ctx) = app();
        app.edit(1);
        app.keyboard.as_mut().unwrap().selected = 10;
        app.action(Action::RightRepeat, &ctx);
        assert_eq!(app.keyboard.as_ref().unwrap().selected, 11);
        app.action(Action::LeftRepeat, &ctx);
        assert_eq!(app.keyboard.as_ref().unwrap().selected, 10);
        app.action(Action::ReleaseHorizontal, &ctx);
        assert_eq!(app.keyboard.as_ref().unwrap().selected, 10);
    }
    #[test]
    fn keyboard_triggers_toggle_shift_and_symbols_and_start_accepts_text() {
        let (mut app, ctx) = app();
        app.edit(1);
        app.keyboard.as_mut().unwrap().selected = 10; // q
        app.action(Action::Subtitles, &ctx); // R2 / RT
        assert!(app.keyboard.as_ref().unwrap().upper);
        app.action(Action::Confirm, &ctx);
        assert_eq!(app.keyboard.as_ref().unwrap().text, "Q");
        app.action(Action::Subtitles, &ctx);
        assert!(!app.keyboard.as_ref().unwrap().upper);
        app.action(Action::Audio, &ctx); // L2 / LT
        assert!(app.keyboard.as_ref().unwrap().symbols);
        app.action(Action::Confirm, &ctx);
        assert_eq!(app.keyboard.as_ref().unwrap().text, "Q!");
        app.action(Action::Delete, &ctx); // X stays Delete
        assert_eq!(app.keyboard.as_ref().unwrap().text, "Q");
        app.action(Action::Search, &ctx); // Y inserts a space, not a page switch
        assert_eq!(app.keyboard.as_ref().unwrap().text, "Q ");
        assert!(app.keyboard.as_ref().unwrap().symbols);
        app.action(Action::Audio, &ctx);
        assert!(!app.keyboard.as_ref().unwrap().symbols);
        app.action(Action::Menu, &ctx); // Start / Menu accepts, does not delete
        assert!(app.keyboard.is_none());
        assert_eq!(app.username, "Q ");
    }
    #[test]
    fn bigscreen_confirm_types_once_without_closing_or_reopening_keyboard() {
        let (mut app, ctx) = app();
        app.edit(1);
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        crate::input::filter_mirrored_navigation(&mut raw.events);
        let _ = ctx.run(raw, |ctx| {
            app.action(Action::Confirm, ctx);
            app.draw_keyboard(ctx);
        });
        assert_eq!(app.keyboard.as_ref().unwrap().text, "1");
        app.keyboard.as_mut().unwrap().selected = app.keyboard.as_ref().unwrap().done_index();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.action(Action::Confirm, ctx);
            egui::CentralPanel::default().show(ctx, |ui| app.draw_setup(ui));
        });
        assert!(app.keyboard.is_none());
        assert_eq!(app.username, "1");
    }
    #[test]
    fn inspection_actions_cannot_change_server_flags_or_start_playback() {
        let (mut app, _) = app();
        let mut api = Api::new("http://127.0.0.1:9/", "fixture").unwrap();
        api.session = Some(Session {
            access_token: "fixture".into(),
            user: crate::jellyfin::User {
                id: "fixture".into(),
                name: "Fixture".into(),
            },
        });
        app.api = Some(api);
        app.detail = Some(Item::default());
        app.dev_action = true;
        for index in [1, 2] {
            app.detail_focus = index;
            app.detail_activate();
            assert!(!app.busy);
        }
        app.play();
        assert!(app.player.is_none());
    }
    #[test]
    fn setup_navigation_and_keyboard_can_be_used_without_a_mouse() {
        let (mut app, ctx) = app();
        app.action(Action::Confirm, &ctx);
        assert!(app.keyboard.is_some());
        app.keyboard.as_mut().unwrap().text = "https://example.org/".into();
        app.keyboard.as_mut().unwrap().selected = app.keyboard.as_ref().unwrap().done_index();
        app.action(Action::Confirm, &ctx);
        assert!(app.keyboard.is_none());
        assert_eq!(app.settings.server, "https://example.org/");
        for _ in 0..10 {
            app.action(Action::Down, &ctx);
        }
        assert_eq!(app.focus, 4);
        app.action(Action::Up, &ctx);
        assert_eq!(app.focus, 3);
    }
    #[test]
    fn navigation_preserves_focus_at_grid_edges_and_returns_to_sections() {
        let (mut app, ctx) = app();
        connect(&mut app);
        app.items = (0..7)
            .map(|i| Item {
                id: i.to_string(),
                name: format!("Film {i}"),
                ..Item::default()
            })
            .collect();
        app.total = 7;
        app.columns = 5;
        app.focus = 6;
        app.action(Action::Left, &ctx);
        assert_eq!(app.focus, 6);
        app.action(Action::Down, &ctx);
        assert_eq!(app.focus, 11);
        app.action(Action::Down, &ctx);
        assert_eq!(app.focus, 12);
        app.action(Action::Up, &ctx);
        assert_eq!(app.focus, 7);
        app.action(Action::Up, &ctx);
        assert_eq!(app.focus, 0);
        app.focus = 8;
        app.action(Action::Confirm, &ctx);
        assert!(app.detail.is_some());
        app.action(Action::Back, &ctx);
        assert_eq!(app.focus, 8);
        assert!(app.detail.is_none());
    }
    #[test]
    fn cancelled_login_cannot_sign_in_from_a_late_response() {
        let (mut app, ctx) = app();
        app.busy = true;
        app.action(Action::Back, &ctx);
        assert!(!app.busy);
        let mut api = Api::new("https://example.org/", "test").unwrap();
        let session = Session {
            access_token: "old".into(),
            user: crate::jellyfin::User {
                id: "u".into(),
                name: "Old".into(),
            },
        };
        api.session = Some(session.clone());
        app.tx.send(Message::Login(0, Ok((api, session)))).unwrap();
        app.messages();
        assert!(app.api.is_none());
    }
    #[test]
    fn detail_controls_and_settings_have_bounded_focus() {
        let (mut app, ctx) = app();
        connect(&mut app);
        app.detail = Some(Item::default());
        for _ in 0..10 {
            app.action(Action::Down, &ctx);
        }
        assert_eq!(app.detail_focus, 3);
        app.action(Action::Confirm, &ctx);
        assert!(app.detail.is_none());
        app.tab = 5;
        app.focus = 6;
        for _ in 0..20 {
            app.action(Action::Down, &ctx);
        }
        assert_eq!(app.focus, 19);
    }
}

#[cfg(test)]
mod catalog_layout_tests {
    use super::*;
    #[test]
    fn initial_focus_keeps_the_banner_visible_and_scrolling_moves_it_out() {
        for size in [
            Vec2::new(960.0, 640.0),
            Vec2::new(1440.0, 900.0),
            Vec2::new(1920.0, 1080.0),
            Vec2::new(3840.0, 2160.0),
        ] {
            let ctx = egui::Context::default();
            let mut app = Aster::new(&ctx, Settings::default());
            app.items = (0..200)
                .map(|i| Item {
                    id: format!("fixture-{i}"),
                    name: if i == 0 {
                        "A long title that should never overlap a different card or the hero".into()
                    } else {
                        format!("Title {i}")
                    },
                    ..Item::default()
                })
                .collect();
            app.total = 200;
            app.focus = 6;
            for frame in 0..4 {
                let _ = ctx.run(
                    egui::RawInput {
                        time: Some(frame as f64 * 0.1),
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.draw_library(ui));
                    },
                );
            }
            let hero = app.hero_bounds.unwrap();
            let catalog = app.catalog_bounds.unwrap();
            let focus = app.focus_bounds.unwrap();
            assert!(hero.top() >= 0.0);
            assert!(
                hero.bottom() < focus.top(),
                "hero overlaps the cards at {size:?}"
            );
            assert!(
                focus.top() >= catalog.top() - 1.0,
                "initial focus scrolled under the header at {size:?}"
            );
            assert!(
                focus.bottom() <= catalog.bottom() + 1.0,
                "initial row exceeds viewport at {size:?}"
            );
            app.focus = app.items.len() + 5;
            app.focus_changed = true;
            for frame in 4..12 {
                let _ = ctx.run(
                    egui::RawInput {
                        time: Some(frame as f64 * 0.1),
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.draw_library(ui));
                    },
                );
            }
            assert!(
                app.hero_bounds.unwrap().bottom() < app.catalog_bounds.unwrap().top(),
                "banner did not scroll out at {size:?}"
            );
        }
    }
}

#[cfg(test)]
mod login_layout_tests {
    use super::*;
    #[test]
    fn sign_in_is_centered_and_fields_stack_inside_the_card() {
        for size in [
            Vec2::new(960.0, 640.0),
            Vec2::new(1440.0, 900.0),
            Vec2::new(1920.0, 1080.0),
        ] {
            let ctx = egui::Context::default();
            let mut app = Aster::new(&ctx, Settings::default());
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.draw_setup(ui));
                    },
                );
            }
            let card = app.login_bounds.unwrap();
            let fields = app.login_fields.map(Option::unwrap);
            let primary = app.login_submit_bounds.unwrap();
            assert!(
                (card.center().x - size.x / 2.0).abs() < 6.0,
                "form is not centered at {size:?}: {card:?}, fields={fields:?}, primary={primary:?}"
            );
            assert!(
                card.width() < 510.0 && card.height() < 600.0,
                "form expanded unexpectedly: {card:?}"
            );
            for f in fields {
                assert!(card.contains_rect(f), "field escaped its card");
                assert!(f.width() > 300.0, "field shrank below a usable width");
            }
            assert!(
                fields[0].bottom() < fields[1].top() && fields[1].bottom() < fields[2].top(),
                "fields do not stack vertically"
            );
            assert!(primary.top() > fields[2].bottom());
            assert!(card.contains_rect(primary));
        }
    }
}

#[cfg(test)]
mod season_navigation_tests {
    use super::*;
    fn page(app: &mut Aster, items: Vec<Item>) {
        let total = items.len();
        app.tx
            .send(Message::Items(
                app.generation,
                false,
                Ok(LibraryPage {
                    items,
                    total_record_count: total,
                }),
            ))
            .unwrap();
        app.messages();
    }
    #[test]
    fn season_back_navigation_restores_the_exact_series_and_season_focus() {
        let ctx = egui::Context::default();
        let mut app = Aster::new(&ctx, Settings::default());
        app.tab = 2;
        app.items = (0..20)
            .map(|n| Item {
                id: format!("show-{n}"),
                name: format!("Show {n}"),
                kind: "Series".into(),
                ..Item::default()
            })
            .collect();
        app.total = 20;
        app.focus = 16;
        app.detail = Some(app.items[10].clone());
        app.list_view = true;
        app.push_route(Route::Seasons {
            series: Box::new(app.items[10].clone()),
        });
        assert!(matches!(app.route, Route::Seasons { .. }));
        let seasons = vec![
            Item {
                id: "specials".into(),
                kind: "Season".into(),
                index_number: Some(0),
                ..Item::default()
            },
            Item {
                id: "season-1".into(),
                kind: "Season".into(),
                index_number: Some(1),
                ..Item::default()
            },
            Item {
                id: "season-2".into(),
                kind: "Season".into(),
                index_number: Some(2),
                ..Item::default()
            },
        ];
        page(&mut app, seasons);
        app.activate_item(1);
        assert!(app.route.is_episodes());
        assert!(!app.list_view);
        page(
            &mut app,
            vec![Item {
                id: "episode".into(),
                kind: "Episode".into(),
                ..Item::default()
            }],
        );
        let mut api = Api::new("https://example.org/", "fixture").unwrap();
        api.session = Some(Session {
            access_token: "fixture".into(),
            user: crate::jellyfin::User {
                id: "u".into(),
                name: "Fixture".into(),
            },
        });
        app.api = Some(api);
        app.action(Action::Back, &ctx);
        assert!(matches!(app.route, Route::Seasons { .. }));
        assert_eq!(app.focus, 7);
        assert_eq!(app.items.len(), 3);
        app.action(Action::Back, &ctx);
        assert!(app.route.is_library());
        assert_eq!(app.focus, 16);
        assert_eq!(app.detail.as_ref().unwrap().id, "show-10");
        assert!(app.list_view);
        app.action(Action::Back, &ctx);
        assert!(app.detail.is_none());
        assert_eq!(app.focus, 16);
    }
}
