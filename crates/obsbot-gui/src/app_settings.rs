//! App-wide behaviour that is not part of any effect profile.

use crate::conf::{config_dir, flag, one_line, parse_lines, write_atomically};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Debug)]
pub struct AppSettings {
    /// Profile applied at launch; empty keeps the settings from last time.
    pub startup_profile: String,
    /// Switch the effects camera on at launch.
    pub start_effects: bool,
    /// Closing the window leaves the app running in the tray.
    pub keep_running: bool,
    /// Re-apply saved image settings whenever the camera connects.
    pub restore_camera: bool,
    /// Put the camera to sleep while the screen is locked.
    pub sleep_on_lock: bool,
    /// Put the camera to sleep when neither this window nor another app is watching.
    pub sleep_when_unused: bool,
    /// Show only the controls, without the live preview.
    pub compact: bool,
    path: PathBuf,
}

impl AppSettings {
    pub fn load() -> Self {
        Self::from_file(config_dir().join("app.conf"))
    }

    pub fn from_file(path: PathBuf) -> Self {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let values = parse_lines(&text);
        Self {
            startup_profile: values
                .get("startup_profile")
                .map(|value| (*value).to_owned())
                .unwrap_or_default(),
            start_effects: flag(&values, "start_effects", false),
            keep_running: flag(&values, "keep_running", false),
            restore_camera: flag(&values, "restore_camera", true),
            sleep_on_lock: flag(&values, "sleep_on_lock", false),
            sleep_when_unused: flag(&values, "sleep_when_unused", false),
            compact: flag(&values, "compact", false),
            path,
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = format!(
            "startup_profile={}\nstart_effects={}\nkeep_running={}\nrestore_camera={}\n\
             sleep_on_lock={}\nsleep_when_unused={}\ncompact={}\n",
            one_line(&self.startup_profile),
            self.start_effects,
            self.keep_running,
            self.restore_camera,
            self.sleep_on_lock,
            self.sleep_when_unused,
            self.compact,
        );
        write_atomically(&self.path, &text)
    }
}

/// The XDG autostart entry that launches the app in the tray at login.
fn autostart_file() -> PathBuf {
    config_dir()
        .parent()
        .map(|config| config.join("autostart/obsbot-control-center.desktop"))
        .unwrap_or_else(|| PathBuf::from("obsbot-control-center.desktop"))
}

pub fn starts_at_login() -> bool {
    autostart_file().is_file()
}

/// Create or remove the autostart entry.
pub fn set_start_at_login(enabled: bool) -> std::io::Result<()> {
    let file = autostart_file();
    if !enabled {
        return match fs::remove_file(&file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=OBSBOT Control Center\n\
         Comment=Camera control and effects, started in the tray\n\
         Exec=\"{}\" --background\nIcon=camera-web\nTerminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    write_atomically(&file, &entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_default_sensibly() {
        let path = std::env::temp_dir().join(format!("obsbot-app-{}.conf", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut settings = AppSettings::from_file(path.clone());
        assert!(settings.restore_camera && !settings.keep_running && !settings.start_effects);
        assert!(settings.startup_profile.is_empty());
        settings.startup_profile = "Meeting".into();
        settings.keep_running = true;
        settings.sleep_on_lock = true;
        settings.restore_camera = false;
        settings.save().unwrap();
        assert_eq!(AppSettings::from_file(path.clone()), settings);
        let _ = fs::remove_file(path);
    }
}
