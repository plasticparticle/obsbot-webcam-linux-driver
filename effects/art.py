"""Procedural sticker artwork.

Every sprite is drawn in code at start-up, so the project ships no image
assets. Sprites are premultiplied BGRA float images with a small mip chain.
Colours are BGR.
"""

from __future__ import annotations

from functools import cache

import cv2
import numpy as np

WHITE = (255, 255, 255)
BLACK = (18, 16, 16)


class Art:
    """A supersampled canvas with 'paint over' compositing and gradients."""

    def __init__(self, width: int, height: int, supersample: int = 3):
        self.ss = supersample
        self.width = width
        self.height = height
        self.rgb = np.zeros((height * supersample, width * supersample, 3), np.float32)
        self.alpha = np.zeros((height * supersample, width * supersample), np.float32)

    def _points(self, points) -> np.ndarray:
        return np.round(np.asarray(points, np.float32) * self.ss).astype(np.int32)

    def blank(self) -> np.ndarray:
        return np.zeros(self.alpha.shape, np.uint8)

    def poly_mask(self, points) -> np.ndarray:
        mask = self.blank()
        cv2.fillPoly(mask, [self._points(points)], 255, cv2.LINE_AA)
        return mask

    def ellipse_mask(self, cx, cy, rx, ry, angle=0.0, start=0.0, end=360.0) -> np.ndarray:
        mask = self.blank()
        center = (int(round(cx * self.ss)), int(round(cy * self.ss)))
        axes = (max(1, int(round(rx * self.ss))), max(1, int(round(ry * self.ss))))
        cv2.ellipse(mask, center, axes, angle, start, end, 255, -1, cv2.LINE_AA)
        return mask

    def stroke_mask(self, points, thickness, closed=False) -> np.ndarray:
        mask = self.blank()
        cv2.polylines(mask, [self._points(points)], closed, 255,
                      max(1, int(round(thickness * self.ss))), cv2.LINE_AA)
        return mask

    def paint(self, mask: np.ndarray, color, color2=None, opacity: float = 1.0,
              horizontal: bool = False, soften: float = 0.0) -> None:
        if soften > 0.0:
            mask = cv2.GaussianBlur(mask, (0, 0), soften * self.ss)
        x, y, w, h = cv2.boundingRect(mask)
        if w == 0 or h == 0:
            return
        region = (slice(y, y + h), slice(x, x + w))
        alpha = mask[region].astype(np.float32) / 255.0 * opacity
        first = np.array(color, np.float32)
        if color2 is None:
            paint = first
        else:
            second = np.array(color2, np.float32)
            if horizontal:
                t = np.linspace(0.0, 1.0, w, dtype=np.float32)[None, :, None]
            else:
                t = np.linspace(0.0, 1.0, h, dtype=np.float32)[:, None, None]
            paint = first * (1.0 - t) + second * t
        below = self.alpha[region] * (1.0 - alpha)
        total = alpha + below
        self.rgb[region] = (paint * alpha[..., None] + self.rgb[region] * below[..., None]) \
            / np.maximum(total, 1e-6)[..., None]
        self.alpha[region] = total

    def erase(self, mask: np.ndarray) -> None:
        self.alpha *= 1.0 - mask.astype(np.float32) / 255.0

    def poly(self, points, color, color2=None, **options) -> None:
        self.paint(self.poly_mask(points), color, color2, **options)

    def ellipse(self, cx, cy, rx, ry, color, color2=None, angle=0.0, **options) -> None:
        self.paint(self.ellipse_mask(cx, cy, rx, ry, angle), color, color2, **options)

    def stroke(self, points, thickness, color, closed=False, **options) -> None:
        self.paint(self.stroke_mask(points, thickness, closed), color, **options)

    def finish(self, mirror: bool = False) -> "Sprite":
        premultiplied = np.dstack([self.rgb * self.alpha[..., None], self.alpha])
        image = cv2.resize(premultiplied, (self.width, self.height), interpolation=cv2.INTER_AREA)
        if mirror:
            image = image[:, ::-1]
        return Sprite(np.ascontiguousarray(image))


class Sprite:
    def __init__(self, image: np.ndarray):
        self.levels = [image]
        while min(self.levels[-1].shape[:2]) > 24 and len(self.levels) < 5:
            self.levels.append(cv2.resize(self.levels[-1], None, fx=0.5, fy=0.5, interpolation=cv2.INTER_AREA))
        self.width = image.shape[1]
        self.height = image.shape[0]

    def mirrored(self) -> "Sprite":
        return Sprite(np.ascontiguousarray(self.levels[0][:, ::-1]))


def blit(frame: np.ndarray, sprite: Sprite, center, width_px: float, angle: float = 0.0,
         pivot: tuple[float, float] = (0.5, 0.5), opacity: float = 1.0) -> None:
    """Draw `sprite` so that its `pivot` lands on `center`, `width_px` wide, rotated by `angle`."""
    scale = width_px / sprite.width
    if width_px < 2.0 or opacity <= 0.003:
        return
    level = 0
    while level + 1 < len(sprite.levels) and scale * (2 ** (level + 1)) <= 1.0:
        level += 1
    image = sprite.levels[level]
    scale *= sprite.width / image.shape[1]
    src_h, src_w = image.shape[:2]
    cos, sin = np.cos(angle) * scale, np.sin(angle) * scale
    pivot_x, pivot_y = pivot[0] * src_w, pivot[1] * src_h
    matrix = np.array([
        [cos, -sin, center[0] - (cos * pivot_x - sin * pivot_y)],
        [sin, cos, center[1] - (sin * pivot_x + cos * pivot_y)],
    ], np.float64)
    corners = matrix @ np.array([[0, src_w, src_w, 0], [0, 0, src_h, src_h], [1, 1, 1, 1]], np.float64)
    height, width = frame.shape[:2]
    x0 = max(0, int(np.floor(corners[0].min())))
    y0 = max(0, int(np.floor(corners[1].min())))
    x1 = min(width, int(np.ceil(corners[0].max())) + 1)
    y1 = min(height, int(np.ceil(corners[1].max())) + 1)
    if x1 <= x0 or y1 <= y0:
        return
    matrix[:, 2] -= (x0, y0)
    warped = cv2.warpAffine(image, matrix, (x1 - x0, y1 - y0), flags=cv2.INTER_LINEAR,
                            borderMode=cv2.BORDER_CONSTANT, borderValue=0)
    roi = frame[y0:y1, x0:x1]
    cover = 1.0 - warped[..., 3:4] * opacity
    roi[:] = np.clip(roi * cover + warped[..., :3] * opacity, 0, 255).astype(np.uint8)


def smooth(points, samples: int = 10, closed: bool = True) -> np.ndarray:
    """Catmull-Rom curve through `points`."""
    pts = np.asarray(points, np.float32)
    count = len(pts)
    t = np.linspace(0.0, 1.0, samples, endpoint=False, dtype=np.float32)[:, None]
    out = []
    last = count if closed else count - 1
    for i in range(last):
        if closed:
            p0, p1, p2, p3 = pts[(i - 1) % count], pts[i], pts[(i + 1) % count], pts[(i + 2) % count]
        else:
            p0, p1, p2, p3 = pts[max(i - 1, 0)], pts[i], pts[i + 1], pts[min(i + 2, count - 1)]
        out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t ** 2
                          + (-p0 + 3 * p1 - 3 * p2 + p3) * t ** 3))
    if not closed:
        out.append(pts[-1:])
    return np.concatenate(out)


def mirror_x(points, width: float) -> np.ndarray:
    pts = np.asarray(points, np.float32).copy()
    pts[:, 0] = width - pts[:, 0]
    return pts


def heart_points(cx: float, cy: float, size: float, samples: int = 90) -> np.ndarray:
    t = np.linspace(0.0, 2.0 * np.pi, samples, endpoint=False)
    x = 16.0 * np.sin(t) ** 3
    y = -(13.0 * np.cos(t) - 5.0 * np.cos(2 * t) - 2.0 * np.cos(3 * t) - np.cos(4 * t))
    return np.stack([cx + x * size / 34.0, cy + (y - 2.5) * size / 34.0], axis=1).astype(np.float32)


def star_points(cx: float, cy: float, outer: float, inner: float, tips: int = 5) -> np.ndarray:
    angles = -np.pi / 2.0 + np.arange(tips * 2) * np.pi / tips
    radii = np.where(np.arange(tips * 2) % 2 == 0, outer, inner)
    return np.stack([cx + np.cos(angles) * radii, cy + np.sin(angles) * radii], axis=1).astype(np.float32)


def lens_points(cx: float, cy: float, rx: float, ry: float, power: float = 2.8) -> np.ndarray:
    t = np.linspace(0.0, 2.0 * np.pi, 72, endpoint=False)
    cos, sin = np.cos(t), np.sin(t)
    x = np.sign(cos) * np.abs(cos) ** (2.0 / power)
    y = np.sign(sin) * np.abs(sin) ** (2.0 / power)
    return np.stack([cx + x * rx, cy + y * ry], axis=1).astype(np.float32)


# --- Eyewear -----------------------------------------------------------------

@cache
def sunglasses() -> Sprite:
    art = Art(400, 150)
    for cx in (104, 296):
        art.poly(lens_points(cx, 78, 88, 62), BLACK)
        art.poly(lens_points(cx, 79, 76, 50), (58, 44, 36), (20, 14, 12), opacity=0.93)
        shine = art.poly_mask([(cx - 52, 44), (cx - 20, 38), (cx - 50, 102), (cx - 66, 96)])
        art.paint(cv2.bitwise_and(shine, art.poly_mask(lens_points(cx, 79, 76, 50))), WHITE,
                  opacity=0.22, soften=2.0)
    art.stroke(smooth([(176, 62), (200, 50), (224, 62)], closed=False), 11, BLACK)
    art.poly([(8, 46), (30, 40), (30, 62), (4, 62)], BLACK)
    art.poly([(392, 46), (370, 40), (370, 62), (396, 62)], BLACK)
    return art.finish()


@cache
def heart_glasses() -> Sprite:
    art = Art(400, 170)
    for cx in (106, 294):
        art.poly(heart_points(cx, 88, 168), (70, 30, 215))
        art.poly(heart_points(cx, 88, 136), (190, 120, 255), (120, 60, 235), opacity=0.82)
        shine = art.ellipse_mask(cx - 30, 60, 16, 9, angle=-35)
        art.paint(shine, WHITE, opacity=0.55, soften=1.5)
    art.stroke(smooth([(180, 70), (200, 58), (220, 70)], closed=False), 9, (70, 30, 215))
    return art.finish()


# --- Animals -----------------------------------------------------------------

@cache
def dog_ear() -> Sprite:
    """Image-left floppy ear; mirror for the other side."""
    art = Art(180, 230)
    outline = smooth([(150, 26), (104, 8), (56, 30), (22, 84), (12, 150), (36, 208), (72, 214),
                      (100, 172), (120, 120), (158, 74)])
    art.poly(outline, (38, 70, 120), (22, 44, 82))
    inner = smooth([(128, 44), (98, 34), (64, 54), (42, 100), (38, 150), (54, 186), (72, 176),
                    (90, 138), (108, 96)])
    art.poly(inner, (60, 96, 150), (36, 62, 108), opacity=0.9)
    return art.finish()


@cache
def dog_ear_mirrored() -> Sprite:
    return dog_ear().mirrored()


@cache
def dog_nose() -> Sprite:
    art = Art(170, 130)
    art.poly(smooth([(85, 14), (132, 22), (154, 50), (128, 86), (85, 100), (42, 86), (16, 50), (38, 22)]),
             (46, 40, 40), BLACK)
    art.stroke([(85, 98), (85, 122)], 7, BLACK)
    art.ellipse(62, 40, 22, 10, WHITE, angle=-18, opacity=0.5, soften=2.5)
    return art.finish()


@cache
def dog_tongue() -> Sprite:
    art = Art(130, 170)
    shape = smooth([(18, 4), (112, 4), (118, 80), (100, 142), (65, 164), (30, 142), (12, 80)])
    art.poly(shape, (150, 120, 250), (110, 70, 225))
    art.stroke([(65, 10), (65, 120)], 5, (95, 55, 195), opacity=0.7, soften=1.0)
    art.ellipse(42, 54, 12, 30, WHITE, opacity=0.25, soften=4.0)
    return art.finish()


@cache
def cat_ear() -> Sprite:
    """Image-left pointed ear; mirror for the other side."""
    art = Art(170, 170)
    art.poly(smooth([(30, 10), (52, 22), (150, 108), (158, 150), (120, 164), (30, 150), (14, 110), (16, 30)], 8),
             (72, 72, 78), (44, 44, 50))
    art.poly(smooth([(42, 44), (120, 118), (112, 140), (48, 134), (34, 104)], 8),
             (190, 170, 250), (150, 120, 235))
    return art.finish()


@cache
def cat_ear_mirrored() -> Sprite:
    return cat_ear().mirrored()


@cache
def cat_face() -> Sprite:
    """Whiskers and a small nose, anchored on the nose tip at the centre."""
    art = Art(520, 150)
    for side in (-1, 1):
        for rise in (-26, 0, 26):
            start = (260 + side * 62, 78 + rise * 0.35)
            end = (260 + side * 248, 78 + rise * 1.9 - 10)
            middle = ((start[0] + end[0]) / 2.0, (start[1] + end[1]) / 2.0 - 8)
            curve = smooth([start, middle, end], closed=False)
            art.stroke(curve, 6, (40, 40, 44), opacity=0.55)
            art.stroke(curve, 3, WHITE)
    art.poly(smooth([(232, 52), (288, 52), (276, 84), (260, 96), (244, 84)], 8), (170, 140, 250), (120, 90, 225))
    art.ellipse(250, 64, 9, 5, WHITE, opacity=0.45, soften=1.5)
    return art.finish()


@cache
def bunny_ears() -> Sprite:
    art = Art(320, 340)
    for side, angle in ((-1, -13), (1, 13)):
        cx = 160 + side * 74
        art.ellipse(cx, 178, 52, 156, (250, 248, 246), (222, 218, 216), angle=angle)
        art.ellipse(cx, 190, 26, 122, (200, 180, 252), (165, 135, 240), angle=angle)
    return art.finish()


@cache
def bunny_nose() -> Sprite:
    art = Art(150, 130)
    art.poly(smooth([(46, 20), (104, 20), (94, 50), (75, 62), (56, 50)], 8), (170, 140, 250), (125, 95, 230))
    for x in (58, 92):
        art.poly(smooth([(x - 15, 74), (x + 15, 74), (x + 14, 116), (x, 124), (x - 14, 116)], 6),
                 (252, 250, 248), (226, 224, 224))
    art.stroke([(75, 76), (75, 120)], 2.5, (150, 150, 160))
    return art.finish()


# --- Facial hair and headwear -------------------------------------------------

@cache
def moustache() -> Sprite:
    art = Art(340, 120)
    half = [(170, 40), (196, 24), (228, 24), (262, 40), (294, 44), (318, 22), (328, 34), (312, 66),
            (276, 84), (230, 86), (194, 74), (170, 62)]
    for points in (half, mirror_x(half, 340)):
        art.poly(smooth(points, 8), (44, 40, 52), (16, 14, 18))
    for points in ([(196, 38), (232, 36), (268, 52), (300, 52)],):
        art.stroke(smooth(points, closed=False), 4, (120, 110, 120), opacity=0.5, soften=1.5)
        art.stroke(smooth(mirror_x(points, 340), closed=False), 4, (120, 110, 120), opacity=0.5, soften=1.5)
    return art.finish()


@cache
def crown() -> Sprite:
    art = Art(380, 240)
    gold, deep = (70, 210, 255), (20, 140, 225)
    body = [(40, 196), (28, 74), (112, 132), (190, 34), (268, 132), (352, 74), (340, 196)]
    art.poly(body, (16, 100, 170))
    inset = [(52, 190), (42, 98), (114, 148), (190, 54), (266, 148), (338, 98), (328, 190)]
    art.poly(inset, gold, deep)
    art.poly([(34, 178), (346, 178), (346, 222), (34, 222)], (16, 100, 170))
    art.poly([(42, 184), (338, 184), (338, 216), (42, 216)], (120, 230, 255), (30, 160, 235))
    for cx, cy in ((28, 70), (190, 30), (352, 70)):
        art.ellipse(cx, cy, 17, 17, (16, 100, 170))
        art.ellipse(cx, cy, 12, 12, (150, 240, 255), gold)
    for cx, color, dark in ((110, (90, 70, 235), (50, 30, 170)), (190, (235, 150, 60), (180, 90, 20)),
                            (270, (120, 200, 70), (70, 140, 30))):
        art.ellipse(cx, 200, 14, 11, color, dark)
        art.ellipse(cx - 4, 196, 5, 3, WHITE, opacity=0.7, soften=0.8)
    return art.finish()


@cache
def party_hat() -> Sprite:
    art = Art(260, 320)
    cone = art.poly_mask([(130, 36), (28, 288), (232, 288)])
    art.paint(cone, (235, 150, 60), (200, 90, 40), horizontal=True)
    for index in range(7):
        y = index * 56 - 10
        color = (80, 220, 255) if index % 2 == 0 else (160, 110, 255)
        stripe = art.poly_mask([(0, y + 70), (260, y - 30), (260, y - 6), (0, y + 94)])
        art.paint(cv2.bitwise_and(stripe, cone), color, opacity=0.95)
    for x, y in ((104, 150), (150, 214), (86, 244), (170, 130), (128, 96)):
        dot = art.ellipse_mask(x, y, 9, 9)
        art.paint(cv2.bitwise_and(dot, cone), WHITE, opacity=0.9)
    shade = art.poly_mask([(130, 36), (232, 288), (176, 288)])
    art.paint(shade, BLACK, opacity=0.16, soften=3.0)
    for x in range(34, 240, 26):
        art.ellipse(x, 290, 16, 14, (250, 248, 250), (220, 214, 226))
    for dx, dy, radius in ((0, 0, 24), (-16, 8, 15), (16, 8, 15), (-9, -14, 14), (10, -13, 14)):
        art.ellipse(130 + dx, 34 + dy, radius, radius, (252, 250, 252), (222, 216, 230))
    return art.finish()


@cache
def top_hat() -> Sprite:
    art = Art(400, 300)
    art.ellipse(200, 256, 190, 34, (40, 36, 36), BLACK)
    art.poly([(100, 46), (300, 46), (292, 254), (108, 254)], (64, 58, 58), BLACK, horizontal=True)
    art.ellipse(200, 46, 100, 20, (74, 68, 68), (40, 36, 36))
    art.poly([(106, 196), (294, 196), (292, 244), (108, 244)], (60, 50, 200), (36, 26, 140))
    art.ellipse(200, 250, 92, 15, BLACK, opacity=0.55, soften=2.0)
    art.poly([(120, 60), (142, 60), (146, 190), (126, 190)], WHITE, opacity=0.1, soften=3.0)
    return art.finish()


@cache
def halo() -> Sprite:
    art = Art(380, 150)
    ring = art.stroke_mask(cv2.ellipse2Poly((190, 75), (146, 38), 0, 0, 360, 4), 15, closed=True)
    art.paint(ring, (90, 225, 255), opacity=0.55, soften=9.0)
    art.paint(ring, (110, 230, 255), (40, 190, 250))
    highlight = art.stroke_mask(cv2.ellipse2Poly((190, 72), (146, 38), 0, 190, 350, 4), 4)
    art.paint(highlight, WHITE, opacity=0.8, soften=0.8)
    return art.finish()


@cache
def horns() -> Sprite:
    art = Art(400, 210)
    left = [(122, 204), (64, 204), (40, 150), (30, 90), (44, 30), (60, 10), (66, 52), (84, 106), (112, 156)]
    for points in (left, mirror_x(left, 400)):
        art.poly(smooth(points, 8), (70, 60, 235), (30, 20, 130))
    shine = [(52, 60), (58, 110), (80, 160)]
    for points in (shine, mirror_x(shine, 400)):
        art.stroke(smooth(points, closed=False), 6, (170, 170, 255), opacity=0.55, soften=2.0)
    return art.finish()


@cache
def clown_nose() -> Sprite:
    art = Art(130, 130)
    art.ellipse(65, 65, 58, 58, (60, 50, 240), (30, 20, 170))
    art.ellipse(65, 92, 40, 20, BLACK, opacity=0.18, soften=6.0)
    art.ellipse(46, 42, 17, 11, WHITE, angle=-30, opacity=0.8, soften=2.0)
    return art.finish()


# --- Particles ---------------------------------------------------------------

def _heart(light, dark) -> Sprite:
    art = Art(110, 110)
    art.poly(heart_points(55, 56, 98), light, dark)
    art.ellipse(36, 36, 11, 7, WHITE, angle=-35, opacity=0.6, soften=1.5)
    return art.finish()


@cache
def heart_red() -> Sprite:
    return _heart((90, 80, 255), (60, 30, 215))


@cache
def heart_pink() -> Sprite:
    return _heart((200, 150, 255), (150, 90, 245))


@cache
def star() -> Sprite:
    art = Art(110, 110)
    art.poly(star_points(55, 58, 52, 23), (30, 150, 235))
    art.poly(star_points(55, 58, 44, 19), (120, 240, 255), (40, 190, 250))
    art.ellipse(44, 44, 8, 5, WHITE, angle=-30, opacity=0.7, soften=1.2)
    return art.finish()


@cache
def sparkle() -> Sprite:
    art = Art(100, 100)
    points = star_points(50, 50, 48, 9, tips=4)
    art.paint(art.poly_mask(points), (150, 240, 255), opacity=0.7, soften=3.0)
    art.poly(points, WHITE)
    return art.finish()


@cache
def storm_cloud() -> Sprite:
    art = Art(440, 230)
    puffs = [(110, 140, 82, 62), (190, 96, 96, 78), (280, 110, 88, 70), (340, 150, 72, 52), (220, 160, 150, 48)]
    for cx, cy, rx, ry in puffs:
        art.ellipse(cx, cy + 8, rx + 6, ry + 6, (40, 36, 38))
    for cx, cy, rx, ry in puffs:
        art.ellipse(cx, cy, rx, ry, (120, 112, 110), (62, 56, 58))
    for cx, cy, rx, ry in ((170, 62, 52, 22), (270, 78, 44, 18), (100, 108, 36, 16)):
        art.ellipse(cx, cy, rx, ry, (190, 184, 182), opacity=0.4, soften=7.0)
    return art.finish()


@cache
def raindrop() -> Sprite:
    art = Art(24, 60)
    art.poly(smooth([(12, 2), (19, 34), (17, 50), (12, 57), (7, 50), (5, 34)], 6), (255, 215, 150), (240, 160, 70))
    art.ellipse(10, 40, 2.5, 7, WHITE, opacity=0.6, soften=0.8)
    return art.finish()


@cache
def lightning_bolt() -> Sprite:
    art = Art(150, 240)
    bolt = [(92, 4), (34, 112), (72, 112), (40, 236), (124, 92), (84, 92), (118, 4)]
    art.paint(art.poly_mask(bolt), (120, 240, 255), opacity=0.55, soften=8.0)
    art.poly(bolt, (40, 200, 250))
    inner = [(96, 22), (56, 100), (88, 100), (62, 190), (104, 104), (72, 104), (104, 22)]
    art.poly(inner, (200, 252, 255))
    return art.finish()

