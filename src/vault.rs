use crate::jellyfin::Session;
use std::{
    io::Write,
    process::{Command, Stdio},
};
pub fn store(server: &str, session: &Session, private: bool) -> Result<(), String> {
    if private {
        return file_store(server, session);
    }
    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label=Aster Jellyfin connection",
            "application",
            "aster",
            "server",
            server,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Install libsecret's secret-tool to remember sign-in securely.".to_string())?;
    let bytes = serde_json::to_vec(session).map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("Could not open credential storage.")?
        .write_all(&bytes)
        .map_err(|e| e.to_string())?;
    if child.wait().map_err(|e| e.to_string())?.success() {
        Ok(())
    } else {
        Err(
            "The system keyring is unavailable or locked. Sign-in remains active for this session."
                .into(),
        )
    }
}
pub fn load(server: &str, private: bool) -> Result<Session, String> {
    if private {
        let saved: Saved = serde_json::from_slice(
            &std::fs::read(existing_file_path()).map_err(|_| "No private-file sign-in found.")?,
        )
        .map_err(|_| "Could not read the saved sign-in.")?;
        return if saved.server == server {
            Ok(saved.session)
        } else {
            Err("Saved sign-in belongs to another server.".into())
        };
    }
    let mut output = Command::new("secret-tool")
        .args(["lookup", "application", "aster", "server", server])
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "Secure credential storage is unavailable.".to_string())?;
    if !output.status.success() {
        output = Command::new("secret-tool")
            .args(["lookup", "application", "hearth", "server", server])
            .stderr(Stdio::null())
            .output()
            .map_err(|_| "Secure credential storage is unavailable.".to_string())?;
    }
    if !output.status.success() {
        return Err("No saved sign-in found. Connect to Jellyfin below.".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "Saved sign-in could not be read. Please sign in again.".into())
}
pub fn clear(server: &str, private: bool) -> Result<(), String> {
    if private {
        let legacy = file_path()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("hearth/session.json");
        if legacy.exists() {
            std::fs::remove_file(legacy)
                .map_err(|_| "Could not remove the legacy private sign-in file.")?;
        }
        return match std::fs::remove_file(file_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Could not remove the private sign-in file.".into()),
        };
    }
    let legacy_status = Command::new("secret-tool")
        .args(["clear", "application", "hearth", "server", server])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let status = Command::new("secret-tool")
        .args(["clear", "application", "aster", "server", server])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| {
            "Secure storage is unavailable; saved credentials could not be removed.".to_string()
        })?;
    if status.success() || legacy_status.is_ok_and(|s| s.success()) {
        Ok(())
    } else {
        Err("Could not remove saved credentials from the system keyring.".into())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    server: String,
    session: Session,
}
fn file_path() -> std::path::PathBuf {
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/share")
        });
    root.join("aster/session.json")
}
fn existing_file_path() -> std::path::PathBuf {
    let path = file_path();
    if path.exists() {
        path
    } else {
        path.parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("hearth/session.json")
    }
}
fn file_store(server: &str, session: &Session) -> Result<(), String> {
    file_store_at(file_path(), server, session)
}
fn file_store_at(path: std::path::PathBuf, server: &str, session: &Session) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|_| "Could not create private credential storage.")?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Could not protect the credential directory.")?;
    let temp = dir.join(format!(".session-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let data = Saved {
            server: server.to_string(),
            session: session.clone(),
        };
        file.write_all(&serde_json::to_vec(&data).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn private_credentials_are_atomic_and_user_only() {
        let dir = std::env::temp_dir().join(format!("aster-vault-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("session.json");
        let session = Session {
            access_token: "fixture-secret".into(),
            user: crate::jellyfin::User {
                id: "u".into(),
                name: "Viewer".into(),
            },
        };
        file_store_at(path.clone(), "https://example.org/", &session).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        file_store_at(path.clone(), "https://example.org/", &session).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let saved: Saved = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.session.access_token, "fixture-secret");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
