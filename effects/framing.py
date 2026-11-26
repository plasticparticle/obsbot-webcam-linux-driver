"""Picture geometry: mirroring, whiteboard straightening and auto-framing."""

from __future__ import annotations

import cv2
import numpy as np

from landmarks import Face


class Keystone:
    """Straightens a region marked by four corners into the full frame."""

    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.spec = ""
        self.matrix: np.ndarray | None = None

    def apply(self, frame: np.ndarray, spec: str) -> np.ndarray:
        if spec != self.spec:
            self.spec = spec
            self.matrix = self._matrix(spec)
        if self.matrix is None:
            return frame
        return cv2.warpPerspective(frame, self.matrix, (self.width, self.height), flags=cv2.INTER_LINEAR,
                                   borderMode=cv2.BORDER_REPLICATE)

    def _matrix(self, spec: str) -> np.ndarray | None:
        """`spec` is eight 0..1 numbers: top-left, top-right, bottom-right, bottom-left."""
        try:
            values = [float(part) for part in spec.split(",")]
        except ValueError:
            return None
        if len(values) != 8 or not all(-0.05 <= value <= 1.05 for value in values):
            return None
        corners = np.array(values, np.float32).reshape(4, 2) * np.array([self.width, self.height], np.float32)
        if abs(cv2.contourArea(corners)) < self.width * self.height * 0.02:
            return None
        full = np.array([[0, 0], [self.width, 0], [self.width, self.height], [0, self.height]], np.float32)
        return cv2.getPerspectiveTransform(corners, full)


class AutoFrame:
    """A smooth digital crop that keeps the people in the picture centred."""

    MAX_ZOOM = 2.5

    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.reset()

    def reset(self) -> None:
        # centre x, centre y, crop height
        self.crop = np.array([self.width / 2.0, self.height / 2.0, float(self.height)], np.float32)
        self.lost_for = 0.0

    def _target(self, faces: list[Face], amount: float) -> np.ndarray:
        points = np.concatenate([face.pts for face in faces])
        left, top = points.min(axis=0)
        right, bottom = points.max(axis=0)
        tallest = max(face.height for face in faces)
        # How much of the picture height a face should fill: wide .. close-up.
        fill = 0.2 + 0.26 * amount
        crop_height = tallest / fill
        # Groups: never crop anyone out.
        aspect = self.width / self.height
        crop_height = max(crop_height, (bottom - top) * 1.9, (right - left) * 1.5 / aspect)
        crop_height = float(np.clip(crop_height, self.height / self.MAX_ZOOM, self.height))
        # Leave headroom: faces sit a little above the middle.
        return np.array([(left + right) / 2.0, (top + bottom) / 2.0 + crop_height * 0.06, crop_height], np.float32)

    def apply(self, frame: np.ndarray, faces: list[Face], amount: float, dt: float) -> np.ndarray:
        if faces:
            self.lost_for = 0.0
            target = self._target(faces, amount)
        else:
            self.lost_for += dt
            # Hold the shot briefly, then ease back out to the full picture.
            target = self.crop if self.lost_for < 1.5 else np.array(
                [self.width / 2.0, self.height / 2.0, float(self.height)], np.float32)
        delta = target - self.crop
        # Ignore small movements so the frame stays calm while you talk.
        if abs(delta[0]) > self.crop[2] * 0.05 or abs(delta[1]) > self.crop[2] * 0.05 \
                or abs(delta[2]) > self.crop[2] * 0.08 or self.lost_for >= 1.5:
            self.crop = self.crop + delta * min(1.0, dt * 2.2)
        crop_height = float(np.clip(self.crop[2], self.height / self.MAX_ZOOM, self.height))
        crop_width = crop_height * self.width / self.height
        x0 = float(np.clip(self.crop[0] - crop_width / 2.0, 0.0, self.width - crop_width))
        y0 = float(np.clip(self.crop[1] - crop_height / 2.0, 0.0, self.height - crop_height))
        if crop_height > self.height - 2.0:
            return frame
        matrix = np.array([[crop_width / self.width, 0.0, x0], [0.0, crop_height / self.height, y0]], np.float32)
        return cv2.warpAffine(frame, matrix, (self.width, self.height),
                              flags=cv2.INTER_CUBIC | cv2.WARP_INVERSE_MAP, borderMode=cv2.BORDER_REPLICATE)
