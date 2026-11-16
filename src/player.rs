use crate::{
    config::Settings,
    jellyfin::{Api, Item},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Default, Debug, serde::Deserialize)]
#[serde(default)]
pub struct Track {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub codec: Option<String>,
    pub selected: bool,
    pub external: bool,
}
#[derive(Debug)]
pub enum Status {
    Ready,
    Position(f64),
    Duration(f64),
    Paused(bool),
    Tracks(Vec<Track>),
    Finished,
    Fullscreen(bool),
    Overlay(Value),
    Playing(Box<Item>),
    Completed(String),
    UpNext(String, u64),
    Error(String),
    Warning(String),
}
#[derive(Clone)]
struct Events {
    sender: Sender<Status>,
    context: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
}
impl Events {
    fn send(&self, status: Status) -> Result<(), mpsc::SendError<Status>> {
        let wake = !matches!(status, Status::Position(_));
        let result = self.sender.send(status);
        if wake && let Some(ctx) = self.context.lock().unwrap().as_ref() {
            ctx.request_repaint();
        }
        result
    }
}
pub struct Player {
    context: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
    pub preview_api: Api,
    pub preview_socket: std::path::PathBuf,
    pub commands: Sender<Value>,
    pub events: Receiver<Status>,
    finished: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Player {
    #[cfg(test)]
    pub fn start(api: Api, item: Item, settings: Settings) -> Self {
        let core = crate::embedded::Core::headless(&settings).expect("libmpv test core");
        Self::start_embedded(api, item, settings, core)
    }
    pub fn start_embedded(
        api: Api,
        item: Item,
        settings: Settings,
        core: std::sync::Arc<crate::embedded::Core>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let (sender, receive) = mpsc::channel();
        let context = std::sync::Arc::new(std::sync::Mutex::new(None));
        let events = Events {
            sender,
            context: context.clone(),
        };
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_finished = finished.clone();
        let preview_api = api.clone();
        let preview_socket = core.socket.clone();
        thread::spawn(move || {
            if let Err(e) = run(api, item, settings, rx, &events, core) {
                let _ = events.send(Status::Error(e));
            }
            worker_finished.store(true, std::sync::atomic::Ordering::Release);
            let _ = events.send(Status::Finished);
        });
        Self {
            context,
            preview_api,
            preview_socket,
            commands: tx,
            events: receive,
            finished,
        }
    }
    pub fn set_context(&self, ctx: eframe::egui::Context) {
        ctx.request_repaint();
        *self.context.lock().unwrap() = Some(ctx);
    }
    pub fn command(&self, command: Value) {
        let _ = self.commands.send(json!({"command":command}));
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.command(json!(["quit"]));
        if self.finished.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(remaining) {
                Ok(Status::Finished) | Err(_) => break,
                _ => {}
            }
        }
    }
}
fn run(
    api: Api,
    item: Item,
    settings: Settings,
    commands: Receiver<Value>,
    events: &Events,
    core: std::sync::Arc<crate::embedded::Core>,
) -> Result<(), String> {
    let plan = api.playback(&item)?;
    let socket = core.socket.clone();
    let result = (|| {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut stream = loop {
            match commands.try_recv() {
                Ok(command) if command["command"][0] == "quit" => {
                    return Err("Playback cancelled.".into());
                }
                Err(mpsc::TryRecvError::Disconnected) => return Err("Playback cancelled.".into()),
                _ => {}
            }
            match UnixStream::connect(&socket) {
                Ok(s) => break s,
                Err(_) => {
                    if Instant::now() > deadline {
                        return Err("mpv did not start its playback connection.".into());
                    }
                    thread::sleep(Duration::from_millis(40));
                }
            }
        };
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_millis(80)))
            .map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let token = &api.session.as_ref().ok_or("Sign in first.")?.access_token;
        send(
            &mut stream,
            &json!({"command":["set_property","http-header-fields",[format!("X-Emby-Token: {token}")]]}),
        )?;
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
        send(
            &mut stream,
            &json!({"command":["set_property","user-data/aster-stop",false]}),
        )?;
        send(
            &mut stream,
            &json!({"command":["set_property","fullscreen",settings.fullscreen]}),
        )?;
        send(
            &mut stream,
            &json!({"command":["set_property","volume",settings.volume]}),
        )?;
        send(
            &mut stream,
            &json!({"command":["set_property","hwdec",if settings.hardware_decode {"auto-safe"}else{"no"}]}),
        )?;
        send(
            &mut stream,
            &json!({"command":["script-message","aster-config",if settings.swap_confirm {"B"}else{"A"},if settings.swap_confirm {"A"}else{"B"}]}),
        )?;
        // Keep the libmpv VO/OSD alive between episodes so the countdown
        // can render after EOF. With vo=libmpv this never creates a window.
        send(
            &mut stream,
            &json!({"command":["set_property","force-window","yes"]}),
        )?;
        for (id, property) in [
            (1, "time-pos"),
            (2, "pause"),
            (3, "duration"),
            (4, "track-list"),
            (5, "user-data/aster-autoplay-action"),
            (6, "user-data/aster-stop"),
            (7, "fullscreen"),
            (8, "user-data/aster"),
            (9, "user-data/aster-episode-action"),
        ] {
            send(
                &mut stream,
                &json!({"command":["observe_property",id,property]}),
            )?;
        }
        let (reports, report_rx) = mpsc::channel::<(Item, String, u64, bool)>();
        let report_api = api.clone();
        let report_events = events.clone();
        thread::spawn(move || {
            while let Ok((item, event, ticks, paused)) = report_rx.recv() {
                if let Err(e) = report_api.report(&event, &item, ticks, paused) {
                    let _ = report_events.send(Status::Warning(e));
                }
            }
        });
        let mut current = item;
        let mut current_plan = plan;
        let mut position = if settings.resume {
            current.user_data.playback_position_ticks as f64 / 10_000_000.0
        } else {
            0.0
        };
        let mut duration = 0.0_f64;
        let mut paused = false;
        let mut loaded = false;
        let mut load_started = Instant::now();
        let mut last_report = Instant::now();
        let mut next_rx: Option<Receiver<Result<Option<PreparedEpisode>, String>>> = None;
        let mut prefetched: Option<Result<Option<PreparedEpisode>, String>> = None;
        let mut pending: Option<PendingNext> = None;
        let mut navigation: Option<Receiver<Result<Option<PreparedEpisode>, String>>> = None;
        let mut navigating_previous = false;
        let mut ignore_replaced_end = false;
        begin_file(&mut stream, &current, &current_plan, position, events)?;
        let mut line = String::new();
        loop {
            for command in commands.try_iter() {
                send(&mut stream, &command)?;
            }
            if let Some(rx) = &navigation {
                match rx.try_recv() {
                    Ok(result) => {
                        navigation = None;
                        match result {
                            Ok(Some(next)) => {
                                if loaded {
                                    let _ = reports.send((
                                        current.clone(),
                                        "/Stopped".into(),
                                        (position * 10_000_000.0) as u64,
                                        paused,
                                    ));
                                }
                                ignore_replaced_end = loaded;
                                current = next.item;
                                current.user_data.playback_position_ticks = 0;
                                current_plan = next.plan;
                                loaded = false;
                                paused = false;
                                position = 0.0;
                                duration = 0.0;
                                pending = None;
                                next_rx = None;
                                prefetched = None;
                                load_started = Instant::now();
                                last_report = Instant::now();
                                begin_file(&mut stream, &current, &current_plan, 0.0, events)?;
                            }
                            Ok(None) => {
                                send(
                                    &mut stream,
                                    &json!({"command":["script-message","aster-episode-unavailable",if navigating_previous {"previous"} else {"next"}]}),
                                )?;
                            }
                            Err(e) => {
                                let _ = events.send(Status::Warning(e));
                                send(
                                    &mut stream,
                                    &json!({"command":["script-message","aster-episode-error"]}),
                                )?;
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        navigation = None;
                        send(
                            &mut stream,
                            &json!({"command":["script-message","aster-episode-error"]}),
                        )?;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if let Some(rx) = &next_rx {
                match rx.try_recv() {
                    Ok(value) => {
                        prefetched = Some(value);
                        next_rx = None;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        prefetched = Some(Err("Could not prepare the next episode.".into()));
                        next_rx = None;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if let Some(waiting) = &mut pending
                && navigation.is_none()
            {
                if waiting.prepared.is_none()
                    && let Some(result) = prefetched.take()
                {
                    match result {
                        Ok(Some(next)) => {
                            waiting.prepared = Some(next);
                            waiting.deadline = Some(
                                Instant::now()
                                    + Duration::from_secs(settings.autoplay_delay_seconds as u64),
                            );
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = events.send(Status::Warning(e));
                            break;
                        }
                    }
                }
                if let Some(next) = &waiting.prepared {
                    let remaining = waiting
                        .deadline
                        .unwrap()
                        .saturating_duration_since(Instant::now())
                        .as_secs_f64()
                        .ceil() as u64;
                    if waiting.shown != Some(remaining) {
                        send(
                            &mut stream,
                            &json!({"command":["script-message","aster-up-next",next.item.playback_title(),remaining.to_string()]}),
                        )?;
                        let _ = events.send(Status::UpNext(next.item.playback_title(), remaining));
                        waiting.shown = Some(remaining);
                    }
                    if remaining == 0 || waiting.play_now {
                        let next = pending.take().unwrap().prepared.unwrap();
                        current = next.item;
                        current.user_data.playback_position_ticks = 0;
                        current_plan = next.plan;
                        loaded = false;
                        paused = false;
                        position = 0.0;
                        duration = 0.0;
                        next_rx = None;
                        prefetched = None;
                        load_started = Instant::now();
                        last_report = Instant::now();
                        begin_file(&mut stream, &current, &current_plan, 0.0, events)?;
                    }
                }
            }
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        if value["event"] == "file-loaded" {
                            loaded = true;
                            for subtitle in &current_plan.subtitles {
                                send(
                                    &mut stream,
                                    &json!({"command":["sub-add",subtitle.url,"auto",subtitle.title,subtitle.language]}),
                                )?;
                            }
                            let _ = events.send(Status::Playing(Box::new(current.clone())));
                            let _ = events.send(Status::Duration(duration));
                            let _ = events.send(Status::Ready);
                            let _ = reports.send((
                                current.clone(),
                                String::new(),
                                (position * 10_000_000.0) as u64,
                                paused,
                            ));
                            if settings.autoplay && current.kind == "Episode" {
                                next_rx = Some(prepare_next(api.clone(), current.clone()));
                            }
                        }
                        if value["event"] == "property-change" {
                            match value["name"].as_str().unwrap_or("") {
                                "user-data/aster-stop" if value["data"] == true => break,
                                "user-data/aster" => {
                                    let _ = events.send(Status::Overlay(value["data"].clone()));
                                }
                                "fullscreen" => {
                                    let _ = events.send(Status::Fullscreen(
                                        value["data"].as_bool().unwrap_or(false),
                                    ));
                                }
                                "time-pos" if loaded => {
                                    if let Some(p) = value["data"].as_f64() {
                                        position = p;
                                        let _ = events.send(Status::Position(p));
                                    }
                                }
                                "pause" => {
                                    paused = value["data"].as_bool().unwrap_or(false);
                                    let _ = events.send(Status::Paused(paused));
                                }
                                "duration" => {
                                    if let Some(d) = value["data"].as_f64() {
                                        duration = d;
                                        let _ = events.send(Status::Duration(d));
                                    }
                                }
                                "track-list" => {
                                    if let Ok(tracks) =
                                        serde_json::from_value::<Vec<Track>>(value["data"].clone())
                                    {
                                        let _ = events.send(Status::Tracks(tracks));
                                    }
                                }
                                "user-data/aster-episode-action" => {
                                    if navigation.is_none()
                                        && current.kind == "Episode"
                                        && (loaded || pending.is_some())
                                        && let Some(direction @ ("previous" | "next")) =
                                            value["data"].as_str()
                                    {
                                        navigating_previous = direction == "previous";
                                        navigation = Some(prepare_adjacent(
                                            api.clone(),
                                            current.clone(),
                                            navigating_previous,
                                        ));
                                        send(
                                            &mut stream,
                                            &json!({"command":["script-message","aster-episode-loading"]}),
                                        )?;
                                    }
                                    if value["data"].as_str().is_some_and(|v| v != "none") {
                                        send(
                                            &mut stream,
                                            &json!({"command":["set_property","user-data/aster-episode-action","none"]}),
                                        )?;
                                    }
                                }
                                "user-data/aster-autoplay-action" => {
                                    if let Some(waiting) = &mut pending {
                                        match value["data"].as_str() {
                                            Some("play") => waiting.play_now = true,
                                            Some("cancel") => break,
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        if value["event"] == "end-file" {
                            if ignore_replaced_end && value["reason"] == "stop" {
                                ignore_replaced_end = false;
                                line.clear();
                                continue;
                            }
                            let natural = value["reason"] == "eof";
                            if value["reason"] == "redirect" {
                                line.clear();
                                continue;
                            }
                            if natural {
                                position = duration.max(position);
                                let _ = events.send(Status::Completed(current.id.clone()));
                            }
                            if loaded {
                                let _ = reports.send((
                                    current.clone(),
                                    "/Stopped".into(),
                                    (position * 10_000_000.0) as u64,
                                    paused,
                                ));
                                loaded = false;
                            }
                            if value["reason"] == "error" {
                                let _ = events.send(Status::Error(
                                    "The video could not be played. Check the source and network."
                                        .into(),
                                ));
                            }
                            if natural && settings.autoplay && current.kind == "Episode" {
                                pending = Some(PendingNext {
                                    prepared: None,
                                    deadline: None,
                                    shown: None,
                                    play_now: false,
                                });
                                send(
                                    &mut stream,
                                    &json!({"command":["script-message","aster-up-next","Finding the next episode…","-1"]}),
                                )?;
                            } else {
                                break;
                            }
                        }
                    }
                    line.clear();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
            if loaded && last_report.elapsed() > Duration::from_secs(10) {
                let _ = reports.send((
                    current.clone(),
                    "/Progress".into(),
                    (position * 10_000_000.0) as u64,
                    paused,
                ));
                last_report = Instant::now();
            }
            if !loaded && pending.is_none() && load_started.elapsed() > Duration::from_secs(45) {
                return Err("Playback timed out. Check the source and network.".into());
            }
        }
        if loaded {
            let _ = reports.send((
                current,
                "/Stopped".into(),
                (position * 10_000_000.0) as u64,
                paused,
            ));
        }
        drop(reports);
        Ok(())
    })();
    if let Ok(mut stream) = UnixStream::connect(&socket) {
        let _ = send(&mut stream, &json!({"command":["stop"]}));
        let _ = send(
            &mut stream,
            &json!({"command":["set_property","force-window","no"]}),
        );
    }
    result
}
struct PreparedEpisode {
    item: Item,
    plan: crate::jellyfin::PlaybackPlan,
}
struct PendingNext {
    prepared: Option<PreparedEpisode>,
    deadline: Option<Instant>,
    shown: Option<u64>,
    play_now: bool,
}
fn prepare_next(api: Api, current: Item) -> Receiver<Result<Option<PreparedEpisode>, String>> {
    prepare_adjacent(api, current, false)
}
fn prepare_adjacent(
    api: Api,
    current: Item,
    previous: bool,
) -> Receiver<Result<Option<PreparedEpisode>, String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| {
            let adjacent = if previous {
                api.previous_episode(&current)?
            } else {
                api.next_episode(&current)?
            };
            let Some(item) = adjacent else {
                return Ok(None);
            };
            let plan = api.playback(&item)?;
            Ok(Some(PreparedEpisode { item, plan }))
        })();
        let _ = tx.send(result);
    });
    rx
}
fn begin_file(
    stream: &mut UnixStream,
    item: &Item,
    plan: &crate::jellyfin::PlaybackPlan,
    start: f64,
    events: &Events,
) -> Result<(), String> {
    send(
        stream,
        &json!({"command":["script-message","aster-clear-next"]}),
    )?;
    send(
        stream,
        &json!({"command":["script-message","aster-episode",if item.kind == "Episode" {"yes"} else {"no"}]}),
    )?;
    send(
        stream,
        &json!({"command":["set_property","user-data/aster-episode-action","none"]}),
    )?;
    send(
        stream,
        &json!({"command":["set_property","user-data/aster-autoplay-action","none"]}),
    )?;
    send(
        stream,
        &json!({"command":["set_property","start",format!("{start:.6}")]}),
    )?;
    send(stream, &json!({"command":["set_property","pause",false]}))?;
    send(
        stream,
        &json!({"command":["set_property","force-media-title",item.playback_title()]}),
    )?;
    let _ = events.send(Status::Position(start));
    let _ = events.send(Status::Duration(0.0));
    let _ = events.send(Status::Tracks(Vec::new()));
    send(stream, &json!({"command":["loadfile",plan.url]}))
}
fn send(stream: &mut UnixStream, value: &Value) -> Result<(), String> {
    if value["command"][0] == "quit" {
        writeln!(
            stream,
            "{}",
            json!({"command":["set_property","user-data/aster-stop",true]})
        )
        .map_err(|_| "Lost connection to the player.".to_string())?;
        return writeln!(stream, "{}", json!({"command":["stop"]}))
            .map_err(|_| "Lost connection to the player.".to_string());
    }
    writeln!(stream, "{value}").map_err(|_| "Lost connection to the player.".into())
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use std::{
        io::Read,
        net::TcpListener,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };
    #[test]
    fn real_mpv_authentication_controls_and_watch_progress() {
        let video = include_bytes!("../tests/fixtures/playback.mp4");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let reports = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
        let worker_reports = reports.clone();
        let server = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line.to_lowercase());
                }
                let length = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let path = first.split_whitespace().nth(1).unwrap_or("");
                let authorized = headers.contains("x-emby-token: fixture-token");
                let mut bytes: Vec<u8> = Vec::new();
                let mut extra = String::new();
                let status = if !authorized {
                    "401 Unauthorized"
                } else if path.starts_with("/Items/") {
                    bytes =
                        br#"{"MediaSources":[{"Id":"source","MediaStreams":[{"Type":"Subtitle","IsExternal":true,"Index":1,"Codec":"subrip","Language":"eng","DisplayTitle":"External English"}]}],"PlaySessionId":"fixture-session"}"#
                            .to_vec();
                    "200 OK"
                } else if path.contains("/Subtitles/") {
                    bytes = include_bytes!("../tests/fixtures/external.srt").to_vec();
                    "200 OK"
                } else if path.starts_with("/Videos/") {
                    let offset = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("range: bytes="))
                        .and_then(|r| r.split('-').next())
                        .and_then(|n| n.parse::<usize>().ok());
                    if let Some(offset) = offset.filter(|i| *i < video.len()) {
                        bytes = video[offset..].to_vec();
                        extra = format!(
                            "Content-Range: bytes {}-{}/{}\r\n",
                            offset,
                            video.len() - 1,
                            video.len()
                        );
                        "206 Partial Content"
                    } else {
                        bytes = video.to_vec();
                        "200 OK"
                    }
                } else if path.starts_with("/Sessions/") {
                    worker_reports
                        .lock()
                        .unwrap()
                        .push((path.to_string(), serde_json::from_slice(&body).unwrap()));
                    "204 No Content"
                } else {
                    "404 Not Found"
                };
                let content_type = if path.contains("/Subtitles/") {
                    "application/x-subrip"
                } else if path.starts_with("/Videos/") {
                    "video/mp4"
                } else {
                    "application/json"
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{extra}Connection: close\r\n\r\n",
                    bytes.len()
                );
                let _ = stream.write_all(header.as_bytes());
                if !first.starts_with("HEAD") {
                    let _ = stream.write_all(&bytes);
                }
            }
        });
        let mut api = Api::new(&base, "native-test").unwrap();
        api.session = Some(crate::jellyfin::Session {
            access_token: "fixture-token".into(),
            user: crate::jellyfin::User {
                id: "fixture-user".into(),
                name: "Fixture".into(),
            },
        });
        let item = Item {
            id: "fixture".into(),
            name: "Aster playback test".into(),
            kind: "Movie".into(),
            ..Item::default()
        };
        let player = Player::start(
            api,
            item,
            Settings {
                volume: 0,
                ..Settings::default()
            },
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut ready = false;
        let mut advanced = false;
        let mut finished = false;
        let mut error = None;
        let mut external_loaded = false;
        while Instant::now() < deadline {
            match player.events.recv_timeout(Duration::from_millis(200)) {
                Ok(Status::Ready) => {
                    ready = true;
                    player.command(json!(["cycle", "pause"]));
                    thread::sleep(Duration::from_millis(120));
                    player.command(json!(["cycle", "pause"]));
                    player.command(json!(["seek", 1, "absolute"]));
                }
                Ok(Status::Position(p)) => advanced |= p > 0.5,
                Ok(Status::Tracks(tracks)) => {
                    external_loaded |= tracks.iter().any(|t| t.kind == "sub" && t.external)
                }
                Ok(Status::Error(e)) => error = Some(e),
                Ok(Status::Finished) => {
                    finished = true;
                    break;
                }
                _ => {}
            }
        }
        drop(player);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline
            && !reports
                .lock()
                .unwrap()
                .iter()
                .any(|(p, _)| p.ends_with("/Stopped"))
        {
            thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        assert!(error.is_none(), "{error:?}");
        assert!(
            ready && advanced && finished,
            "Playback did not complete: ready={ready}, advanced={advanced}, finished={finished}"
        );
        assert!(
            external_loaded,
            "Authenticated external subtitles were not loaded."
        );
        let reports = reports.lock().unwrap();
        assert!(reports.iter().any(|(p, _)| p == "/Sessions/Playing"));
        assert!(
            reports
                .iter()
                .any(|(p, v)| p.ends_with("/Stopped")
                    && v["PositionTicks"].as_u64().unwrap_or(0) > 0)
        );
    }
}
