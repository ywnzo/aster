use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub server: String,
    pub device_id: String,
    pub fullscreen: bool,
    pub ui_scale: f32,
    pub volume: u8,
    pub hardware_decode: bool,
    pub resume: bool,
    pub autoplay: bool,
    pub autoplay_delay_seconds: u8,
    pub stick_deadzone: f32,
    pub swap_confirm: bool,
    pub remember_session: bool,
    pub private_token_file: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            server: String::new(),
            device_id: uuid::Uuid::new_v4().to_string(),
            fullscreen: false,
            ui_scale: 1.0,
            volume: 80,
            hardware_decode: true,
            resume: true,
            autoplay: true,
            autoplay_delay_seconds: 7,
            stick_deadzone: 0.55,
            swap_confirm: false,
            remember_session: true,
            private_token_file: false,
        }
    }
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            })
            .join("aster/settings.json")
    }
    pub fn load() -> Result<Self, String> {
        let canonical = Self::path();
        let legacy = canonical
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("hearth/settings.json");
        let path = if canonical.exists() {
            canonical
        } else {
            legacy
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        let settings: Self = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            ui_scale: settings.ui_scale.clamp(0.8, 1.6),
            volume: settings.volume.min(100),
            autoplay_delay_seconds: settings.autoplay_delay_seconds.min(15),
            stick_deadzone: settings.stick_deadzone.clamp(0.2, 0.9),
            ..settings
        })
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temp = path.with_extension("tmp");
        fs::write(
            &temp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(temp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_contains_preferences_but_no_credentials() {
        let s = Settings {
            server: "https://example.org/jellyfin/".into(),
            volume: 45,
            swap_confirm: true,
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("access_token"));
        let copy: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(copy.server, s.server);
        assert_eq!(copy.volume, 45);
        assert!(copy.swap_confirm);
    }
}

#[cfg(test)]
mod default_tests {
    #[test]
    fn defaults_remember_signin_and_start_windowed() {
        let settings = super::Settings::default();
        assert!(!settings.fullscreen);
        assert!(settings.remember_session);
        assert!(settings.autoplay);
        assert_eq!(settings.autoplay_delay_seconds, 7);
    }
}
