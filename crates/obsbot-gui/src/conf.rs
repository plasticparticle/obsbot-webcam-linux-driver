//! Small helpers shared by everything that reads or writes the app's
//! `key=value` files.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "obsbot-control-center";

fn base(var: &str, home_fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(home_fallback)))
        .unwrap_or_else(std::env::temp_dir)
}

/// `$XDG_CONFIG_HOME/obsbot-control-center`: settings, profiles, user files.
pub fn config_dir() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config").join(APP_DIR)
}

/// `$XDG_DATA_HOME/obsbot-control-center`: the installed effects runtime.
pub fn data_dir() -> PathBuf {
    base("XDG_DATA_HOME", ".local/share").join(APP_DIR)
}

pub fn parse_lines(text: &str) -> HashMap<&str, &str> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

pub fn flag(values: &HashMap<&str, &str>, key: &str, default: bool) -> bool {
    match values.get(key) {
        Some(value) => matches!(*value, "1" | "true" | "yes" | "on"),
        None => default,
    }
}

/// One line of text safe to store as a value.
pub fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// Write-then-rename, so a reader never sees a half-written file.
pub fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}
