use crate::preview::Preview;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Clone, Copy, PartialEq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
    pub label: &'static str,
}

pub const RESOLUTIONS: [Resolution; 4] = [
    Resolution {
        width: 640,
        height: 360,
        label: "640 × 360",
    },
    Resolution {
        width: 640,
        height: 480,
        label: "640 × 480",
    },
    Resolution {
        width: 1280,
        height: 720,
        label: "1280 × 720",
    },
    Resolution {
        width: 1920,
        height: 1080,
        label: "1920 × 1080",
    },
];

pub const BACKGROUNDS: [&str; 3] = ["Off", "Blur", "Image"];
pub const EYE_COLORS: [&str; 6] = ["Natural", "Blue", "Green", "Hazel", "Gray", "Violet"];

pub struct EffectsController {
    pub enabled: bool,
    pub resolution: usize,
    pub fps: u32,
    pub background: usize,
    pub background_path: String,
    pub blur: f32,
    pub smoothing: f32,
    pub slimming: f32,
    pub skin_tone: f32,
    pub eye_color: usize,
    pub message: Option<String>,
    process: Option<Child>,
    config_path: PathBuf,
    output_path: Option<PathBuf>,
}

impl Default for EffectsController {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution: 2,
            fps: 30,
            background: 0,
            background_path: String::new(),
            blur: 0.55,
            smoothing: 0.0,
            slimming: 0.0,
            skin_tone: 0.0,
            eye_color: 0,
            message: None,
            process: None,
            config_path: std::env::temp_dir()
                .join(format!("obsbot-effects-{}.conf", std::process::id())),
            output_path: None,
        }
    }
}

impl EffectsController {
    pub fn selected_resolution(&self) -> Resolution {
        RESOLUTIONS[self.resolution.min(RESOLUTIONS.len() - 1)]
    }

    pub fn write_config(&mut self) -> io::Result<()> {
        let background = match self.background {
            1 => "blur",
            2 => "image",
            _ => "off",
        };
        let eye_color = EYE_COLORS[self.eye_color.min(EYE_COLORS.len() - 1)].to_lowercase();
        let background_path = self.background_path.replace(['\n', '\r'], "");
        let config = format!(
            "background={background}\nbackground_path={background_path}\nblur={}\n\
             smoothing={}\nslimming={}\nskin_tone={}\neye_color={eye_color}\n",
            self.blur, self.smoothing, self.slimming, self.skin_tone
        );
        fs::write(&self.config_path, config)
    }

    pub fn start(&mut self, input: &Path, preview: &mut Preview) -> Result<(), String> {
        self.stop(preview);
        let output = find_virtual_camera().ok_or_else(|| {
            "Virtual camera not found. Run: bash scripts/setup-virtual-camera.sh".to_owned()
        })?;
        let root = repository_root();
        let python = root.join(".effects-venv/bin/python");
        if !python.exists() {
            return Err("Effects runtime not installed. Run: bash scripts/setup-effects.sh".into());
        }
        let processor = root.join("effects/processor.py");
        let model_dir = root.join("effects/models");
        if !model_dir.join("face_landmarker.task").exists()
            || !model_dir.join("selfie_segmenter.tflite").exists()
        {
            return Err("MediaPipe models are missing. Run: bash scripts/setup-effects.sh".into());
        }
        self.write_config().map_err(|error| error.to_string())?;
        let resolution = self.selected_resolution();

        // Release the physical capture stream before the worker opens it.
        preview.pause();
        let child = Command::new(python)
            .arg(processor)
            .arg("--input")
            .arg(input)
            .arg("--output")
            .arg(&output)
            .arg("--config")
            .arg(&self.config_path)
            .arg("--model-dir")
            .arg(model_dir)
            .arg("--width")
            .arg(resolution.width.to_string())
            .arg("--height")
            .arg(resolution.height.to_string())
            .arg("--fps")
            .arg(self.fps.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                preview.show_camera();
                format!("failed to start effects worker: {error}")
            })?;

        preview.show_device(output.clone(), resolution.width, resolution.height);
        self.process = Some(child);
        self.output_path = Some(output);
        self.enabled = true;
        self.message = Some("Publishing OBSBOT Virtual Camera".into());
        Ok(())
    }

    pub fn restart(&mut self, input: &Path, preview: &mut Preview) -> Result<(), String> {
        if self.enabled {
            self.start(input, preview)
        } else {
            self.write_config().map_err(|error| error.to_string())
        }
    }

    pub fn stop(&mut self, preview: &mut Preview) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        if self.enabled {
            preview.show_camera();
        }
        self.enabled = false;
        self.output_path = None;
        self.message = None;
    }

    pub fn poll(&mut self, preview: &mut Preview) {
        let Some(process) = &mut self.process else {
            return;
        };
        match process.try_wait() {
            Ok(Some(status)) => {
                self.process = None;
                self.enabled = false;
                self.output_path = None;
                self.message = Some(format!("Effects worker stopped ({status})"));
                preview.show_camera();
            }
            Ok(None) => {}
            Err(error) => {
                self.message = Some(format!("Could not inspect effects worker: {error}"));
            }
        }
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn find_virtual_camera() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/video4linux").ok()?;
    for entry in entries.flatten() {
        let name = fs::read_to_string(entry.path().join("name")).ok()?;
        if name.trim() == "OBSBOT Virtual Camera" {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}
