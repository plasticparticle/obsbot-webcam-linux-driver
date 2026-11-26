"""Anonymiser: pixelate, blur or black out the eyes or the whole face.

If the face tracker loses a face for a moment, the last hidden area stays
covered (slightly enlarged) instead of suddenly revealing it.
"""

from __future__ import annotations

from dataclasses import dataclass

import cv2
import numpy as np

from landmarks import FACE_OVAL, Face, bounds

HOLD_SECONDS = 2.0


def _region(face: Face, target: str) -> np.ndarray:
    pts = face.pts
    if target == "eyes":
        half_width = float(np.linalg.norm(pts[263] - pts[33])) * 0.72
        half_height = face.size * 0.12
        center = face.eye_center
        across, down = face.right * half_width, face.down * half_height
        return np.array([center - across - down, center + across - down,
                         center + across + down, center - across + down], np.float32)
    oval = pts[FACE_OVAL]
    center = oval.mean(axis=0)
    grown = center + (oval - center) * 1.12
    # The landmark oval stops at the hairline; reach a little further up.
    return grown - face.down * face.size * 0.04


@dataclass
class Hidden:
    polygon: np.ndarray
    size: float
    lost_for: float = 0.0


class Anonymiser:
    def __init__(self):
        self.areas: list[Hidden] = []
        self.target = ""

    def reset(self) -> None:
        self.areas = []

    def apply(self, frame: np.ndarray, faces: list[Face], target: str, style: str,
              amount: float, dt: float) -> None:
        if target != self.target:
            self.areas = []
            self.target = target
        current = [Hidden(_region(face, target), face.size) for face in faces]
        # Keep covering areas whose face has just dropped out of tracking.
        for old in self.areas:
            center = old.polygon.mean(axis=0)
            tracked = any(np.linalg.norm(new.polygon.mean(axis=0) - center) < max(old.size, new.size) * 0.7
                          for new in current)
            if tracked or old.lost_for >= HOLD_SECONDS:
                continue
            if old.lost_for == 0.0:
                old.polygon = center + (old.polygon - center) * 1.25
            old.lost_for += max(dt, 1e-3)
            current.append(old)
        self.areas = current
        for area in current:
            _cover(frame, area, style, amount)


def _cover(frame: np.ndarray, area: Hidden, style: str, amount: float) -> None:
    height, width = frame.shape[:2]
    block = max(6, int(round(area.size * (0.07 + 0.12 * amount))))
    box = bounds(area.polygon, block, width, height)
    if box is None:
        return
    # Snap the area to the block grid so the mosaic does not shimmer.
    x0, y0, x1, y1 = box
    x0, y0 = (x0 // block) * block, (y0 // block) * block
    x1 = min(width, -(-x1 // block) * block)
    y1 = min(height, -(-y1 // block) * block)
    roi = frame[y0:y1, x0:x1]
    local = np.round(area.polygon - (x0, y0)).astype(np.int32)
    mask = np.zeros(roi.shape[:2], np.uint8)
    cv2.fillPoly(mask, [local], 255, cv2.LINE_AA)

    if style == "bar":
        roi[mask > 127] = (12, 12, 12)
        return
    cells = (max(1, -(-roi.shape[1] // block)), max(1, -(-roi.shape[0] // block)))
    size = (roi.shape[1], roi.shape[0])
    if style == "blur":
        small = cv2.resize(roi, cells, interpolation=cv2.INTER_AREA)
        covered = cv2.GaussianBlur(cv2.resize(small, size, interpolation=cv2.INTER_LINEAR), (0, 0), block * 0.6)
        alpha = cv2.GaussianBlur(mask, (0, 0), block * 0.35).astype(np.float32)[..., None] / 255.0
        # Fully opaque inside; only the outer edge is feathered.
        alpha = np.clip(alpha * 1.6, 0.0, 1.0)
        roi[:] = (roi * (1.0 - alpha) + covered * alpha).astype(np.uint8)
        return
    mosaic = cv2.resize(cv2.resize(roi, cells, interpolation=cv2.INTER_AREA), size,
                        interpolation=cv2.INTER_NEAREST)
    # Whole blocks only: a block is covered if any of it touches the area.
    touched = cv2.resize(cv2.resize(mask, cells, interpolation=cv2.INTER_AREA), size,
                         interpolation=cv2.INTER_NEAREST) > 20
    roi[touched] = mosaic[touched]
