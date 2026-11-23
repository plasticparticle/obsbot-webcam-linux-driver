"""Face geometry shared by every landmark-driven effect."""

from __future__ import annotations

import numpy as np

FACE_OVAL = [10, 338, 297, 332, 284, 251, 389, 356, 454, 323, 361, 288, 397,
             365, 379, 378, 400, 377, 152, 148, 176, 149, 150, 136, 172, 58,
             132, 93, 234, 127, 162, 21, 54, 103, 67, 109]
LIPS_OUTER = [61, 146, 91, 181, 84, 17, 314, 405, 321, 375, 291, 409, 270, 269, 267, 0, 37, 39, 40, 185]
LIPS_INNER = [78, 95, 88, 178, 87, 14, 317, 402, 318, 324, 308, 415, 310, 311, 312, 13, 82, 81, 80, 191]
# "Right"/"left" follow the subject; the right eye appears on the image left.
RIGHT_EYE = [33, 7, 163, 144, 145, 153, 154, 155, 133, 173, 157, 158, 159, 160, 161, 246]
LEFT_EYE = [362, 382, 381, 380, 374, 373, 390, 249, 263, 466, 388, 387, 386, 385, 384, 398]
RIGHT_BROW = [70, 63, 105, 66, 107, 55, 65, 52, 53, 46]
LEFT_BROW = [300, 293, 334, 296, 336, 285, 295, 282, 283, 276]
RIGHT_IRIS = [469, 470, 471, 472]
LEFT_IRIS = [474, 475, 476, 477]


class Face:
    """Pixel-space landmarks plus a stable head frame (axes and scale)."""

    def __init__(self, pts: np.ndarray, blendshapes: dict[str, float] | None = None,
                 depth: np.ndarray | None = None):
        self.pts = pts
        self.blendshapes = blendshapes or {}
        # Per-landmark depth in pixels, negative towards the camera.
        self.depth = depth
        eye_line = pts[263] - pts[33]
        norm = float(np.linalg.norm(eye_line)) or 1.0
        self.right = eye_line / norm
        self.down = np.array([-self.right[1], self.right[0]], np.float32)
        self.angle = float(np.arctan2(self.right[1], self.right[0]))
        self.width = float(np.linalg.norm(pts[454] - pts[234]))
        self.height = float(np.linalg.norm(pts[152] - pts[10]))
        # Turning the head shrinks one dimension; the larger one stays honest.
        self.size = max(self.width, self.height / 1.32, 1.0)
        self.center = (pts[234] + pts[454]) / 2.0

    def moved(self, pts: np.ndarray) -> "Face":
        return Face(pts, self.blendshapes, self.depth)

    def eyes(self) -> list[tuple[np.ndarray, float]]:
        """(centre, width) for the image-left then image-right eye."""
        result = []
        for outer, inner in ((33, 133), (263, 362)):
            center = (self.pts[outer] + self.pts[inner]) / 2.0
            result.append((center, float(np.linalg.norm(self.pts[outer] - self.pts[inner]))))
        return result

    @property
    def eye_center(self) -> np.ndarray:
        return self.pts[[33, 133, 362, 263]].mean(axis=0)

    @property
    def mouth_center(self) -> np.ndarray:
        return (self.pts[13] + self.pts[14]) / 2.0

    @property
    def mouth_width(self) -> float:
        return float(np.linalg.norm(self.pts[291] - self.pts[61]))

    def offset(self, anchor: np.ndarray, right: float, down: float) -> np.ndarray:
        """Move from `anchor` by multiples of the head size along the head axes."""
        return anchor + (self.right * right + self.down * down) * self.size

    def shape(self, name: str) -> float:
        return self.blendshapes.get(name, 0.0)


class LandmarkSmoother:
    """Adaptive smoothing: steady when still, immediate when moving fast."""

    def __init__(self):
        self.previous: np.ndarray | None = None

    def reset(self) -> None:
        self.previous = None

    def update(self, pts: np.ndarray) -> np.ndarray:
        if self.previous is None or self.previous.shape != pts.shape:
            self.previous = pts
            return pts
        scale = float(np.linalg.norm(pts[454] - pts[234])) or 1.0
        motion = float(np.mean(np.linalg.norm(pts - self.previous, axis=1))) / scale
        blend = float(np.clip(0.3 + motion * 45.0, 0.3, 1.0))
        self.previous = self.previous + (pts - self.previous) * blend
        return self.previous


def bounds(points: np.ndarray, pad: float, width: int, height: int) -> tuple[int, int, int, int] | None:
    x0 = max(0, int(np.floor(points[:, 0].min() - pad)))
    y0 = max(0, int(np.floor(points[:, 1].min() - pad)))
    x1 = min(width, int(np.ceil(points[:, 0].max() + pad)) + 1)
    y1 = min(height, int(np.ceil(points[:, 1].max() + pad)) + 1)
    if x1 - x0 < 2 or y1 - y0 < 2:
        return None
    return x0, y0, x1, y1
