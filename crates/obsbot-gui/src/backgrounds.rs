//! The background library: bundled pictures plus the ones the user added,
//! each with a thumbnail for its tile.

use crate::picker::FilePicker;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
const THUMB: (u32, u32) = (192, 108);

pub struct Backdrop {
    pub name: String,
    pub path: PathBuf,
    /// Added by the user (and so removable), as opposed to bundled.
    pub user: bool,
    thumb: Option<ColorImage>,
    texture: Option<TextureHandle>,
    /// Set when the picture could not be decoded, so it is not retried.
    broken: bool,
}

fn is_picture(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["jpg", "jpeg", "png"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

/// Decode a picture and shrink it to a cover-cropped 16:9 thumbnail.
fn thumbnail(path: &Path) -> Option<ColorImage> {
    let picture = image::open(path).ok()?;
    let thumb = picture
        .resize_to_fill(THUMB.0, THUMB.1, image::imageops::FilterType::Triangle)
        .to_rgb8();
    Some(ColorImage::from_rgb(
        [thumb.width() as usize, thumb.height() as usize],
        thumb.as_raw(),
    ))
}

pub struct BackdropLibrary {
    pub items: Vec<Backdrop>,
    bundled_dir: PathBuf,
    user_dir: PathBuf,
    last_scan: Instant,
    pub picker: FilePicker,
    /// Chooser for the corner logo, which lives on the same tab.
    pub logo_picker: FilePicker,
    /// Outcome of the last import or removal: (text, is an error).
    pub message: Option<(String, bool)>,
}

impl BackdropLibrary {
    pub fn new(bundled_dir: PathBuf, user_dir: PathBuf) -> Self {
        let mut library = Self {
            items: Vec::new(),
            bundled_dir,
            user_dir,
            last_scan: Instant::now(),
            picker: FilePicker::default(),
            logo_picker: FilePicker::default(),
            message: None,
        };
        library.rescan();
        library
    }

    pub fn user_dir(&self) -> &Path {
        &self.user_dir
    }

    pub fn rescan_if_due(&mut self) {
        if self.last_scan.elapsed() >= Duration::from_secs(2) {
            self.rescan();
        }
    }

    /// Re-read both folders, keeping thumbnails already made.
    pub fn rescan(&mut self) {
        self.last_scan = Instant::now();
        let mut known = std::mem::take(&mut self.items);
        for (dir, user) in [(&self.bundled_dir, false), (&self.user_dir, true)] {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| is_picture(path))
                .collect();
            paths.sort();
            for path in paths {
                let item = match known.iter().position(|item| item.path == path) {
                    Some(index) => known.swap_remove(index),
                    None => Backdrop {
                        name: path
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().replace('_', " "))
                            .unwrap_or_default(),
                        path,
                        user,
                        thumb: None,
                        texture: None,
                        broken: false,
                    },
                };
                self.items.push(item);
            }
        }
    }

    pub fn find(&self, path: &str) -> Option<&Backdrop> {
        self.items.iter().find(|item| item.path == Path::new(path))
    }

    /// The thumbnail texture for item `index`, decoding at most one picture
    /// per call across the whole library so the UI never stalls.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        budget: &mut bool,
    ) -> Option<&TextureHandle> {
        let item = self.items.get_mut(index)?;
        if item.thumb.is_none() && !item.broken && *budget {
            *budget = false;
            item.thumb = thumbnail(&item.path);
            item.broken = item.thumb.is_none();
        }
        if item.texture.is_none() {
            if let Some(thumb) = &item.thumb {
                item.texture = Some(ctx.load_texture(
                    format!("backdrop-{index}"),
                    thumb.clone(),
                    TextureOptions::LINEAR,
                ));
            }
        }
        item.texture.as_ref()
    }

    /// Drop GPU textures; call when the window closes (they belong to it).
    pub fn forget_textures(&mut self) {
        for item in &mut self.items {
            item.texture = None;
        }
    }

    /// Check `source` is a readable picture and copy it into the user's folder.
    pub fn import(&mut self, source: &Path) -> Result<PathBuf, String> {
        if !is_picture(source) {
            return Err("Only JPEG and PNG pictures can be added.".into());
        }
        let meta =
            fs::metadata(source).map_err(|error| format!("Cannot read the file: {error}"))?;
        if meta.len() > MAX_FILE_BYTES {
            return Err("That picture is too large.".into());
        }
        if thumbnail(source).is_none() {
            return Err("That file is not a readable picture.".into());
        }
        let name = source.file_name().ok_or("That path has no file name.")?;
        fs::create_dir_all(&self.user_dir)
            .map_err(|error| format!("Cannot create the backgrounds folder: {error}"))?;
        let target = self.user_dir.join(name);
        if target != source {
            fs::copy(source, &target)
                .map_err(|error| format!("Cannot save the picture: {error}"))?;
        }
        self.rescan();
        Ok(target)
    }

    /// Delete a picture the user added. Bundled pictures are never removed.
    pub fn remove(&mut self, path: &Path) -> Result<(), String> {
        let removable = self.items.iter().any(|item| item.user && item.path == path);
        if !removable || path.parent() != Some(self.user_dir.as_path()) {
            return Err("Only pictures you added can be removed.".into());
        }
        fs::remove_file(path).map_err(|error| format!("Cannot remove the picture: {error}"))?;
        self.rescan();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("obsbot-bg-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path) {
        image::RgbImage::from_pixel(64, 48, image::Rgb([200, 40, 10]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn bundled_backgrounds_are_listed_and_decode() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../effects/backgrounds");
        let library = BackdropLibrary::new(dir, scratch("none"));
        assert!(library.items.len() >= 6, "found {}", library.items.len());
        let thumb = thumbnail(&library.items[0].path).expect("thumbnail");
        assert_eq!(thumb.size, [THUMB.0 as usize, THUMB.1 as usize]);
    }

    #[test]
    fn import_accepts_pictures_only_and_remove_spares_bundled_ones() {
        let (source, bundled, user) = (scratch("src"), scratch("bundled"), scratch("user"));
        write_png(&bundled.join("Stock.png"));
        write_png(&source.join("My_Room.png"));
        fs::write(source.join("fake.jpg"), "not a picture").unwrap();
        fs::write(source.join("notes.txt"), "x").unwrap();

        let mut library = BackdropLibrary::new(bundled.clone(), user.clone());
        assert_eq!(library.items.len(), 1);
        assert!(library.import(&source.join("fake.jpg")).is_err());
        assert!(library.import(&source.join("notes.txt")).is_err());
        let added = library.import(&source.join("My_Room.png")).unwrap();
        assert_eq!(added, user.join("My_Room.png"));
        let item = library.find(added.to_str().unwrap()).unwrap();
        assert!(item.user && item.name == "My Room");

        assert!(library.remove(&bundled.join("Stock.png")).is_err());
        library.remove(&added).unwrap();
        assert!(!added.exists() && library.items.len() == 1);
        for dir in [source, bundled, user] {
            let _ = fs::remove_dir_all(dir);
        }
    }
}
