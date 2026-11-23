"""Accumulated displacement field for face reshaping and funny distortions.

Every warp adds into one field that is resampled once per frame, and only
inside the region that was actually touched.
"""

from __future__ import annotations

import cv2
import numpy as np

from landmarks import Face


class WarpField:
    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.dx = np.zeros((height, width), np.float32)
        self.dy = np.zeros((height, width), np.float32)
        grid_y, grid_x = np.indices((height, width), dtype=np.float32)
        self.grid_x = grid_x
        self.grid_y = grid_y
        self.box: tuple[int, int, int, int] | None = None

    @property
    def active(self) -> bool:
        return self.box is not None

    def reset(self) -> None:
        if self.box is not None:
            x0, y0, x1, y1 = self.box
            self.dx[y0:y1, x0:x1] = 0.0
            self.dy[y0:y1, x0:x1] = 0.0
            self.box = None

    def _region(self, center: np.ndarray, radius: float):
        x0 = max(0, int(center[0] - radius))
        y0 = max(0, int(center[1] - radius))
        x1 = min(self.width, int(center[0] + radius) + 2)
        y1 = min(self.height, int(center[1] + radius) + 2)
        if x0 >= x1 or y0 >= y1 or radius < 1.0:
            return None
        if self.box is None:
            self.box = (x0, y0, x1, y1)
        else:
            bx0, by0, bx1, by1 = self.box
            self.box = (min(bx0, x0), min(by0, y0), max(bx1, x1), max(by1, y1))
        region = (slice(y0, y1), slice(x0, x1))
        gx = self.grid_x[region] - center[0]
        gy = self.grid_y[region] - center[1]
        weight = np.clip(1.0 - (gx * gx + gy * gy) / (radius * radius), 0.0, 1.0)
        return region, gx, gy, weight * weight

    def push(self, source: np.ndarray, target: np.ndarray, radius: float, amount: float) -> None:
        """Drag the image content at `source` towards `target`."""
        found = self._region(target, radius)
        if found is None:
            return
        region, _, _, weight = found
        dx, dy = (target - source) * amount
        self.dx[region] -= dx * weight
        self.dy[region] -= dy * weight

    def bulge(self, center: np.ndarray, radius: float, strength: float) -> None:
        """Magnify (strength > 0) or pinch (strength < 0) around `center`."""
        found = self._region(center, radius)
        if found is None:
            return
        region, gx, gy, weight = found
        strength = float(np.clip(strength, -0.85, 0.85))
        self.dx[region] -= gx * strength * weight
        self.dy[region] -= gy * strength * weight

    def swirl(self, center: np.ndarray, radius: float, angle: float) -> None:
        found = self._region(center, radius)
        if found is None:
            return
        region, gx, gy, weight = found
        theta = angle * weight
        cos, sin = np.cos(theta), np.sin(theta)
        self.dx[region] += gx * cos - gy * sin - gx
        self.dy[region] += gx * sin + gy * cos - gy

    def apply(self, image: np.ndarray, border: int = cv2.BORDER_REFLECT101) -> np.ndarray:
        if self.box is None:
            return image
        x0, y0, x1, y1 = self.box
        map_x = self.grid_x[y0:y1, x0:x1] + self.dx[y0:y1, x0:x1]
        map_y = self.grid_y[y0:y1, x0:x1] + self.dy[y0:y1, x0:x1]
        result = image.copy()
        result[y0:y1, x0:x1] = cv2.remap(image, map_x, map_y, cv2.INTER_LINEAR, borderMode=border)
        return result

    def move_points(self, points: np.ndarray) -> np.ndarray:
        """Where input-image points end up after the warp (first-order inverse)."""
        if self.box is None:
            return points
        xs = np.clip(points[:, 0].round().astype(int), 0, self.width - 1)
        ys = np.clip(points[:, 1].round().astype(int), 0, self.height - 1)
        moved = points.copy()
        moved[:, 0] -= self.dx[ys, xs]
        moved[:, 1] -= self.dy[ys, xs]
        return moved


def reshape_face(field: WarpField, face: Face, slimming: float, nose_slim: float, eye_size: float) -> None:
    pts = face.pts
    if slimming > 0.01:
        center = pts[1]
        for index in (234, 454):
            cheek = pts[index]
            target = cheek + (center - cheek) * (0.055 + 0.11 * slimming)
            radius = max(30.0, float(np.linalg.norm(cheek - center)) * 0.9)
            field.push(cheek, target, radius, slimming)
    if nose_slim > 0.01:
        bridge = pts[5]
        for index in (129, 358):
            wing = pts[index]
            field.push(wing, wing + (bridge - wing) * 0.22, face.width * 0.2, nose_slim)
    if eye_size > 0.01:
        for center, width in face.eyes():
            field.bulge(center, width * 1.25, 0.3 * eye_size)


def distort(field: WarpField, face: Face, name: str, strength: float) -> None:
    pts = face.pts
    size = face.width
    if name == "big_head":
        field.bulge(face.center - face.down * size * 0.08, size * 1.3, 0.5 * strength)
    elif name == "tiny_face":
        field.bulge((pts[6] + pts[13]) / 2.0, size * 0.66, -0.8 * strength)
    elif name == "bug_eyes":
        for center, width in face.eyes():
            field.bulge(center, width * 1.9, 0.62 * strength)
    elif name == "alien":
        for center, width in face.eyes():
            field.bulge(center, width * 2.1, 0.5 * strength)
        field.bulge(pts[10], size * 0.7, 0.3 * strength)
        field.bulge(pts[152] + (pts[13] - pts[152]) * 0.25, size * 0.55, -0.6 * strength)
    elif name == "big_mouth":
        field.bulge(face.mouth_center, face.mouth_width * 1.35, 0.62 * strength)
    elif name == "big_nose":
        field.bulge(pts[4], size * 0.34, 0.68 * strength)
    elif name == "chipmunk":
        for index in (205, 425):
            field.bulge(pts[index], size * 0.38, 0.58 * strength)
    elif name == "swirl":
        field.swirl(pts[4], size * 1.0, 1.7 * strength)
