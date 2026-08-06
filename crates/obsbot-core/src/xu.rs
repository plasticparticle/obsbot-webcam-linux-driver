//! Proprietary OBSBOT commands via UVC Extension Unit queries.
//!
//! Protocol notes: docs/protocol.md. All commands go to XU unit 2 on the
//! video node; short commands use selector 0x06, framed ones selector 0x02.

use std::fmt;
use std::io;

/// XU unit id of the OBSBOT vendor extension.
pub const UNIT: u8 = 2;
pub const SEL_FRAMED: u8 = 0x02;
pub const SEL_SIMPLE: u8 = 0x06;

pub const UVC_SET_CUR: u8 = 0x01;
pub const UVC_GET_CUR: u8 = 0x81;
pub const UVC_GET_LEN: u8 = 0x85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMode {
    NoTracking,
    NormalTracking,
    UpperBody,
    CloseUp,
    Headless,
    LowerBody,
    DeskMode,
    Whiteboard,
    Hand,
    Group,
}

impl AiMode {
    pub const ALL: [AiMode; 10] = [
        AiMode::NoTracking,
        AiMode::NormalTracking,
        AiMode::UpperBody,
        AiMode::CloseUp,
        AiMode::Headless,
        AiMode::LowerBody,
        AiMode::DeskMode,
        AiMode::Whiteboard,
        AiMode::Hand,
        AiMode::Group,
    ];

    pub fn code(self) -> (u8, u8) {
        match self {
            AiMode::NoTracking => (0, 0),
            AiMode::Group => (1, 0),
            AiMode::NormalTracking => (2, 0),
            AiMode::UpperBody => (2, 1),
            AiMode::CloseUp => (2, 2),
            AiMode::Headless => (2, 3),
            AiMode::LowerBody => (2, 4),
            AiMode::Whiteboard => (4, 0),
            AiMode::DeskMode => (5, 0),
            AiMode::Hand => (6, 0),
        }
    }

    pub fn from_code(m: u8, n: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.code() == (m, n))
    }

    pub fn label(self) -> &'static str {
        match self {
            AiMode::NoTracking => "off",
            AiMode::NormalTracking => "normal",
            AiMode::UpperBody => "upper-body",
            AiMode::CloseUp => "close-up",
            AiMode::Headless => "headless",
            AiMode::LowerBody => "lower-body",
            AiMode::DeskMode => "desk",
            AiMode::Whiteboard => "whiteboard",
            AiMode::Hand => "hand",
            AiMode::Group => "group",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.label() == s)
    }
}

impl fmt::Display for AiMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fov {
    Wide,   // 86°
    Normal, // 78°
    Narrow, // 65°
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingSpeed {
    Standard,
    Sport,
}

/// Decoded selector-0x06 status block.
#[derive(Debug, Clone, Copy)]
pub struct Status {
    pub sleeping: bool,
    pub hdr: bool,
    pub ai_mode: Option<AiMode>,
    pub tracking_speed: TrackingSpeed,
}

impl Status {
    pub fn decode(b: &[u8]) -> Self {
        Status {
            sleeping: b[0x02] == 1,
            hdr: b[0x06] != 0,
            ai_mode: AiMode::from_code(b[0x18], b[0x1c]),
            tracking_speed: if b[0x21] == 2 {
                TrackingSpeed::Sport
            } else {
                TrackingSpeed::Standard
            },
        }
    }
}

/// A stored gimbal preset, read via flat selectors 12/13.
#[derive(Debug, Clone)]
pub struct PresetEntry {
    pub slot: u32,
    /// Tilt in degrees (wire: hundredths, pitch-positive-down convention).
    pub tilt_deg: f32,
    /// Pan in degrees.
    pub pan_deg: f32,
    /// Zoom ratio (1.0 = no zoom).
    pub zoom: f32,
    pub name: String,
}

impl PresetEntry {
    pub fn decode(raw: &[u8]) -> Self {
        let pitch = i16::from_le_bytes([raw[4], raw[5]]) as f32 / 100.0;
        let yaw = i16::from_le_bytes([raw[6], raw[7]]) as f32 / 100.0;
        let name_raw: Vec<u8> =
            raw[10..].iter().copied().take_while(|&b| b != 0).collect();
        PresetEntry {
            slot: raw[1] as u32,
            tilt_deg: -pitch,
            pan_deg: yaw,
            zoom: raw[8] as f32 / 100.0,
            name: String::from_utf8_lossy(&name_raw).into_owned(),
        }
    }
}

/// Framed "V3" vendor commands on selector 0x02.
///
/// Frame: `AA | flags | seq u16 | len=12 u16 | token u16 | sender | receiver |
/// cmd u16 | nested{len2 u16, token2 u16, payload}` — tokens are CRC-16/USB
/// over the covered bytes with the token field zeroed. Protocol decoded by the
/// obsbot-mcp project (MIT); see docs/protocol.md.
pub mod frame {
    pub const SENDER_HOST: u8 = 0x0a;
    pub const RECV_CAMERA: u8 = 0x02;
    pub const RECV_GIMBAL_HOME: u8 = 0x03;
    pub const RECV_AI: u8 = 0x04;

    /// (cmd, receiver) pairs for the commands we use.
    pub const CMD_SLEEP: (u16, u8) = (0xA0C2, RECV_CAMERA);
    pub const CMD_RECENTER: (u16, u8) = (0x00C3, RECV_GIMBAL_HOME);
    pub const CMD_GIMBAL_SPEED: (u16, u8) = (0x6484, RECV_AI);
    pub const CMD_GIMBAL_MOVE: (u16, u8) = (0x6444, RECV_AI);
    pub const CMD_TRACK_SPEED: (u16, u8) = (0x0CC4, RECV_AI);
    pub const CMD_PRESET_ADD: (u16, u8) = (0x3944, RECV_AI);
    pub const CMD_PRESET_UPDATE: (u16, u8) = (0x3E04, RECV_AI);
    pub const CMD_PRESET_RECALL: (u16, u8) = (0x39C4, RECV_AI);
    pub const CMD_PRESET_DELETE: (u16, u8) = (0x3984, RECV_AI);

    /// CRC-16/USB: poly 0xA001 (reflected 0x8005), init 0xFFFF, xorout 0xFFFF.
    pub fn crc16_usb(data: &[u8]) -> u16 {
        let mut crc: u16 = 0xFFFF;
        for &b in data {
            crc ^= b as u16;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
            }
        }
        crc ^ 0xFFFF
    }

    /// Build a 60-byte SET frame.
    pub fn build(seq: u16, (cmd, receiver): (u16, u8), payload: &[u8]) -> [u8; 60] {
        assert!(payload.len() <= 44, "payload too long for a single frame");
        let mut f = [0u8; 60];
        f[0] = 0xAA;
        f[1] = 0x25;
        f[2..4].copy_from_slice(&seq.to_le_bytes());
        f[4..6].copy_from_slice(&12u16.to_le_bytes());
        f[8] = SENDER_HOST;
        f[9] = receiver;
        f[10..12].copy_from_slice(&cmd.to_le_bytes());
        let token = crc16_usb(&f[0..12]);
        f[6..8].copy_from_slice(&token.to_le_bytes());
        if !payload.is_empty() {
            let len2 = payload.len() as u16;
            f[12..14].copy_from_slice(&len2.to_le_bytes());
            f[16..16 + payload.len()].copy_from_slice(payload);
            let token2 = crc16_usb(&f[12..16 + payload.len()]);
            f[14..16].copy_from_slice(&token2.to_le_bytes());
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::frame::{self, build};

    /// Frames captured from OBSBOT Center by the community projects; our
    /// builder must reproduce them byte-for-byte given the same sequence nr.
    #[test]
    fn builder_matches_captured_frames() {
        let cases: &[(u16, (u16, u8), Vec<u8>, [u8; 20])] = &[
            (
                0x00a5,
                frame::CMD_SLEEP,
                vec![0, 0, 0, 0],
                [
                    0xaa, 0x25, 0xa5, 0x00, 0x0c, 0x00, 0x5f, 0xef, 0x0a, 0x02, 0xc2, 0xa0, 0x04,
                    0x00, 0xbe, 0x07, 0x00, 0x00, 0x00, 0x00,
                ],
            ),
            (
                0x0042,
                frame::CMD_SLEEP,
                vec![1, 0, 0, 0],
                [
                    0xaa, 0x25, 0x42, 0x00, 0x0c, 0x00, 0xea, 0x63, 0x0a, 0x02, 0xc2, 0xa0, 0x04,
                    0x00, 0xbf, 0xfb, 0x01, 0x00, 0x00, 0x00,
                ],
            ),
            (
                0x0021,
                frame::CMD_TRACK_SPEED,
                vec![2],
                [
                    0xaa, 0x25, 0x21, 0x00, 0x0c, 0x00, 0xfa, 0x0e, 0x0a, 0x04, 0xc4, 0x0c, 0x01,
                    0x00, 0x67, 0xfe, 0x02, 0x00, 0x00, 0x00,
                ],
            ),
        ];
        for (seq, cmd, payload, expected) in cases {
            let f = build(*seq, *cmd, payload);
            assert_eq!(&f[..20], expected, "frame mismatch for cmd {cmd:?}");
        }
    }

    #[test]
    fn recall_frame_matches_capture() {
        let mut payload = 1u32.to_le_bytes().to_vec();
        for _ in 0..4 {
            payload.extend_from_slice(&1.0f32.to_le_bytes());
        }
        let f = build(0x001a, frame::CMD_PRESET_RECALL, &payload);
        let expected: [u8; 20] = [
            0xaa, 0x25, 0x1a, 0x00, 0x0c, 0x00, 0x4b, 0x03, 0x0a, 0x04, 0xc4, 0x39, 0x14, 0x00,
            0xeb, 0x2a, 0x01, 0x00, 0x00, 0x00,
        ];
        assert_eq!(&f[..20], &expected);
        // appendix: idx floats land at 20.. as four 1.0f32
        assert_eq!(&f[20..24], &1.0f32.to_le_bytes());
    }
}

#[repr(C)]
struct UvcXuControlQuery {
    unit: u8,
    selector: u8,
    query: u8,
    size: u16,
    data: *mut u8,
}

const UVCIOC_CTRL_QUERY: libc::c_ulong = 0xC010_7521;

pub(crate) fn xu_io(
    file: &std::fs::File,
    selector: u8,
    query: u8,
    data: &mut [u8],
) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let mut q = UvcXuControlQuery {
        unit: UNIT,
        selector,
        query,
        size: data.len() as u16,
        data: data.as_mut_ptr(),
    };
    let ret = unsafe { libc::ioctl(file.as_raw_fd(), UVCIOC_CTRL_QUERY as _, &mut q) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
