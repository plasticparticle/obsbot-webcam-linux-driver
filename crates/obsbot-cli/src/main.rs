use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use obsbot_core::{ctrl, AiMode, Device, Fov, TrackingSpeed};

#[derive(Parser)]
#[command(name = "obsbotctl", about = "Control OBSBOT cameras on Linux", version)]
struct Cli {
    /// Video device node (default: auto-discover)
    #[arg(short, long, global = true)]
    device: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show current gimbal, zoom, AI and image settings
    Status,
    /// Set pan angle in degrees (-130..130)
    Pan {
        #[arg(allow_hyphen_values = true)]
        degrees: f64,
    },
    /// Set tilt angle in degrees (-90..90)
    Tilt {
        #[arg(allow_hyphen_values = true)]
        degrees: f64,
    },
    /// Set zoom level (0..100)
    Zoom { level: i32 },
    /// Recenter gimbal and reset zoom
    Center,
    /// Put the camera to sleep or wake it up
    Sleep {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// AI tracking mode
    Track {
        /// off|normal|upper-body|close-up|headless|lower-body|desk|whiteboard|hand|group
        mode: String,
    },
    /// Tracking speed
    Speed {
        #[arg(value_enum)]
        speed: SpeedArg,
    },
    /// Move gimbal to a preset position (1-3)
    Preset { slot: u32 },
    /// Save the current pan/tilt/zoom as preset (1-3)
    PresetSave { slot: u32 },
    /// Delete a preset (1-3)
    PresetDelete { slot: u32 },
    /// List stored presets
    Presets,
    /// Return the gimbal to its home pose
    Recenter,
    /// HDR on/off
    Hdr {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Face-priority auto exposure on/off
    FaceAe {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Field of view
    Fov {
        #[arg(value_enum)]
        fov: FovArg,
    },
    /// Noise reduction on/off
    NoiseReduction {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Auto-sleep timer in seconds (0 = never)
    AutoSleep { seconds: u16 },
    /// Image parameter (brightness, contrast, saturation, sharpness: 0..100)
    Image {
        #[arg(value_enum)]
        param: ImageParam,
        /// New value; omit to read the current one
        value: Option<i32>,
    },
    /// White balance: "auto" or color temperature in Kelvin (2000-10000)
    Wb { value: String },
    /// Exposure: "auto" or manual exposure time in 100µs units (1-2500)
    Exposure { value: String },
    /// Focus: "auto" or manual focus position (0-100)
    Focus { value: String },
    /// Anti-flicker: off, 50hz or 60hz
    AntiFlicker { value: String },
    /// Get or set a raw V4L2 control by hex id
    Raw {
        #[arg(value_parser = parse_hex)]
        id: u32,
        value: Option<i32>,
    },
    /// Send raw bytes to an XU selector (hex, e.g. "16 02 02 00")
    XuSend { selector: u8, bytes: String },
    /// Hexdump an XU selector's current value block
    XuDump { selector: u8 },
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

impl From<OnOff> for bool {
    fn from(v: OnOff) -> bool {
        matches!(v, OnOff::On)
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum SpeedArg {
    Standard,
    Sport,
}

#[derive(Clone, Copy, ValueEnum)]
enum FovArg {
    Wide,
    Normal,
    Narrow,
}

#[derive(Clone, Copy, ValueEnum)]
enum ImageParam {
    Brightness,
    Contrast,
    Saturation,
    Sharpness,
    Hue,
}

impl ImageParam {
    fn id(self) -> u32 {
        match self {
            ImageParam::Brightness => ctrl::BRIGHTNESS,
            ImageParam::Contrast => ctrl::CONTRAST,
            ImageParam::Saturation => ctrl::SATURATION,
            ImageParam::Sharpness => ctrl::SHARPNESS,
            ImageParam::Hue => ctrl::HUE,
        }
    }
}

fn parse_hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|e| e.to_string())
}

fn main() -> Result<()> {
    // Die quietly when piped into `head` etc. instead of panicking.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let cli = Cli::parse();
    let dev = match &cli.device {
        Some(path) => Device::open(path).with_context(|| format!("opening {path}"))?,
        None => Device::discover().context("discovering OBSBOT camera")?,
    };

    match cli.command {
        Command::Status => {
            println!("device:   {} ({})", dev.path().display(), dev.card());
            println!("pan:      {:+.1}°", dev.pan_deg()?);
            println!("tilt:     {:+.1}°", dev.tilt_deg()?);
            println!("zoom:     {}", dev.zoom()?);
            match dev.status() {
                Ok(s) => {
                    println!("state:    {}", if s.sleeping { "sleeping" } else { "awake" });
                    println!(
                        "tracking: {} ({})",
                        s.ai_mode.map_or("unknown", |m| m.label()),
                        match s.tracking_speed {
                            TrackingSpeed::Standard => "standard",
                            TrackingSpeed::Sport => "sport",
                        }
                    );
                    println!("hdr:      {}", if s.hdr { "on" } else { "off" });
                }
                Err(e) => println!("xu state: unavailable ({e})"),
            }
            println!("focus:    {}", auto_label(dev.get(ctrl::FOCUS_AUTO)?));
            println!(
                "exposure: {}",
                if dev.get(ctrl::AUTO_EXPOSURE)? == 1 { "manual" } else { "auto" }
            );
            println!("wb:       {}", auto_label(dev.get(ctrl::WHITE_BALANCE_AUTO)?));
        }
        Command::Pan { degrees } => dev.set_pan_deg(degrees)?,
        Command::Tilt { degrees } => dev.set_tilt_deg(degrees)?,
        Command::Zoom { level } => dev.set_zoom(level)?,
        Command::Center => dev.center()?,
        Command::Sleep { state } => dev.set_sleep(matches!(state, OnOff::On))?,
        Command::Track { mode } => {
            let mode = AiMode::from_label(&mode).with_context(|| {
                format!(
                    "unknown mode '{mode}'; valid: {}",
                    AiMode::ALL.map(|m| m.label()).join(", ")
                )
            })?;
            dev.set_ai_mode(mode)?;
        }
        Command::Speed { speed } => dev.set_tracking_speed(match speed {
            SpeedArg::Standard => TrackingSpeed::Standard,
            SpeedArg::Sport => TrackingSpeed::Sport,
        })?,
        Command::Preset { slot } => {
            dev.goto_preset(user_slot(slot)?)?;
        }
        Command::PresetSave { slot } => {
            let pan = dev.pan_deg()? as f32;
            let tilt = dev.tilt_deg()? as f32;
            let zoom = 1.0 + dev.zoom()? as f32 / 100.0 * 3.0;
            dev.save_preset(user_slot(slot)?, pan, tilt, zoom)?;
            println!("saved preset {slot}: pan {pan:+.1}° tilt {tilt:+.1}° zoom {zoom:.2}x");
        }
        Command::PresetDelete { slot } => dev.delete_preset(user_slot(slot)?)?,
        Command::Presets => {
            let entries = dev.preset_list()?;
            if entries.is_empty() {
                println!("no presets stored");
            }
            for e in entries {
                println!(
                    "preset {}: pan {:+.1}° tilt {:+.1}° zoom {:.2}x{}",
                    e.slot + 1,
                    e.pan_deg,
                    e.tilt_deg,
                    e.zoom,
                    if e.name.is_empty() { String::new() } else { format!("  ({})", e.name) }
                );
            }
        }
        Command::Recenter => dev.recenter()?,
        Command::Hdr { state } => dev.set_hdr(state.into())?,
        Command::FaceAe { state } => dev.set_face_ae(state.into())?,
        Command::Fov { fov } => dev.set_fov(match fov {
            FovArg::Wide => Fov::Wide,
            FovArg::Normal => Fov::Normal,
            FovArg::Narrow => Fov::Narrow,
        })?,
        Command::NoiseReduction { state } => dev.set_noise_reduction(state.into())?,
        Command::AutoSleep { seconds } => dev.set_auto_sleep_secs(seconds)?,
        Command::Image { param, value } => match value {
            Some(v) => dev.set(param.id(), v)?,
            None => println!("{}", dev.get(param.id())?),
        },
        Command::Wb { value } => {
            if value == "auto" {
                dev.set(ctrl::WHITE_BALANCE_AUTO, 1)?;
            } else {
                let kelvin: i32 = value.parse().context("expected 'auto' or Kelvin value")?;
                dev.set(ctrl::WHITE_BALANCE_AUTO, 0)?;
                dev.set(ctrl::WHITE_BALANCE_TEMPERATURE, kelvin)?;
            }
        }
        Command::Exposure { value } => {
            if value == "auto" {
                dev.set(ctrl::AUTO_EXPOSURE, 0)?;
            } else {
                let time: i32 = value.parse().context("expected 'auto' or exposure time")?;
                dev.set(ctrl::AUTO_EXPOSURE, 1)?;
                dev.set(ctrl::EXPOSURE_TIME_ABSOLUTE, time)?;
            }
        }
        Command::Focus { value } => {
            if value == "auto" {
                dev.set(ctrl::FOCUS_AUTO, 1)?;
            } else {
                let pos: i32 = value.parse().context("expected 'auto' or position 0-100")?;
                dev.set(ctrl::FOCUS_AUTO, 0)?;
                dev.set(ctrl::FOCUS_ABSOLUTE, pos)?;
            }
        }
        Command::AntiFlicker { value } => {
            let v = match value.as_str() {
                "off" => 0,
                "50hz" | "50" => 1,
                "60hz" | "60" => 2,
                _ => bail!("expected off, 50hz or 60hz"),
            };
            dev.set(ctrl::POWER_LINE_FREQUENCY, v)?;
        }
        Command::Raw { id, value } => match value {
            Some(v) => dev.set(id, v)?,
            None => println!("{}", dev.get(id)?),
        },
        Command::XuSend { selector, bytes } => {
            let cmd: Vec<u8> = bytes
                .split_whitespace()
                .map(|b| u8::from_str_radix(b, 16))
                .collect::<Result<_, _>>()
                .context("bytes must be hex like '16 02 02 00'")?;
            dev.xu_send(selector, &cmd)?;
        }
        Command::XuDump { selector } => {
            let data = dev.xu_get(selector)?;
            for (i, chunk) in data.chunks(16).enumerate() {
                let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                println!("{:04x}: {}", i * 16, hex.join(" "));
            }
        }
    }
    Ok(())
}

fn user_slot(slot: u32) -> Result<u32> {
    if !(1..=3).contains(&slot) {
        bail!("preset slot must be 1-3");
    }
    Ok(slot - 1)
}

fn auto_label(v: i32) -> &'static str {
    if v != 0 { "auto" } else { "manual" }
}
