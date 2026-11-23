"""Face paint: looks drawn onto the face from its landmarks, keeping natural shading."""

from __future__ import annotations

import cv2
import numpy as np

from art import smooth
from beauty import grow
from landmarks import FACE_OVAL, LEFT_EYE, LIPS_INNER, LIPS_OUTER, RIGHT_EYE, Face, bounds

MASKS = ("hero", "anon", "clown", "skull", "zombie", "gold", "freckles")

_FRECKLE_GROUPS = (
    [50, 101, 118, 117, 123, 205, 36, 142, 100, 120, 47],
    [280, 330, 347, 346, 352, 425, 266, 371, 329, 349, 277],
    [6, 197, 195, 5, 51, 281, 3, 248],
)


def _freckle_recipe() -> list[tuple[int, int, int, np.ndarray, float]]:
    rng = np.random.default_rng(11)
    recipe = []
    for group, count in zip(_FRECKLE_GROUPS, (22, 22, 12)):
        for _ in range(count):
            a, b, c = rng.choice(group, 3, replace=False)
            weights = rng.dirichlet((1.0, 1.0, 1.0)).astype(np.float32)
            recipe.append((int(a), int(b), int(c), weights, float(rng.choice((0.6, 0.9, 1.25)))))
    return recipe


_FRECKLES = _freckle_recipe()
_GOLD_LUT = np.zeros((256, 3), np.float32)
for _i in range(256):
    _t = _i / 255.0
    _stops = [(0.0, (10, 30, 60)), (0.45, (40, 140, 205)), (0.8, (110, 225, 255)), (1.0, (235, 252, 255))]
    for (_t0, _c0), (_t1, _c1) in zip(_stops[:-1], _stops[1:]):
        if _t0 <= _t <= _t1:
            _k = (_t - _t0) / (_t1 - _t0)
            _GOLD_LUT[_i] = np.array(_c0) * (1.0 - _k) + np.array(_c1) * _k


class Painter:
    """Collects paint layers for one face and blends them in a single pass.

    Layers are built on a reduced canvas so the cost does not grow with the
    output resolution; only the final blend runs at full size.
    """

    CANVAS = 300.0

    def __init__(self, frame: np.ndarray, face: Face):
        self.face = face
        height, width = frame.shape[:2]
        box = bounds(face.pts[FACE_OVAL], face.size * 0.2, width, height)
        self.valid = box is not None
        if not self.valid:
            return
        self.x0, self.y0, x1, y1 = box
        self.roi = frame[self.y0:y1, self.x0:x1]
        self.k = min(1.0, self.CANVAS / max(self.roi.shape[:2]))
        self.shape = (max(2, int(round(self.roi.shape[0] * self.k))), max(2, int(round(self.roi.shape[1] * self.k))))
        self.origin = np.array([self.x0, self.y0], np.float32)
        self.color = np.zeros((*self.shape, 3), np.float32)
        self.alpha = np.zeros(self.shape, np.float32)
        self.gray = cv2.cvtColor(self.roi, cv2.COLOR_BGR2GRAY).astype(np.float32)
        small_gray = cv2.resize(self.gray, (self.shape[1], self.shape[0]), interpolation=cv2.INTER_AREA)
        skin = self.mask([face.pts[FACE_OVAL]])
        weight = float(skin.sum())
        self.reference = max(float((small_gray * skin).sum()) / weight if weight > 1.0 else 128.0, 40.0)

    def _local(self, points: np.ndarray) -> np.ndarray:
        return np.round((np.asarray(points, np.float32) - self.origin) * self.k).astype(np.int32)

    def _finish(self, layer: np.ndarray, blur: float) -> np.ndarray:
        if blur > 0.0:
            layer = cv2.GaussianBlur(layer, (0, 0), max(0.5, blur * self.k))
        return layer.astype(np.float32) / 255.0

    def mask(self, fill, cut=(), blur: float = 0.0) -> np.ndarray:
        layer = np.zeros(self.shape, np.uint8)
        for polygon in fill:
            cv2.fillPoly(layer, [self._local(polygon)], 255, cv2.LINE_AA)
        for polygon in cut:
            cv2.fillPoly(layer, [self._local(polygon)], 0, cv2.LINE_AA)
        return self._finish(layer, blur)

    def discs(self, centers, radius: float, blur: float = 0.0) -> np.ndarray:
        layer = np.zeros(self.shape, np.uint8)
        for center in centers:
            cv2.circle(layer, tuple(int(v) for v in self._local(center)),
                       max(1, int(round(radius * self.k))), 255, -1, cv2.LINE_AA)
        return self._finish(layer, blur)

    def lines(self, segments, thickness: float) -> np.ndarray:
        layer = np.zeros(self.shape, np.uint8)
        for start, end in segments:
            cv2.line(layer, tuple(int(v) for v in self._local(start)), tuple(int(v) for v in self._local(end)),
                     255, max(1, int(round(thickness * self.k))), cv2.LINE_AA)
        return self._finish(layer, 0.0)

    def paint(self, mask: np.ndarray, color, opacity: float = 1.0) -> None:
        alpha = mask * opacity
        below = self.alpha * (1.0 - alpha)
        total = alpha + below
        self.color = (np.array(color, np.float32) * alpha[..., None] + self.color * below[..., None]) \
            / np.maximum(total, 1e-6)[..., None]
        self.alpha = total

    def commit(self, strength: float, shaded: bool = True, color: np.ndarray | None = None) -> None:
        size = (self.roi.shape[1], self.roi.shape[0])
        alpha = cv2.resize(self.alpha * strength, size, interpolation=cv2.INTER_LINEAR)[..., None]
        if color is None:
            color = cv2.resize(self.color, size, interpolation=cv2.INTER_LINEAR)
            if shaded:
                color *= np.clip(self.gray / self.reference, 0.35, 1.5)[..., None]
        self.roi[:] = np.clip(self.roi * (1.0 - alpha) + color * alpha, 0, 255).astype(np.uint8)


def apply(frame: np.ndarray, face: Face, name: str, strength: float) -> None:
    painter = Painter(frame, face)
    if not painter.valid:
        return
    pts = face.pts
    size = face.size
    soft = max(1.5, size / 70.0)
    feather = soft * 2.5
    eyes = [pts[RIGHT_EYE], pts[LEFT_EYE]]
    eye_centers = [center for center, _ in face.eyes()]
    eye_width = max(width for _, width in face.eyes())
    oval = pts[FACE_OVAL]
    cheeks = [pts[205], pts[425]]
    dark = (20, 18, 18)

    if name == "hero":
        spacing = float(np.linalg.norm(eye_centers[1] - eye_centers[0]))
        half = [(0.0, -0.36), (0.3, -0.56), (0.72, -0.62), (1.08, -0.5), (1.3, -0.3), (1.24, 0.0),
                (1.02, 0.3), (0.62, 0.42), (0.26, 0.32), (0.08, 0.16), (0.0, 0.12)]
        outline = np.array(half + [(-x, y) for x, y in reversed(half[1:-1])], np.float32)
        outline = smooth(outline, 6)
        polygon = face.eye_center + (outline[:, :1] * face.right + outline[:, 1:] * face.down) * spacing
        holes = [grow(eye, 1.3) for eye in eyes]
        body = painter.mask([polygon], holes, max(0.8, size / 260.0))
        painter.paint(body, (34, 24, 26), 0.97)
        sheen = painter.mask([polygon - face.down * spacing * 0.2], holes, soft * 3.0) * body
        painter.paint(sheen, (120, 90, 96), 0.35)
        painter.commit(strength, shaded=False)

    elif name == "anon":
        # An original take on the pale, smiling moustache-and-goatee mask.
        up = -face.down
        painter.paint(painter.mask([oval], [grow(eye, 1.2) for eye in eyes], feather), (242, 244, 244), 0.94)
        painter.paint(painter.discs(cheeks, size * 0.1, soft * 4.0), (150, 140, 240), 0.5)
        painter.paint(painter.mask([pts[LIPS_OUTER]], [pts[LIPS_INNER]], 1.0), (140, 130, 225), 0.55)
        ink = painter.mask([grow(eye, 1.5) for eye in eyes], [grow(eye, 1.2) for eye in eyes], 1.0) * 0.8
        strokes = []
        for brow in ([70, 63, 105, 66, 107], [300, 293, 334, 296, 336]):
            arch = smooth(pts[brow] + up * size * 0.035, 5, closed=False)
            strokes += list(zip(arch[:-1], arch[1:]))
        ink = np.maximum(ink, painter.lines(strokes, size * 0.014))
        under_nose = pts[2] + face.down * size * 0.025
        for corner, side in ((pts[61], -1.0), (pts[291], 1.0)):
            out = face.right * side
            curl = smooth(np.array([
                under_nose + out * size * 0.01,
                (under_nose + corner) / 2.0 + face.down * size * 0.015,
                corner + out * size * 0.05 + up * size * 0.005,
                corner + out * size * 0.13 + up * size * 0.07,
                corner + out * size * 0.15 + up * size * 0.15,
            ], np.float32), 8, closed=False)
            half = len(curl) // 2
            ink = np.maximum(ink, painter.lines(list(zip(curl[:half], curl[1:half + 1])), size * 0.03))
            ink = np.maximum(ink, painter.lines(list(zip(curl[half:-1], curl[half + 1:])), size * 0.017))
            grin = smooth(np.array([face.mouth_center, corner, corner + out * size * 0.03 + up * size * 0.03],
                                   np.float32), 6, closed=False)
            ink = np.maximum(ink, painter.lines(list(zip(grin[:-1], grin[1:])), size * 0.009))
        tuft = pts[17] + face.down * size * 0.025
        goatee = np.array([tuft - face.right * size * 0.03, tuft + face.right * size * 0.03,
                           pts[152] + face.down * size * 0.02], np.float32)
        ink = np.maximum(ink, painter.mask([goatee], blur=0.8))
        painter.paint(ink, dark, 0.95)
        painter.commit(strength)

    elif name == "clown":
        painter.paint(painter.mask([oval], [grow(eye, 1.25) for eye in eyes], feather), (248, 246, 246), 0.88)
        diamonds = []
        for eye in eyes:
            center = eye.mean(axis=0)
            reach = float(np.linalg.norm(eye[0] - eye[8]))
            diamonds.append(np.array([center - face.down * reach * 1.25, center + face.right * reach * 0.42,
                                      center + face.down * reach * 1.25, center - face.right * reach * 0.42],
                                     np.float32))
        painter.paint(painter.mask(diamonds, [grow(eye, 1.2) for eye in eyes], 1.0), (225, 150, 40), 0.9)
        painter.paint(painter.discs(cheeks, size * 0.085, soft), (150, 120, 255), 0.75)
        red = np.maximum(painter.mask([grow(pts[LIPS_OUTER], 1.55)], [pts[LIPS_INNER]], 1.2),
                         painter.discs([pts[4]], size * 0.1, 1.0))
        painter.paint(red, (50, 40, 230), 0.97)
        painter.paint(painter.discs([pts[4] - (face.right + face.down) * size * 0.03], size * 0.028, 2.0),
                      (255, 255, 255), 0.7)
        painter.commit(strength)

    elif name == "skull":
        painter.paint(painter.mask([oval], blur=feather), (236, 240, 240), 0.92)
        hollows = [cheek + face.down * size * 0.05 for cheek in cheeks]
        painter.paint(painter.discs(hollows, size * 0.1, soft * 4.0), (90, 90, 96), 0.5)
        sockets = painter.discs([center - face.down * eye_width * 0.08 for center in eye_centers],
                                eye_width * 0.82, soft)
        sockets *= 1.0 - painter.mask([grow(eye, 1.12) for eye in eyes], blur=1.0)
        nose = np.array([pts[6] + (pts[4] - pts[6]) * 0.55, pts[129] + (pts[4] - pts[129]) * 0.25,
                         pts[2], pts[358] + (pts[4] - pts[358]) * 0.25], np.float32)
        left, right = pts[61], pts[291]
        reach = (right - left) * 0.3
        stitches = [(left - reach, right + reach)]
        for step in np.linspace(-0.22, 1.22, 12):
            base = left + (right - left) * step
            stitches.append((base - face.down * size * 0.055, base + face.down * size * 0.055))
        bones = np.maximum(np.maximum(sockets, painter.mask([nose], blur=1.2)),
                           painter.lines(stitches, size * 0.013))
        painter.paint(bones, dark, 0.95)
        painter.commit(strength)

    elif name == "zombie":
        painter.paint(painter.mask([oval], [grow(eye, 1.1) for eye in eyes], feather), (120, 160, 120), 0.62)
        rings = painter.discs(eye_centers, eye_width * 0.8, soft * 2.5)
        rings *= 1.0 - painter.mask([grow(eye, 1.05) for eye in eyes], blur=1.0)
        painter.paint(rings, (70, 40, 70), 0.6)
        painter.paint(painter.mask([pts[LIPS_OUTER]], [pts[LIPS_INNER]], 1.0), (80, 60, 90), 0.7)
        painter.paint(painter.discs([cheek + face.down * size * 0.04 for cheek in cheeks], size * 0.11, soft * 4.0),
                      (70, 90, 70), 0.4)
        painter.commit(strength)

    elif name == "gold":
        painter.alpha = painter.mask([oval], [grow(eye, 1.05) for eye in eyes] + [pts[LIPS_INNER]], feather) * 0.94
        tone = np.clip(118.0 + (painter.gray - painter.reference) * 2.1, 0, 255).astype(np.uint8)
        painter.commit(strength, color=_GOLD_LUT[tone])

    elif name == "freckles":
        radius = max(1.0, size / 150.0)
        dots = np.zeros(painter.shape, np.float32)
        for scale in (0.6, 0.9, 1.25):
            spots = [pts[a] * w[0] + pts[b] * w[1] + pts[c] * w[2]
                     for a, b, c, w, spot in _FRECKLES if abs(spot - scale) < 0.2]
            dots = np.maximum(dots, painter.discs(spots, radius * scale))
        dots = cv2.GaussianBlur(dots, (0, 0), max(0.5, radius * 0.45 * painter.k))
        painter.paint(dots, (50, 80, 130), 0.62)
        painter.paint(painter.discs(cheeks, size * 0.1, soft * 5.0), (130, 110, 240), 0.3)
        painter.commit(strength)
