"""One-click picture correction: exposure, contrast and white balance.

The correction is measured on a small copy of the frame a few times a second
and eased in, so it never pumps or flickers; applying it is one table lookup.
"""

from __future__ import annotations

import cv2
import numpy as np

_IDENTITY = np.arange(256, dtype=np.float32)


class AutoEnhance:
    MEASURE_EVERY = 4

    def __init__(self):
        self.frame = 0
        # black point, white point, gamma, then B, G, R gains
        self.state = np.array([0.0, 1.0, 1.0, 1.0, 1.0, 1.0], np.float32)
        self.settled = False
        self.lut: np.ndarray | None = None
        self.lut_amount = -1.0

    def reset(self) -> None:
        self.settled = False

    def _measure(self, frame: np.ndarray, face_box) -> np.ndarray:
        height, width = frame.shape[:2]
        small = cv2.resize(frame, (160, 90), interpolation=cv2.INTER_AREA).astype(np.float32) / 255.0
        luma = small @ np.array([0.114, 0.587, 0.299], np.float32)
        black, white = np.percentile(luma, (1.0, 99.0))
        black = float(np.clip(black, 0.0, 0.18))
        white = float(np.clip(white, 0.35, 1.0))
        levelled = np.clip((luma - black) / max(white - black, 0.2), 0.0, 1.0)
        if face_box is not None:
            # Expose for the face: that is what people look at.
            x0, y0, x1, y1 = face_box
            region = levelled[int(y0 / height * 90):max(int(y1 / height * 90), int(y0 / height * 90) + 1),
                              int(x0 / width * 160):max(int(x1 / width * 160), int(x0 / width * 160) + 1)]
            middle, target, darkest = float(region.mean()) if region.size else 0.5, 0.56, 1.4
        else:
            ys, xs = np.indices(luma.shape, dtype=np.float32)
            weight = np.exp(-(((xs - 80.0) / 70.0) ** 2 + ((ys - 45.0) / 45.0) ** 2))
            # Without a face to judge by, brighten freely but barely ever darken.
            middle, target, darkest = float(np.sum(levelled * weight) / np.sum(weight)), 0.46, 1.12
        gamma = float(np.clip(np.log(target) / np.log(np.clip(middle, 0.05, 0.95)), 0.5, darkest))
        # White balance from the brightest tenth of the picture, which is most
        # often a neutral surface; kept gentle because that is only a guess.
        bright = luma >= np.percentile(luma, 90.0)
        means = small[bright].mean(axis=0)
        gains = np.clip(means.mean() / np.maximum(means, 1e-3), 0.84, 1.2)
        gains = 1.0 + (gains - 1.0) * 0.8
        return np.array([black, white, gamma, *gains], np.float32)

    def apply(self, frame: np.ndarray, amount: float, face_box=None) -> np.ndarray:
        if self.frame % self.MEASURE_EVERY == 0 or not self.settled:
            target = self._measure(frame, face_box)
            self.state = target if not self.settled else self.state + (target - self.state) * 0.22
            self.settled = True
            self.lut = None
        self.frame += 1
        if self.lut is None or abs(amount - self.lut_amount) > 0.004:
            black, white, gamma = self.state[:3]
            # Balance the colour first, so stretching the levels cannot clip
            # one channel before the others and leave a tint behind.
            balanced = _IDENTITY[:, None] / 255.0 * self.state[3:][None, :]
            table = np.clip((balanced - black) / max(white - black, 0.2), 0.0, 1.0) ** gamma * 255.0
            table = _IDENTITY[:, None] * (1.0 - amount) + table * amount
            self.lut = np.round(table).astype(np.uint8).reshape(256, 1, 3)
            self.lut_amount = amount
        return cv2.LUT(frame, self.lut)
