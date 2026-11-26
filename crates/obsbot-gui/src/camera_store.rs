//! Camera settings that are saved and put back whenever the camera
//! connects, because the camera itself forgets some of them.

use crate::conf::{flag, parse_lines};

#[derive(Clone, PartialEq, Debug, Default)]
pub struct CameraSettings {
    pub zoom: i32,
    pub brightness: i32,
    pub contrast: i32,
    pub saturation: i32,
    pub sharpness: i32,
    pub wb_auto: bool,
    pub wb_temp: i32,
    pub exp_auto: bool,
    pub exp_time: i32,
    pub focus_auto: bool,
    pub anti_flicker: i32,
    pub hdr: bool,
    pub face_ae: bool,
    /// Index into the field-of-view choices.
    pub fov: usize,
    /// Tracking mode, by its label.
    pub ai_mode: Option<String>,
    pub sport_speed: bool,
}

impl CameraSettings {
    pub fn to_text(&self) -> String {
        format!(
            "zoom={}\nbrightness={}\ncontrast={}\nsaturation={}\nsharpness={}\nwb_auto={}\n\
             wb_temp={}\nexp_auto={}\nexp_time={}\nfocus_auto={}\nanti_flicker={}\nhdr={}\n\
             face_ae={}\nfov={}\nai_mode={}\nsport_speed={}\n",
            self.zoom,
            self.brightness,
            self.contrast,
            self.saturation,
            self.sharpness,
            self.wb_auto,
            self.wb_temp,
            self.exp_auto,
            self.exp_time,
            self.focus_auto,
            self.anti_flicker,
            self.hdr,
            self.face_ae,
            self.fov,
            self.ai_mode.as_deref().unwrap_or(""),
            self.sport_speed,
        )
    }

    /// `None` when the text holds no saved settings at all.
    pub fn from_text(text: &str) -> Option<Self> {
        let values = parse_lines(text);
        if values.is_empty() {
            return None;
        }
        let number = |key: &str, low: i32, high: i32, default: i32| -> i32 {
            values
                .get(key)
                .and_then(|value| value.parse::<i32>().ok())
                .map_or(default, |value| value.clamp(low, high))
        };
        Some(Self {
            zoom: number("zoom", 0, 100, 0),
            brightness: number("brightness", 0, 100, 50),
            contrast: number("contrast", 0, 100, 50),
            saturation: number("saturation", 0, 100, 50),
            sharpness: number("sharpness", 0, 100, 50),
            wb_auto: flag(&values, "wb_auto", true),
            wb_temp: number("wb_temp", 2000, 10000, 5000),
            exp_auto: flag(&values, "exp_auto", true),
            exp_time: number("exp_time", 1, 2500, 156),
            focus_auto: flag(&values, "focus_auto", true),
            anti_flicker: number("anti_flicker", 0, 2, 1),
            hdr: flag(&values, "hdr", false),
            face_ae: flag(&values, "face_ae", true),
            fov: number("fov", 0, 2, 0) as usize,
            ai_mode: values
                .get("ai_mode")
                .filter(|value| !value.is_empty())
                .map(|value| (*value).to_owned()),
            sport_speed: flag(&values, "sport_speed", false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_clamp_bad_values() {
        let settings = CameraSettings {
            zoom: 35,
            brightness: 60,
            contrast: 40,
            saturation: 55,
            sharpness: 70,
            wb_auto: false,
            wb_temp: 4300,
            exp_auto: false,
            exp_time: 200,
            focus_auto: true,
            anti_flicker: 2,
            hdr: true,
            face_ae: false,
            fov: 1,
            ai_mode: Some("Upper body".into()),
            sport_speed: true,
        };
        assert_eq!(
            CameraSettings::from_text(&settings.to_text()),
            Some(settings)
        );
        assert_eq!(CameraSettings::from_text(""), None);
        let odd = CameraSettings::from_text("zoom=900\nfov=7\nwb_temp=abc\nai_mode=\n").unwrap();
        assert_eq!(
            (odd.zoom, odd.fov, odd.wb_temp, odd.ai_mode),
            (100, 2, 5000, None)
        );
    }
}
