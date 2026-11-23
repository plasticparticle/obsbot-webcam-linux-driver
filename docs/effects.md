# Effects pipeline

How the Effects panel turns settings into pictures, and how to extend it.
User-facing instructions are in the README; this file is for contributors.

## Overview

```
┌──────────────────────────┐  settings file   ┌──────────────────────────────┐
│ obsbot-control-center    │ ───────────────► │ effects/processor.py (worker) │
│  effects.rs   state      │                  │  camera → effects → output    │
│  effects_ui.rs panel     │ ◄─────────────── │                               │
│  preview.rs   live view  │  frames (stdout) └───────────────┬──────────────┘
└──────────────────────────┘  status file                     │ optional
                                                              ▼
                                               /dev/video42 "OBSBOT Virtual Camera"
```

The GUI never processes video itself. When the first effect is switched on it
releases the camera and starts the Python worker, which opens the camera,
applies the effects and sends finished frames back. If the `v4l2loopback`
device exists the worker also publishes to it; otherwise the effects are
visible in the app's preview only (the panel shows **PREVIEW** rather than
**LIVE**).

## The three channels

| Channel | Direction | Format |
| --- | --- | --- |
| Settings | GUI → worker | `key=value` lines in `$XDG_CONFIG_HOME/obsbot-control-center/effects.conf`. Written with write-then-rename; the worker re-reads it when its modification time changes, so edits apply live. Unknown keys and bad values fall back to defaults on both sides. |
| Frames | worker → GUI | The worker's stdout. Each frame is `b"OBSF"`, little-endian `u32` width and height, then `width × height × 3` RGB bytes. Frames wider than 1280 px are scaled down for the preview. Everything else the worker prints goes to stderr. |
| Status | worker → GUI | A small `key=value` file written about once a second: `fps`, `frame_ms`, and `face` (`yes`, `no`, or `none` when no enabled effect needs a face). |

`output=` and `fps=` in the settings file belong to the GUI (the worker takes
them as command-line arguments); the worker ignores them.

## Settings keys

Amounts are `0..1` unless noted. Names must match between
`crates/obsbot-gui/src/effects.rs` and the worker.

| Key | Values |
| --- | --- |
| `look`, `look_strength` | `none`, `golden`, `arctic`, `pop`, `vintage`, `cinema`, `sunset`, `dream`, `neon`, `mono`, `noir`, `sepia` |
| `lut`, `lut_strength` | absolute path of a 3D `.cube` file; empty for none |
| `grain`, `vignette`, `glow` | amounts |
| `light_key`, `light_x`, `light_y`, `light_warmth` | key light; position and warmth are `-1..1`, `+y` is above the head |
| `light_fill`, `light_rim`, `light_rim_color`, `light_dim` | rim colours: `white`, `warm`, `blue`, `pink`, `purple`, `green` |
| `smoothing`, `slimming`, `nose_slim`, `eye_size`, `eye_brighten`, `blush`, `teeth` | amounts |
| `skin_tone` | `-1..1` |
| `eye_color` | `natural`, `blue`, `green`, `hazel`, `gray`, `violet`, `amber` |
| `lip_color`, `lip_strength` | `none`, `rose`, `red`, `coral`, `berry`, `plum`, `nude` |
| `stickers` | comma-separated: `sunglasses`, `heart_glasses`, `dog`, `cat`, `bunny`, `moustache`, `crown`, `party_hat`, `top_hat`, `halo`, `horns`, `clown_nose` |
| `distortion`, `distortion_strength` | `none`, `big_head`, `tiny_face`, `bug_eyes`, `alien`, `big_mouth`, `big_nose`, `chipmunk`, `swirl` |
| `mask`, `mask_strength` | face paint: `none`, `hero`, `anon`, `clown`, `skull`, `zombie`, `gold`, `freckles` |
| `anonymise`, `anonymise_style`, `anonymise_amount` | `off`/`eyes`/`face`; `pixel`/`blur`/`bar` |
| `triggers` | comma-separated: `hearts`, `rainbow`, `stars`, `kisses`, `storm` |
| `background`, `background_path`, `blur` | `off`, `blur`, `image` |

## Order of operations

`Effects.apply` in `processor.py` runs these steps on every frame. The order
is deliberate:

1. **Detect** the face (478 landmarks with depth, plus expression scores) and,
   if needed, the person outline. Landmarks are smoothed over time.
2. **Paint on the untouched face** — skin, teeth, lips, blush, eyes, face
   paint — so make-up follows any later reshaping.
3. **Reshape** — beauty tweaks and funny distortions accumulate into one
   displacement field (`warp.py`) that is resampled once. The landmarks and
   the person outline are moved with it.
4. **Background**, then **lights** (`lighting.py`), using the person outline.
5. **Anonymise** (`privacy.py`), late, so nothing drawn earlier shows through.
6. **Stickers** and **expression triggers** on top.
7. **Colour**: LUT, look, glow, vignette, grain — last, so stickers and
   backgrounds share the grade.

## Modules

| File | Role |
| --- | --- |
| `processor.py` | Settings parsing, camera and output I/O, the pipeline above |
| `landmarks.py` | `Face` (landmarks, head axes and scale), landmark index lists, smoothing |
| `looks.py` | Looks as a 3×3 matrix plus per-channel curves; `.cube` loading; film finish |
| `lighting.py` | Key, fill and rim light and background dimming as one low-resolution gain map |
| `beauty.py` | Retouching inside small face regions |
| `warp.py` | The displacement field, face reshaping and distortions |
| `masks.py` | Face paint, drawn on a fixed-size canvas so cost does not grow with resolution |
| `privacy.py` | The anonymiser, including holding the hidden area when tracking drops |
| `art.py` | Procedurally drawn sprites (no image assets) and the sprite blitter |
| `stickers.py` | Where each sprite sits relative to the head |
| `triggers.py` | Expression-triggered particles, rainbow and storm cloud |
| `luts/generate.py` | Regenerates the bundled `.cube` files from formulas |

## Design notes

- **Cost must not scale with effect count where avoidable.** Warps share one
  resample; lights share one gain map; face paint and LUTs have fixed-size
  work. Typical combinations take roughly 15–20 ms per frame at 1080p on a
  desktop CPU; stacking nearly everything is around 45 ms at 720p.
- **Expression triggers compare against a resting level**, not fixed
  thresholds (`Resting` in `triggers.py`), because scores such as "brows
  down" rest at very different values from face to face.
- **LUTs** are expanded once into a table with an entry for every 24-bit
  colour (about 67 MB while a LUT is selected), so grading a frame is a single
  lookup.
- **The anonymiser is not a security feature.** It covers nothing until a
  face is first detected, and other programs reading the camera directly see
  the unfiltered picture.

## Adding an effect

1. Implement it in the matching worker module and call it from the right
   step of `Effects.apply`.
2. Add its key to `Settings` and the field lists in `processor.py`.
3. Add it to the catalogue and `EffectSettings` in
   `crates/obsbot-gui/src/effects.rs` (`to_config`, `from_config`, `active`,
   `clear`), and to its tab in `effects_ui.rs`.
4. Icons must exist in the bundled egui fonts; the glyph tests fail otherwise.
5. Extend the round-trip test in `effects.rs`, then run
   `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.
