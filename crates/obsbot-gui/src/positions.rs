//! Named camera positions: where the gimbal points, how far it is zoomed,
//! and optionally four corners of a whiteboard to straighten.

use crate::conf::write_atomically;
use std::fs;
use std::path::PathBuf;

pub const MAX_NAME_CHARS: usize = 24;
/// Corner order: top-left, top-right, bottom-right, bottom-left; x then y, 0..1.
pub type Corners = [f32; 8];
/// Where the corner handles start: a comfortable inset rectangle.
pub const DEFAULT_CORNERS: Corners = [0.15, 0.15, 0.85, 0.15, 0.85, 0.85, 0.15, 0.85];

#[derive(Clone, PartialEq, Debug)]
pub struct Position {
    pub name: String,
    pub pan: f64,
    pub tilt: f64,
    pub zoom: i32,
    pub corners: Option<Corners>,
}

pub fn corners_text(corners: &Corners) -> String {
    corners
        .iter()
        .map(|value| format!("{value:.4}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_corners(text: &str) -> Option<Corners> {
    let values: Vec<f32> = text
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect::<Option<_>>()?;
    let corners: Corners = values.try_into().ok()?;
    corners
        .iter()
        .all(|value| (0.0..=1.0).contains(value))
        .then_some(corners)
}

impl Position {
    fn to_line(&self) -> String {
        format!(
            "{}\t{:.2}\t{:.2}\t{}\t{}",
            self.name,
            self.pan,
            self.tilt,
            self.zoom,
            self.corners.as_ref().map(corners_text).unwrap_or_default()
        )
    }

    fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.split('\t');
        let name = clean_name(parts.next()?).ok()?;
        Some(Self {
            name,
            pan: parts.next()?.parse::<f64>().ok()?.clamp(-130.0, 130.0),
            tilt: parts.next()?.parse::<f64>().ok()?.clamp(-90.0, 90.0),
            zoom: parts.next()?.parse::<i32>().ok()?.clamp(0, 100),
            corners: parts.next().and_then(parse_corners),
        })
    }
}

pub fn clean_name(raw: &str) -> Result<String, String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("Give the position a name.".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("Keep the name under {MAX_NAME_CHARS} characters."));
    }
    if name.chars().any(char::is_control) {
        return Err("Names cannot contain control characters.".into());
    }
    Ok(name)
}

pub struct Positions {
    pub list: Vec<Position>,
    path: PathBuf,
    /// The position most recently recalled or saved.
    pub current: Option<String>,
    /// `Some` while the "save as" name box is open.
    pub draft: Option<String>,
    pub focus_draft: bool,
    pub confirm_delete: Option<String>,
    /// Corners being dragged on the preview for the current position.
    pub adjusting: Option<Corners>,
    pub message: Option<(String, bool)>,
}

impl Positions {
    pub fn load(path: PathBuf) -> Self {
        let list = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(Position::from_line)
            .collect();
        Self {
            list,
            path,
            current: None,
            draft: None,
            focus_draft: false,
            confirm_delete: None,
            adjusting: None,
            message: None,
        }
    }

    fn store(&self) -> Result<(), String> {
        let text: String = self
            .list
            .iter()
            .map(|position| position.to_line() + "\n")
            .collect();
        write_atomically(&self.path, &text)
            .map_err(|error| format!("Could not save positions: {error}"))
    }

    pub fn find(&self, name: &str) -> Option<&Position> {
        self.list.iter().find(|position| position.name == name)
    }

    /// Add a position, or replace the one with the same name (keeping its
    /// whiteboard corners unless new ones are given).
    pub fn save(&mut self, position: Position) -> Result<String, String> {
        let name = clean_name(&position.name)?;
        let mut position = Position {
            name: name.clone(),
            ..position
        };
        match self.list.iter_mut().find(|existing| existing.name == name) {
            Some(existing) => {
                if position.corners.is_none() {
                    position.corners = existing.corners;
                }
                *existing = position;
            }
            None => self.list.push(position),
        }
        self.store()?;
        self.current = Some(name.clone());
        Ok(name)
    }

    /// Set or clear the whiteboard corners of a saved position.
    pub fn set_corners(&mut self, name: &str, corners: Option<Corners>) -> Result<(), String> {
        let position = self
            .list
            .iter_mut()
            .find(|position| position.name == name)
            .ok_or("That position no longer exists.")?;
        position.corners = corners;
        self.store()
    }

    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        let before = self.list.len();
        self.list.retain(|position| position.name != name);
        if self.list.len() == before {
            return Err("That position no longer exists.".into());
        }
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        self.store()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "obsbot-positions-{name}-{}.conf",
            std::process::id()
        ))
    }

    fn desk() -> Position {
        Position {
            name: "Desk".into(),
            pan: -12.5,
            tilt: 4.0,
            zoom: 30,
            corners: None,
        }
    }

    #[test]
    fn positions_persist_with_and_without_whiteboard_corners() {
        let path = scratch("persist");
        let _ = fs::remove_file(&path);
        let mut positions = Positions::load(path.clone());
        positions.save(desk()).unwrap();
        positions
            .save(Position {
                name: " White  board ".into(),
                pan: 40.0,
                tilt: 10.0,
                zoom: 55,
                corners: Some(DEFAULT_CORNERS),
            })
            .unwrap();
        assert_eq!(positions.current.as_deref(), Some("White board"));
        let reloaded = Positions::load(path.clone());
        assert_eq!(reloaded.list, positions.list);
        assert_eq!(
            reloaded.find("White board").unwrap().corners,
            Some(DEFAULT_CORNERS)
        );
        assert_eq!(reloaded.find("Desk").unwrap().pan, -12.5);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn updating_a_position_keeps_its_corners_until_cleared() {
        let path = scratch("update");
        let _ = fs::remove_file(&path);
        let mut positions = Positions::load(path.clone());
        positions.save(desk()).unwrap();
        positions
            .set_corners("Desk", Some(DEFAULT_CORNERS))
            .unwrap();
        positions.save(Position { zoom: 80, ..desk() }).unwrap();
        assert_eq!(positions.list.len(), 1);
        assert_eq!(positions.find("Desk").unwrap().zoom, 80);
        assert_eq!(
            positions.find("Desk").unwrap().corners,
            Some(DEFAULT_CORNERS)
        );
        positions.set_corners("Desk", None).unwrap();
        assert_eq!(
            Positions::load(path.clone()).find("Desk").unwrap().corners,
            None
        );
        positions.delete("Desk").unwrap();
        assert!(positions.delete("Desk").is_err());
        assert!(positions.set_corners("Desk", None).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn damaged_lines_are_skipped() {
        assert!(Position::from_line("Desk\t1\t2").is_none());
        assert!(Position::from_line("\t1\t2\t3\t").is_none());
        let odd = Position::from_line("Far\t999\t-999\t500\t0.1,0.2").unwrap();
        assert_eq!(
            (odd.pan, odd.tilt, odd.zoom, odd.corners),
            (130.0, -90.0, 100, None)
        );
        assert!(parse_corners("0,0,1,0,1,1,0,2").is_none());
        assert_eq!(corners_text(&DEFAULT_CORNERS).split(',').count(), 8);
    }
}
