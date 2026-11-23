//! Named profiles: complete sets of effect settings the user can switch
//! between with one click. Each profile is one `key=value` file.

use crate::effects::{BeautyPreset, EffectSettings};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_NAME_CHARS: usize = 24;
/// Profiles reachable with Ctrl+1 … Ctrl+9.
pub const SHORTCUTS: usize = 9;

#[derive(Clone, PartialEq, Debug)]
pub struct Profile {
    pub name: String,
    pub settings: EffectSettings,
}

pub struct ProfileStore {
    pub profiles: Vec<Profile>,
    dir: PathBuf,
    /// The profile most recently applied or saved, offered for "Update".
    pub current: Option<String>,
    /// `Some` while the "save as" name box is open.
    pub draft: Option<String>,
    /// Set when the name box opens so it takes keyboard focus once.
    pub focus_draft: bool,
    /// Name awaiting a second click to confirm deletion.
    pub confirm_delete: Option<String>,
    /// Outcome of the last save or delete: (text, is an error).
    pub message: Option<(String, bool)>,
}

/// Tidy a typed name; `Err` explains what is wrong with it.
pub fn clean_name(raw: &str) -> Result<String, String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("Give the profile a name.".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("Keep the name under {MAX_NAME_CHARS} characters."));
    }
    if name.starts_with('.') || name.chars().any(|c| c.is_control() || "/\\:".contains(c)) {
        return Err("Names cannot start with a dot or contain / \\ :".into());
    }
    Ok(name)
}

impl ProfileStore {
    /// Load the profiles in `dir`. With `seed`, a few starter profiles are
    /// written the first time, when the folder does not exist yet.
    pub fn load(dir: PathBuf, seed: bool) -> Self {
        if seed && !dir.exists() && fs::create_dir_all(&dir).is_ok() {
            for (name, settings) in starters() {
                let _ = fs::write(dir.join(format!("{name}.conf")), settings.to_config());
            }
        }
        let mut store = Self {
            profiles: Vec::new(),
            dir,
            current: None,
            draft: None,
            focus_draft: false,
            confirm_delete: None,
            message: None,
        };
        store.reload();
        store
    }

    fn reload(&mut self) {
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for path in entries.flatten().map(|entry| entry.path()) {
                if path.extension().is_none_or(|extension| extension != "conf") {
                    continue;
                }
                let (Some(stem), Ok(text)) = (path.file_stem(), fs::read_to_string(&path)) else {
                    continue;
                };
                profiles.push(Profile {
                    name: stem.to_string_lossy().into_owned(),
                    settings: EffectSettings::from_config(&text),
                });
            }
        }
        profiles.sort_by_key(|profile| profile.name.to_lowercase());
        self.profiles = profiles;
    }

    fn path_of(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.conf"))
    }

    pub fn find(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    /// The profile whose settings are exactly what is on screen now.
    pub fn matching(&self, settings: &EffectSettings) -> Option<&Profile> {
        // Prefer the one the user picked when several are identical.
        self.current
            .as_deref()
            .and_then(|name| self.find(name))
            .filter(|profile| &profile.settings == settings)
            .or_else(|| self.profiles.iter().find(|p| &p.settings == settings))
    }

    /// Save `settings` under `name`, replacing a profile of the same name.
    pub fn save(&mut self, name: &str, settings: &EffectSettings) -> Result<String, String> {
        let name = clean_name(name)?;
        fs::create_dir_all(&self.dir)
            .and_then(|()| write_atomically(&self.path_of(&name), &settings.to_config()))
            .map_err(|error| format!("Could not save the profile: {error}"))?;
        self.reload();
        self.current = Some(name.clone());
        Ok(name)
    }

    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        if self.find(name).is_none() {
            return Err("That profile no longer exists.".into());
        }
        fs::remove_file(self.path_of(name))
            .map_err(|error| format!("Could not delete the profile: {error}"))?;
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        self.reload();
        Ok(())
    }
}

fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    let temporary = path.with_extension("conf.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

/// Profiles written on first launch so the bar is not empty.
fn starters() -> Vec<(&'static str, EffectSettings)> {
    let mut meeting = EffectSettings {
        background: 1,
        ..EffectSettings::default()
    };
    meeting.apply_beauty_preset(BeautyPreset::Natural);
    meeting.apply_light_preset(2);

    let mut studio = EffectSettings {
        look: 1,
        look_strength: 0.5,
        ..EffectSettings::default()
    };
    studio.apply_beauty_preset(BeautyPreset::Natural);
    studio.apply_light_preset(1);

    let mut party = EffectSettings {
        look: 3,
        look_strength: 0.6,
        ..EffectSettings::default()
    };
    party.stickers[7] = true;
    party.triggers[0] = true;
    party.triggers[2] = true;

    let incognito = EffectSettings {
        anonymise: 2,
        ..EffectSettings::default()
    };
    vec![
        ("Meeting", meeting),
        ("Studio", studio),
        ("Party", party),
        ("Incognito", incognito),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("obsbot-profiles-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn names_are_tidied_and_unsafe_ones_rejected() {
        assert_eq!(clean_name("  Late   night ").unwrap(), "Late night");
        assert!(clean_name("   ").is_err());
        assert!(clean_name("a/b").is_err());
        assert!(clean_name("..").is_err());
        assert!(clean_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
    }

    #[test]
    fn starter_profiles_appear_once_and_stay_deleted() {
        let dir = scratch("seed");
        let mut store = ProfileStore::load(dir.clone(), true);
        let names: Vec<&str> = store.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Incognito", "Meeting", "Party", "Studio"]);
        assert!(store.profiles.iter().all(|p| p.settings.any_active()));
        for name in ["Incognito", "Meeting", "Party", "Studio"] {
            store.delete(name).unwrap();
        }
        assert!(ProfileStore::load(dir.clone(), true).profiles.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saved_profiles_round_trip_and_can_be_updated_and_deleted() {
        let dir = scratch("save");
        let mut store = ProfileStore::load(dir.clone(), false);
        assert!(store.profiles.is_empty());
        let mut settings = EffectSettings {
            look: 4,
            grain: 0.25,
            lut: "/luts/My Film.cube".into(),
            ..EffectSettings::default()
        };
        settings.stickers[3] = true;

        assert_eq!(store.save("  Stream  ", &settings).unwrap(), "Stream");
        assert!(store.save("bad/name", &settings).is_err());
        assert_eq!(store.current.as_deref(), Some("Stream"));
        let reloaded = ProfileStore::load(dir.clone(), false);
        assert_eq!(reloaded.find("Stream").unwrap().settings, settings);
        assert_eq!(reloaded.matching(&settings).unwrap().name, "Stream");
        assert!(reloaded.matching(&EffectSettings::default()).is_none());

        settings.grain = 0.75;
        store.save("Stream", &settings).unwrap();
        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.find("Stream").unwrap().settings.grain, 0.75);

        store.delete("Stream").unwrap();
        assert!(store.delete("Stream").is_err());
        assert!(store.profiles.is_empty() && store.current.is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
