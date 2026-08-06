#!/usr/bin/env python3
"""Low-latency OBSBOT effects processor for a V4L2 virtual camera."""

from __future__ import annotations

import argparse
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import cv2
import mediapipe as mp
import numpy as np


@dataclass
class Settings:
    background: str = "off"
    background_path: str = ""
    blur: float = 0.55
    smoothing: float = 0.0
    slimming: float = 0.0
    skin_tone: float = 0.0
    eye_color: str = "natural"


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
            values: dict[str, str] = {}
            for line in self.path.read_text(encoding="utf-8").splitlines():
                key, separator, value = line.partition("=")
                if separator:
                    values[key.strip()] = value.strip()
            self.settings = Settings(
                background=values.get("background", "off"),
                background_path=values.get("background_path", ""),
                blur=clamp_float(values.get("blur"), 0.55),
                smoothing=clamp_float(values.get("smoothing"), 0.0),
                slimming=clamp_float(values.get("slimming"), 0.0),
                skin_tone=clamp_float(values.get("skin_tone"), 0.0, -1.0, 1.0),
                eye_color=values.get("eye_color", "natural"),
            )
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
        grid_y, grid_x = np.indices((height, width), dtype=np.float32)
        self.grid_x = grid_x
        self.grid_y = grid_y

    def apply(self, frame: np.ndarray, settings: Settings, timestamp_ms: int) -> np.ndarray:
        rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        mp_image = mp.Image(image_format=mp.ImageFormat.SRGB, data=np.ascontiguousarray(rgb))

        landmarks = None
        needs_face = any(
            (settings.smoothing > 0.01, abs(settings.skin_tone) > 0.01,
             settings.slimming > 0.01, settings.eye_color != "natural")
        )
        if needs_face:
            result = self.models.landmarker.detect_for_video(mp_image, timestamp_ms)
            if result.face_landmarks:
                landmarks = result.face_landmarks[0]

        if landmarks and settings.slimming > 0.01:
            frame = self.slim_face(frame, landmarks, settings.slimming)

        if landmarks and (settings.smoothing > 0.01 or abs(settings.skin_tone) > 0.01):
            frame = self.skin_effects(frame, landmarks, settings.smoothing, settings.skin_tone)

        if landmarks and settings.eye_color != "natural":
            frame = self.recolor_eyes(frame, landmarks, settings.eye_color)

        if settings.background != "off":
            segment = self.models.segmenter.segment_for_video(mp_image, timestamp_ms)
            if segment.confidence_masks:
                mask = segment.confidence_masks[0].numpy_view()
                mask = cv2.resize(mask, (self.width, self.height), interpolation=cv2.INTER_LINEAR)
                mask = np.clip((mask - 0.12) / 0.78, 0.0, 1.0).astype(np.float32)
                mask = cv2.GaussianBlur(mask, (0, 0), 3.5)
                if self.last_mask is not None:
                    mask = 0.68 * self.last_mask + 0.32 * mask
                self.last_mask = mask
                frame = self.composite_background(frame, mask, settings)
        else:
            self.last_mask = None
        return frame

    def composite_background(self, frame: np.ndarray, mask: np.ndarray, settings: Settings) -> np.ndarray:
        if settings.background == "image" and settings.background_path:
            background = self.load_background(settings.background_path)
        else:
            sigma = 5.0 + settings.blur * 24.0
            background = cv2.GaussianBlur(frame, (0, 0), sigma)
        if background is None:
            return frame
        alpha = mask[..., None]
        return np.clip(frame * alpha + background * (1.0 - alpha), 0, 255).astype(np.uint8)

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

    def points(self, landmarks) -> np.ndarray:
        return np.array([(p.x * self.width, p.y * self.height) for p in landmarks], np.float32)

    def face_mask(self, landmarks) -> np.ndarray:
        points = self.points(landmarks)
        oval = np.array(
            [10, 338, 297, 332, 284, 251, 389, 356, 454, 323, 361, 288, 397,
             365, 379, 378, 400, 377, 152, 148, 176, 149, 150, 136, 172, 58,
             132, 93, 234, 127, 162, 21, 54, 103, 67, 109],
            dtype=np.int32,
        )
        mask = np.zeros((self.height, self.width), np.uint8)
        cv2.fillConvexPoly(mask, points[oval].astype(np.int32), 255)
        return cv2.GaussianBlur(mask, (0, 0), max(2.0, self.width / 320.0)).astype(np.float32) / 255.0

    def skin_effects(self, frame: np.ndarray, landmarks, smoothing: float, tone: float) -> np.ndarray:
        mask = self.face_mask(landmarks)[..., None]
        result = frame.astype(np.float32)
        if smoothing > 0.01:
            filtered = cv2.bilateralFilter(frame, 0, 18.0 + 35.0 * smoothing, 7.0 + 18.0 * smoothing)
            detail = frame.astype(np.float32) - cv2.GaussianBlur(frame, (0, 0), 1.4).astype(np.float32)
            softened = filtered.astype(np.float32) + detail * (0.35 - 0.25 * smoothing)
            amount = mask * (0.15 + 0.65 * smoothing)
            result = result * (1.0 - amount) + softened * amount
        if abs(tone) > 0.01:
            lab = cv2.cvtColor(np.clip(result, 0, 255).astype(np.uint8), cv2.COLOR_BGR2LAB).astype(np.float32)
            lab[..., 0] += tone * 8.0 * mask[..., 0]
            lab[..., 1] += tone * 7.0 * mask[..., 0]
            lab[..., 2] += tone * 10.0 * mask[..., 0]
            toned = cv2.cvtColor(np.clip(lab, 0, 255).astype(np.uint8), cv2.COLOR_LAB2BGR)
            result = toned.astype(np.float32)
        return np.clip(result, 0, 255).astype(np.uint8)

    def slim_face(self, frame: np.ndarray, landmarks, amount: float) -> np.ndarray:
        points = self.points(landmarks)
        center = points[1]
        result = frame
        for index in (234, 454):
            cheek = points[index]
            target = cheek + (center - cheek) * (0.055 + 0.11 * amount)
            radius = max(30.0, np.linalg.norm(cheek - center) * 0.9)
            result = self.local_warp(result, cheek, target, radius, amount)
        return result

    def local_warp(self, frame: np.ndarray, source: np.ndarray, target: np.ndarray,
                   radius: float, amount: float) -> np.ndarray:
        distance_sq = (self.grid_x - target[0]) ** 2 + (self.grid_y - target[1]) ** 2
        weight = np.clip(1.0 - distance_sq / (radius * radius), 0.0, 1.0) ** 2
        dx, dy = (target - source) * amount
        map_x = self.grid_x - dx * weight
        map_y = self.grid_y - dy * weight
        return cv2.remap(frame, map_x, map_y, cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT101)

    def recolor_eyes(self, frame: np.ndarray, landmarks, color_name: str) -> np.ndarray:
        colors = {
            "blue": (108, 68, 28),
            "green": (62, 82, 35),
            "hazel": (35, 78, 105),
            "gray": (105, 100, 94),
            "violet": (110, 55, 92),
        }
        color = colors.get(color_name)
        if color is None:
            return frame
        points = self.points(landmarks)
        overlay = frame.copy()
        mask = np.zeros((self.height, self.width), np.uint8)
        for indices in ([469, 470, 471, 472], [474, 475, 476, 477]):
            if max(indices) >= len(points):
                continue
            iris = points[indices]
            center = iris.mean(axis=0)
            radius = max(2, int(np.max(np.linalg.norm(iris - center, axis=1))))
            cv2.circle(mask, tuple(center.astype(int)), radius, 255, -1, cv2.LINE_AA)
        if not np.any(mask):
            return frame
        luminance = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY).astype(np.float32) / 150.0
        tint = np.zeros_like(frame)
        tint[:] = color
        tinted = tint.astype(np.float32) * np.clip(luminance[..., None], 0.35, 1.45)
        alpha = (cv2.GaussianBlur(mask, (0, 0), 1.0).astype(np.float32) / 255.0 * 0.58)[..., None]
        overlay = overlay.astype(np.float32) * (1.0 - alpha) + tinted * alpha
        return np.clip(overlay, 0, 255).astype(np.uint8)


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
    parser.add_argument("--output", required=True)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--model-dir", required=True, type=Path)
    parser.add_argument("--width", required=True, type=int)
    parser.add_argument("--height", required=True, type=int)
    parser.add_argument("--fps", default=30, type=int)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    models = VisionModels(args.model_dir)
    effects = Effects(args.width, args.height, models)
    config = ConfigFile(args.config)
    capture = open_capture(args.input, args.width, args.height, args.fps)
    start = time.monotonic()
    frames = 0
    sink = VirtualCameraSink(args.output, args.width, args.height, args.fps)
    last_timestamp_ms = -1
    next_frame_at = time.monotonic()
    try:
        print(f"virtual camera ready: {args.output}", flush=True)
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
            processed = effects.apply(frame, config.refresh(), timestamp_ms)
            sink.send(processed)
            frames += 1
            next_frame_at += 1.0 / args.fps
            time.sleep(max(0.0, next_frame_at - time.monotonic()))
    except KeyboardInterrupt:
        return 0
    finally:
        capture.release()
        sink.close()
        models.close()


if __name__ == "__main__":
    raise SystemExit(main())
