//! Finding the effects runtime, whether the app runs from a source checkout
//! or from an installed copy.

use crate::conf::data_dir;
use std::path::{Path, PathBuf};

fn checkout() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Candidate folders for the worker scripts, most specific first.
fn effects_candidates() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(dir) = std::env::var_os("OBSBOT_EFFECTS_DIR") {
        list.push(PathBuf::from(dir));
    }
    if let Some(prefix) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(Path::to_path_buf))
    {
        list.push(prefix.join("share/obsbot-control-center/effects"));
    }
    list.push(data_dir().join("effects"));
    list.push("/usr/local/share/obsbot-control-center/effects".into());
    list.push("/usr/share/obsbot-control-center/effects".into());
    list.push(checkout().join("effects"));
    list
}

fn first_with(candidates: Vec<PathBuf>, marker: &str) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|dir| dir.join(marker).is_file())
}

/// The folder holding `processor.py`, the bundled LUTs and backgrounds.
pub fn effects_dir() -> Option<PathBuf> {
    first_with(effects_candidates(), "processor.py")
}

/// The Python interpreter that has the worker's dependencies installed.
pub fn python() -> Option<PathBuf> {
    let mut list = Vec::new();
    if let Some(python) = std::env::var_os("OBSBOT_EFFECTS_PYTHON") {
        list.push(PathBuf::from(python));
    }
    list.push(data_dir().join("venv/bin/python"));
    list.push(checkout().join(".effects-venv/bin/python"));
    list.into_iter().find(|python| python.is_file())
}

/// The folder holding the MediaPipe model files.
pub fn models_dir() -> Option<PathBuf> {
    let mut list = vec![data_dir().join("models")];
    list.extend(effects_dir().map(|dir| dir.join("models")));
    list.into_iter().find(|dir| {
        dir.join("face_landmarker.task").is_file() && dir.join("selfie_segmenter.tflite").is_file()
    })
}

/// Whether the hand-tracking model for the gesture effects is installed.
pub fn has_hand_model() -> bool {
    models_dir().is_some_and(|dir| dir.join("hand_landmarker.task").is_file())
}

/// A bundled resource folder (`luts`, `backgrounds`); may not exist.
pub fn bundled(name: &str) -> PathBuf {
    effects_dir()
        .unwrap_or_else(|| checkout().join("effects"))
        .join(name)
}

/// A human-readable summary of what was found where, for `--paths`.
pub fn report() -> String {
    let show = |path: Option<PathBuf>| {
        path.map_or_else(|| "not found".to_owned(), |path| path.display().to_string())
    };
    format!(
        "effects: {}\npython:  {}\nmodels:  {}\nconfig:  {}",
        show(effects_dir()),
        show(python()),
        show(models_dir()),
        crate::conf::config_dir().display()
    )
}

/// What to tell the user when part of the runtime is missing.
pub fn setup_hint() -> &'static str {
    if checkout().join("scripts/setup-effects.sh").is_file() {
        "Run: bash scripts/setup-effects.sh"
    } else {
        "Run the installer again (scripts/install.sh) to set it up."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checkout_is_found_when_nothing_is_installed() {
        let dir = effects_dir().expect("effects folder");
        assert!(dir.join("processor.py").is_file());
        assert!(bundled("luts").join("Moonlight.cube").is_file());
    }

    #[test]
    fn first_with_skips_folders_missing_the_marker() {
        let root = std::env::temp_dir().join(format!("obsbot-paths-{}", std::process::id()));
        let (empty, full) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&full).unwrap();
        std::fs::write(full.join("processor.py"), "").unwrap();
        assert_eq!(
            first_with(vec![empty, full.clone()], "processor.py"),
            Some(full)
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
