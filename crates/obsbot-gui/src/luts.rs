//! The LUT library: bundled `.cube` files plus the ones the user imported.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

/// Largest `.cube` file accepted for import (a 129-point LUT is about 60 MB).
const MAX_FILE_BYTES: u64 = 96 * 1024 * 1024;

/// A parsed 3D LUT, used here only to draw a preview swatch.
#[derive(Debug)]
pub struct Cube {
    size: usize,
    /// RGB triples, red changing fastest, then green, then blue.
    data: Vec<[f32; 3]>,
}

impl Cube {
    /// Parse the text of a `.cube` file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut size = 0usize;
        let mut domain = (0.0f32, 1.0f32);
        let mut data = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_whitespace();
            let Some(head) = words.next() else { continue };
            match head.to_ascii_uppercase().as_str() {
                "LUT_3D_SIZE" => {
                    size = words
                        .next()
                        .and_then(|word| word.parse().ok())
                        .ok_or("LUT_3D_SIZE has no number")?;
                    if !(2..=129).contains(&size) {
                        return Err(format!("LUT size {size} is not supported"));
                    }
                }
                "LUT_1D_SIZE" => return Err("1D LUTs are not supported".into()),
                "DOMAIN_MIN" => domain.0 = words.next().and_then(|w| w.parse().ok()).unwrap_or(0.0),
                "DOMAIN_MAX" => domain.1 = words.next().and_then(|w| w.parse().ok()).unwrap_or(1.0),
                _ => {
                    let Ok(red) = head.parse::<f32>() else {
                        continue; // TITLE and other keywords
                    };
                    let green = words.next().and_then(|w| w.parse::<f32>().ok());
                    let blue = words.next().and_then(|w| w.parse::<f32>().ok());
                    match (green, blue) {
                        (Some(green), Some(blue)) => data.push([red, green, blue]),
                        _ => return Err("a colour row is incomplete".into()),
                    }
                }
            }
        }
        if size == 0 {
            return Err("not a 3D .cube file (LUT_3D_SIZE is missing)".into());
        }
        if data.len() != size * size * size {
            return Err(format!(
                "expected {} colour rows, found {}",
                size * size * size,
                data.len()
            ));
        }
        let span = (domain.1 - domain.0).max(1e-6);
        for entry in &mut data {
            for channel in entry.iter_mut() {
                *channel = ((*channel - domain.0) / span).clamp(0.0, 1.0);
            }
        }
        Ok(Self { size, data })
    }

    /// The graded colour for `rgb`, using the nearest LUT entry.
    pub fn sample(&self, rgb: [u8; 3]) -> [u8; 3] {
        let top = (self.size - 1) as f32;
        let at = |value: u8| (f32::from(value) / 255.0 * top).round() as usize;
        let index = at(rgb[0]) + at(rgb[1]) * self.size + at(rgb[2]) * self.size * self.size;
        self.data[index].map(|channel| (channel * 255.0).round() as u8)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Lut {
    pub name: String,
    pub path: PathBuf,
    /// A warm skin tone and a sky blue as this LUT renders them.
    pub swatch: [[u8; 3]; 2],
    /// Imported by the user (and so removable), as opposed to bundled.
    pub user: bool,
    modified: Option<SystemTime>,
}

const SWATCH_INPUT: [[u8; 3]; 2] = [[232, 176, 140], [70, 124, 196]];

fn read_lut(path: &Path, user: bool) -> Option<Lut> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let cube = Cube::parse(&fs::read_to_string(path).ok()?).ok()?;
    Some(Lut {
        name: path.file_stem()?.to_string_lossy().replace('_', " "),
        path: path.to_path_buf(),
        swatch: SWATCH_INPUT.map(|color| cube.sample(color)),
        user,
        modified: meta.modified().ok(),
    })
}

fn is_cube(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cube"))
}

pub struct LutLibrary {
    pub luts: Vec<Lut>,
    bundled_dir: PathBuf,
    user_dir: PathBuf,
    picker: Option<Receiver<Option<PathBuf>>>,
    last_scan: Instant,
    /// Text in the "paste a path" box.
    pub import_path: String,
    /// Outcome of the last import or removal: (text, is an error).
    pub message: Option<(String, bool)>,
}

impl LutLibrary {
    pub fn new(bundled_dir: PathBuf, user_dir: PathBuf) -> Self {
        let mut library = Self {
            luts: Vec::new(),
            bundled_dir,
            user_dir,
            picker: None,
            last_scan: Instant::now(),
            import_path: String::new(),
            message: None,
        };
        library.rescan();
        library
    }

    pub fn user_dir(&self) -> &Path {
        &self.user_dir
    }

    /// Pick up files dropped into the folders while the app is open.
    pub fn rescan_if_due(&mut self) {
        if self.last_scan.elapsed() >= Duration::from_secs(2) {
            self.rescan();
        }
    }

    /// Re-read both folders, re-parsing only files that are new or changed.
    pub fn rescan(&mut self) {
        self.last_scan = Instant::now();
        let mut found = Vec::new();
        for (dir, user) in [(&self.bundled_dir, false), (&self.user_dir, true)] {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| is_cube(path))
                .collect();
            paths.sort();
            for path in paths {
                let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
                let known = self
                    .luts
                    .iter()
                    .find(|lut| lut.path == path && lut.modified == modified);
                match known {
                    Some(lut) => found.push(lut.clone()),
                    None => found.extend(read_lut(&path, user)),
                }
            }
        }
        self.luts = found;
    }

    pub fn find(&self, path: &str) -> Option<&Lut> {
        self.luts.iter().find(|lut| lut.path == Path::new(path))
    }

    /// Validate `source` and copy it into the user's LUT folder.
    pub fn import(&mut self, source: &Path) -> Result<PathBuf, String> {
        if !is_cube(source) {
            return Err("Only .cube LUT files can be imported.".into());
        }
        let meta =
            fs::metadata(source).map_err(|error| format!("Cannot read the file: {error}"))?;
        if meta.len() > MAX_FILE_BYTES {
            return Err("That LUT file is too large.".into());
        }
        let text =
            fs::read_to_string(source).map_err(|error| format!("Cannot read the file: {error}"))?;
        Cube::parse(&text).map_err(|error| format!("Not a usable LUT: {error}."))?;
        let name = source.file_name().ok_or("That path has no file name.")?;
        fs::create_dir_all(&self.user_dir)
            .map_err(|error| format!("Cannot create the LUT folder: {error}"))?;
        let target = self.user_dir.join(name);
        if target != source {
            fs::write(&target, text).map_err(|error| format!("Cannot save the LUT: {error}"))?;
        }
        self.rescan();
        Ok(target)
    }

    /// Delete an imported LUT. Bundled LUTs are never removed.
    pub fn remove(&mut self, path: &Path) -> Result<(), String> {
        let removable = self.luts.iter().any(|lut| lut.user && lut.path == path);
        if !removable || path.parent() != Some(self.user_dir.as_path()) {
            return Err("Only imported LUTs can be removed.".into());
        }
        fs::remove_file(path).map_err(|error| format!("Cannot remove the LUT: {error}"))?;
        self.rescan();
        Ok(())
    }

    pub fn can_browse() -> bool {
        picker_command().is_some()
    }

    /// Open the desktop's file chooser without blocking the UI.
    pub fn browse(&mut self) {
        let Some((program, args)) = picker_command() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.picker = Some(rx);
        std::thread::spawn(move || {
            let output = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output();
            let picked = output.ok().and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                (output.status.success() && !text.is_empty()).then(|| PathBuf::from(text))
            });
            let _ = tx.send(picked);
        });
    }

    pub fn browsing(&self) -> bool {
        self.picker.is_some()
    }

    /// The file chosen in the dialog opened by `browse`, once there is one.
    pub fn take_picked(&mut self) -> Option<PathBuf> {
        let result = match self.picker.as_ref()?.try_recv() {
            Ok(picked) => picked,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => None,
        };
        self.picker = None;
        result
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

fn picker_command() -> Option<(&'static str, Vec<&'static str>)> {
    if on_path("zenity") {
        Some((
            "zenity",
            vec![
                "--file-selection",
                "--title=Choose a .cube LUT",
                "--file-filter=LUT files (*.cube) | *.cube *.CUBE",
            ],
        ))
    } else if on_path("kdialog") {
        Some((
            "kdialog",
            vec!["--getopenfilename", ".", "LUT files (*.cube)"],
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2-point LUT that swaps red and blue.
    fn swap_cube() -> String {
        let mut text = String::from("TITLE \"Swap\"\n# comment\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\n");
        for blue in [0.0, 1.0] {
            for green in [0.0, 1.0] {
                for red in [0.0, 1.0] {
                    text.push_str(&format!("{blue} {green} {red}\n"));
                }
            }
        }
        text
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("obsbot-lut-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cube_sampling_follows_red_fastest_order() {
        let cube = Cube::parse(&swap_cube()).unwrap();
        assert_eq!(cube.sample([255, 0, 0]), [0, 0, 255]);
        assert_eq!(cube.sample([0, 255, 0]), [0, 255, 0]);
        assert_eq!(cube.sample([10, 250, 240]), [255, 255, 0]);
    }

    #[test]
    fn malformed_cubes_are_rejected_with_a_reason() {
        assert!(Cube::parse("hello").unwrap_err().contains("LUT_3D_SIZE"));
        assert!(Cube::parse("LUT_1D_SIZE 4\n0 0 0\n")
            .unwrap_err()
            .contains("1D"));
        assert!(Cube::parse("LUT_3D_SIZE 2\n0 0 0\n")
            .unwrap_err()
            .contains("expected 8"));
        assert!(Cube::parse("LUT_3D_SIZE 2\n0 0\n")
            .unwrap_err()
            .contains("incomplete"));
        assert!(Cube::parse("LUT_3D_SIZE 500\n")
            .unwrap_err()
            .contains("not supported"));
    }

    #[test]
    fn bundled_luts_all_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../effects/luts");
        let library = LutLibrary::new(dir, scratch_dir("bundled-none"));
        assert!(library.luts.len() >= 8, "found {}", library.luts.len());
        assert!(library.luts.iter().all(|lut| !lut.user));
    }

    #[test]
    fn import_copies_valid_luts_and_remove_deletes_only_those() {
        let source_dir = scratch_dir("source");
        let bundled = scratch_dir("bundled");
        let user = scratch_dir("user");
        fs::write(bundled.join("Stock.cube"), swap_cube()).unwrap();
        fs::write(source_dir.join("My_Grade.cube"), swap_cube()).unwrap();
        fs::write(source_dir.join("broken.cube"), "LUT_3D_SIZE 2\n").unwrap();
        fs::write(source_dir.join("notes.txt"), swap_cube()).unwrap();

        let mut library = LutLibrary::new(bundled.clone(), user.clone());
        assert_eq!(library.luts.len(), 1);
        assert!(library.import(&source_dir.join("broken.cube")).is_err());
        assert!(library.import(&source_dir.join("notes.txt")).is_err());
        assert!(library.import(&source_dir.join("missing.cube")).is_err());

        let imported = library.import(&source_dir.join("My_Grade.cube")).unwrap();
        assert_eq!(imported, user.join("My_Grade.cube"));
        let lut = library.find(imported.to_str().unwrap()).unwrap();
        assert_eq!(lut.name, "My Grade");
        assert!(lut.user);
        assert_eq!(lut.swatch[0], [255, 255, 255]);

        assert!(library.remove(&bundled.join("Stock.cube")).is_err());
        assert!(bundled.join("Stock.cube").exists());
        library.remove(&imported).unwrap();
        assert!(!imported.exists());
        assert_eq!(library.luts.len(), 1);
        for dir in [source_dir, bundled, user] {
            let _ = fs::remove_dir_all(dir);
        }
    }
}
