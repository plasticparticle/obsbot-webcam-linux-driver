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
bash scripts/setup-virtual-camera.sh --persist   # optional: share with other apps
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
| **Lights** | **Auto-correct** for dim rooms and colour casts, and virtual studio lighting: a key light you aim by dragging a lamp around your head (brightness and warmth), fill light, a coloured rim light and background dimming; presets for Studio, Soft, Dramatic, Spotlight, Golden, Neon and Night |
| **Beauty** | Skin smoothing and warmth, blush, face and nose slimming, bigger and brighter eyes, eye colour, lip colour, teeth whitening; *Natural* and *Glam* presets |
| **Stickers** | Shades, heart glasses, puppy, kitty, bunny, moustache, crown, party hat, top hat, halo, horns and a red nose — combine freely, they follow your head |
| **Funny** | Big head, tiny face, bug eyes, alien, big mouth, big nose, chipmunk cheeks and swirl |
| **Magic** | Effects set off by your expression: smile for hearts, open your mouth for a rainbow, raise your eyebrows for jiggling question marks, blow a kiss for flying hearts, look concerned for a thundercloud with rain and lightning (with a sensitivity slider), clap for rising clapping hands, raise your index finger for a light bulb and an explosion of stars, shake your fist for fireworks |
| **Paint** | Face paint that keeps your natural shading: hero mask, anon mask, clown, skull, zombie, gold and freckles; plus an anonymiser that pixelates, blurs or blacks out your eyes or whole face |
| **Scene** | Background blur or a picture from a thumbnail library (six bundled, add your own); **auto-framing** that keeps you centred; how many people to follow (1–4); mirror and flip; a **name badge** with name and title, whose design you control (five styles, title below or on one line, six positions, size, edge distance, colour, panel opacity, any installed font, bold and capitals); a corner **logo** |

Everything that is switched on appears as a removable chip above the preview,
each tab has its own **Reset**, and clicking a slider's value restores its
default. Settings are saved to
`$XDG_CONFIG_HOME/obsbot-control-center/effects.conf` and restored on the next
launch.

**Profiles:** the Profiles card at the top of the Effects panel stores every
effect setting under a name. Click a profile to switch to it, or press
**Ctrl+1** … **Ctrl+9** anywhere in the window for the first nine (in
alphabetical order). **Save as…** stores the current settings; after tweaking
an applied profile you can **Update** or **Revert** it; an unmodified profile
can be deleted. The built-in **Neutral** profile is always first (Ctrl+1): it
switches every effect off and cannot be changed or deleted. Four starters
(Incognito, Meeting, Party, Studio) are created on first launch. Profiles are plain files in
`$XDG_CONFIG_HOME/obsbot-control-center/profiles/`, so they are easy to back
up or share.

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
output size if many effects are stacked. The virtual camera
needs the `v4l2loopback` kernel module from your distribution; `--persist`
recreates the camera at every boot, without it the camera lasts until the next
reboot, and `--remove` takes it away again.

The **Effects camera** card shows what other apps can get right now: *Not set
up*, *Ready* (set up, but hidden from other apps until the effects camera is
on), *Connecting*, or *On air* with the number of programs that have the
camera open. Other apps only list the virtual camera while effects are being
sent to it, so keep the control center running during a call (**App → Keep
running when the window is closed**) and reload the page or restart the video app if
the camera is still missing from its list.

The hand gestures in **Magic** — clap, raise your index finger, shake your
fist — use a hand tracker that only runs while one of them is switched on.
Clapping needs three quick claps; with your hands close to the camera, fast
waving can also set it off. The question marks and the idea stars have their
own colour, and the thundercloud has a sensitivity slider.

## Everyday use in the office

**Controls only.** *Hide preview*, at the top of the controls column, shrinks
the window to the controls so you do not see yourself twice during a call.
The on-air status and the step-away buttons move into the column. With the
preview hidden the app stops drawing pictures for itself, so effects keep
running only when the virtual camera is set up.

**Stepping away.** *Be right back* and *Freeze*, above the preview, replace
your picture with an away card or the last frame. Nothing live is sent while
either is on. The message is set under **App → Away message**.

**Always ready.** In the **App** panel:

- *Start with* a chosen profile, so yesterday's effects never follow you into
  a call, and optionally switch effects on at start.
- *Keep running when the window is closed* leaves the app in the tray. The
  tray menu switches profiles and toggles effects, *Be right back*, *Freeze*
  and privacy mode. With the window closed the app stops reading the camera
  unless the virtual camera is in use.
- *Start in the tray when I log in* writes an XDG autostart entry. Desktops
  that ignore those (plain Hyprland, for example) need one line instead:
  `exec-once = obsbot-control-center --background`.

**Shortcuts that work everywhere.** Every action is a command you can bind in
your desktop's keyboard settings:

```sh
obsbotctl app brb                 # toggle the away card
obsbotctl app brb Back at 3       # …with a message
obsbotctl app freeze
obsbotctl app privacy toggle      # camera sleep
obsbotctl app effects toggle
obsbotctl app profile Meeting     # or: profile next / profile previous
obsbotctl app position Whiteboard
obsbotctl app show                # or: hide, quit, status
```

For Hyprland: `bind = SUPER, F9, exec, obsbotctl app brb`.

**Automatic privacy** (App panel, both off by default): sleep the camera while
the screen is locked, and sleep it when nothing is watching — that is, when
this window is closed and no other program has the virtual camera open. The
camera wakes by itself afterwards, unless you put it to sleep yourself.

**Camera settings that stick.** Zoom, field of view, tracking mode, exposure,
white balance and the other Image settings are saved as you change them and
put back whenever the camera connects.

**Positions.** The Camera panel's *Positions* card remembers where the camera
points and how far it is zoomed, under names of your choice ("Desk",
"Whiteboard", "Room"). A position can also **straighten a whiteboard**: drag
four dots onto its corners in the preview and it fills the picture, squared
up. Recalling such a position switches tracking off so the camera holds still.

**Meeting rooms.** *People to follow* on the Scene tab makes effects,
auto-framing and the anonymiser work for up to four faces. Expression
triggers and the key light follow the largest face.

## Installing

`scripts/install.sh` builds the release and installs it for your user — the
programs to `~/.local/bin`, the effects to
`~/.local/share/obsbot-control-center`, plus a launcher entry — so the app no
longer depends on this checkout. It also creates the effects runtime (a Python
environment and the MediaPipe models, about 300 MB) and sets up the virtual
camera so it is there after every boot; that last step is the only one that
asks for your password, and the install still finishes without it. `--prefix
DIR` installs elsewhere, `--skip-runtime` leaves the runtime alone,
`--skip-virtual-camera` leaves the system untouched, and `--uninstall` removes
everything, including the virtual camera, except your settings. `obsbot-control-center --paths` shows which
runtime the app found. There are no distribution packages yet.

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
