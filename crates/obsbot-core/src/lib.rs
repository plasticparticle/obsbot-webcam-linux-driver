//! Control library for OBSBOT cameras on Linux.
//!
//! Standard features (gimbal, zoom, image tuning) are driven through V4L2
//! controls on `/dev/video*`; the kernel `uvcvideo` driver does the USB work.
//! Proprietary features (AI tracking, presets, sleep) will be added via UVC
//! extension unit commands in a later phase.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

pub mod app_ipc;
pub mod xu;
pub use xu::{AiMode, Fov, Status, TrackingSpeed};

/// V4L2 control IDs verified present on the OBSBOT Tiny 2 Lite
/// (see docs/device-recon/ctrls.txt for ranges and defaults).
pub mod ctrl {
    pub const BRIGHTNESS: u32 = 0x0098_0900;
    pub const CONTRAST: u32 = 0x0098_0901;
    pub const SATURATION: u32 = 0x0098_0902;
    pub const HUE: u32 = 0x0098_0903;
    pub const WHITE_BALANCE_AUTO: u32 = 0x0098_090c;
    pub const RED_BALANCE: u32 = 0x0098_090e;
    pub const BLUE_BALANCE: u32 = 0x0098_090f;
    pub const GAIN: u32 = 0x0098_0913;
    pub const POWER_LINE_FREQUENCY: u32 = 0x0098_0918;
    pub const WHITE_BALANCE_TEMPERATURE: u32 = 0x0098_091a;
    pub const SHARPNESS: u32 = 0x0098_091b;
    pub const BACKLIGHT_COMPENSATION: u32 = 0x0098_091c;
    pub const AUTO_EXPOSURE: u32 = 0x009a_0901;
    pub const EXPOSURE_TIME_ABSOLUTE: u32 = 0x009a_0902;
    pub const PAN_ABSOLUTE: u32 = 0x009a_0908;
    pub const TILT_ABSOLUTE: u32 = 0x009a_0909;
    pub const FOCUS_ABSOLUTE: u32 = 0x009a_090a;
    pub const FOCUS_AUTO: u32 = 0x009a_090c;
    pub const ZOOM_ABSOLUTE: u32 = 0x009a_090d;
    pub const PAN_SPEED: u32 = 0x009a_0920;
    pub const TILT_SPEED: u32 = 0x009a_0921;
}

/// UVC pan/tilt absolute values are in arc-seconds.
const ARCSEC_PER_DEG: f64 = 3600.0;

pub const PAN_RANGE_DEG: (f64, f64) = (-130.0, 130.0);
pub const TILT_RANGE_DEG: (f64, f64) = (-90.0, 90.0);
pub const ZOOM_RANGE: (i32, i32) = (0, 100);

#[repr(C)]
struct V4l2Control {
    id: u32,
    value: i32,
}

#[repr(C)]
struct V4l2Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

const VIDIOC_QUERYCAP: libc::c_ulong = 0x8068_5600;
const VIDIOC_G_CTRL: libc::c_ulong = 0xC008_561B;
const VIDIOC_S_CTRL: libc::c_ulong = 0xC008_561C;

fn ioctl<T>(file: &File, request: libc::c_ulong, arg: &mut T) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(file.as_raw_fd(), request as _, arg as *mut T) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// An open OBSBOT camera control node.
pub struct Device {
    file: File,
    path: PathBuf,
    card: String,
    seq: std::cell::Cell<u16>,
}

impl Device {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let mut cap: V4l2Capability = unsafe { std::mem::zeroed() };
        ioctl(&file, VIDIOC_QUERYCAP, &mut cap)?;
        let card = String::from_utf8_lossy(&cap.card)
            .trim_end_matches('\0')
            .to_string();
        Ok(Self { file, path, card, seq: std::cell::Cell::new(1) })
    }

    /// Find the first video node whose card name contains "OBSBOT".
    pub fn discover() -> io::Result<Self> {
        let mut nodes: Vec<PathBuf> = std::fs::read_dir("/dev")?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("video"))
            })
            .collect();
        nodes.sort();
        for path in nodes {
            if let Ok(dev) = Self::open(&path) {
                // Only the first node of the camera exposes controls.
                if dev.card.contains("OBSBOT") && dev.get(ctrl::PAN_ABSOLUTE).is_ok() {
                    return Ok(dev);
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no OBSBOT camera with control support found",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn card(&self) -> &str {
        &self.card
    }

    pub fn get(&self, id: u32) -> io::Result<i32> {
        let mut c = V4l2Control { id, value: 0 };
        ioctl(&self.file, VIDIOC_G_CTRL, &mut c)?;
        Ok(c.value)
    }

    pub fn set(&self, id: u32, value: i32) -> io::Result<()> {
        let mut c = V4l2Control { id, value };
        ioctl(&self.file, VIDIOC_S_CTRL, &mut c)
    }

    pub fn pan_deg(&self) -> io::Result<f64> {
        Ok(self.get(ctrl::PAN_ABSOLUTE)? as f64 / ARCSEC_PER_DEG)
    }

    pub fn set_pan_deg(&self, deg: f64) -> io::Result<()> {
        let deg = deg.clamp(PAN_RANGE_DEG.0, PAN_RANGE_DEG.1);
        self.set(ctrl::PAN_ABSOLUTE, (deg * ARCSEC_PER_DEG) as i32)
    }

    pub fn tilt_deg(&self) -> io::Result<f64> {
        Ok(self.get(ctrl::TILT_ABSOLUTE)? as f64 / ARCSEC_PER_DEG)
    }

    pub fn set_tilt_deg(&self, deg: f64) -> io::Result<()> {
        let deg = deg.clamp(TILT_RANGE_DEG.0, TILT_RANGE_DEG.1);
        self.set(ctrl::TILT_ABSOLUTE, (deg * ARCSEC_PER_DEG) as i32)
    }

    pub fn zoom(&self) -> io::Result<i32> {
        self.get(ctrl::ZOOM_ABSOLUTE)
    }

    pub fn set_zoom(&self, level: i32) -> io::Result<()> {
        self.set(ctrl::ZOOM_ABSOLUTE, level.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1))
    }

    /// Recenter the gimbal and reset zoom.
    pub fn center(&self) -> io::Result<()> {
        self.set(ctrl::PAN_ABSOLUTE, 0)?;
        self.set(ctrl::TILT_ABSOLUTE, 0)?;
        self.set(ctrl::ZOOM_ABSOLUTE, 0)
    }

    // ---- Proprietary features via the UVC extension unit ----

    fn xu_len(&self, selector: u8) -> io::Result<usize> {
        let mut buf = [0u8; 2];
        xu::xu_io(&self.file, selector, xu::UVC_GET_LEN, &mut buf)?;
        Ok(u16::from_le_bytes(buf) as usize)
    }

    /// Send a command, zero-padded to the selector's reported length.
    pub fn xu_send(&self, selector: u8, cmd: &[u8]) -> io::Result<()> {
        let len = self.xu_len(selector)?;
        if cmd.len() > len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "command too long"));
        }
        let mut buf = vec![0u8; len];
        buf[..cmd.len()].copy_from_slice(cmd);
        xu::xu_io(&self.file, selector, xu::UVC_SET_CUR, &mut buf)
    }

    /// Read the current value block of a selector.
    pub fn xu_get(&self, selector: u8) -> io::Result<Vec<u8>> {
        let len = self.xu_len(selector)?;
        let mut buf = vec![0u8; len];
        xu::xu_io(&self.file, selector, xu::UVC_GET_CUR, &mut buf)?;
        Ok(buf)
    }

    /// Decoded proprietary status (sleep, HDR, AI mode, tracking speed).
    pub fn status(&self) -> io::Result<Status> {
        let raw = self.xu_get(xu::SEL_SIMPLE)?;
        if raw.len() < 0x22 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "status block too short"));
        }
        Ok(Status::decode(&raw))
    }

    pub fn set_ai_mode(&self, mode: AiMode) -> io::Result<()> {
        let (m, n) = mode.code();
        self.xu_send(xu::SEL_SIMPLE, &[0x16, 0x02, m, n])
    }

    pub fn set_hdr(&self, on: bool) -> io::Result<()> {
        self.xu_send(xu::SEL_SIMPLE, &[0x01, 0x01, on as u8])
    }

    pub fn set_face_ae(&self, on: bool) -> io::Result<()> {
        self.xu_send(xu::SEL_SIMPLE, &[0x03, 0x01, on as u8])
    }

    pub fn set_fov(&self, fov: Fov) -> io::Result<()> {
        let v = match fov {
            Fov::Wide => 1,
            Fov::Normal => 2,
            Fov::Narrow => 3,
        };
        self.xu_send(xu::SEL_SIMPLE, &[0x04, 0x01, v])
    }

    pub fn set_noise_reduction(&self, on: bool) -> io::Result<()> {
        self.xu_send(xu::SEL_SIMPLE, &[0x0a, 0x01, on as u8])
    }

    /// Auto-sleep timer in seconds (0 disables, subject to firmware support).
    pub fn set_auto_sleep_secs(&self, secs: u16) -> io::Result<()> {
        let [lo, hi] = secs.to_le_bytes();
        self.xu_send(xu::SEL_SIMPLE, &[0x0b, 0x02, lo, hi])
    }

    /// Send a framed V3 vendor command (selector 0x02).
    pub fn send_frame(&self, cmd: (u16, u8), payload: &[u8]) -> io::Result<()> {
        let seq = self.seq.get();
        self.seq.set(seq.wrapping_add(1));
        let frame = xu::frame::build(seq, cmd, payload);
        self.xu_send(xu::SEL_FRAMED, &frame)
    }

    pub fn set_sleep(&self, sleep: bool) -> io::Result<()> {
        self.send_frame(xu::frame::CMD_SLEEP, &[sleep as u8, 0, 0, 0])
    }

    /// Return the gimbal to its home pose (firmware recenter).
    pub fn recenter(&self) -> io::Result<()> {
        self.send_frame(xu::frame::CMD_RECENTER, &[0; 6])
    }

    /// Continuous gimbal motion in degrees/second (0,0,0 stops).
    pub fn gimbal_speed(&self, pitch_dps: f32, yaw_dps: f32) -> io::Result<()> {
        let mut p = Vec::with_capacity(12);
        p.extend_from_slice(&0.0f32.to_le_bytes()); // roll
        p.extend_from_slice(&pitch_dps.to_le_bytes());
        p.extend_from_slice(&yaw_dps.to_le_bytes());
        self.send_frame(xu::frame::CMD_GIMBAL_SPEED, &p)
    }

    pub fn set_tracking_speed(&self, speed: TrackingSpeed) -> io::Result<()> {
        let v = match speed {
            TrackingSpeed::Standard => 0u8,
            TrackingSpeed::Sport => 2,
        };
        self.send_frame(xu::frame::CMD_TRACK_SPEED, &[v])
    }

    fn preset_payload(slot: u32, pose: [f32; 4], sentinel: f32) -> Vec<u8> {
        let mut p = slot.to_le_bytes().to_vec();
        for v in pose {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p.extend_from_slice(&sentinel.to_le_bytes());
        p
    }

    /// Move to preset slot 0..=2.
    pub fn goto_preset(&self, slot: u32) -> io::Result<()> {
        check_slot(slot)?;
        let mut p = slot.to_le_bytes().to_vec();
        for _ in 0..4 {
            p.extend_from_slice(&1.0f32.to_le_bytes());
        }
        self.send_frame(xu::frame::CMD_PRESET_RECALL, &p)
    }

    /// Save the given pose (UVC conventions: pan°, tilt° positive-up, zoom
    /// ratio) to a slot, creating or overwriting it. The wire format uses the
    /// vendor pitch convention (positive-down), hence the tilt negation.
    pub fn save_preset(&self, slot: u32, pan: f32, tilt: f32, zoom: f32) -> io::Result<()> {
        check_slot(slot)?;
        let occupied = self.preset_list()?.iter().any(|e| e.slot == slot);
        let cmd = if occupied { xu::frame::CMD_PRESET_UPDATE } else { xu::frame::CMD_PRESET_ADD };
        self.send_frame(cmd, &Self::preset_payload(slot, [pan, -tilt, 0.0, zoom], -1000.0))
    }

    pub fn delete_preset(&self, slot: u32) -> io::Result<()> {
        check_slot(slot)?;
        self.send_frame(xu::frame::CMD_PRESET_DELETE, &slot.to_le_bytes())
    }

    /// Read stored presets via flat selectors 12 (list) and 13 (entry cursor).
    pub fn preset_list(&self) -> io::Result<Vec<xu::PresetEntry>> {
        let list = self.xu_get(12)?;
        let count = list[0] as usize;
        let mut entries = Vec::with_capacity(count);
        if count == 0 {
            return Ok(entries);
        }
        // Echoing the list block back resets selector 13's read cursor.
        let echo = list.clone();
        self.xu_send(12, &echo)?;
        for _ in 0..count {
            let raw = self.xu_get(13)?;
            if raw[0] == 0x02 {
                break; // cursor exhausted
            }
            entries.push(xu::PresetEntry::decode(&raw));
        }
        Ok(entries)
    }
}

fn check_slot(slot: u32) -> io::Result<()> {
    if slot > 2 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "preset slot must be 0-2"));
    }
    Ok(())
}
