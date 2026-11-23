#!/usr/bin/env python3
"""Low-latency OBSBOT effects processor for a V4L2 virtual camera."""

from __future__ import annotations

import argparse
import os
import struct
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import cv2
import mediapipe as mp
import numpy as np

import beauty
import masks
import stickers
from landmarks import Face, LandmarkSmoother
from lighting import Lights
from looks import Grader
from privacy import Anonymiser
from triggers import Triggers
from warp import WarpField, distort, reshape_face


@dataclass
class Settings:
    background: str = "off"
    background_path: str = ""
    blur: float = 0.55
    look: str = "none"
    look_strength: float = 1.0
    lut: str = ""
    lut_strength: float = 1.0
    grain: float = 0.0
    vignette: float = 0.0
    glow: float = 0.0
    smoothing: float = 0.0
    slimming: float = 0.0
    skin_tone: float = 0.0
    eye_color: str = "natural"
    eye_brighten: float = 0.0
    eye_size: float = 0.0
    nose_slim: float = 0.0
    lip_color: str = "none"
    lip_strength: float = 0.6
    blush: float = 0.0
    teeth: float = 0.0
    stickers: tuple[str, ...] = ()
    distortion: str = "none"
    distortion_strength: float = 0.6
    mask: str = "none"
    mask_strength: float = 1.0
    triggers: tuple[str, ...] = ()
    anonymise: str = "off"
    anonymise_style: str = "pixel"
    anonymise_amount: float = 0.5
    light_key: float = 0.0
    light_x: float = -0.5
    light_y: float = 0.5
    light_warmth: float = 0.0
    light_fill: float = 0.0
    light_rim: float = 0.0
    light_rim_color: str = "white"
    light_dim: float = 0.0


_UNIT_FIELDS = ("look_strength", "grain", "vignette", "glow", "smoothing", "slimming", "eye_brighten",
                "eye_size", "nose_slim", "lip_strength", "blush", "teeth", "distortion_strength",
                "mask_strength", "blur", "lut_strength", "anonymise_amount", "light_key", "light_fill", "light_rim", "light_dim")
_SIGNED_FIELDS = ("skin_tone", "light_x", "light_y", "light_warmth")
_NAME_FIELDS = ("background", "background_path", "look", "eye_color", "lip_color", "distortion", "mask",
                "light_rim_color", "lut", "anonymise", "anonymise_style")
_LIST_FIELDS = ("stickers", "triggers")


def parse_settings(text: str) -> Settings:
    """Parse `key=value` lines; unknown keys and bad values fall back to defaults."""
    values: dict[str, str] = {}
    for line in text.splitlines():
        key, separator, value = line.partition("=")
        if separator:
            values[key.strip()] = value.strip()
    settings = Settings()
    for name in _UNIT_FIELDS:
        setattr(settings, name, clamp_float(values.get(name), getattr(settings, name)))
    for name in _SIGNED_FIELDS:
        setattr(settings, name, clamp_float(values.get(name), getattr(settings, name), -1.0, 1.0))
    for name in _NAME_FIELDS:
        setattr(settings, name, values.get(name, getattr(settings, name)))
    for name in _LIST_FIELDS:
        setattr(settings, name, tuple(item.strip() for item in values.get(name, "").split(",") if item.strip()))
    return settings


class ConfigFile:
    def __init__(self, path: Path):
        self.path = path
        self.mtime_ns = -1
        self.settings = Settings()

    def refresh(self) -> Settings:
        try:
            stat = self.path.stat()
            if stat.st_mtime_ns == self.mtime_ns:
                return self.settings
            self.settings = parse_settings(self.path.read_text(encoding="utf-8"))
            self.mtime_ns = stat.st_mtime_ns
        except (OSError, ValueError):
            pass
        return self.settings


def clamp_float(value: str | None, default: float, low: float = 0.0, high: float = 1.0) -> float:
    try:
        return max(low, min(high, float(value)))
    except (TypeError, ValueError):
        return default


class VisionModels:
    def __init__(self, model_dir: Path):
        base = mp.tasks.BaseOptions
        vision = mp.tasks.vision
        self.segmenter = vision.ImageSegmenter.create_from_options(
            vision.ImageSegmenterOptions(
                base_options=base(model_asset_path=str(model_dir / "selfie_segmenter.tflite")),
                running_mode=vision.RunningMode.VIDEO,
                output_confidence_masks=True,
                output_category_mask=False,
            )
        )
        self.landmarker = vision.FaceLandmarker.create_from_options(
            vision.FaceLandmarkerOptions(
                base_options=base(model_asset_path=str(model_dir / "face_landmarker.task")),
                running_mode=vision.RunningMode.VIDEO,
                num_faces=1,
                output_face_blendshapes=True,
                min_face_detection_confidence=0.45,
                min_face_presence_confidence=0.45,
                min_tracking_confidence=0.45,
            )
        )

    def close(self) -> None:
        self.segmenter.close()
        self.landmarker.close()


class VirtualCameraSink:
    def __init__(self, device: str, width: int, height: int, fps: int):
        command = [
            "gst-launch-1.0",
            "-q",
            "fdsrc",
            "fd=0",
            "do-timestamp=true",
            "!",
            "rawvideoparse",
            "format=bgr",
            f"width={width}",
            f"height={height}",
            f"framerate={fps}/1",
            "!",
            "queue",
            "leaky=downstream",
            "max-size-buffers=1",
            "!",
            "videoconvert",
            "!",
            "video/x-raw,format=YUY2",
            "!",
            "v4l2sink",
            f"device={device}",
            "sync=false",
        ]
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
        )

    def send(self, frame: np.ndarray) -> None:
        if self.process.poll() is not None:
            raise RuntimeError(f"GStreamer virtual-camera sink exited ({self.process.returncode})")
        if self.process.stdin is None:
            raise RuntimeError("GStreamer virtual-camera sink has no input pipe")
        self.process.stdin.write(np.ascontiguousarray(frame).tobytes())

    def close(self) -> None:
        if self.process.stdin is not None:
            try:
                self.process.stdin.close()
            except BrokenPipeError:
                pass
        try:
            self.process.wait(timeout=2.0)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=2.0)


class Effects:
    def __init__(self, width: int, height: int, models: VisionModels):
        self.width = width
        self.height = height
        self.models = models
        self.last_mask: np.ndarray | None = None
        self.background_path = ""
        self.background_image: np.ndarray | None = None
        self.field = WarpField(width, height)
        self.grader = Grader(width, height)
        self.triggers = Triggers(width, height)
        self.smoother = LandmarkSmoother()
        self.lights = Lights(width, height)
        self.anonymiser = Anonymiser()
        self.last_depth: np.ndarray | None = None
        self.last_time_s: float | None = None
        # None while no enabled effect needs a face.
        self.face_found: bool | None = None

    def detect_face(self, mp_image, timestamp_ms: int) -> Face | None:
        result = self.models.landmarker.detect_for_video(mp_image, timestamp_ms)
        if not result.face_landmarks:
            self.smoother.reset()
            return None
        landmarks = result.face_landmarks[0]
        points = np.array([(p.x * self.width, p.y * self.height) for p in landmarks], np.float32)
        depth = np.array([p.z * self.width for p in landmarks], np.float32)
        if self.last_depth is not None and self.last_depth.shape == depth.shape:
            depth = self.last_depth + (depth - self.last_depth) * 0.4
        self.last_depth = depth
        shapes = {}
        if result.face_blendshapes:
            shapes = {category.category_name: category.score for category in result.face_blendshapes[0]}
        return Face(self.smoother.update(points), shapes, depth)

    def apply(self, frame: np.ndarray, settings: Settings, timestamp_ms: int) -> np.ndarray:
        time_s = timestamp_ms / 1000.0
        dt = 0.0 if self.last_time_s is None else min(0.25, time_s - self.last_time_s)
        self.last_time_s = time_s
        mp_image = None

        retouch = (settings.smoothing > 0.01 or abs(settings.skin_tone) > 0.01
                   or settings.eye_color != "natural" or settings.eye_brighten > 0.01
                   or (settings.lip_color != "none" and settings.lip_strength > 0.01)
                   or settings.blush > 0.01 or settings.teeth > 0.01)
        reshape = settings.slimming > 0.01 or settings.nose_slim > 0.01 or settings.eye_size > 0.01
        distortion = settings.distortion != "none" and settings.distortion_strength > 0.01
        paint = settings.mask != "none" and settings.mask_strength > 0.01
        lit = (settings.light_key > 0.01 or settings.light_fill > 0.01
               or settings.light_rim > 0.01 or settings.light_dim > 0.01)
        # Lights use the person mask so they fall on you, not on the wall behind.
        needs_mask = settings.background != "off" or lit
        hidden = settings.anonymise in ("eyes", "face")
        needs_face = bool(retouch or reshape or distortion or paint or settings.stickers or settings.triggers
                          or hidden
                          or settings.light_key > 0.01 or settings.light_fill > 0.01)

        face = None
        if needs_face or needs_mask:
            rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
            mp_image = mp.Image(image_format=mp.ImageFormat.SRGB, data=np.ascontiguousarray(rgb))
        if needs_face:
            face = self.detect_face(mp_image, timestamp_ms)
            self.face_found = face is not None
        else:
            self.face_found = None
            self.smoother.reset()

        # 1. Paint on the untouched face so makeup follows any later reshaping.
        if face is not None:
            if settings.smoothing > 0.01 or abs(settings.skin_tone) > 0.01:
                beauty.skin(frame, face, settings.smoothing, settings.skin_tone)
            if settings.teeth > 0.01:
                beauty.whiten_teeth(frame, face, settings.teeth)
            if settings.lip_color != "none" and settings.lip_strength > 0.01:
                beauty.tint_lips(frame, face, settings.lip_color, settings.lip_strength)
            if settings.blush > 0.01:
                beauty.blush(frame, face, settings.blush)
            if settings.eye_brighten > 0.01:
                beauty.brighten_eyes(frame, face, settings.eye_brighten)
            if settings.eye_color != "natural":
                beauty.recolor_eyes(frame, face, settings.eye_color)
            if paint:
                masks.apply(frame, face, settings.mask, settings.mask_strength)

        # 2. Reshape: beauty tweaks and funny distortions share one resample.
        self.field.reset()
        if face is not None and (reshape or distortion):
            reshape_face(self.field, face, settings.slimming, settings.nose_slim, settings.eye_size)
            if distortion:
                distort(self.field, face, settings.distortion, settings.distortion_strength)
            frame = self.field.apply(frame)
            face = face.moved(self.field.move_points(face.pts))

        # 3. Background and lights, using a person mask that went through the same reshape.
        person = None
        if needs_mask:
            segment = self.models.segmenter.segment_for_video(mp_image, timestamp_ms)
            if segment.confidence_masks:
                mask = segment.confidence_masks[0].numpy_view()
                mask = cv2.resize(mask, (self.width, self.height), interpolation=cv2.INTER_LINEAR)
                mask = np.clip((mask - 0.12) / 0.78, 0.0, 1.0).astype(np.float32)
                mask = cv2.GaussianBlur(mask, (0, 0), 3.5)
                if self.last_mask is not None:
                    mask = 0.68 * self.last_mask + 0.32 * mask
                self.last_mask = mask
                person = self.field.apply(mask, cv2.BORDER_REPLICATE)
        else:
            self.last_mask = None
        if person is not None and settings.background != "off":
            frame = self.composite_background(frame, person, settings)
        if lit:
            frame = self.lights.apply(frame, face, person, settings.light_key, settings.light_x,
                                      settings.light_y, settings.light_warmth, settings.light_fill,
                                      settings.light_rim, settings.light_rim_color, settings.light_dim)

        # 4. Anonymise late, so nothing drawn earlier can show through, then
        #    add the things that sit on top of the picture.
        if hidden:
            self.anonymiser.apply(frame, face, settings.anonymise, settings.anonymise_style,
                                  settings.anonymise_amount, dt)
        else:
            self.anonymiser.reset()
        if face is not None and settings.stickers:
            stickers.draw_all(frame, face, settings.stickers, time_s)
        if settings.triggers or self.triggers.busy:
            self.triggers.update(face, settings.triggers, dt, time_s)
            self.triggers.draw(frame, face, time_s)

        # 5. The grade goes last so stickers and backgrounds share the look.
        return self.grader.apply(frame, settings.look, settings.look_strength,
                                 settings.grain, settings.vignette, settings.glow,
                                 settings.lut, settings.lut_strength)

    def composite_background(self, frame: np.ndarray, mask: np.ndarray, settings: Settings) -> np.ndarray:
        if settings.background == "image" and settings.background_path:
            background = self.load_background(settings.background_path)
        else:
            # Blur a quarter-size copy: same look, a fraction of the cost.
            sigma = (5.0 + settings.blur * 24.0) * self.width / 1280.0
            small = cv2.resize(frame, (self.width // 4, self.height // 4), interpolation=cv2.INTER_AREA)
            small = cv2.GaussianBlur(small, (0, 0), max(0.8, sigma / 4.0))
            background = cv2.resize(small, (self.width, self.height), interpolation=cv2.INTER_LINEAR)
        if background is None:
            return frame
        return cv2.blendLinear(frame, background, mask, 1.0 - mask)

    def load_background(self, path: str) -> np.ndarray | None:
        if path == self.background_path:
            return self.background_image
        self.background_path = path
        source = cv2.imread(path, cv2.IMREAD_COLOR)
        if source is None:
            self.background_image = None
            return None
        src_h, src_w = source.shape[:2]
        scale = max(self.width / src_w, self.height / src_h)
        resized = cv2.resize(source, None, fx=scale, fy=scale, interpolation=cv2.INTER_AREA)
        y = max(0, (resized.shape[0] - self.height) // 2)
        x = max(0, (resized.shape[1] - self.width) // 2)
        self.background_image = resized[y:y + self.height, x:x + self.width].copy()
        return self.background_image


class PreviewStream:
    """Sends finished frames to the control center over the worker's stdout.

    Each frame is `b"OBSF"`, little-endian u32 width and height, then RGB bytes.
    """

    MAX_WIDTH = 1280

    def __init__(self):
        # Keep the real stdout for frames; anything else that prints goes to stderr.
        self.pipe = os.fdopen(os.dup(1), "wb", buffering=0)
        os.dup2(2, 1)

    def send(self, frame: np.ndarray) -> None:
        height, width = frame.shape[:2]
        if width > self.MAX_WIDTH:
            height = round(height * self.MAX_WIDTH / width)
            width = self.MAX_WIDTH
            frame = cv2.resize(frame, (width, height), interpolation=cv2.INTER_AREA)
        rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        self.pipe.write(b"OBSF" + struct.pack("<II", width, height))
        self.pipe.write(memoryview(np.ascontiguousarray(rgb)).cast("B"))


def write_status(path: Path, fps: float, frame_ms: float, face_found: bool | None) -> None:
    """Publish worker health for the control center; best effort."""
    face = "none" if face_found is None else ("yes" if face_found else "no")
    try:
        temporary = path.with_name(path.name + ".tmp")
        temporary.write_text(f"fps={fps:.1f}\nframe_ms={frame_ms:.1f}\nface={face}\n", encoding="utf-8")
        os.replace(temporary, path)
    except OSError:
        pass


def open_capture(path: str, width: int, height: int, fps: int) -> cv2.VideoCapture:
    while True:
        capture = cv2.VideoCapture(path, cv2.CAP_V4L2)
        pixel_format = "YUYV" if width <= 640 else "MJPG"
        capture.set(cv2.CAP_PROP_FOURCC, cv2.VideoWriter_fourcc(*pixel_format))
        capture.set(cv2.CAP_PROP_FRAME_WIDTH, width)
        capture.set(cv2.CAP_PROP_FRAME_HEIGHT, height)
        capture.set(cv2.CAP_PROP_FPS, fps)
        capture.set(cv2.CAP_PROP_BUFFERSIZE, 1)
        if capture.isOpened():
            return capture
        capture.release()
        print(f"waiting for input camera {path}", file=sys.stderr, flush=True)
        time.sleep(1.0)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", help="V4L2 loopback device to publish to")
    parser.add_argument("--preview", action="store_true", help="write frames to stdout")
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--status", type=Path)
    parser.add_argument("--model-dir", required=True, type=Path)
    parser.add_argument("--width", required=True, type=int)
    parser.add_argument("--height", required=True, type=int)
    parser.add_argument("--fps", default=30, type=int)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    preview = PreviewStream() if args.preview else None
    models = VisionModels(args.model_dir)
    effects = Effects(args.width, args.height, models)
    config = ConfigFile(args.config)
    capture = open_capture(args.input, args.width, args.height, args.fps)
    start = time.monotonic()
    window_start = start
    window_frames = 0
    window_busy = 0.0
    sink = VirtualCameraSink(args.output, args.width, args.height, args.fps) if args.output else None
    last_timestamp_ms = -1
    next_frame_at = time.monotonic()
    try:
        print(f"effects worker ready (publishing to {args.output or 'preview only'})",
              file=sys.stderr, flush=True)
        while True:
            ok, frame = capture.read()
            if not ok:
                capture.release()
                capture = open_capture(args.input, args.width, args.height, args.fps)
                continue
            if frame.shape[:2] != (args.height, args.width):
                frame = cv2.resize(frame, (args.width, args.height), interpolation=cv2.INTER_AREA)
            timestamp_ms = max(last_timestamp_ms + 1, int((time.monotonic() - start) * 1000))
            last_timestamp_ms = timestamp_ms
            began = time.monotonic()
            processed = effects.apply(frame, config.refresh(), timestamp_ms)
            if sink is not None:
                sink.send(processed)
            if preview is not None:
                preview.send(processed)
            now = time.monotonic()
            window_frames += 1
            window_busy += now - began
            if args.status is not None and now - window_start >= 1.0:
                write_status(args.status, window_frames / (now - window_start),
                             1000.0 * window_busy / window_frames, effects.face_found)
                window_start, window_frames, window_busy = now, 0, 0.0
            next_frame_at += 1.0 / args.fps
            time.sleep(max(0.0, next_frame_at - time.monotonic()))
    except (KeyboardInterrupt, BrokenPipeError):
        # A closed preview pipe means the control center went away.
        return 0
    finally:
        capture.release()
        if sink is not None:
            sink.close()
        models.close()


if __name__ == "__main__":
    raise SystemExit(main())
