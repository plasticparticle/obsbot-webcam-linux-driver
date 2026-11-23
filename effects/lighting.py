"""Virtual studio lights: key, fill and rim light plus background dimming.

Everything is folded into one low-resolution gain map and one additive map,
so the cost is two full-frame operations however many lights are on.
"""

from __future__ import annotations

import cv2
import numpy as np

from landmarks import FACE_OVAL, Face

# BGR gels for the rim light.
RIM_COLORS = {
    "white": (255, 255, 255),
    "warm": (120, 190, 255),
    "blue": (255, 150, 70),
    "pink": (190, 90, 255),
    "purple": (255, 90, 170),
    "green": (130, 255, 110),
}
SCALE = 4


def _warmth_tint(warmth: float) -> np.ndarray:
    """Per-channel (BGR) light colour for a warmth of -1 (cool) .. 1 (warm)."""
    return np.array([1.0 - 0.3 * warmth, 1.0 - 0.04 * abs(warmth), 1.0 + 0.3 * warmth], np.float32)


class Lights:
    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.small = (width // SCALE, height // SCALE)
        ys, xs = np.indices((self.small[1], self.small[0]), dtype=np.float32)
        self.xs = xs
        self.ys = ys

    def _face_shading(self, face: Face, direction: np.ndarray) -> tuple[np.ndarray, np.ndarray] | None:
        """Lambert shading from the landmark depth, plus the face region it is valid in."""
        if face.depth is None:
            return None
        pts = face.pts / SCALE
        shape = (self.small[1], self.small[0])
        xi = np.clip(np.round(pts[:, 0]).astype(int), 0, shape[1] - 1)
        yi = np.clip(np.round(pts[:, 1]).astype(int), 0, shape[0] - 1)
        # Scatter the landmark heights and blur them into a smooth surface.
        weight = np.zeros(shape, np.float32)
        height = np.zeros(shape, np.float32)
        np.add.at(weight, (yi, xi), 1.0)
        np.add.at(height, (yi, xi), -face.depth / SCALE)
        sigma = max(1.5, face.size / SCALE / 9.0)
        weight = cv2.GaussianBlur(weight, (0, 0), sigma)
        height = cv2.GaussianBlur(height, (0, 0), sigma) / np.maximum(weight, 1e-4)
        height = cv2.GaussianBlur(height, (0, 0), sigma * 0.6)
        slope_x = cv2.Sobel(height, cv2.CV_32F, 1, 0, ksize=3) / 8.0
        slope_y = cv2.Sobel(height, cv2.CV_32F, 0, 1, ksize=3) / 8.0
        relief = 1.6
        normal_x, normal_y = -slope_x * relief, -slope_y * relief
        length = np.sqrt(normal_x * normal_x + normal_y * normal_y + 1.0)
        shading = (normal_x * direction[0] + normal_y * direction[1] + direction[2]) / length
        region = np.zeros(shape, np.uint8)
        cv2.fillPoly(region, [np.round(pts[FACE_OVAL]).astype(np.int32)], 255, cv2.LINE_AA)
        region = cv2.GaussianBlur(region, (0, 0), sigma * 1.2).astype(np.float32) / 255.0
        return shading - direction[2], region

    def apply(self, frame: np.ndarray, face: Face | None, person: np.ndarray | None, key: float,
              light_x: float, light_y: float, warmth: float, fill: float, rim: float,
              rim_color: str, dim: float) -> np.ndarray:
        shape = (self.small[1], self.small[0])
        subject = cv2.resize(person, self.small, interpolation=cv2.INTER_AREA) if person is not None else None
        gain = np.ones((*shape, 3), np.float32)
        glow = None

        if face is not None and (key > 0.01 or fill > 0.01):
            center = face.center / SCALE
            size = face.size / SCALE
            distance = (self.xs - center[0]) ** 2 + (self.ys - center[1]) ** 2
            # Without a person mask, keep the lights on the head and shoulders.
            reach = subject if subject is not None else np.exp(-distance / (2.0 * (size * 1.7) ** 2))

            if key > 0.01:
                direction = np.array([light_x, -light_y, 0.85], np.float32)
                direction /= np.linalg.norm(direction)
                lamp = center + np.array([light_x, -light_y], np.float32) * size * 1.1
                pool = np.exp(-((self.xs - lamp[0]) ** 2 + (self.ys - lamp[1]) ** 2) / (2.0 * (size * 1.25) ** 2))
                # Lit near the lamp, gently shadowed on the far side.
                level = (0.42 * pool - 0.2 * (1.0 - pool) * min(1.0, np.hypot(light_x, light_y) * 1.5)) * reach
                shaded = self._face_shading(face, direction)
                if shaded is not None:
                    shading, region = shaded
                    level = level + np.clip(shading * 0.85, -0.2, 0.28) * region
                level = level * key
                tint = _warmth_tint(warmth)
                gain += np.maximum(level, 0.0)[..., None] * tint + np.minimum(level, 0.0)[..., None]

            if fill > 0.01:
                small = cv2.resize(frame, self.small, interpolation=cv2.INTER_AREA)
                darkness = 1.0 - cv2.cvtColor(small, cv2.COLOR_BGR2GRAY).astype(np.float32) / 255.0
                darkness = cv2.GaussianBlur(darkness, (0, 0), max(1.5, size / 10.0))
                gain += (0.75 * fill * darkness * darkness * reach)[..., None]

        if subject is not None:
            if dim > 0.01:
                gain *= (1.0 - 0.8 * dim * (1.0 - subject))[..., None]
            if rim > 0.01:
                sigma = max(1.5, (face.size if face is not None else self.width * 0.2) / SCALE * 0.045)
                inside = cv2.GaussianBlur(subject, (0, 0), sigma)
                edge = np.clip(subject * (1.0 - inside) * 2.4, 0.0, 1.0)
                edge = cv2.GaussianBlur(edge, (0, 0), sigma * 0.5)
                glow = edge[..., None] * np.array(RIM_COLORS.get(rim_color, RIM_COLORS["white"]), np.float32) * (0.8 * rim)

        full = (self.width, self.height)
        frame = cv2.multiply(frame, cv2.resize(gain, full, interpolation=cv2.INTER_LINEAR), dtype=cv2.CV_8U)
        if glow is not None:
            frame = cv2.add(frame, cv2.resize(glow, full, interpolation=cv2.INTER_LINEAR), dtype=cv2.CV_8U)
        return frame
