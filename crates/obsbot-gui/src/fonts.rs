//! The fonts installed on this system, for the name badge's font menu.

use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

#[derive(Default)]
pub struct FontList {
    pub families: Vec<String>,
    pending: Option<Receiver<Vec<String>>>,
    asked: bool,
    /// Text typed into the menu's search box.
    pub filter: String,
}

/// Turn `fc-list` output (one family per line, possibly with duplicates and
/// comma-separated aliases) into a sorted list of unique names.
pub fn parse_families(output: &str) -> Vec<String> {
    let mut families: Vec<String> = output
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .map(str::to_owned)
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.dedup();
    families
}

impl FontList {
    /// A list that is already loaded, so tests never run `fc-list`.
    #[cfg(test)]
    pub fn with_families(families: &[&str]) -> Self {
        Self {
            families: families.iter().map(|name| (*name).to_owned()).collect(),
            asked: true,
            ..Self::default()
        }
    }

    /// Start listing fonts in the background the first time this is called,
    /// and pick up the result when it is ready.
    pub fn refresh(&mut self) {
        if !self.asked {
            self.asked = true;
            let (tx, rx) = mpsc::channel();
            self.pending = Some(rx);
            std::thread::spawn(move || {
                let listed = Command::new("fc-list")
                    .args(["--format", "%{family}\n"])
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output()
                    .map(|output| parse_families(&String::from_utf8_lossy(&output.stdout)))
                    .unwrap_or_default();
                let _ = tx.send(listed);
            });
        }
        if let Some(rx) = &self.pending {
            if let Ok(families) = rx.try_recv() {
                self.families = families;
                self.pending = None;
            }
        }
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Families matching the search box, best matches (prefix) first.
    pub fn matches(&self) -> Vec<&str> {
        let wanted = self.filter.trim().to_lowercase();
        let mut found: Vec<&str> = self
            .families
            .iter()
            .map(String::as_str)
            .filter(|name| wanted.is_empty() || name.to_lowercase().contains(&wanted))
            .collect();
        found.sort_by_key(|name| !name.to_lowercase().starts_with(&wanted));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_unique_sorted_and_use_the_first_alias() {
        let listed = parse_families(
            "Noto Sans\nDejaVu Serif,DejaVu Serif Condensed\nnoto Mono\nNoto Sans\n\n.Hidden\n",
        );
        assert_eq!(listed, ["DejaVu Serif", "noto Mono", "Noto Sans"]);
    }

    #[test]
    fn search_puts_prefix_matches_first() {
        let fonts = FontList {
            families: vec!["Liberation Mono".into(), "Mono Sans".into(), "Serif".into()],
            filter: "mono".into(),
            ..FontList::default()
        };
        assert_eq!(fonts.matches(), ["Mono Sans", "Liberation Mono"]);
    }
}
