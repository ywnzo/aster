//! Opt-in development control for inspecting this app's real rendered screens.
//! In-memory sign-in handoff is confined to a private Unix socket; no token is
//! written to a file or sent to a network service. Disabled in ordinary launches.
use crate::jellyfin::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::Duration,
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    pub server: String,
    pub session: Session,
}
pub struct Control {
    pub commands: Receiver<Value>,
    connection: Arc<Mutex<Option<Connection>>>,
    state: Arc<Mutex<Value>>,
    stop: Arc<AtomicBool>,
    path: PathBuf,
    context: Arc<Mutex<Option<eframe::egui::Context>>>,
}
impl Control {
    pub fn start(path: PathBuf) -> Result<Self, String> {
        let dir = path.parent().ok_or("Missing control directory")?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| e.to_string())?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;

        let (send, commands) = mpsc::channel();
        let connection = Arc::new(Mutex::new(None::<Connection>));
        let shared = connection.clone();
        let state = Arc::new(Mutex::new(json!({"ready":false})));
        let worker_state = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let context = Arc::new(Mutex::new(None::<eframe::egui::Context>));
        let worker_context = context.clone();
        thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new(&stream)
                    .take(65536)
                    .read_line(&mut line)
                    .is_err()
                {
                    continue;
                }
                let Ok(request) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let response = if request["command"] == "session" {
                    json!({"connection":*shared.lock().unwrap()})
                } else if request["command"] == "status" {
                    worker_state.lock().unwrap().clone()
                } else if send.send(request).is_ok() {
                    if let Some(ctx) = worker_context.lock().unwrap().as_ref() {
                        ctx.request_repaint();
                    }
                    json!({"queued":true})
                } else {
                    break;
                };
                let _ = writeln!(stream, "{response}");
            }
        });
        Ok(Self {
            commands,
            connection,
            state,
            stop,
            path,
            context,
        })
    }
    pub fn set_context(&self, ctx: eframe::egui::Context) {
        *self.context.lock().unwrap() = Some(ctx);
    }
    pub fn report(&self, state: Value) {
        *self.state.lock().unwrap() = state;
    }
    pub fn publish(&self, connection: Option<Connection>) {
        *self.connection.lock().unwrap() = connection;
    }
    pub fn restore(path: &Path) -> Result<Option<Connection>, String> {
        let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        writeln!(stream, "{}", json!({"command":"session"})).map_err(|e| e.to_string())?;
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&response).map_err(|e| e.to_string())?;
        if value["connection"].is_null() {
            Ok(None)
        } else {
            serde_json::from_value(value["connection"].clone())
                .map(Some)
                .map_err(|e| e.to_string())
        }
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the blocking accept so an idle development bridge exits cleanly.
        let _ = UnixStream::connect(&self.path);
        let _ = std::fs::remove_file(&self.path);
    }
}
use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opt_in_bridge_is_user_only_and_keeps_sessions_in_memory() {
        let dir = std::env::temp_dir().join(format!("aster-control-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("control.sock");
        let control = Control::start(path.clone()).unwrap();
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(Control::restore(&path).unwrap().is_none());
        control.publish(Some(Connection {
            server: "https://example.org/".into(),
            session: Session {
                access_token: "fixture-only".into(),
                user: crate::jellyfin::User {
                    id: "u".into(),
                    name: "Viewer".into(),
                },
            },
        }));
        let restored = Control::restore(&path).unwrap().unwrap();
        assert_eq!(restored.session.access_token, "fixture-only");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        drop(control);
        assert!(!path.exists());
        std::fs::remove_dir(dir).unwrap();
    }
}
