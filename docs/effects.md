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

## One process, optional window

`main` owns a `Core` (camera handle, effects controller, saved settings,
tray, command channel, session monitor) and runs it in a loop: while the
window is open, `App::update` calls `Core::tick` and draws; when the window is
closed and *keep running* is on, `main` keeps calling `Core::tick` ten times a
second and opens the window again on request. eframe keeps its event loop
between `run_native` calls, which is what makes reopening possible.

With the window closed, or in the compact controls-only layout (`compact` in
`app.conf`), the controller writes `preview=false`, so the worker stops
sending frames; if no virtual camera exists it stops the worker altogether
and restarts it when the preview returns.

`Core` looks for the loopback device once a second. `Broadcast` in
`effects.rs` turns device, worker and viewer count into the state shown on
the effects card, and when the device appears while effects run in the
preview only, the worker is restarted so it goes live by itself.

Outside commands arrive on a Unix socket (`obsbot_core::app_ipc`, one command
line in, one reply line out) from `obsbotctl app …`, and from the tray menu.
Both feed the same `Command` channel that `Core::handle` drains.

## Session settings

Some output options are not part of a profile: mirror, flip, the away text,
the name, title and design of the badge, the logo path, the whiteboard corners of the
current camera position, and the hold state. The controller appends them to
`effects.conf` after the profile settings; later lines win, which is also how
it overrides auto-framing while whiteboard corners are being placed.

## Profiles

A profile is a snapshot of every effect setting, stored as
`profiles/<name>.conf` next to `effects.conf` in the same `key=value` format
(without `output=` and `fps=`, which stay per machine). Applying one copies it
over the live settings, which then reach the worker like any other edit. The
GUI shows a profile as selected whenever the live settings equal it exactly.
Starter profiles are written only when the folder does not exist yet, so
deleting them is permanent. See `crates/obsbot-gui/src/profiles.rs`.

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
| `triggers` | comma-separated: `hearts`, `rainbow`, `questions`, `kisses`, `storm`, `applause`, `idea`, `fireworks` |
| `storm_sensitivity` | 0–1, how readily a concerned look starts the storm |
| `question_color`, `star_color` | `blue`, `gold`, `pink`, `green`, `purple`, `red`, `orange`, `white` |
| `background`, `background_path`, `blur` | `off`, `blur`, `image` |
| `auto_enhance` | amount; exposure and white-balance correction |
| `autoframe_on`, `autoframe` | flag; tightness 0 (wide) .. 1 (close-up) |
| `faces` | 1..4 people to follow |
| `badge`, `badge_name`, `badge_title` | flag and texts for the name badge |
| `badge_style`, `badge_layout` | `bar`, `underline`, `pill`, `block`, `plain`; `stacked`, `inline` |
| `badge_position` | `top-left`, `top-center`, `top-right`, `bottom-left`, `bottom-center`, `bottom-right` |
| `badge_size`, `badge_margin`, `badge_opacity` | amounts |
| `badge_accent` | `r,g,b` |
| `badge_font`, `badge_bold`, `badge_caps` | a fontconfig family name (empty for the default sans-serif); flags |
| `logo` | path of the corner logo (written only while the logo is on; the profile stores `logo_on`) |
| `mirror`, `flip` | flags, applied to the camera frame before anything else |
| `keystone` | eight 0..1 numbers: top-left, top-right, bottom-right, bottom-left corners to straighten |
| `hold`, `hold_text` | `off`, `freeze`, `card`; the card's message |
| `preview` | `false` while the app window is closed |

## Order of operations

`Effects.process` in `processor.py` handles one camera frame. If a hold is on
it returns the frozen frame or the away card and does nothing else — no
detection runs and nothing live leaves the worker. Otherwise:

0. **Geometry**: mirror/flip, then whiteboard straightening, so everything
   after sees the picture the viewer will see.
1. **Auto-correct** (`enhance.py`), exposing for the face found last frame.
2. **Detect** up to `faces` faces (478 landmarks with depth, plus expression
   scores) and, if needed, the person outline. Each face is paired with its
   own smoothing history, because the tracker may reorder faces.
3. **Paint on the untouched faces** — skin, teeth, lips, blush, eyes, face
   paint — so make-up follows any later reshaping.
4. **Reshape** — beauty tweaks and funny distortions for every face
   accumulate into one displacement field (`warp.py`) that is resampled once.
5. **Background**, then **lights** (`lighting.py`), using the person outline.
6. **Anonymise** (`privacy.py`), late, so nothing drawn earlier shows through.
7. **Stickers** on every face; **expression triggers** on the largest.
8. **Colour**: LUT, look, glow, vignette, grain.
9. **Auto-framing** (`framing.py`), then **logo and name badge**
   (`overlay.py`) last, so branding is never cropped, graded or mirrored.

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
| `enhance.py` | Auto-correct: measured a few times a second, applied as one lookup table |
| `framing.py` | Whiteboard straightening and the smoothed auto-framing crop |
| `overlay.py` | Name badge, logo and the away card, with text drawn by Pillow |
| `backgrounds/generate.py` | Regenerates the bundled background pictures |
| `privacy.py` | The anonymiser, including holding the hidden area when tracking drops |
| `art.py` | Procedurally drawn sprites (no image assets) and the sprite blitter |
| `stickers.py` | Where each sprite sits relative to the head |
| `triggers.py` | Expression-triggered particles, rainbow and storm cloud; applause and idea effects |
| `gestures.py` | Clap, raised-index-finger and shaken-fist detection from hand landmarks (the hand tracker runs only while `applause`, `idea` or `fireworks` is on) |
| `luts/generate.py` | Regenerates the bundled `.cube` files from formulas |

GUI side (`crates/obsbot-gui/src/`):

| File | Role |
| --- | --- |
| `main.rs` | `Core` (everything that outlives the window), the window, the Camera panel |
| `effects.rs` | Effect catalogue, `EffectSettings`, `Session`, worker control |
| `effects_ui.rs`, `app_panel.rs`, `positions_ui.rs` | The Effects and App panels and the Positions card |
| `profiles.rs`, `positions.rs`, `camera_store.rs`, `app_settings.rs` | Things saved to disk |
| `luts.rs`, `backgrounds.rs`, `picker.rs` | LUT and background libraries, the file chooser |
| `commands.rs`, `ipc.rs`, `tray.rs` | Outside commands, their socket, the tray icon |
| `automation.rs` | Lock and viewer detection, and the auto-sleep decision |
| `paths.rs`, `conf.rs` | Finding the runtime (installed or checkout); `key=value` helpers |
| `widgets.rs`, `theme.rs`, `preview.rs` | Custom controls, colours, the live view |

## Design notes

- **Cost must not scale with effect count where avoidable.** Warps share one
  resample; lights share one gain map; face paint and LUTs have fixed-size
  work. Typical combinations take roughly 15–20 ms per frame at 1080p on a
  desktop CPU; stacking nearly everything is around 45 ms at 720p.
- **Expression triggers compare against a resting level**, not fixed
  thresholds (`Resting` in `triggers.py`), because scores such as "brows
  down" rest at very different values from face to face. The storm uses
  `Usual`, which settles in the middle of the everyday wobble instead of at
  its floor, plus a short hold and the user's sensitivity setting.
- **Clapping close to the camera is seen as one hand, not two.** The palms
  overlap, so the tracker reports a single hand that drifts and snaps back
  with every clap. `Shake` in `gestures.py` counts those direction changes;
  the same shaking is a clap with open hands and a cheer with a fist. The
  two-hand rule (palms closing by a small distance) covers hands further away.
  The hand model (`hand_landmarker.task`) is optional: without it the worker
  logs a note and the three gestures stay off.
- **LUTs** are expanded once into a table with an entry for every 24-bit
  colour (about 67 MB while a LUT is selected), so grading a frame is a single
  lookup.
- **Auto-privacy only wakes a camera it put to sleep itself**
  (`AutoSleep` in `automation.rs`), and the camera takes about three seconds
  to report a sleep or wake, so `Core` trusts its own command for a few
  seconds before believing the camera again.
- **Viewer detection** counts other processes of the same user that hold the
  virtual camera open, by reading `/proc/<pid>/fd`. It cannot see other
  users' processes.
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
