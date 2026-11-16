use gilrs::{Axis, Button, EventType, Gilrs};
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Left,
    Right,
    LeftRepeat,
    RightRepeat,
    ReleaseHorizontal,
    Up,
    Down,
    Confirm,
    Back,
    Menu,
    Delete,
    Search,
    Prev,
    Next,
    Audio,
    Subtitles,
}
struct RawInput {
    gilrs: Option<Gilrs>,
    held: Option<Action>,
    next: Instant,
    pub controller: String,
    pub error: Option<String>,
}
impl RawInput {
    pub fn new() -> Self {
        let result = Gilrs::new();
        let error = result
            .as_ref()
            .err()
            .map(|e| format!("Controller input unavailable: {e}"));
        Self {
            gilrs: result.ok(),
            held: None,
            next: Instant::now(),
            controller: String::new(),
            error,
        }
    }
    pub fn poll(&mut self, deadzone: f32, swap: bool) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(g) = &mut self.gilrs else {
            return actions;
        };
        while let Some(event) = g.next_event() {
            if !is_gamepad(&g.gamepad(event.id)) {
                continue;
            }
            if let EventType::ButtonPressed(button, _) = event.event {
                let action = match button {
                    Button::South => Some(if swap { Action::Back } else { Action::Confirm }),
                    Button::East => Some(if swap { Action::Confirm } else { Action::Back }),
                    Button::North => Some(Action::Search),
                    Button::West => Some(Action::Delete),
                    Button::Start => Some(Action::Menu),
                    Button::LeftTrigger => Some(Action::Prev),
                    Button::RightTrigger => Some(Action::Next),
                    Button::LeftTrigger2 => Some(Action::Audio),
                    Button::RightTrigger2 => Some(Action::Subtitles),
                    _ => None,
                };
                if let Some(a) = action {
                    actions.push(a);
                }
            }
        }
        self.controller = g
            .gamepads()
            .filter(|(_, p)| is_gamepad(p))
            .map(|(_, p)| p.name().to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        let direction = g
            .gamepads()
            .filter(|(_, p)| is_gamepad(p))
            .find_map(|(_, p)| {
                let x = p.value(Axis::LeftStickX);
                let y = p.value(Axis::LeftStickY);
                if p.is_pressed(Button::DPadLeft) {
                    Some(Action::Left)
                } else if p.is_pressed(Button::DPadRight) {
                    Some(Action::Right)
                } else if p.is_pressed(Button::DPadUp) {
                    Some(Action::Up)
                } else if p.is_pressed(Button::DPadDown) {
                    Some(Action::Down)
                } else {
                    stick_direction(x, y, deadzone)
                }
            });
        let now = Instant::now();
        if direction != self.held {
            if matches!(self.held, Some(Action::Left | Action::Right)) {
                actions.push(Action::ReleaseHorizontal);
            }
            self.held = direction;
            self.next = now + Duration::from_millis(340);
            if let Some(d) = direction {
                actions.push(d);
            }
        } else if now >= self.next
            && let Some(d) = direction
        {
            actions.push(match d {
                Action::Left => Action::LeftRepeat,
                Action::Right => Action::RightRepeat,
                _ => d,
            });
            self.next = now + Duration::from_millis(130);
        }
        actions
    }
}

#[derive(Clone)]
struct Routing {
    deadzone: f32,
    swap: bool,
    focused: bool,
    player: Option<std::sync::mpsc::Sender<serde_json::Value>>,
}
pub struct Input {
    actions: std::sync::mpsc::Receiver<Action>,
    status: std::sync::mpsc::Receiver<(String, Option<String>, bool)>,
    routing: std::sync::Arc<std::sync::Mutex<Routing>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub controller: String,
    pub error: Option<String>,
    pub mirrored_navigation: bool,
}
impl Input {
    pub fn new(ctx: eframe::egui::Context) -> Self {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let (send, actions) = mpsc::sync_channel(64);
        let (status_send, status) = mpsc::channel();
        let routing = Arc::new(Mutex::new(Routing {
            deadzone: 0.55,
            swap: false,
            focused: false,
            player: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_routing = routing.clone();
        std::thread::spawn(move || {
            // Bigscreen synthesizes arrows/Return/Escape for the same gamepad
            // that gilrs reads directly. Detect its service without changing it.
            let bigscreen = bigscreen_input_available();
            let mut raw = RawInput::new();
            let mut previous = None;
            let mut was_focused = false;
            while !thread_stop.load(Ordering::Relaxed) {
                let route = thread_routing.lock().unwrap().clone();
                if was_focused
                    && !route.focused
                    && let Some(player) = &route.player
                {
                    let _ = player.send(
                        serde_json::json!({"command":playback_command(Action::ReleaseHorizontal)}),
                    );
                }
                was_focused = route.focused;
                for action in raw.poll(route.deadzone, route.swap) {
                    if !route.focused {
                        continue;
                    }
                    if let Some(player) = &route.player {
                        let _ =
                            player.send(serde_json::json!({"command":playback_command(action)}));
                    } else if route.focused {
                        let _ = send.try_send(action);
                        ctx.request_repaint();
                    }
                }
                let state = (
                    raw.controller.clone(),
                    raw.error.clone(),
                    bigscreen && !raw.controller.is_empty(),
                );
                if previous.as_ref() != Some(&state) {
                    previous = Some(state.clone());
                    ctx.request_repaint();
                    if status_send.send(state).is_err() {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(if raw.controller.is_empty() {
                    100
                } else {
                    12
                }));
            }
        });
        Self {
            actions,
            status,
            routing,
            stop,
            controller: String::new(),
            error: None,
            mirrored_navigation: false,
        }
    }
    pub fn poll(
        &mut self,
        deadzone: f32,
        swap: bool,
        focused: bool,
        player: Option<std::sync::mpsc::Sender<serde_json::Value>>,
    ) -> Vec<Action> {
        *self.routing.lock().unwrap() = Routing {
            deadzone,
            swap,
            focused,
            player,
        };
        while let Ok((controller, error, mirrored_navigation)) = self.status.try_recv() {
            self.controller = controller;
            self.error = error;
            self.mirrored_navigation = mirrored_navigation;
        }
        self.actions.try_iter().collect()
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
// GLib's client is available in the Flatpak runtime; Qt's qdbus6 is not.
// Ask the bus about the owner without changing the Bigscreen input service.
fn bigscreen_input_available() -> bool {
    let query = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--timeout",
            "2",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            "org.kde.plasma.bigscreen.inputhandler",
        ])
        .stderr(std::process::Stdio::null())
        .output();
    match query {
        Ok(output) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "(true,)"
        }
        Err(_) => std::process::Command::new("qdbus6")
            .args([
                "org.kde.plasma.bigscreen.inputhandler",
                "/InputHandler",
                "org.kde.plasma.bigscreen.inputhandler.inputSuppressed",
            ])
            .stderr(std::process::Stdio::null())
            .output()
            .is_ok_and(|output| output.status.success()),
    }
}
pub fn playback_command(action: Action) -> serde_json::Value {
    use serde_json::json;
    let name = match action {
        Action::Confirm => "confirm",
        Action::Menu | Action::Delete => "menu",
        Action::Back => "back",
        Action::Left => "left",
        Action::Right => "right",
        Action::LeftRepeat => "left-repeat",
        Action::RightRepeat => "right-repeat",
        Action::ReleaseHorizontal => "seek-release",
        Action::Up => "up",
        Action::Down => "down",
        Action::Audio => "audio",
        Action::Subtitles => "subtitles",
        Action::Prev => "previous",
        Action::Next => "next",
        Action::Search => "chapters",
    };
    json!(["script-message", "aster-input", name])
}
fn is_gamepad(p: &gilrs::Gamepad<'_>) -> bool {
    p.button_code(Button::South).is_some()
        && (p.axis_code(Axis::LeftStickX).is_some() || p.button_code(Button::DPadUp).is_some())
}
fn stick_direction(x: f32, y: f32, d: f32) -> Option<Action> {
    if x.abs().max(y.abs()) < d {
        None
    } else if x.abs() > y.abs() {
        Some(if x < 0.0 { Action::Left } else { Action::Right })
    } else {
        Some(if y > 0.0 { Action::Up } else { Action::Down })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadzone_and_dominant_axis() {
        assert_eq!(stick_direction(0.2, 0.3, 0.55), None);
        assert_eq!(stick_direction(-0.9, 0.6, 0.55), Some(Action::Left));
        assert_eq!(stick_direction(0.0, 0.9, 0.55), Some(Action::Up));
    }
}

/// Preserve typed text and pointer events, but prevent Bigscreen's mirrored
/// gamepad keys from reaching either our navigation or egui's focused widgets.
pub fn filter_mirrored_navigation(events: &mut Vec<eframe::egui::Event>) {
    use eframe::egui::{Event, Key};
    events.retain(|event| {
        !matches!(
            event,
            Event::Key {
                key: Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::Enter
                    | Key::Escape
                    | Key::Space
                    | Key::Tab
                    | Key::Backspace
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown
                    | Key::F1
                    | Key::F3,
                ..
            }
        )
    });
}
#[cfg(test)]
mod bigscreen_tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers};
    #[test]
    fn mirrored_keys_cannot_accept_or_reopen_keyboard_but_text_still_types() {
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let mut events = vec![
            key(Key::Enter),
            key(Key::ArrowDown),
            key(Key::Escape),
            Event::Text("a".into()),
            key(Key::A),
        ];
        filter_mirrored_navigation(&mut events);
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Event::Text(s) if s == "a"));
        assert!(matches!(events[1], Event::Key { key: Key::A, .. }));
    }
    #[test]
    fn player_direction_messages_distinguish_presses_repeats_and_releases() {
        for (action, name) in [
            (Action::Left, "left"),
            (Action::Right, "right"),
            (Action::LeftRepeat, "left-repeat"),
            (Action::RightRepeat, "right-repeat"),
            (Action::ReleaseHorizontal, "seek-release"),
        ] {
            assert_eq!(
                playback_command(action),
                serde_json::json!(["script-message", "aster-input", name])
            );
        }
    }
    #[test]
    fn shoulders_request_episode_navigation() {
        assert_eq!(
            playback_command(Action::Prev),
            serde_json::json!(["script-message", "aster-input", "previous"])
        );
        assert_eq!(
            playback_command(Action::Next),
            serde_json::json!(["script-message", "aster-input", "next"])
        );
    }
}
