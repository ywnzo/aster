//! Native fixtures never touch the user's server or real watch history.
use crate::{
    config::Settings,
    input::{Action, playback_command},
    jellyfin::{Api, Item, Session, User},
    player::{Player, Status},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
struct Server {
    base: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    reports: Arc<Mutex<Vec<(String, Value)>>>,
    lookups: Arc<Mutex<usize>>,
}
fn episode(id: &str, season: u32, number: u32) -> Item {
    Item {
        id: id.into(),
        name: format!("Episode {id}"),
        kind: "Episode".into(),
        series_id: Some("show".into()),
        series_name: Some("Fixture series".into()),
        season_id: Some(format!("season-{season}")),
        parent_index_number: Some(season),
        index_number: Some(number),
        run_time_ticks: Some(30_000_000),
        ..Item::default()
    }
}
fn dto(id: &str, season: u32, number: u32) -> Value {
    json!({"Id":id,"Name":format!("Episode {id}"),"Type":"Episode","SeriesId":"show","SeriesName":"Fixture series","SeasonId":format!("season-{season}"),"ParentIndexNumber":season,"IndexNumber":number,"RunTimeTicks":30000000})
}
impl Server {
    fn new(fail: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let reports = Arc::new(Mutex::new(Vec::new()));
        let output = reports.clone();
        let lookups = Arc::new(Mutex::new(0));
        let counts = lookups.clone();
        let worker = thread::spawn(move || {
            let video = include_bytes!("../tests/fixtures/playback.mp4");
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
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut headers = String::new();
                loop {
                    let mut l = String::new();
                    if reader.read_line(&mut l).unwrap_or(0) == 0 || l == "\r\n" {
                        break;
                    }
                    headers.push_str(&l.to_lowercase());
                }
                let length = headers
                    .lines()
                    .find_map(|h| h.strip_prefix("content-length: "))
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let target = first.split_whitespace().nth(1).unwrap_or("/");
                let url = url::Url::parse(&format!("http://localhost{target}")).unwrap();
                let path = url.path();
                let mut extra = String::new();
                let mut bytes = Vec::new();
                let code = if !headers.contains("x-emby-token: fixture-token") {
                    401
                } else if path.starts_with("/Shows/") {
                    *counts.lock().unwrap() += 1;
                    let start = url
                        .query_pairs()
                        .find(|(k, _)| k == "StartItemId")
                        .map(|(_, v)| v.into_owned());
                    let items = match start.as_deref() {
                        Some("one") => {
                            vec![dto("one", 1, 24), dto("special", 0, 3), dto("two", 2, 1)]
                        }
                        Some("two") => vec![dto("two", 2, 1)],
                        _ => vec![dto("special", 0, 3), dto("one", 1, 24), dto("two", 2, 1)],
                    };
                    bytes =
                        serde_json::to_vec(&json!({"TotalRecordCount":items.len(),"Items":items}))
                            .unwrap();
                    200
                } else if path.ends_with("/PlaybackInfo") {
                    bytes = br#"{"MediaSources":[{"Id":"source"}]}"#.to_vec();
                    200
                } else if path.starts_with("/Videos/") {
                    if fail && path.contains("/one/") {
                        404
                    } else {
                        let offset = headers
                            .lines()
                            .find_map(|h| h.strip_prefix("range: bytes="))
                            .and_then(|r| r.split('-').next())
                            .and_then(|n| n.parse::<usize>().ok());
                        if let Some(offset) = offset.filter(|n| *n < video.len()) {
                            bytes = video[offset..].to_vec();
                            extra = format!(
                                "Content-Range: bytes {offset}-{}/{}\r\n",
                                video.len() - 1,
                                video.len()
                            );
                            206
                        } else {
                            bytes = video.to_vec();
                            200
                        }
                    }
                } else if path.starts_with("/Sessions/") {
                    output
                        .lock()
                        .unwrap()
                        .push((path.to_string(), serde_json::from_slice(&body).unwrap()));
                    204
                } else {
                    404
                };
                let kind = if path.starts_with("/Videos/") {
                    "video/mp4"
                } else {
                    "application/json"
                };
                let header = format!(
                    "HTTP/1.1 {code} OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{extra}Connection: close\r\n\r\n",
                    bytes.len()
                );
                let _ = stream.write_all(header.as_bytes());
                if !first.starts_with("HEAD") {
                    let _ = stream.write_all(&bytes);
                }
            }
        });
        Self {
            base,
            stop,
            worker: Some(worker),
            reports,
            lookups,
        }
    }
    fn api(&self) -> Api {
        let mut api = Api::new(&self.base, "autoplay-fixture").unwrap();
        api.session = Some(Session {
            access_token: "fixture-token".into(),
            user: User {
                id: "user".into(),
                name: "Fixture".into(),
            },
        });
        api
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
#[test]
fn native_autoplay_crosses_seasons_but_never_follows_stop_cancel_disable_or_error() {
    for mode in ["auto", "play-now", "stop", "cancel", "disabled", "error"] {
        let server = Server::new(mode == "error");
        let settings = Settings {
            autoplay: mode != "disabled",
            autoplay_delay_seconds: if mode == "cancel" || mode == "play-now" {
                7
            } else {
                0
            },
            volume: 0,
            ..Settings::default()
        };
        let player = Player::start(server.api(), episode("one", 1, 24), settings);
        let mut played = Vec::new();
        let mut up_next = false;
        let mut finished = false;
        let mut errors = Vec::new();
        let mut cancelled = false;
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            match player.events.recv_timeout(Duration::from_millis(150)) {
                Ok(Status::Playing(item)) => {
                    eprintln!("{mode}: playing {}", item.id);
                    played.push(item.id)
                }
                Ok(Status::Ready) => {
                    eprintln!("{mode}: ready");
                    if mode == "stop" {
                        player.command(json!(["quit"]));
                    }
                }
                Ok(Status::UpNext(_, _)) => {
                    eprintln!("{mode}: up next");
                    up_next = true;
                    if mode == "play-now" {
                        player.command(playback_command(Action::Confirm));
                    }
                    if mode == "cancel" && !cancelled {
                        cancelled = true;
                        player.command(playback_command(Action::Back));
                    }
                }
                Ok(Status::Error(error)) => {
                    eprintln!("{mode}: error {error}");
                    errors.push(error)
                }
                Ok(Status::Finished) => {
                    finished = true;
                    break;
                }
                _ => {}
            }
        }
        drop(player);
        assert!(finished, "{mode}: player did not finish");
        if mode == "auto" || mode == "play-now" {
            assert_eq!(
                played,
                ["one", "two"],
                "must cross a season boundary and skip the special"
            );
            assert!(up_next);
        } else if mode == "error" {
            assert!(played.is_empty());
            assert!(!errors.is_empty());
            assert!(!up_next);
        } else {
            assert_eq!(played, ["one"], "{mode}: unexpected autoplay");
            if mode != "cancel" {
                assert!(!up_next);
            }
        }
        if mode == "disabled" || mode == "error" {
            assert_eq!(
                *server.lookups.lock().unwrap(),
                0,
                "{mode}: next episode should not be queried"
            );
        }
        if mode != "error" {
            assert!(errors.is_empty(), "{mode}: {errors:?}");
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if server
                .reports
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, _)| p.ends_with("/Stopped"))
                .count()
                >= played.len()
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let reports = server.reports.lock().unwrap();
        let starts: Vec<_> = reports
            .iter()
            .filter(|(p, _)| p == "/Sessions/Playing")
            .map(|(_, v)| v["ItemId"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            starts, played,
            "{mode}: session reports must match actual episode starts"
        );
    }
}

#[test]
fn native_resume_uses_saved_position_and_respects_the_resume_setting() {
    for (resume, saved, minimum, maximum) in [
        (true, 14_000_000, 1.3, 2.0),
        (false, 14_000_000, 0.0, 0.5),
        (true, 0, 0.0, 0.5),
    ] {
        let server = Server::new(false);
        let mut item = episode("one", 1, 24);
        item.user_data.playback_position_ticks = saved;
        let player = Player::start(
            server.api(),
            item,
            Settings {
                resume,
                autoplay: false,
                volume: 0,
                ..Settings::default()
            },
        );
        let mut ready = false;
        let mut actual = None;
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            match player.events.recv_timeout(Duration::from_millis(100)) {
                Ok(Status::Ready) => ready = true,
                Ok(Status::Position(p)) if ready => {
                    actual = Some(p);
                    player.command(json!(["quit"]));
                    break;
                }
                Ok(Status::Error(e)) => panic!("{e}"),
                _ => {}
            }
        }
        drop(player);
        let actual = actual.expect("no actual playback position observed after file loaded");
        assert!(
            actual >= minimum && actual < maximum,
            "resume={resume}, saved={saved}: actual start {actual}"
        );
    }
}

#[test]
fn manual_episode_switching_works_without_autoplay_and_preserves_stop_reports() {
    let server = Server::new(false);
    let player = Player::start(
        server.api(),
        episode("one", 1, 24),
        Settings {
            autoplay: false,
            volume: 0,
            ..Settings::default()
        },
    );
    let wait_playing = |id: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match player.events.recv_timeout(Duration::from_millis(100)) {
                Ok(Status::Playing(item)) if item.id == id => {
                    player.command(json!(["set_property", "pause", true]));
                    return;
                }
                Ok(Status::Finished) => panic!("player exited before {id}"),
                Ok(Status::Error(e)) => panic!("{e}"),
                _ => {}
            }
        }
        panic!("did not start {id}");
    };
    let wait_boundary = |_direction: &str| {
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match player.events.recv_timeout(Duration::from_millis(50)) {
                Ok(Status::Finished) => panic!("boundary stopped playback"),
                Ok(Status::Playing(_)) => panic!("boundary wrapped to another episode"),
                Ok(Status::Error(e)) => panic!("{e}"),
                _ => {}
            }
        }
    };
    wait_playing("one");
    player.command(playback_command(Action::Prev));
    wait_boundary("previous");
    player.command(playback_command(Action::Next));
    wait_playing("two");
    player.command(playback_command(Action::Next));
    wait_boundary("next");
    player.command(playback_command(Action::Prev));
    wait_playing("one");
    player.command(playback_command(Action::Back));
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if matches!(
            player.events.recv_timeout(Duration::from_millis(100)),
            Ok(Status::Finished)
        ) {
            break;
        }
    }
    drop(player);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let reports = server.reports.lock().unwrap();
        let stops: Vec<_> = reports
            .iter()
            .filter(|(p, _)| p.ends_with("/Stopped"))
            .map(|(_, v)| v["ItemId"].as_str().unwrap().to_string())
            .collect();
        if stops.len() == 3 {
            assert_eq!(stops, ["one", "two", "one"]);
            break;
        }
        drop(reports);
        assert!(
            Instant::now() < deadline,
            "all skipped episodes need a stop report"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let reports = server.reports.lock().unwrap();
    let starts: Vec<_> = reports
        .iter()
        .filter(|(p, _)| p == "/Sessions/Playing")
        .collect();
    assert_eq!(starts.len(), 3);
    assert!(
        starts.iter().all(|(_, v)| v["PositionTicks"] == 0),
        "manual navigation starts from zero"
    );
}
