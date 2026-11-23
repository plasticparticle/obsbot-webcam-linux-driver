# OBSBOT Linux Control Center

Native Linux control for the **OBSBOT Tiny 2 Lite** webcam (and closely
related Tiny 2 family models): gimbal, presets, AI tracking, image tuning —
the features OBSBOT Center provides on Windows/macOS, without OBSBOT Center.

No kernel driver needed: video streams through the standard `uvcvideo`
driver; this project drives everything else over V4L2 controls and the
camera's UVC extension unit (see [docs/protocol.md](docs/protocol.md)).

## Important: unofficial third-party software

> [!CAUTION]
> **This is unofficial third-party software.** OBSBOT Linux Control Center is
> an independent community project. It is not affiliated with, endorsed,
> sponsored, supported, or approved by OBSBOT.

OBSBOT, OBSBOT Center, product names, logos, and related trademarks belong to
their respective owners. No trademark ownership is claimed by this project.

## Caution: pre-release software

> [!WARNING]
> **This is a pre-release version and has not undergone security hardening.**

Expect bugs and unfinished features. Review setup scripts before running them,
do not expose the application to untrusted users, and do not rely on it for
security-critical, privacy-critical, or production use.

## Components

- **`obsbot-core`** — Rust library: device discovery, V4L2 controls,
  vendor frame protocol (CRC-16/USB framed commands).
- **`obsbotctl`** — CLI: every feature, scriptable.
- **`obsbot-control-center`** — native GUI (egui): live preview, gimbal
  pad, presets, AI tracking, image settings. No webview, no local server.

## Build

```sh
cargo build --release
# binaries: target/release/obsbotctl, target/release/obsbot-control-center
```

Requires only a Rust toolchain; the GUI needs the usual desktop GL/Wayland
or X11 libraries at runtime.

## Virtual camera and effects

The control center applies effects to the live preview and can publish the
result as **OBSBOT Virtual Camera** for browsers, meeting apps, and OBS.
One-time setup:

```sh
bash scripts/setup-effects.sh          # effects runtime (required)
bash scripts/setup-virtual-camera.sh   # optional: share with other apps
cargo run --release -p obsbot-gui
```

The second script needs the `v4l2loopback` kernel module (for example
`v4l2loopback-dkms`) and tells you how to install it if it is missing. Without
it, effects still work in the preview; the panel then shows **PREVIEW**
instead of **LIVE**.

The effects runtime uses local MediaPipe models for person segmentation and
facial landmarks; frames are not uploaded. In the GUI, switch the side panel
to **Effects**. Picking any effect switches the effects camera on; to use it
elsewhere, select **OBSBOT Virtual Camera** in the receiving application.

| Tab | What it does |
| --- | --- |
| **Looks** | Eleven colour grades (Golden, Arctic, Pop, Vintage, Cinema, Sunset, Dream, Neon, Mono, Noir, Sepia) with an intensity slider; eight bundled LUTs and your own imported `.cube` LUTs; plus film grain, vignette and glow |
| **Lights** | Virtual studio lighting: a key light you aim by dragging a lamp around your head (brightness and warmth), fill light, a coloured rim light and background dimming; presets for Studio, Soft, Dramatic, Spotlight, Golden, Neon and Night |
| **Beauty** | Skin smoothing and warmth, blush, face and nose slimming, bigger and brighter eyes, eye colour, lip colour, teeth whitening; *Natural* and *Glam* presets |
| **Stickers** | Shades, heart glasses, puppy, kitty, bunny, moustache, crown, party hat, top hat, halo, horns and a red nose — combine freely, they follow your head |
| **Funny** | Big head, tiny face, bug eyes, alien, big mouth, big nose, chipmunk cheeks and swirl |
| **Magic** | Effects set off by your expression: smile for hearts, open your mouth for a rainbow, raise your eyebrows for stars, blow a kiss for flying hearts, look concerned for a thundercloud with rain and lightning |
| **Paint** | Face paint that keeps your natural shading: hero mask, anon mask, clown, skull, zombie, gold and freckles; plus an anonymiser that pixelates, blurs or blacks out your eyes or whole face |
| **Backdrop** | Background blur or a replacement picture |

Everything that is switched on appears as a removable chip above the preview,
each tab has its own **Reset**, and clicking a slider's value restores its
default. Settings are saved to
`$XDG_CONFIG_HOME/obsbot-control-center/effects.conf` and restored on the next
launch.

**LUTs:** the Looks tab lists the bundled LUTs from `effects/luts/` and any
3D `.cube` file you add with **Add LUT…**, by pasting a path, or by dropping
it into `$XDG_CONFIG_HOME/obsbot-control-center/luts/` (**Open folder**).
Imported LUTs can be removed again from the same tab. The bundled LUTs are
computed by `effects/luts/generate.py`, not taken from any film stock or
commercial pack.

Sticker artwork is drawn in code (`effects/art.py`), so the project
ships no third-party image assets.

How the pipeline works, the settings file format and how to add an effect
are described in [`docs/effects.md`](docs/effects.md).

Use 720p/30 for the best quality/performance balance. Resolution or frame-rate
changes restart the stream; other effects update live. Effects run on the CPU:
the virtual camera card shows the achieved frame rate and suggests a smaller
output size if many effects are stacked. If the virtual camera is missing
after reboot, rerun `bash scripts/setup-virtual-camera.sh`.

## CLI usage

```sh
obsbotctl status                 # gimbal pose, zoom, AI state, image modes
obsbotctl pan 30                 # degrees, -130..130
obsbotctl tilt -- -20            # degrees, -90..90
obsbotctl zoom 50                # 0..100
obsbotctl recenter               # firmware gimbal home
obsbotctl sleep on|off           # privacy mode
obsbotctl track normal           # off|normal|upper-body|close-up|headless|
                                 # lower-body|desk|whiteboard|hand|group
obsbotctl speed sport            # tracking speed: standard|sport
obsbotctl preset-save 1          # save current pose to slot 1-3
obsbotctl preset 1               # recall
obsbotctl presets                # list stored presets
obsbotctl hdr on
obsbotctl fov wide               # wide(86°)|normal(78°)|narrow(65°)
obsbotctl face-ae on             # face-priority auto exposure
obsbotctl wb 5600                # Kelvin, or "auto"
obsbotctl exposure auto          # or manual time in 100µs units
obsbotctl focus auto             # or manual position 0-100
obsbotctl anti-flicker 50hz
obsbotctl image brightness 60
```

Low-level escape hatches for protocol work: `obsbotctl raw`, `xu-send`,
`xu-dump`.

## Notes & limitations

- **4K requires USB 3**: on a USB 2.0 link the camera only offers up to
  1080p (MJPG/YUYV).
- After vendor-path moves (preset recall, recenter) the kernel's cached
  pan/tilt readback is stale — a uvcvideo limitation, not a camera one.
- Firmware updates remain out of scope; use the official tool.

## Credits

Protocol knowledge builds on the reverse-engineering work of
[obsbot-mcp](https://github.com/lxman/obsbot-mcp) (MIT; CRC and frame
format), [Tiny4Linux](https://github.com/OpenFoxes/Tiny4Linux),
[cgevans/tiny2](https://github.com/cgevans/tiny2) and
[samliddicott/meet4k](https://github.com/samliddicott/meet4k).
All facts re-verified against a Tiny 2 Lite on this machine.
