# OBSBOT Tiny 2 Lite — control protocol notes

Sources: reverse-engineering by the Tiny4Linux, cgevans/tiny2 and
samliddicott/meet4k projects (EUPL-1.2 licensed research, protocol facts
re-verified here against a Tiny 2 Lite), plus our own device recon
(`docs/device-recon/`).

## Transport

All control goes through the UVC video node (`/dev/video0`):

- **Standard features** — plain V4L2 controls (`VIDIOC_G_CTRL`/`S_CTRL`).
  Pan/tilt absolute (arcseconds), zoom 0–100, focus, exposure, WB, image
  params. See `device-recon/ctrls.txt`.
- **Proprietary features** — UVC Extension Unit queries
  (`UVCIOC_CTRL_QUERY`, ioctl `'u' 0x21`) against **XU unit 2**
  (GUID `9a1e7291-...`). Always issue `UVC_GET_LEN` first; SET_CUR payloads
  are zero-padded to the reported length (60 bytes).

## XU unit 2, selector 0x06 — simple commands (SET_CUR)

| Command | Bytes | Notes |
|---|---|---|
| HDR off/on | `01 01 00/01` | |
| Face-priority AE off/on | `03 01 00/01` | |
| FOV wide/normal/narrow | `04 01 01/02/03` | 86°/78°/65° (tiny2 values; meet4k uses 0/1/2 — verify) |
| AI mode | `16 02 <m> <n>` | see table below |
| Auto-sleep timer | `0b 02 <u16 LE seconds>` | from meet4k, verify on Tiny 2 Lite |
| Noise reduction off/on | `0a 01 00/01` | from meet4k, verify |
| Auto-framing | `0d 02 ...` | from meet4k, verify |

AI modes (`m`, `n`): none 0,0 · group 1,0 · normal 2,0 · upper-body 2,1 ·
close-up 2,2 · headless 2,3 · lower-body 2,4 · whiteboard 4,0 · desk 5,0 ·
hand 6,0.

## XU unit 2, selector 0x06 — status (GET_CUR, 60 bytes)

| Offset | Meaning |
|---|---|
| 0x02 | sleep state: 0 awake, 1 sleeping |
| 0x06 | HDR: 0 off, 1 on |
| 0x18, 0x1c | AI mode `(m, n)` as above |
| 0x21 | tracking speed: 0 standard, 2 sport |

## XU unit 2, selector 0x02 — framed "V3" commands

Decoded fully by the [obsbot-mcp](https://github.com/lxman/obsbot-mcp)
project (MIT). 60-byte zero-padded frames:

```
off 0     0xAA magic
off 1     flags: 0x25 = SET, 0x01 = header-only GET
off 2-3   seq u16 LE (camera does not enforce continuity)
off 4-5   len u16 LE = 0x0C (bytes covered by the token)
off 6-7   token u16 LE = CRC-16/USB over frame[0..12] with this field zeroed
off 8     sender (host = 0x0A)
off 9     receiver (0x02 camera, 0x03 gimbal-home, 0x04 AI)
off 10-11 cmd u16 LE
off 12+   nested payload segment: len2 u16, token2 u16 (CRC-16/USB over
          [12..16+len2] with token2 zeroed), payload bytes
```

CRC-16/USB: poly 0xA001 (reflected 0x8005), init 0xFFFF, xorout 0xFFFF.
**The camera validates both tokens** (verified on the Tiny 2 Lite: zeroed
tokens → frame silently ignored). Synthesized frames with correct CRCs are
accepted — verified live for every command below.

Commands verified working on the Tiny 2 Lite (cmd / receiver / payload):

| Command | cmd | recv | payload |
|---|---|---|---|
| wake / sleep | `0xA0C2` | 0x02 | `u8 state` (0 wake, 1 sleep) + 3 pad |
| recenter | `0x00C3` | 0x03 | 6 zero bytes |
| gimbal speed | `0x6484` | 0x04 | roll, pitch, yaw — 3× f32 LE deg/s |
| tracking speed | `0x0CC4` | 0x04 | `u8` 0 standard / 2 sport |
| preset add | `0x3944` | 0x04 | `idx u32` + pan,pitch,roll,zoom f32 + f32 −1000 |
| preset update | `0x3E04` | 0x04 | same as add |
| preset recall | `0x39C4` | 0x04 | `idx u32` + 4× f32 `1.0` |
| preset delete | `0x3984` | 0x04 | `idx u32` |

Pitch on the wire is positive-down (negate UVC tilt). Presets are read via
flat selectors 12 (list: `count u8` + slot indices; echo it back via SET_CUR
to reset the cursor) and 13 (entry: slot@1, pitch i16 LE ÷100 @4,
yaw i16 LE ÷100 @6, zoom u8 ÷100 @8, name @10).

**Safety (from obsbot-mcp's hardware testing): never blind-sweep unknown
opcodes** — one full GET sweep dropped the device off the USB bus. Avoid
the Upgrade/SysMg/BLE subsystems entirely. Writes are silently ignored
while the camera sleeps — wake first.

**Linux caveat:** `uvcvideo` serves pan/tilt `VIDIOC_G_CTRL` from a
setpoint cache, so after vendor-path moves (preset recall, recenter) the
V4L2 position readback is stale.
