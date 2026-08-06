# OBSBOT Tiny 2 Lite — Linux Control Center

A Linux control center for the OBSBOT Tiny 2 Lite webcam, aiming for feature
parity with the official OBSBOT Center (Windows/macOS only).

## How the camera works on Linux today

- **Video stream:** works out of the box via the kernel `uvcvideo` driver
  (`/dev/video*`). No kernel driver needs to be written.
- **Standard controls** (exposure, white balance, focus, some zoom): exposed as
  V4L2 controls, settable with `v4l2-ctl`.
- **Proprietary features** (gimbal, AI tracking, presets, gestures, sleep):
  driven through UVC Extension Unit (XU) commands — vendor-specific USB
  requests on the same device. Community projects (Tiny4Linux, meet4k,
  obsbot-controller) have reverse-engineered much of this protocol for the
  Tiny 2 / Meet 4K family.
- **Official SDK:** OBSBOT offers a Linux (Ubuntu) SDK covering the Tiny 2
  Lite, gated behind an application form. Plan: apply for it and use it as
  protocol documentation / validation reference, but build the shipping app on
  the open V4L2 + XU path so it stays redistributable open source.

## Feature list (parity with OBSBOT Center)

### Gimbal & zoom
- [x] Pan/tilt control (virtual joystick + keyboard arrows)
- [x] Zoom slider (1–4x digital)
- [x] Recenter / reset gimbal
- [x] Preset positions: save & recall (OBSBOT Center offers 3–4 slots)
- [ ] Boot position behavior

### AI features
- [x] Human tracking on/off
- [x] Tracking modes: standard / upper body / close-up / headroom, desk mode
- [x] Tracking speed (slow / standard / fast)
- [ ] Auto-framing / auto-zoom
- [ ] Gesture control toggles (target select, dynamic zoom)

### Image
- [x] Exposure: auto / manual (EV compensation, ISO, shutter)
- [x] Face-priority auto exposure
- [x] White balance: auto / manual (color temperature)
- [x] HDR on/off
- [x] Anti-flicker: off / 50 Hz / 60 Hz
- [x] Brightness / contrast / saturation / sharpness / hue
- [x] Field of view selection (wide / medium / narrow)
- [x] Focus: auto / manual, face-priority AF
- [ ] Mirror / flip
- [ ] Image style presets

### Device
- [x] Sleep / wake, auto-sleep timer
- [ ] Status LED behavior
- [ ] Audio: mic gain, noise reduction (if exposed by firmware)
- [ ] Device info: model, firmware version, serial
- [ ] Settings persistence & restore on reconnect
- [ ] Firmware update (stretch goal — high risk, official tool may stay required)

### Linux-native extras (beyond OBSBOT Center)
- [x] CLI (`obsbotctl`) with full feature coverage — scriptable
- [x] Live preview in the GUI
- [ ] Profiles (e.g. "meeting", "streaming") switchable via CLI/hotkey
- [ ] System tray quick controls
- [ ] D-Bus API so other tools (Stream Deck, OBS scripts) can drive the camera
- [x] Hot-plug handling (udev)
- [ ] Packaging: Flatpak, AUR, .deb

## Architecture

```
┌────────────────────────────────────────────┐
│ GUI Control Center          System tray    │
├────────────────────────────────────────────┤
│ CLI (obsbotctl)   │   D-Bus service (opt.) │
├────────────────────────────────────────────┤
│ libobsbot: device model, profiles          │
│  ├─ V4L2 backend (standard UVC controls)   │
│  └─ UVC XU backend (proprietary commands)  │
├────────────────────────────────────────────┤
│ Kernel: uvcvideo (unmodified)              │
└────────────────────────────────────────────┘
```

Language: Rust (matches the ecosystem we can borrow from — Tiny4Linux, tiny2,
meet4k are all Rust/adjacent; safe USB/V4L2 bindings exist: `rusb`,
`v4l`/`v4l2` crates). GUI: **native toolkit only** — Iced, egui, or
GTK4/libadwaita (decide in Phase 3). No webview, no Electron/Tauri, and no
local webserver: the control center is a plain desktop process talking
directly to the device.

## Roadmap

### Phase 0 — Reconnaissance (1–2 sessions)
- Apply for the official OBSBOT SDK (reference material).
- Plug in the camera; capture `lsusb -v`, `v4l2-ctl --list-devices`,
  `--list-ctrls-menus`, `--list-formats-ext` output into `docs/device-recon/`.
- Enumerate the UVC extension units and their GUIDs.
- Study Tiny4Linux / meet4k source for the known XU command set.
- Pick a license (MIT or GPL) and init the repo.

**Milestone:** we know exactly which controls are standard V4L2 vs. XU, and
have a documented protocol notes file.

### Phase 1 — Protocol layer (core risk retirement)
- Implement XU read/write plumbing (UVCIOC_CTRL_QUERY ioctls).
- Port/verify the known Tiny 2 command set against the Tiny 2 Lite; document
  differences.
- Prove the "big four" end to end: wake/sleep, pan/tilt, zoom, tracking on/off.

**Milestone:** a Rust test binary moves the gimbal on your desk.

### Phase 2 — Library + CLI
- `libobsbot` crate: typed device API, error handling, hot-plug.
- `obsbotctl` CLI covering every proven feature.
- Config file + named profiles.

**Milestone:** full camera control from the terminal; scriptable.

### Phase 3 — Control Center GUI (MVP)
- Window with live preview, joystick pad, zoom slider, preset buttons,
  tracking toggle, image controls.
- Settings persistence.

**Milestone:** daily-drivable replacement for OBSBOT Center basics.

### Phase 4 — Full parity
- All AI modes, gestures config, FOV, HDR, anti-flicker, face AE/AF.
- Tray icon, hotkeys, D-Bus service.

**Milestone:** feature checklist above fully green (minus firmware update).

### Phase 5 — Ship it
- Flatpak + AUR + .deb packaging, udev rules, docs, screenshots.
- Publish; upstream protocol findings to the community projects.

## Risks

- **Tiny 2 Lite ≠ Tiny 2:** the reverse-engineered command set targets the
  Tiny 2; some commands may differ or be missing. Phase 1 exists to find out
  early. Fallback: USB captures from OBSBOT Center in a Windows VM with USB
  passthrough (Wireshark/usbmon) to recover any missing commands.
- **SDK license:** if the official SDK is usable but closed, keep it strictly
  as a reference — never link it into the open-source build.
- **Firmware update:** bricking risk; deliberately last / possibly out of scope.
