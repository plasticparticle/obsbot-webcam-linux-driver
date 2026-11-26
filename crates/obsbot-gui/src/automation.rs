//! Watches the session for the things auto-privacy reacts to: a locked
//! screen, and other programs watching the virtual camera.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LOCKERS: [&str; 7] = [
    "hyprlock",
    "swaylock",
    "waylock",
    "gtklock",
    "i3lock",
    "xsecurelock",
    "kscreenlocker_g",
];

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Session {
    pub locked: bool,
    /// Other programs that have the virtual camera open.
    pub viewers: usize,
}

fn process_names() -> Vec<(u32, String)> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let name = fs::read_to_string(entry.path().join("comm")).ok()?;
            Some((pid, name.trim().to_owned()))
        })
        .collect()
}

fn logind_says_locked() -> bool {
    let session = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "auto".into());
    Command::new("loginctl")
        .args(["show-session", &session, "-p", "LockedHint", "--value"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "yes")
}

/// Programs, other than this app and its own helpers, holding `device` open.
pub fn viewers_of(device: &Path, processes: &[(u32, String)]) -> usize {
    let own = std::process::id();
    processes
        .iter()
        .filter(|(pid, name)| *pid != own && !name.starts_with("gst-launch"))
        .filter(|(pid, _)| {
            fs::read_dir(format!("/proc/{pid}/fd")).is_ok_and(|fds| {
                fds.flatten()
                    .any(|fd| fs::read_link(fd.path()).is_ok_and(|target| target == device))
            })
        })
        .count()
}

pub struct Monitor {
    state: Arc<Mutex<Session>>,
    device: Arc<Mutex<Option<PathBuf>>>,
}

impl Monitor {
    /// Start sampling in the background every two seconds.
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(Session::default()));
        let device: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let (shared_state, shared_device) = (state.clone(), device.clone());
        let _ = std::thread::Builder::new()
            .name("session-monitor".into())
            .spawn(move || loop {
                let processes = process_names();
                let locked = logind_says_locked()
                    || processes
                        .iter()
                        .any(|(_, name)| LOCKERS.contains(&name.as_str()));
                let watched = shared_device.lock().ok().and_then(|device| device.clone());
                let viewers = watched.map_or(0, |device| viewers_of(&device, &processes));
                if let Ok(mut state) = shared_state.lock() {
                    *state = Session { locked, viewers };
                }
                std::thread::sleep(Duration::from_secs(2));
            });
        Self { state, device }
    }

    pub fn session(&self) -> Session {
        self.state.lock().map(|state| *state).unwrap_or_default()
    }

    /// Which device to count viewers on (the virtual camera), if any.
    pub fn watch(&self, device: Option<PathBuf>) {
        if let Ok(mut current) = self.device.lock() {
            *current = device;
        }
    }
}

/// Decides when auto-privacy should put the camera to sleep or wake it.
#[derive(Default)]
pub struct AutoSleep {
    /// True while the camera is asleep because this logic put it there.
    pub asleep_by_us: bool,
    idle_for: f32,
}

#[derive(PartialEq, Debug)]
pub enum Action {
    None,
    Sleep,
    Wake,
}

impl AutoSleep {
    /// Seconds with nobody watching before the camera is put to sleep.
    pub const GRACE: f32 = 20.0;

    /// One step. `in_use` means this window or another app is showing the
    /// picture; `sleeping` is what the camera reports.
    pub fn step(
        &mut self,
        dt: f32,
        on_lock: bool,
        when_unused: bool,
        locked: bool,
        in_use: bool,
        sleeping: bool,
    ) -> Action {
        if !sleeping {
            // Someone woke the camera by hand: that is theirs to keep.
            self.asleep_by_us = false;
        }
        self.idle_for = if in_use { 0.0 } else { self.idle_for + dt };
        let want_sleep = (on_lock && locked) || (when_unused && self.idle_for >= Self::GRACE);
        if want_sleep && !sleeping {
            self.asleep_by_us = true;
            Action::Sleep
        } else if !want_sleep && sleeping && self.asleep_by_us {
            self.asleep_by_us = false;
            Action::Wake
        } else {
            Action::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locking_sleeps_the_camera_and_unlocking_wakes_it() {
        let mut auto = AutoSleep::default();
        assert_eq!(
            auto.step(0.1, true, false, false, true, false),
            Action::None
        );
        assert_eq!(
            auto.step(0.1, true, false, true, true, false),
            Action::Sleep
        );
        assert_eq!(auto.step(0.1, true, false, true, true, true), Action::None);
        assert_eq!(auto.step(0.1, true, false, false, true, true), Action::Wake);
    }

    #[test]
    fn a_camera_the_user_put_to_sleep_is_left_alone() {
        let mut auto = AutoSleep::default();
        // Asleep already, not by us: unlocking must not wake it.
        assert_eq!(auto.step(0.1, true, false, true, true, true), Action::None);
        assert_eq!(auto.step(0.1, true, false, false, true, true), Action::None);
    }

    #[test]
    fn an_unwatched_camera_sleeps_after_the_grace_period() {
        let mut auto = AutoSleep::default();
        assert_eq!(
            auto.step(AutoSleep::GRACE - 1.0, false, true, false, false, false),
            Action::None
        );
        assert_eq!(
            auto.step(2.0, false, true, false, false, false),
            Action::Sleep
        );
        assert_eq!(
            auto.step(1.0, false, true, false, false, true),
            Action::None
        );
        assert_eq!(auto.step(1.0, false, true, false, true, true), Action::Wake);
        // Disabled options never act.
        assert_eq!(
            auto.step(999.0, false, false, true, false, false),
            Action::None
        );
    }

    #[test]
    fn viewers_counts_other_programs_holding_the_device() {
        let file = std::env::temp_dir().join(format!("obsbot-viewers-{}", std::process::id()));
        fs::write(&file, "x").unwrap();
        // Our own handle must not count.
        let _own = fs::File::open(&file).unwrap();
        assert_eq!(viewers_of(&file, &process_names()), 0);
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("exec 3<\"$0\"; sleep 30")
            .arg(&file)
            .spawn()
            .unwrap();
        let mut seen = 0;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(50));
            seen = viewers_of(&file, &process_names());
            if seen > 0 {
                break;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(&file);
        assert_eq!(seen, 1);
    }
}
