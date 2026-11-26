//! Effects state: the catalogue of filters, the user's settings, and the
//! worker process that renders them into the virtual camera.

use crate::backgrounds::BackdropLibrary;
use crate::conf::{config_dir, flag, one_line, parse_lines, write_atomically};
use crate::luts::LutLibrary;
use crate::paths;
use crate::positions::{corners_text, Corners};
use crate::preview::Preview;
use crate::profiles::ProfileStore;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

/// A selectable effect. `key` is the name the effects worker understands.
pub struct Choice {
    pub key: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub hint: &'static str,
}

const fn choice(
    key: &'static str,
    label: &'static str,
    icon: &'static str,
    hint: &'static str,
) -> Choice {
    Choice {
        key,
        label,
        icon,
        hint,
    }
}

/// A colour look with the two swatch colours shown on its tile.
pub struct Look {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub swatch: [[u8; 3]; 2],
}

const fn look(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    swatch: [[u8; 3]; 2],
) -> Look {
    Look {
        key,
        label,
        hint,
        swatch,
    }
}

/// A named colour, used for lips and eyes.
pub struct Tint {
    pub key: &'static str,
    pub label: &'static str,
    pub rgb: [u8; 3],
}

const fn tint(key: &'static str, label: &'static str, rgb: [u8; 3]) -> Tint {
    Tint { key, label, rgb }
}

pub const BACKGROUNDS: [Choice; 3] = [
    choice("off", "Off", "", "Show your real background"),
    choice("blur", "Blur", "", "Soften everything behind you"),
    choice(
        "image",
        "Image",
        "",
        "Replace the background with a picture",
    ),
];

pub const LOOKS: [Look; 12] = [
    look(
        "none",
        "Original",
        "No colour grading",
        [[120, 130, 155], [70, 78, 100]],
    ),
    look(
        "golden",
        "Golden",
        "Warm, sunlit tones",
        [[255, 204, 110], [214, 112, 48]],
    ),
    look(
        "arctic",
        "Arctic",
        "Cool and crisp",
        [[176, 222, 255], [72, 118, 196]],
    ),
    look(
        "pop",
        "Pop",
        "Punchy colour and contrast",
        [[255, 92, 138], [255, 200, 60]],
    ),
    look(
        "vintage",
        "Vintage",
        "Faded, nostalgic film",
        [[224, 190, 136], [112, 130, 116]],
    ),
    look(
        "cinema",
        "Cinema",
        "Teal shadows, orange highlights",
        [[255, 170, 90], [20, 110, 130]],
    ),
    look(
        "sunset",
        "Sunset",
        "Deep orange and purple",
        [[255, 150, 84], [140, 60, 160]],
    ),
    look(
        "dream",
        "Dream",
        "Soft pastel glow",
        [[255, 210, 236], [166, 178, 255]],
    ),
    look(
        "neon",
        "Neon",
        "Electric magenta and blue",
        [[255, 70, 220], [60, 90, 255]],
    ),
    look(
        "mono",
        "Mono",
        "Gentle black and white",
        [[222, 222, 222], [112, 112, 112]],
    ),
    look(
        "noir",
        "Noir",
        "High-contrast black and white",
        [[245, 245, 245], [12, 12, 12]],
    ),
    look(
        "sepia",
        "Sepia",
        "Warm brown monochrome",
        [[230, 196, 144], [108, 72, 40]],
    ),
];

pub const EYE_COLORS: [Tint; 7] = [
    tint("natural", "Natural", [0, 0, 0]),
    tint("blue", "Blue", [74, 134, 214]),
    tint("green", "Green", [78, 160, 110]),
    tint("hazel", "Hazel", [170, 128, 70]),
    tint("gray", "Gray", [150, 160, 170]),
    tint("violet", "Violet", [150, 100, 200]),
    tint("amber", "Amber", [214, 150, 50]),
];

pub const LIP_COLORS: [Tint; 7] = [
    tint("none", "Natural", [0, 0, 0]),
    tint("rose", "Rose", [214, 84, 110]),
    tint("red", "Red", [196, 30, 58]),
    tint("coral", "Coral", [240, 110, 90]),
    tint("berry", "Berry", [150, 30, 90]),
    tint("plum", "Plum", [110, 40, 95]),
    tint("nude", "Nude", [190, 120, 105]),
];

pub const STICKERS: [Choice; 12] = [
    choice("sunglasses", "Shades", "🕶", "Classic dark sunglasses"),
    choice(
        "heart_glasses",
        "Love",
        "😍",
        "Pink heart-shaped sunglasses",
    ),
    choice(
        "dog",
        "Puppy",
        "🐶",
        "Floppy ears and a nose — open your mouth for the tongue",
    ),
    choice("cat", "Kitty", "🐱", "Cat ears, nose and whiskers"),
    choice("bunny", "Bunny", "🐰", "Tall ears, pink nose and teeth"),
    choice("moustache", "Stache", "👨", "A dashing handlebar moustache"),
    choice("crown", "Crown", "👑", "A jewelled gold crown"),
    choice("party_hat", "Party", "🎉", "A striped party hat"),
    choice("top_hat", "Top hat", "🎩", "A formal top hat"),
    choice("halo", "Angel", "😇", "A glowing halo"),
    choice("horns", "Devil", "😈", "Little red horns"),
    choice("clown_nose", "Red nose", "🔴", "A shiny red nose"),
];

pub const DISTORTIONS: [Choice; 9] = [
    choice("none", "None", "🚫", "No distortion"),
    choice("big_head", "Big head", "🎈", "Inflate your whole head"),
    choice("tiny_face", "Tiny face", "👶", "Shrink your features"),
    choice("bug_eyes", "Bug eyes", "👀", "Huge cartoon eyes"),
    choice("alien", "Alien", "👽", "Big eyes, big brain, tiny chin"),
    choice("big_mouth", "Big mouth", "👄", "An enormous mouth"),
    choice("big_nose", "Big nose", "👃", "A very prominent nose"),
    choice("chipmunk", "Chipmunk", "🐹", "Puffy cheeks"),
    choice("swirl", "Swirl", "🌀", "Twist your face into a spiral"),
];

pub const PAINTS: [Choice; 8] = [
    choice("none", "None", "🚫", "No face paint"),
    choice("hero", "Hero", "🎭", "A masked-hero eye mask"),
    choice(
        "anon",
        "Anon",
        "👺",
        "A pale smiling mask with a moustache and goatee",
    ),
    choice("clown", "Clown", "😜", "Full clown make-up"),
    choice("skull", "Skull", "💀", "Skeleton face paint"),
    choice("zombie", "Zombie", "👹", "Undead green skin"),
    choice("gold", "Gold", "🏆", "A golden statue face"),
    choice("freckles", "Freckles", "🌸", "Freckles and rosy cheeks"),
];

/// What the anonymiser hides. The first entry switches it off.
pub const ANONYMISE: [Choice; 3] = [
    choice("off", "Off", "", "Your face is shown normally"),
    choice("eyes", "Eyes", "", "Covers a band across both eyes"),
    choice("face", "Face", "", "Covers your whole face"),
];

/// How the anonymiser hides it; `hint` is the adjective used in summaries.
pub const ANONYMISE_STYLES: [Choice; 3] = [
    choice("pixel", "Pixels", "", "Pixelated"),
    choice("blur", "Blur", "", "Blurred"),
    choice("bar", "Black", "", "Blacked-out"),
];

/// How the name badge is drawn; `hint` describes the look.
pub const BADGE_STYLES: [Choice; 5] = [
    choice("bar", "Bar", "", "Dark panel with a coloured bar"),
    choice(
        "underline",
        "Line",
        "",
        "Dark panel with a coloured underline",
    ),
    choice("pill", "Pill", "", "Rounded panel with a coloured dot"),
    choice("block", "Block", "", "Solid panel in the badge colour"),
    choice("plain", "Plain", "", "Text only, with a soft shadow"),
];

pub const BADGE_LAYOUTS: [Choice; 2] = [
    choice(
        "stacked",
        "Title below",
        "",
        "Name with the title underneath",
    ),
    choice("inline", "One line", "", "Name and title side by side"),
];

/// Where the badge sits, as rows of a 3×2 picker (top row first).
pub const BADGE_POSITIONS: [&str; 6] = [
    "top-left",
    "top-center",
    "top-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];

pub const BADGE_COLORS: [Tint; 8] = [
    tint("purple", "Purple", [116, 105, 255]),
    tint("blue", "Blue", [70, 150, 255]),
    tint("teal", "Teal", [40, 190, 190]),
    tint("green", "Green", [67, 211, 145]),
    tint("amber", "Amber", [255, 184, 92]),
    tint("red", "Red", [246, 105, 116]),
    tint("pink", "Pink", [255, 110, 190]),
    tint("white", "White", [240, 240, 245]),
];

/// The look and placement of the name badge.
#[derive(Clone, PartialEq, Debug)]
pub struct BadgeDesign {
    /// Font family; empty uses the system's default sans-serif.
    pub font: String,
    pub bold: bool,
    pub caps: bool,
    pub size: f32,
    pub position: usize,
    pub style: usize,
    pub layout: usize,
    pub color: usize,
    /// How solid the panel behind the text is.
    pub opacity: f32,
    /// Distance from the edge of the picture.
    pub margin: f32,
}

impl Default for BadgeDesign {
    fn default() -> Self {
        Self {
            font: String::new(),
            bold: true,
            caps: false,
            size: 0.35,
            position: 3,
            style: 0,
            layout: 0,
            color: 0,
            opacity: 0.8,
            margin: 0.35,
        }
    }
}

impl BadgeDesign {
    fn to_config(&self) -> String {
        let [r, g, b] = BADGE_COLORS[self.color].rgb;
        format!(
            "badge_font={}\nbadge_bold={}\nbadge_caps={}\nbadge_size={}\nbadge_position={}\n\
             badge_style={}\nbadge_layout={}\nbadge_accent={r},{g},{b}\nbadge_opacity={}\n\
             badge_margin={}\n",
            one_line(&self.font),
            self.bold,
            self.caps,
            self.size,
            BADGE_POSITIONS[self.position],
            BADGE_STYLES[self.style].key,
            BADGE_LAYOUTS[self.layout].key,
            self.opacity,
            self.margin,
        )
    }

    fn from_values(values: &std::collections::HashMap<&str, &str>) -> Self {
        let defaults = Self::default();
        let unit = |key: &str, default: f32| -> f32 {
            values
                .get(key)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .map_or(default, |value| value.clamp(0.0, 1.0))
        };
        let pick = |key: &str, keys: &[&str], default: usize| -> usize {
            values
                .get(key)
                .and_then(|value| keys.iter().position(|candidate| candidate == value))
                .unwrap_or(default)
        };
        let accent = values.get("badge_accent").copied().unwrap_or_default();
        Self {
            font: values
                .get("badge_font")
                .map(|value| (*value).to_owned())
                .unwrap_or_default(),
            bold: flag(values, "badge_bold", defaults.bold),
            caps: flag(values, "badge_caps", defaults.caps),
            size: unit("badge_size", defaults.size),
            position: pick("badge_position", &BADGE_POSITIONS, defaults.position),
            style: pick("badge_style", &BADGE_STYLES.each_ref().map(|c| c.key), 0),
            layout: pick("badge_layout", &BADGE_LAYOUTS.each_ref().map(|c| c.key), 0),
            color: BADGE_COLORS
                .iter()
                .position(|tint| {
                    let [r, g, b] = tint.rgb;
                    format!("{r},{g},{b}") == accent
                })
                .unwrap_or(0),
            opacity: unit("badge_opacity", defaults.opacity),
            margin: unit("badge_margin", defaults.margin),
        }
    }
}

/// Expression-triggered effects: `label` is what you do, `hint` what happens.
/// Colours for the question marks and the idea stars.
pub const MAGIC_COLORS: [Tint; 8] = [
    tint("blue", "Blue", [120, 190, 255]),
    tint("gold", "Gold", [255, 205, 60]),
    tint("pink", "Pink", [255, 130, 200]),
    tint("green", "Green", [110, 225, 120]),
    tint("purple", "Purple", [180, 120, 255]),
    tint("red", "Red", [245, 80, 80]),
    tint("orange", "Orange", [255, 150, 50]),
    tint("white", "White", [245, 245, 245]),
];

/// Positions in `TRIGGERS` of the effects that have a colour setting.
pub const QUESTIONS: usize = 2;
pub const IDEA: usize = 6;
/// Position of the storm in `TRIGGERS`; it has a sensitivity setting.
pub const STORM: usize = 4;
/// Triggers from here on watch your hands and need the hand-tracking model.
pub const FIRST_GESTURE: usize = 5;

pub const TRIGGERS: [Choice; 8] = [
    choice("hearts", "Smile", "😊", "Hearts float up around you"),
    choice("rainbow", "Open your mouth", "😮", "A rainbow pours out"),
    choice(
        "questions",
        "Raise your eyebrows",
        "❓",
        "Question marks rise from your head",
    ),
    choice("kisses", "Blow a kiss", "😘", "Little hearts fly away"),
    choice(
        "storm",
        "Look concerned",
        "😟",
        "A thundercloud rains on you",
    ),
    choice(
        "applause",
        "Clap your hands",
        "👏",
        "Clapping hands rise around you",
    ),
    choice(
        "idea",
        "Raise your index finger",
        "💡",
        "Great idea: stars explode from your head",
    ),
    choice(
        "fireworks",
        "Shake your fist",
        "🎆",
        "Celebrate: fireworks go off around you",
    ),
];

/// Coloured gels for the rim light.
pub const RIM_COLORS: [Tint; 6] = [
    tint("white", "White", [240, 240, 245]),
    tint("warm", "Warm", [255, 190, 120]),
    tint("blue", "Blue", [70, 150, 255]),
    tint("pink", "Pink", [255, 90, 190]),
    tint("purple", "Purple", [170, 90, 255]),
    tint("green", "Green", [110, 255, 130]),
];

/// A one-click lighting set-up. The first entry switches the lights off.
pub struct LightPreset {
    pub label: &'static str,
    pub icon: &'static str,
    pub hint: &'static str,
    pub key: f32,
    pub x: f32,
    pub y: f32,
    pub warmth: f32,
    pub fill: f32,
    pub rim: f32,
    pub rim_color: usize,
    pub dim: f32,
}

#[allow(clippy::too_many_arguments)]
const fn lights(
    label: &'static str,
    icon: &'static str,
    hint: &'static str,
    key: f32,
    (x, y): (f32, f32),
    warmth: f32,
    fill: f32,
    (rim, rim_color): (f32, usize),
    dim: f32,
) -> LightPreset {
    LightPreset {
        label,
        icon,
        hint,
        key,
        x,
        y,
        warmth,
        fill,
        rim,
        rim_color,
        dim,
    }
}

pub const LIGHT_X: f32 = -0.5;
pub const LIGHT_Y: f32 = 0.5;

pub const LIGHT_PRESETS: [LightPreset; 8] = [
    lights(
        "Off",
        "🚫",
        "No virtual lights",
        0.0,
        (LIGHT_X, LIGHT_Y),
        0.0,
        0.0,
        (0.0, 0),
        0.0,
    ),
    lights(
        "Studio",
        "📷",
        "Classic three-point lighting",
        0.5,
        (-0.5, 0.5),
        0.1,
        0.35,
        (0.3, 0),
        0.15,
    ),
    lights(
        "Soft",
        "💫",
        "Even, flattering front light",
        0.35,
        (0.0, 0.3),
        0.15,
        0.6,
        (0.0, 0),
        0.0,
    ),
    lights(
        "Dramatic",
        "🎭",
        "Hard side light with deep shadows",
        0.9,
        (-0.9, 0.25),
        0.0,
        0.05,
        (0.15, 0),
        0.5,
    ),
    lights(
        "Spotlight",
        "🔆",
        "Lit from above on a dark stage",
        0.8,
        (0.0, 0.85),
        0.1,
        0.15,
        (0.2, 0),
        0.85,
    ),
    lights(
        "Golden",
        "🌅",
        "Low, warm evening sun",
        0.7,
        (0.8, 0.1),
        0.9,
        0.25,
        (0.35, 1),
        0.2,
    ),
    lights(
        "Neon",
        "💖",
        "Cool key with a pink rim glow",
        0.5,
        (-0.7, 0.2),
        -0.8,
        0.1,
        (0.9, 3),
        0.6,
    ),
    lights(
        "Night",
        "🌟",
        "Blue moonlight from behind",
        0.3,
        (0.6, 0.6),
        -1.0,
        0.0,
        (0.8, 2),
        0.7,
    ),
];

/// Short names for the triggers, used where the gesture is not spelled out.
const TRIGGER_NAMES: [&str; TRIGGERS.len()] = [
    "Smile hearts",
    "Mouth rainbow",
    "Eyebrow questions",
    "Kiss hearts",
    "Storm cloud",
    "Applause",
    "Idea stars",
    "Fireworks",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Looks,
    Lights,
    Beauty,
    Stickers,
    Funny,
    Magic,
    Paint,
    Backdrop,
}

impl Group {
    pub const ALL: [Group; 8] = [
        Group::Looks,
        Group::Lights,
        Group::Beauty,
        Group::Stickers,
        Group::Funny,
        Group::Magic,
        Group::Paint,
        Group::Backdrop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::Looks => "Looks",
            Group::Lights => "Lights",
            Group::Beauty => "Beauty",
            Group::Stickers => "Stickers",
            Group::Funny => "Funny",
            Group::Magic => "Magic",
            Group::Paint => "Paint",
            Group::Backdrop => "Scene",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Group::Looks => "🎨",
            Group::Lights => "🔆",
            Group::Beauty => "💄",
            Group::Stickers => "🕶",
            Group::Funny => "😜",
            Group::Magic => "✨",
            Group::Paint => "🎭",
            Group::Backdrop => "🖼",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Group::Looks => "Colour looks",
            Group::Lights => "Studio lights",
            Group::Beauty => "Beauty",
            Group::Stickers => "Stickers",
            Group::Funny => "Funny faces",
            Group::Magic => "Magic moments",
            Group::Paint => "Face paint",
            Group::Backdrop => "Scene",
        }
    }
}

/// One removable entry in the "active effects" summary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Active {
    Look,
    Lut,
    Finish,
    AutoCorrect,
    Lights,
    Beauty,
    Sticker(usize),
    Distortion,
    Paint,
    Anonymise,
    Trigger(usize),
    Backdrop,
    AutoFrame,
    Badge,
    Logo,
}

/// A stand-in picture shown instead of the live one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Hold {
    #[default]
    Off,
    /// The last picture, frozen.
    Freeze,
    /// A "be right back" card.
    Card,
}

impl Hold {
    fn key(self) -> &'static str {
        match self {
            Hold::Off => "off",
            Hold::Freeze => "freeze",
            Hold::Card => "card",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BeautyPreset {
    Natural,
    Glam,
}

const ON: f32 = 0.01;

#[derive(Clone, PartialEq, Debug)]
pub struct EffectSettings {
    pub background: usize,
    pub background_path: String,
    pub blur: f32,
    pub look: usize,
    pub look_strength: f32,
    /// Path of the selected `.cube` LUT; empty for none.
    pub lut: String,
    pub lut_strength: f32,
    pub grain: f32,
    pub vignette: f32,
    pub glow: f32,
    pub smoothing: f32,
    pub slimming: f32,
    pub skin_tone: f32,
    pub eye_color: usize,
    pub eye_brighten: f32,
    pub eye_size: f32,
    pub nose_slim: f32,
    pub lip_color: usize,
    pub lip_strength: f32,
    pub blush: f32,
    pub teeth: f32,
    pub stickers: [bool; STICKERS.len()],
    pub distortion: usize,
    pub distortion_strength: f32,
    pub paint: usize,
    pub paint_strength: f32,
    pub anonymise: usize,
    pub anonymise_style: usize,
    pub anonymise_amount: f32,
    /// How readily a concerned look starts the storm (0 = only a deep frown).
    pub storm_sensitivity: f32,
    /// Indexes into `MAGIC_COLORS`.
    pub question_color: usize,
    pub star_color: usize,
    pub triggers: [bool; TRIGGERS.len()],
    /// Key light brightness; `light_x`/`light_y` place it (-1..1, +y is up).
    pub light_key: f32,
    pub light_x: f32,
    pub light_y: f32,
    pub light_warmth: f32,
    pub light_fill: f32,
    pub light_rim: f32,
    pub light_rim_color: usize,
    pub light_dim: f32,
    /// Automatic exposure and colour correction, 0 = off.
    pub auto_enhance: f32,
    pub autoframe_on: bool,
    /// How tightly auto-framing crops: 0 = wide, 1 = close-up.
    pub autoframe: f32,
    /// How many people effects follow (1..=MAX_FACES).
    pub faces: usize,
    pub badge: bool,
    pub logo: bool,
}

pub const MAX_FACES: usize = 4;

impl Default for EffectSettings {
    fn default() -> Self {
        Self {
            background: 0,
            background_path: String::new(),
            blur: 0.55,
            look: 0,
            look_strength: 1.0,
            lut: String::new(),
            lut_strength: 1.0,
            grain: 0.0,
            vignette: 0.0,
            glow: 0.0,
            smoothing: 0.0,
            slimming: 0.0,
            skin_tone: 0.0,
            eye_color: 0,
            eye_brighten: 0.0,
            eye_size: 0.0,
            nose_slim: 0.0,
            lip_color: 0,
            lip_strength: 0.6,
            blush: 0.0,
            teeth: 0.0,
            stickers: [false; STICKERS.len()],
            distortion: 0,
            distortion_strength: 0.6,
            paint: 0,
            paint_strength: 1.0,
            anonymise: 0,
            anonymise_style: 0,
            anonymise_amount: 0.5,
            storm_sensitivity: 0.5,
            question_color: 0,
            star_color: 1,
            triggers: [false; TRIGGERS.len()],
            light_key: 0.0,
            light_x: LIGHT_X,
            light_y: LIGHT_Y,
            light_warmth: 0.0,
            light_fill: 0.0,
            light_rim: 0.0,
            light_rim_color: 0,
            light_dim: 0.0,
            auto_enhance: 0.0,
            autoframe_on: false,
            autoframe: 0.5,
            faces: 1,
            badge: false,
            logo: false,
        }
    }
}

fn join_enabled(choices: &[Choice], enabled: &[bool]) -> String {
    choices
        .iter()
        .zip(enabled)
        .filter(|(_, on)| **on)
        .map(|(choice, _)| choice.key)
        .collect::<Vec<_>>()
        .join(",")
}

impl EffectSettings {
    /// Serialise as the `key=value` lines read by `effects/processor.py`.
    pub fn to_config(&self) -> String {
        let path = self.background_path.replace(['\n', '\r'], "");
        let mut out = String::new();
        let mut put = |key: &str, value: &dyn std::fmt::Display| {
            out.push_str(&format!("{key}={value}\n"));
        };
        put("background", &BACKGROUNDS[self.background].key);
        put("background_path", &path);
        put("blur", &self.blur);
        put("look", &LOOKS[self.look].key);
        put("look_strength", &self.look_strength);
        put("lut", &self.lut.replace(['\n', '\r'], ""));
        put("lut_strength", &self.lut_strength);
        put("grain", &self.grain);
        put("vignette", &self.vignette);
        put("glow", &self.glow);
        put("smoothing", &self.smoothing);
        put("slimming", &self.slimming);
        put("skin_tone", &self.skin_tone);
        put("eye_color", &EYE_COLORS[self.eye_color].key);
        put("eye_brighten", &self.eye_brighten);
        put("eye_size", &self.eye_size);
        put("nose_slim", &self.nose_slim);
        put("lip_color", &LIP_COLORS[self.lip_color].key);
        put("lip_strength", &self.lip_strength);
        put("blush", &self.blush);
        put("teeth", &self.teeth);
        put("stickers", &join_enabled(&STICKERS, &self.stickers));
        put("distortion", &DISTORTIONS[self.distortion].key);
        put("distortion_strength", &self.distortion_strength);
        put("mask", &PAINTS[self.paint].key);
        put("mask_strength", &self.paint_strength);
        put("anonymise", &ANONYMISE[self.anonymise].key);
        put(
            "anonymise_style",
            &ANONYMISE_STYLES[self.anonymise_style].key,
        );
        put("anonymise_amount", &self.anonymise_amount);
        put("storm_sensitivity", &self.storm_sensitivity);
        put("question_color", &MAGIC_COLORS[self.question_color].key);
        put("star_color", &MAGIC_COLORS[self.star_color].key);
        put("triggers", &join_enabled(&TRIGGERS, &self.triggers));
        put("light_key", &self.light_key);
        put("light_x", &self.light_x);
        put("light_y", &self.light_y);
        put("light_warmth", &self.light_warmth);
        put("light_fill", &self.light_fill);
        put("light_rim", &self.light_rim);
        put("light_rim_color", &RIM_COLORS[self.light_rim_color].key);
        put("light_dim", &self.light_dim);
        put("auto_enhance", &self.auto_enhance);
        put("autoframe_on", &self.autoframe_on);
        put("autoframe", &self.autoframe);
        put("faces", &self.faces);
        put("badge", &self.badge);
        put("logo_on", &self.logo);
        out
    }

    /// Parse `key=value` lines; anything missing or unknown keeps its default.
    pub fn from_config(text: &str) -> Self {
        let values = parse_lines(text);
        let defaults = Self::default();
        let unit = |key: &str, default: f32| -> f32 {
            values
                .get(key)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .map_or(default, |value| value.clamp(0.0, 1.0))
        };
        let signed = |key: &str, default: f32| -> f32 {
            values
                .get(key)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .map_or(default, |value| value.clamp(-1.0, 1.0))
        };
        let index = |key: &str, keys: &[&str]| -> usize {
            values
                .get(key)
                .and_then(|value| keys.iter().position(|candidate| candidate == value))
                .unwrap_or(0)
        };
        let named = |key: &str, default: usize| -> usize {
            values
                .get(key)
                .and_then(|value| MAGIC_COLORS.iter().position(|tint| tint.key == *value))
                .unwrap_or(default)
        };
        let flags = |key: &str, choices: &[Choice], flags: &mut [bool]| {
            let Some(list) = values.get(key) else { return };
            for name in list.split(',').map(str::trim) {
                if let Some(position) = choices.iter().position(|choice| choice.key == name) {
                    flags[position] = true;
                }
            }
        };
        let mut settings = Self {
            background: index("background", &BACKGROUNDS.each_ref().map(|c| c.key)),
            background_path: values
                .get("background_path")
                .map(|value| (*value).to_owned())
                .unwrap_or_default(),
            blur: unit("blur", defaults.blur),
            look: index("look", &LOOKS.each_ref().map(|l| l.key)),
            look_strength: unit("look_strength", defaults.look_strength),
            lut: values
                .get("lut")
                .map(|value| (*value).to_owned())
                .unwrap_or_default(),
            lut_strength: unit("lut_strength", defaults.lut_strength),
            grain: unit("grain", 0.0),
            vignette: unit("vignette", 0.0),
            glow: unit("glow", 0.0),
            smoothing: unit("smoothing", 0.0),
            slimming: unit("slimming", 0.0),
            skin_tone: signed("skin_tone", 0.0),
            eye_color: index("eye_color", &EYE_COLORS.each_ref().map(|t| t.key)),
            eye_brighten: unit("eye_brighten", 0.0),
            eye_size: unit("eye_size", 0.0),
            nose_slim: unit("nose_slim", 0.0),
            lip_color: index("lip_color", &LIP_COLORS.each_ref().map(|t| t.key)),
            lip_strength: unit("lip_strength", defaults.lip_strength),
            blush: unit("blush", 0.0),
            teeth: unit("teeth", 0.0),
            stickers: defaults.stickers,
            distortion: index("distortion", &DISTORTIONS.each_ref().map(|c| c.key)),
            distortion_strength: unit("distortion_strength", defaults.distortion_strength),
            paint: index("mask", &PAINTS.each_ref().map(|c| c.key)),
            paint_strength: unit("mask_strength", defaults.paint_strength),
            anonymise: index("anonymise", &ANONYMISE.each_ref().map(|c| c.key)),
            anonymise_style: index(
                "anonymise_style",
                &ANONYMISE_STYLES.each_ref().map(|c| c.key),
            ),
            anonymise_amount: unit("anonymise_amount", defaults.anonymise_amount),
            storm_sensitivity: unit("storm_sensitivity", defaults.storm_sensitivity),
            question_color: named("question_color", defaults.question_color),
            star_color: named("star_color", defaults.star_color),
            triggers: defaults.triggers,
            light_key: unit("light_key", 0.0),
            light_x: signed("light_x", LIGHT_X),
            light_y: signed("light_y", LIGHT_Y),
            light_warmth: signed("light_warmth", 0.0),
            light_fill: unit("light_fill", 0.0),
            light_rim: unit("light_rim", 0.0),
            light_rim_color: index("light_rim_color", &RIM_COLORS.each_ref().map(|t| t.key)),
            light_dim: unit("light_dim", 0.0),
            auto_enhance: unit("auto_enhance", 0.0),
            autoframe_on: flag(&values, "autoframe_on", false),
            autoframe: unit("autoframe", defaults.autoframe),
            faces: values
                .get("faces")
                .and_then(|value| value.parse::<usize>().ok())
                .map_or(1, |count| count.clamp(1, MAX_FACES)),
            badge: flag(&values, "badge", false),
            logo: flag(&values, "logo_on", false),
        };
        flags("stickers", &STICKERS, &mut settings.stickers);
        flags("triggers", &TRIGGERS, &mut settings.triggers);
        settings
    }

    fn lights_active(&self) -> bool {
        self.light_key > ON || self.light_fill > ON || self.light_rim > ON || self.light_dim > ON
    }

    /// The preset matching the current lights, if they were not hand-tuned since.
    pub fn light_preset(&self) -> Option<usize> {
        let close = |a: f32, b: f32| (a - b).abs() < 0.005;
        LIGHT_PRESETS.iter().position(|preset| {
            let lit = preset.key > ON;
            close(self.light_key, preset.key)
                && (!lit || (close(self.light_x, preset.x) && close(self.light_y, preset.y)))
                && (!lit || close(self.light_warmth, preset.warmth))
                && close(self.light_fill, preset.fill)
                && close(self.light_rim, preset.rim)
                && (preset.rim <= ON || self.light_rim_color == preset.rim_color)
                && close(self.light_dim, preset.dim)
        })
    }

    pub fn apply_light_preset(&mut self, index: usize) {
        let preset = &LIGHT_PRESETS[index];
        self.light_key = preset.key;
        self.light_x = preset.x;
        self.light_y = preset.y;
        self.light_warmth = preset.warmth;
        self.light_fill = preset.fill;
        self.light_rim = preset.rim;
        self.light_rim_color = preset.rim_color;
        self.light_dim = preset.dim;
    }

    fn finish_active(&self) -> bool {
        self.grain > ON || self.vignette > ON || self.glow > ON
    }

    fn beauty_active(&self) -> bool {
        self.smoothing > ON
            || self.slimming > ON
            || self.skin_tone.abs() > ON
            || self.eye_color != 0
            || self.eye_brighten > ON
            || self.eye_size > ON
            || self.nose_slim > ON
            || (self.lip_color != 0 && self.lip_strength > ON)
            || self.blush > ON
            || self.teeth > ON
    }

    /// Everything currently changing the picture, in display order.
    pub fn active(&self) -> Vec<(Active, String)> {
        let mut list = Vec::new();
        if self.look != 0 && self.look_strength > ON {
            list.push((Active::Look, LOOKS[self.look].label.to_owned()));
        }
        if !self.lut.is_empty() && self.lut_strength > ON {
            let name = Path::new(&self.lut)
                .file_stem()
                .map(|stem| stem.to_string_lossy().replace('_', " "))
                .unwrap_or_default();
            list.push((Active::Lut, format!("{name} LUT")));
        }
        if self.finish_active() {
            list.push((Active::Finish, "Film finish".to_owned()));
        }
        if self.auto_enhance > ON {
            list.push((Active::AutoCorrect, "Auto-correct".to_owned()));
        }
        if self.lights_active() {
            let name = match self.light_preset() {
                Some(index) => format!("{} lights", LIGHT_PRESETS[index].label),
                None => "Studio lights".to_owned(),
            };
            list.push((Active::Lights, name));
        }
        if self.beauty_active() {
            list.push((Active::Beauty, "Beauty".to_owned()));
        }
        for (index, sticker) in STICKERS.iter().enumerate() {
            if self.stickers[index] {
                list.push((Active::Sticker(index), sticker.label.to_owned()));
            }
        }
        if self.distortion != 0 && self.distortion_strength > ON {
            list.push((
                Active::Distortion,
                DISTORTIONS[self.distortion].label.to_owned(),
            ));
        }
        if self.paint != 0 && self.paint_strength > ON {
            list.push((Active::Paint, format!("{} paint", PAINTS[self.paint].label)));
        }
        if self.anonymise != 0 {
            list.push((
                Active::Anonymise,
                format!(
                    "{} {}",
                    ANONYMISE_STYLES[self.anonymise_style].hint,
                    ANONYMISE[self.anonymise].label.to_lowercase()
                ),
            ));
        }
        for (index, name) in TRIGGER_NAMES.iter().enumerate() {
            if self.triggers[index] {
                list.push((Active::Trigger(index), (*name).to_owned()));
            }
        }
        if self.background != 0 {
            list.push((
                Active::Backdrop,
                format!("{} backdrop", BACKGROUNDS[self.background].label),
            ));
        }
        if self.autoframe_on {
            list.push((Active::AutoFrame, "Auto-framing".to_owned()));
        }
        if self.badge {
            list.push((Active::Badge, "Name badge".to_owned()));
        }
        if self.logo {
            list.push((Active::Logo, "Logo".to_owned()));
        }
        list
    }

    pub fn any_active(&self) -> bool {
        !self.active().is_empty()
    }

    /// True when an enabled effect cannot work without seeing a face.
    pub fn needs_face(&self) -> bool {
        self.active().iter().any(|(active, _)| match active {
            Active::Look
            | Active::Lut
            | Active::Finish
            | Active::Backdrop
            | Active::Badge
            | Active::Logo => false,
            // Fill, rim and dimming work from the person outline alone.
            Active::Lights => self.light_key > ON,
            _ => true,
        })
    }

    pub fn clear(&mut self, active: Active) {
        let defaults = Self::default();
        match active {
            Active::Look => self.look = 0,
            Active::Lights => self.apply_light_preset(0),
            Active::Lut => self.lut.clear(),
            Active::Finish => {
                self.grain = 0.0;
                self.vignette = 0.0;
                self.glow = 0.0;
            }
            Active::Beauty => {
                self.smoothing = 0.0;
                self.slimming = 0.0;
                self.skin_tone = 0.0;
                self.eye_color = 0;
                self.eye_brighten = 0.0;
                self.eye_size = 0.0;
                self.nose_slim = 0.0;
                self.lip_color = 0;
                self.lip_strength = defaults.lip_strength;
                self.blush = 0.0;
                self.teeth = 0.0;
            }
            Active::Sticker(index) => self.stickers[index] = false,
            Active::Distortion => self.distortion = 0,
            Active::Paint => self.paint = 0,
            Active::Anonymise => self.anonymise = 0,
            Active::Trigger(index) => self.triggers[index] = false,
            Active::Backdrop => self.background = 0,
            Active::AutoCorrect => self.auto_enhance = 0.0,
            Active::AutoFrame => self.autoframe_on = false,
            Active::Badge => self.badge = false,
            Active::Logo => self.logo = false,
        }
    }

    /// How many effects in `group` are switched on (drives the tab badges).
    pub fn count(&self, group: Group) -> usize {
        self.active()
            .iter()
            .filter(|(active, _)| group_of(*active) == group)
            .count()
    }

    pub fn reset(&mut self, group: Group) {
        for (active, _) in self.active() {
            if group_of(active) == group {
                self.clear(active);
            }
        }
        let defaults = Self::default();
        match group {
            Group::Looks => {
                self.look_strength = defaults.look_strength;
                self.lut_strength = defaults.lut_strength;
            }
            Group::Funny => self.distortion_strength = defaults.distortion_strength,
            Group::Magic => {
                self.storm_sensitivity = defaults.storm_sensitivity;
                self.question_color = defaults.question_color;
                self.star_color = defaults.star_color;
            }
            Group::Paint => {
                self.paint_strength = defaults.paint_strength;
                self.anonymise_style = defaults.anonymise_style;
                self.anonymise_amount = defaults.anonymise_amount;
            }
            Group::Backdrop => {
                self.blur = defaults.blur;
                self.autoframe = defaults.autoframe;
                self.faces = defaults.faces;
            }
            Group::Lights | Group::Beauty | Group::Stickers => {}
        }
    }

    pub fn reset_all(&mut self) {
        let path = std::mem::take(&mut self.background_path);
        *self = Self::default();
        self.background_path = path;
    }

    pub fn apply_beauty_preset(&mut self, preset: BeautyPreset) {
        self.clear(Active::Beauty);
        match preset {
            BeautyPreset::Natural => {
                self.smoothing = 0.3;
                self.eye_brighten = 0.2;
                self.teeth = 0.2;
            }
            BeautyPreset::Glam => {
                self.smoothing = 0.55;
                self.slimming = 0.25;
                self.eye_size = 0.25;
                self.eye_brighten = 0.45;
                self.lip_color = 1;
                self.lip_strength = 0.5;
                self.blush = 0.4;
                self.teeth = 0.4;
            }
        }
    }
}

fn group_of(active: Active) -> Group {
    match active {
        Active::Look | Active::Lut | Active::Finish => Group::Looks,
        Active::AutoCorrect | Active::Lights => Group::Lights,
        Active::Beauty => Group::Beauty,
        Active::Sticker(_) => Group::Stickers,
        Active::Distortion => Group::Funny,
        Active::Trigger(_) => Group::Magic,
        Active::Paint | Active::Anonymise => Group::Paint,
        Active::Backdrop | Active::AutoFrame | Active::Badge | Active::Logo => Group::Backdrop,
    }
}

/// Health report written by the effects worker about once a second.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorkerStatus {
    pub fps: f32,
    /// `None` when no enabled effect is looking for a face.
    pub face: Option<bool>,
}

impl WorkerStatus {
    pub fn parse(text: &str) -> Option<Self> {
        let values = parse_lines(text);
        Some(Self {
            fps: values.get("fps")?.parse().ok()?,
            face: match *values.get("face")? {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            },
        })
    }
}

pub struct Notice {
    pub text: String,
    pub is_error: bool,
}

/// Output settings that apply whatever profile is active.
#[derive(Clone, PartialEq, Debug)]
pub struct Session {
    pub mirror: bool,
    pub flip: bool,
    /// Whiteboard corners to straighten, set by the current camera position.
    pub keystone: Option<Corners>,
    /// True while corners are being dragged: the picture is shown unstraightened.
    pub adjusting: bool,
    pub hold: Hold,
    pub hold_text: String,
    pub badge_name: String,
    pub badge_title: String,
    pub badge_design: BadgeDesign,
    pub logo_path: String,
}

impl Session {
    fn from_values(values: &std::collections::HashMap<&str, &str>) -> Self {
        let text = |key: &str, default: &str| -> String {
            values.get(key).map_or(default, |value| *value).to_owned()
        };
        Self {
            mirror: flag(values, "mirror", false),
            flip: flag(values, "flip", false),
            keystone: None,
            adjusting: false,
            hold: Hold::Off,
            hold_text: text("hold_text", "Be right back"),
            badge_name: text("badge_name", ""),
            badge_title: text("badge_title", ""),
            badge_design: BadgeDesign::from_values(values),
            logo_path: text("logo_path", ""),
        }
    }

    /// True when something here needs the worker even with every effect off.
    fn needs_worker(&self) -> bool {
        self.mirror || self.flip || self.keystone.is_some() || self.hold != Hold::Off
    }
}

/// What other apps can currently get from the virtual camera.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Broadcast {
    /// The loopback device does not exist on this system.
    NotSetUp,
    /// The device exists, but nothing is sent, so apps do not list it.
    Idle,
    /// Effects run in the preview only and are about to move to the device.
    Connecting,
    /// Sending; `viewers` other programs have the camera open.
    Live { viewers: usize },
}

impl Broadcast {
    pub fn of(device: bool, enabled: bool, publishing: bool, viewers: usize) -> Self {
        match (device, enabled, publishing) {
            (false, _, _) => Self::NotSetUp,
            (true, false, _) => Self::Idle,
            (true, true, false) => Self::Connecting,
            (true, true, true) => Self::Live { viewers },
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::NotSetUp => "NOT SET UP".into(),
            Self::Idle => "READY".into(),
            Self::Connecting => "CONNECTING".into(),
            Self::Live { viewers: 0 } => "ON AIR".into(),
            Self::Live { viewers: 1 } => "ON AIR  •  1 app".into(),
            Self::Live { viewers } => format!("ON AIR  •  {viewers} apps"),
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Self::NotSetUp => {
                "Other apps cannot see your effects yet. Run \
                 scripts/setup-virtual-camera.sh --persist once; no restart needed."
            }
            Self::Idle => "Set up. It only appears in other apps while the effects camera is on.",
            Self::Connecting => "Moving your effects to the virtual camera…",
            Self::Live { viewers: 0 } => {
                "Choose “OBSBOT Virtual Camera” in your video app. If it is not \
                 listed, reload the page or restart the app."
            }
            Self::Live { .. } => "Another app is showing your effects camera right now.",
        }
    }
}

pub struct EffectsController {
    pub enabled: bool,
    /// True when the worker also feeds the system-wide virtual camera;
    /// false means the effects are visible in the preview only.
    pub publishing: bool,
    /// The loopback device, looked up again every second.
    pub virtual_camera: Option<PathBuf>,
    /// Other programs that have the virtual camera open.
    pub viewers: usize,
    last_device_check: Instant,
    pub resolution: usize,
    pub fps: u32,
    pub settings: EffectSettings,
    pub session: Session,
    pub notice: Option<Notice>,
    pub status: Option<WorkerStatus>,
    pub luts: LutLibrary,
    pub backdrops: BackdropLibrary,
    pub fonts: crate::fonts::FontList,
    pub profiles: ProfileStore,
    process: Option<Child>,
    config_path: PathBuf,
    status_path: PathBuf,
    last_status_read: Instant,
    /// Set after a failed automatic start, or when the user switches the
    /// camera off, so that editing an effect does not start it again.
    autostart_blocked: bool,
    /// Whether the app window is open; without it the worker sends no preview.
    window_open: bool,
    /// The worker was stopped only because nothing could see its picture.
    resume_when_shown: bool,
}

impl EffectsController {
    /// Restore the settings from the last session, if any.
    pub fn load() -> Self {
        Self::from_file(config_dir().join("effects.conf"), true)
    }

    fn from_file(config_path: PathBuf, seed_profiles: bool) -> Self {
        let text = fs::read_to_string(&config_path).unwrap_or_default();
        let values = parse_lines(&text);
        let resolution = values
            .get("output")
            .and_then(|label| {
                RESOLUTIONS
                    .iter()
                    .position(|r| format!("{}x{}", r.width, r.height) == *label)
            })
            .unwrap_or(2);
        let fps = match values.get("fps").and_then(|value| value.parse().ok()) {
            Some(15) => 15,
            _ => 30,
        };
        Self {
            enabled: false,
            publishing: false,
            virtual_camera: find_virtual_camera(),
            viewers: 0,
            last_device_check: Instant::now(),
            resolution,
            fps,
            settings: EffectSettings::from_config(&text),
            session: Session::from_values(&values),
            notice: None,
            status: None,
            luts: LutLibrary::new(paths::bundled("luts"), config_path.with_file_name("luts")),
            backdrops: BackdropLibrary::new(
                paths::bundled("backgrounds"),
                config_path.with_file_name("backgrounds"),
            ),
            fonts: crate::fonts::FontList::default(),
            profiles: ProfileStore::load(config_path.with_file_name("profiles"), seed_profiles),
            process: None,
            config_path,
            status_path: std::env::temp_dir()
                .join(format!("obsbot-effects-{}.status", std::process::id())),
            last_status_read: Instant::now(),
            autostart_blocked: false,
            window_open: true,
            resume_when_shown: false,
        }
    }

    pub fn selected_resolution(&self) -> Resolution {
        RESOLUTIONS[self.resolution.min(RESOLUTIONS.len() - 1)]
    }

    /// Everything the worker reads: the profile-able settings, then the
    /// session's additions and overrides (later lines win).
    pub fn worker_config(&self) -> String {
        let session = &self.session;
        let resolution = self.selected_resolution();
        let mut out = self.settings.to_config();
        let mut put = |key: &str, value: &dyn std::fmt::Display| {
            out.push_str(&format!("{key}={value}\n"));
        };
        put("mirror", &session.mirror);
        put("flip", &session.flip);
        put("hold", &session.hold.key());
        put("hold_text", &one_line(&session.hold_text));
        put("badge_name", &one_line(&session.badge_name));
        put("badge_title", &one_line(&session.badge_title));
        out.push_str(&session.badge_design.to_config());
        let mut put = |key: &str, value: &dyn std::fmt::Display| {
            out.push_str(&format!("{key}={value}\n"));
        };
        put("logo_path", &one_line(&session.logo_path));
        if self.settings.logo {
            put("logo", &one_line(&session.logo_path));
        }
        if session.adjusting {
            // Show the plain picture while whiteboard corners are placed.
            put("autoframe_on", &false);
        } else if let Some(corners) = &session.keystone {
            put("keystone", &corners_text(corners));
        }
        put("preview", &self.window_open);
        put(
            "output",
            &format!("{}x{}", resolution.width, resolution.height),
        );
        put("fps", &self.fps);
        out
    }

    /// Persist the settings. The running worker picks the change up live.
    pub fn save(&self) -> io::Result<()> {
        write_atomically(&self.config_path, &self.worker_config())
    }

    fn fail(&mut self, text: String) {
        self.notice = Some(Notice {
            text,
            is_error: true,
        });
    }

    /// True when any effect or session option needs the worker running.
    pub fn wants_worker(&self) -> bool {
        self.settings.any_active() || self.session.needs_worker()
    }

    /// Call after any settings edit: saves, and starts the effects camera the
    /// first time something is switched on so the change is visible at once.
    pub fn settings_changed(&mut self, input: Option<&Path>, preview: &mut Preview) {
        if let Err(error) = self.save() {
            self.fail(format!("Could not save effects: {error}"));
            return;
        }
        if self.starts_automatically() && self.wants_worker() {
            if let Some(input) = input {
                if self.start(input, preview).is_err() {
                    self.autostart_blocked = true;
                }
            }
        }
    }

    /// True while switching an effect on will start the camera by itself.
    pub fn starts_automatically(&self) -> bool {
        !self.enabled && !self.autostart_blocked && self.window_open
    }

    /// The user switched the camera off: stop, and stay off until asked.
    pub fn stop_by_user(&mut self, preview: &mut Preview) {
        self.session.hold = Hold::Off;
        self.resume_when_shown = false;
        self.stop(preview);
        self.autostart_blocked = true;
        let _ = self.save();
    }

    /// Switch to the saved profile at `index` (as listed), if there is one.
    pub fn apply_profile(&mut self, index: usize, input: Option<&Path>, preview: &mut Preview) {
        let Some(profile) = self.profiles.profiles.get(index) else {
            return;
        };
        self.settings = profile.settings.clone();
        self.profiles.current = Some(profile.name.clone());
        self.profiles.message = None;
        self.settings_changed(input, preview);
    }

    /// Switch to the profile called `name`. Returns false if there is none.
    pub fn apply_profile_named(
        &mut self,
        name: &str,
        input: Option<&Path>,
        preview: &mut Preview,
    ) -> bool {
        let wanted = name.trim().to_lowercase();
        let found = self
            .profiles
            .profiles
            .iter()
            .position(|profile| profile.name.to_lowercase() == wanted);
        if let Some(index) = found {
            self.apply_profile(index, input, preview);
        }
        found.is_some()
    }

    /// Move `step` profiles forwards or backwards from the current one.
    pub fn step_profile(&mut self, step: i32, input: Option<&Path>, preview: &mut Preview) {
        let count = self.profiles.profiles.len() as i32;
        if count == 0 {
            return;
        }
        let current = self
            .profiles
            .matching(&self.settings)
            .map(|profile| profile.name.clone())
            .or_else(|| self.profiles.current.clone())
            .and_then(|name| self.profiles.profiles.iter().position(|p| p.name == name));
        let next = match current {
            Some(index) => (index as i32 + step).rem_euclid(count),
            None if step >= 0 => 0,
            None => count - 1,
        };
        self.apply_profile(next as usize, input, preview);
    }

    /// Show or clear the stand-in picture. Showing it starts the worker if
    /// needed, because the point is that other apps see it.
    pub fn set_hold(&mut self, hold: Hold, input: Option<&Path>, preview: &mut Preview) {
        self.session.hold = hold;
        if let Err(error) = self.save() {
            self.fail(format!("Could not save effects: {error}"));
        }
        if hold != Hold::Off && !self.enabled {
            if let Some(input) = input {
                let _ = self.start(input, preview);
            }
        }
    }

    /// Tell the controller whether the app window is open. With it closed the
    /// worker stops sending preview frames, and stops entirely when no other
    /// app could see its picture either.
    pub fn set_window_open(&mut self, open: bool, input: Option<&Path>, preview: &mut Preview) {
        if open == self.window_open {
            return;
        }
        self.window_open = open;
        if open {
            if std::mem::take(&mut self.resume_when_shown) {
                if let Some(input) = input {
                    let _ = self.start(input, preview);
                }
            } else if self.enabled {
                let _ = self.save();
            } else {
                preview.show_camera();
            }
        } else if self.enabled && !self.publishing {
            self.stop(preview);
            self.resume_when_shown = true;
            preview.pause();
        } else if self.enabled {
            let _ = self.save();
        } else {
            // Nothing is watching: let go of the camera altogether.
            preview.pause();
        }
    }

    /// Start (or restart) the worker. Errors are also stored in `notice`.
    pub fn start(&mut self, input: &Path, preview: &mut Preview) -> Result<(), ()> {
        if !self.window_open && find_virtual_camera().is_none() {
            // Nobody could see the picture: wait until the window is opened.
            self.resume_when_shown = true;
            return Ok(());
        }
        self.stop(preview);
        match self.spawn(input, preview) {
            Ok(()) => {
                self.autostart_blocked = false;
                Ok(())
            }
            Err(text) => {
                self.fail(text);
                if !self.window_open {
                    preview.pause();
                }
                Err(())
            }
        }
    }

    fn spawn(&mut self, input: &Path, preview: &mut Preview) -> Result<(), String> {
        // Optional: without it the effects still show in the preview.
        let output = find_virtual_camera();
        let hint = paths::setup_hint();
        let python =
            paths::python().ok_or_else(|| format!("Effects runtime not installed. {hint}"))?;
        let processor = paths::effects_dir()
            .map(|dir| dir.join("processor.py"))
            .ok_or_else(|| format!("Effects files are missing. {hint}"))?;
        let model_dir =
            paths::models_dir().ok_or_else(|| format!("MediaPipe models are missing. {hint}"))?;
        self.save()
            .map_err(|error| format!("Could not save effects: {error}"))?;
        let resolution = self.selected_resolution();
        let _ = fs::remove_file(&self.status_path);

        // Release the physical capture stream before the worker opens it.
        preview.pause();
        let mut command = Command::new(python);
        command.arg(processor).arg("--preview");
        if let Some(output) = &output {
            command.arg("--output").arg(output);
        }
        let mut child = command
            .arg("--input")
            .arg(input)
            .arg("--config")
            .arg(&self.config_path)
            .arg("--status")
            .arg(&self.status_path)
            .arg("--model-dir")
            .arg(model_dir)
            .arg("--width")
            .arg(resolution.width.to_string())
            .arg("--height")
            .arg(resolution.height.to_string())
            .arg("--fps")
            .arg(self.fps.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                preview.show_camera();
                format!("Failed to start the effects worker: {error}")
            })?;

        match child.stdout.take() {
            Some(frames) => preview.show_stream(Box::new(frames)),
            None => preview.show_camera(),
        }
        self.process = Some(child);
        self.enabled = true;
        self.publishing = output.is_some();
        self.notice = None;
        Ok(())
    }

    /// Apply an output format change: restart if running, otherwise just save.
    pub fn restart(&mut self, input: &Path, preview: &mut Preview) {
        if self.enabled {
            let _ = self.start(input, preview);
        } else if let Err(error) = self.save() {
            self.fail(format!("Could not save effects: {error}"));
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
        let _ = fs::remove_file(&self.status_path);
        self.enabled = false;
        self.publishing = false;
        self.status = None;
        self.notice = None;
    }

    pub fn broadcast(&self) -> Broadcast {
        let device = self.virtual_camera.is_some();
        Broadcast::of(device, self.enabled, self.publishing, self.viewers)
    }

    /// Keep track of the virtual camera. Returns true when it has appeared
    /// while effects run in the preview only, so the caller can restart the
    /// worker and go live without the user switching anything.
    pub fn watch_virtual_camera(&mut self) -> bool {
        if self.last_device_check.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.last_device_check = Instant::now();
        let appeared = self.virtual_camera.is_none();
        self.virtual_camera = find_virtual_camera();
        appeared && self.virtual_camera.is_some() && self.enabled && !self.publishing
    }

    pub fn poll(&mut self, preview: &mut Preview) {
        let Some(process) = &mut self.process else {
            return;
        };
        match process.try_wait() {
            Ok(Some(status)) => {
                self.process = None;
                self.enabled = false;
                self.publishing = false;
                self.status = None;
                self.autostart_blocked = true;
                self.fail(format!("Effects worker stopped ({status})"));
                if self.window_open {
                    preview.show_camera();
                } else {
                    preview.pause();
                }
                return;
            }
            Ok(None) => {}
            Err(error) => self.fail(format!("Could not inspect effects worker: {error}")),
        }
        if self.last_status_read.elapsed() >= Duration::from_millis(500) {
            self.last_status_read = Instant::now();
            if let Ok(text) = fs::read_to_string(&self.status_path) {
                self.status = WorkerStatus::parse(&text);
            }
        }
    }
}

/// The loopback device other apps read the effects picture from, if set up.
pub fn find_virtual_camera() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/video4linux").ok()?;
    for entry in entries.flatten() {
        let name = fs::read_to_string(entry.path().join("name")).ok()?;
        if name.trim() == "OBSBOT Virtual Camera" {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}

#[cfg(test)]
impl EffectsController {
    /// A controller that never touches the user's saved settings.
    pub fn for_tests() -> Self {
        let dir = std::env::temp_dir().join(format!("obsbot-effects-tests-{}", std::process::id()));
        Self::from_file(dir.join("unused.conf"), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_is_only_live_while_effects_feed_the_device() {
        assert_eq!(Broadcast::of(false, true, false, 0), Broadcast::NotSetUp);
        assert_eq!(Broadcast::of(true, false, false, 0), Broadcast::Idle);
        assert_eq!(Broadcast::of(true, true, false, 0), Broadcast::Connecting);
        let live = Broadcast::of(true, true, true, 2);
        assert_eq!(live, Broadcast::Live { viewers: 2 });
        assert_eq!(live.label(), "ON AIR  •  2 apps");
        let texts = ["•", "…", "“", "”"];
        assert_eq!(crate::widgets::missing_glyphs(&texts), Vec::<&str>::new());
    }

    #[test]
    fn default_settings_have_no_active_effects() {
        let settings = EffectSettings::default();
        assert!(!settings.any_active());
        assert!(!settings.needs_face());
    }

    #[test]
    fn config_round_trips_every_field() {
        let mut settings = EffectSettings {
            background: 2,
            background_path: "/tmp/my room.jpg".into(),
            lut: "/home/me/luts/Warm Film.cube".into(),
            lut_strength: 0.625,
            blur: 0.25,
            look: 5,
            look_strength: 0.75,
            grain: 0.5,
            vignette: 0.125,
            glow: 0.375,
            smoothing: 0.5,
            slimming: 0.25,
            skin_tone: -0.5,
            eye_color: 3,
            eye_brighten: 0.625,
            eye_size: 0.25,
            nose_slim: 0.5,
            lip_color: 2,
            lip_strength: 0.875,
            blush: 0.25,
            teeth: 0.5,
            distortion: 4,
            distortion_strength: 0.25,
            paint: 3,
            paint_strength: 0.5,
            anonymise: 2,
            anonymise_style: 1,
            anonymise_amount: 0.875,
            storm_sensitivity: 0.25,
            question_color: 4,
            star_color: 2,
            light_key: 0.75,
            light_x: 0.25,
            light_y: -0.5,
            light_warmth: -0.25,
            light_fill: 0.5,
            light_rim: 0.625,
            light_rim_color: 3,
            light_dim: 0.375,
            auto_enhance: 0.75,
            autoframe_on: true,
            autoframe: 0.25,
            faces: 3,
            badge: true,
            logo: true,
            ..EffectSettings::default()
        };
        settings.stickers[0] = true;
        settings.stickers[6] = true;
        settings.triggers[1] = true;
        assert_eq!(EffectSettings::from_config(&settings.to_config()), settings);
    }

    #[test]
    fn config_uses_worker_key_names() {
        let mut settings = EffectSettings {
            paint: 1,
            ..EffectSettings::default()
        };
        settings.stickers[2] = true;
        settings.stickers[3] = true;
        let config = settings.to_config();
        assert!(config.contains("stickers=dog,cat\n"));
        assert!(config.contains("mask=hero\n"));
        assert!(config.contains("look=none\n"));
    }

    #[test]
    fn bad_config_values_fall_back_to_defaults() {
        let settings = EffectSettings::from_config(
            "look=does-not-exist\nsmoothing=banana\ngrain=7\nskin_tone=-9\nstickers=dog,,nope\nnonsense",
        );
        assert_eq!(settings.look, 0);
        assert_eq!(settings.smoothing, 0.0);
        assert_eq!(settings.grain, 1.0);
        assert_eq!(settings.skin_tone, -1.0);
        assert_eq!(settings.stickers.iter().filter(|on| **on).count(), 1);
    }

    #[test]
    fn clearing_an_active_entry_removes_only_that_effect() {
        let mut settings = EffectSettings {
            look: 1,
            ..EffectSettings::default()
        };
        settings.stickers[0] = true;
        settings.stickers[4] = true;
        settings.apply_beauty_preset(BeautyPreset::Glam);
        assert_eq!(settings.active().len(), 4);
        settings.clear(Active::Sticker(0));
        settings.clear(Active::Beauty);
        let left: Vec<Active> = settings.active().into_iter().map(|(a, _)| a).collect();
        assert_eq!(left, vec![Active::Look, Active::Sticker(4)]);
    }

    #[test]
    fn group_counts_and_reset_follow_the_tabs() {
        let mut settings = EffectSettings {
            look: 2,
            grain: 0.4,
            distortion: 1,
            ..EffectSettings::default()
        };
        settings.triggers[0] = true;
        assert_eq!(settings.count(Group::Looks), 2);
        assert_eq!(settings.count(Group::Magic), 1);
        assert_eq!(settings.count(Group::Stickers), 0);
        settings.reset(Group::Looks);
        assert_eq!(settings.count(Group::Looks), 0);
        assert_eq!(settings.count(Group::Funny), 1);
    }

    #[test]
    fn only_face_driven_effects_need_a_face() {
        let mut settings = EffectSettings {
            look: 1,
            background: 1,
            ..EffectSettings::default()
        };
        assert!(!settings.needs_face());
        settings.triggers[2] = true;
        assert!(settings.needs_face());
    }

    #[test]
    fn reset_all_keeps_the_background_picture_path() {
        let mut settings = EffectSettings {
            background: 2,
            background_path: "/tmp/a.png".into(),
            ..EffectSettings::default()
        };
        settings.reset_all();
        assert!(!settings.any_active());
        assert_eq!(settings.background_path, "/tmp/a.png");
    }

    #[test]
    fn a_selected_lut_is_a_removable_look() {
        let mut settings = EffectSettings {
            lut: "/x/Faded_Film.cube".into(),
            ..EffectSettings::default()
        };
        assert_eq!(
            settings.active()[0],
            (Active::Lut, "Faded Film LUT".to_owned())
        );
        assert_eq!(settings.count(Group::Looks), 1);
        assert!(!settings.needs_face());
        settings.clear(Active::Lut);
        assert!(!settings.any_active());
    }

    #[test]
    fn the_anonymiser_is_summarised_by_style_and_target() {
        let mut settings = EffectSettings {
            anonymise: 1,
            anonymise_style: 2,
            ..EffectSettings::default()
        };
        assert_eq!(
            settings.active(),
            vec![(Active::Anonymise, "Blacked-out eyes".to_owned())]
        );
        assert!(settings.needs_face());
        assert!(settings
            .to_config()
            .contains("anonymise=eyes\nanonymise_style=bar\n"));
        settings.reset(Group::Paint);
        assert_eq!(settings, EffectSettings::default());
    }

    #[test]
    fn office_options_are_listed_and_only_framing_needs_a_face() {
        let mut settings = EffectSettings {
            badge: true,
            logo: true,
            auto_enhance: 0.6,
            ..EffectSettings::default()
        };
        let names: Vec<String> = settings
            .active()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        assert_eq!(names, ["Auto-correct", "Name badge", "Logo"]);
        assert!(settings.needs_face(), "auto-correct exposes for the face");
        settings.auto_enhance = 0.0;
        assert!(!settings.needs_face());
        settings.autoframe_on = true;
        assert!(settings.needs_face());
        settings.reset(Group::Backdrop);
        assert!(!settings.any_active());
    }

    #[test]
    fn worker_config_adds_session_options_and_overrides() {
        let mut effects = EffectsController::for_tests();
        effects.settings.logo = true;
        effects.settings.autoframe_on = true;
        effects.session.logo_path = "/logos/acme.png".into();
        effects.session.badge_name = "Ada\nLovelace".into();
        effects.session.mirror = true;
        effects.session.keystone = Some(crate::positions::DEFAULT_CORNERS);
        let config = effects.worker_config();
        assert!(config.contains("logo=/logos/acme.png\n"));
        assert!(config.contains("badge_name=Ada Lovelace\n"));
        assert!(config.contains("mirror=true\n"));
        assert!(config.contains("keystone=0.1500,0.1500,"));
        assert!(config.contains("preview=true\n"));
        assert!(effects.wants_worker());

        effects.session.adjusting = true;
        let adjusting = effects.worker_config();
        assert!(!adjusting.contains("keystone="));
        assert!(
            adjusting.rfind("autoframe_on=false").unwrap()
                > adjusting.find("autoframe_on=true").unwrap()
        );

        // Session options survive a reload; transient ones do not.
        let values = parse_lines(&config);
        let restored = Session::from_values(&values);
        assert!(restored.mirror && restored.keystone.is_none() && restored.hold == Hold::Off);
        assert_eq!(restored.logo_path, "/logos/acme.png");
    }

    #[test]
    fn badge_design_round_trips_through_the_worker_config() {
        let mut effects = EffectsController::for_tests();
        effects.session.badge_design = BadgeDesign {
            font: "Liberation Serif".into(),
            bold: false,
            caps: true,
            size: 0.75,
            position: 1,
            style: 3,
            layout: 1,
            color: 4,
            opacity: 0.25,
            margin: 0.5,
        };
        let config = effects.worker_config();
        assert!(config.contains("badge_position=top-center\n"));
        assert!(config.contains("badge_style=block\n"));
        assert!(config.contains("badge_accent=255,184,92\n"));
        assert!(config.contains("badge_font=Liberation Serif\n"));
        let restored = BadgeDesign::from_values(&parse_lines(&config));
        assert_eq!(restored, effects.session.badge_design);
        // Unknown values fall back to the defaults.
        let odd = BadgeDesign::from_values(&parse_lines(
            "badge_style=neon\nbadge_accent=1,2,3\nbadge_size=9",
        ));
        assert_eq!((odd.style, odd.color, odd.size), (0, 0, 1.0));
    }

    #[test]
    fn light_presets_are_recognised_until_hand_tuned() {
        let mut settings = EffectSettings::default();
        assert_eq!(settings.light_preset(), Some(0));
        for index in 1..LIGHT_PRESETS.len() {
            settings.apply_light_preset(index);
            assert_eq!(settings.light_preset(), Some(index));
            assert_eq!(settings.count(Group::Lights), 1);
        }
        settings.light_fill += 0.1;
        assert_eq!(settings.light_preset(), None);
        assert_eq!(settings.active()[0].1, "Studio lights");
        settings.clear(Active::Lights);
        assert!(!settings.any_active());
    }

    #[test]
    fn only_the_key_light_needs_a_face() {
        let mut settings = EffectSettings {
            light_rim: 0.5,
            light_dim: 0.5,
            ..EffectSettings::default()
        };
        assert!(!settings.needs_face());
        settings.light_key = 0.5;
        assert!(settings.needs_face());
    }

    #[test]
    fn worker_status_parses_face_states() {
        let status = WorkerStatus::parse("fps=29.6\nframe_ms=12.0\nface=no\n").unwrap();
        assert_eq!(status.face, Some(false));
        assert!((status.fps - 29.6).abs() < 0.01);
        assert_eq!(
            WorkerStatus::parse("fps=30\nface=none\n").unwrap().face,
            None
        );
        assert!(WorkerStatus::parse("garbage").is_none());
    }
}
