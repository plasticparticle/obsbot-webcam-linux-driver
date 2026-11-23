"""Retouching: skin, eyes, lips, cheeks and teeth. All edits stay inside small face regions."""

from __future__ import annotations

import cv2
import numpy as np

from landmarks import (FACE_OVAL, LEFT_BROW, LEFT_EYE, LEFT_IRIS, LIPS_INNER, LIPS_OUTER,
                       RIGHT_BROW, RIGHT_EYE, RIGHT_IRIS, Face, bounds)

# BGR
EYE_COLORS = {
    "blue": (108, 68, 28),
    "green": (62, 82, 35),
    "hazel": (35, 78, 105),
    "gray": (105, 100, 94),
    "violet": (110, 55, 92),
    "amber": (30, 110, 170),
}
LIP_COLORS = {
    "rose": (110, 84, 214),
    "red": (58, 30, 196),
    "coral": (90, 110, 240),
    "berry": (90, 30, 150),
    "plum": (95, 40, 110),
    "nude": (105, 120, 190),
}
BLUSH_COLOR = np.array([125, 105, 238], np.float32)


def region_mask(shape: tuple[int, int], origin: tuple[int, int], fill: list[np.ndarray],
                cut: list[np.ndarray] = (), blur: float = 0.0) -> np.ndarray:
    """Float mask (0..1) for polygons given in frame coordinates, drawn in a region."""
    mask = np.zeros(shape, np.uint8)
    shift = np.array(origin, np.float32)
    for polygon in fill:
        cv2.fillPoly(mask, [np.round(polygon - shift).astype(np.int32)], 255, cv2.LINE_AA)
    for polygon in cut:
        cv2.fillPoly(mask, [np.round(polygon - shift).astype(np.int32)], 0, cv2.LINE_AA)
    if blur > 0.0:
        mask = cv2.GaussianBlur(mask, (0, 0), blur)
    return mask.astype(np.float32) / 255.0


def grow(polygon: np.ndarray, factor: float) -> np.ndarray:
    center = polygon.mean(axis=0)
    return center + (polygon - center) * factor


def skin(frame: np.ndarray, face: Face, smoothing: float, tone: float) -> None:
    pts = face.pts
    height, width = frame.shape[:2]
    feather = max(2.0, face.size / 60.0)
    box = bounds(pts[FACE_OVAL], feather * 3.0, width, height)
    if box is None:
        return
    x0, y0, x1, y1 = box
    roi = frame[y0:y1, x0:x1]
    keep = [grow(pts[RIGHT_EYE], 1.35), grow(pts[LEFT_EYE], 1.35), pts[LIPS_OUTER],
            pts[RIGHT_BROW], pts[LEFT_BROW]]
    mask = region_mask(roi.shape[:2], (x0, y0), [pts[FACE_OVAL]], keep, feather)[..., None]
    result = roi.astype(np.float32)
    if smoothing > 0.01:
        # Filter a reduced copy: skin texture is low-frequency, and this keeps
        # the cost flat regardless of output resolution.
        scale = min(1.0, 240.0 / max(roi.shape[:2]))
        small = cv2.resize(roi, None, fx=scale, fy=scale, interpolation=cv2.INTER_AREA) if scale < 1.0 else roi
        small = cv2.bilateralFilter(small, 9, 22.0 + 40.0 * smoothing, 4.0 + 3.0 * smoothing)
        if smoothing > 0.5:
            small = cv2.bilateralFilter(small, 7, 20.0 + 30.0 * smoothing, 4.0)
        filtered = cv2.resize(small, (roi.shape[1], roi.shape[0]), interpolation=cv2.INTER_LINEAR)
        detail = result - cv2.GaussianBlur(roi, (0, 0), 1.4).astype(np.float32)
        softened = filtered.astype(np.float32) + detail * (0.35 - 0.25 * smoothing)
        amount = mask * (0.25 + 0.65 * smoothing)
        result = result * (1.0 - amount) + softened * amount
    if abs(tone) > 0.01:
        lab = cv2.cvtColor(np.clip(result, 0, 255).astype(np.uint8), cv2.COLOR_BGR2LAB).astype(np.float32)
        lab[..., 0] += tone * 7.0 * mask[..., 0]
        lab[..., 1] += tone * 5.0 * mask[..., 0]
        lab[..., 2] += tone * 7.0 * mask[..., 0]
        result = cv2.cvtColor(np.clip(lab, 0, 255).astype(np.uint8), cv2.COLOR_LAB2BGR).astype(np.float32)
    roi[:] = np.clip(result, 0, 255).astype(np.uint8)


def recolor_eyes(frame: np.ndarray, face: Face, color_name: str) -> None:
    color = EYE_COLORS.get(color_name)
    pts = face.pts
    if color is None or len(pts) <= max(LEFT_IRIS):
        return
    height, width = frame.shape[:2]
    for iris_indices, eye_indices in ((RIGHT_IRIS, RIGHT_EYE), (LEFT_IRIS, LEFT_EYE)):
        iris = pts[iris_indices]
        center = iris.mean(axis=0)
        radius = float(np.max(np.linalg.norm(iris - center, axis=1)))
        box = bounds(iris, radius * 0.5 + 2, width, height)
        if box is None or radius < 1.5:
            continue
        x0, y0, x1, y1 = box
        roi = frame[y0:y1, x0:x1]
        disc = np.zeros(roi.shape[:2], np.uint8)
        local = (int(round(center[0] - x0)), int(round(center[1] - y0)))
        cv2.circle(disc, local, max(2, int(round(radius))), 255, -1, cv2.LINE_AA)
        # Leave the pupil dark and never paint over the eyelids.
        cv2.circle(disc, local, max(1, int(round(radius * 0.38))), 70, -1, cv2.LINE_AA)
        lids = region_mask(roi.shape[:2], (x0, y0), [pts[eye_indices]])
        alpha = (cv2.GaussianBlur(disc, (0, 0), 1.0).astype(np.float32) / 255.0 * lids * 0.6)[..., None]
        luminance = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY).astype(np.float32) / 150.0
        tinted = np.array(color, np.float32) * np.clip(luminance[..., None], 0.35, 1.45)
        roi[:] = np.clip(roi * (1.0 - alpha) + tinted * alpha, 0, 255).astype(np.uint8)


def brighten_eyes(frame: np.ndarray, face: Face, amount: float) -> None:
    pts = face.pts
    height, width = frame.shape[:2]
    for indices in (RIGHT_EYE, LEFT_EYE):
        eye = pts[indices]
        box = bounds(eye, 4, width, height)
        if box is None:
            continue
        x0, y0, x1, y1 = box
        roi = frame[y0:y1, x0:x1]
        mask = region_mask(roi.shape[:2], (x0, y0), [eye], blur=max(1.0, face.size / 220.0))[..., None]
        source = roi.astype(np.float32)
        crisp = source + (source - cv2.GaussianBlur(roi, (0, 0), 2.0).astype(np.float32)) * 0.9
        lifted = crisp * (1.0 + 0.10 * amount) + 6.0 * amount
        alpha = mask * amount * 0.85
        roi[:] = np.clip(source * (1.0 - alpha) + lifted * alpha, 0, 255).astype(np.uint8)


def tint_lips(frame: np.ndarray, face: Face, color_name: str, strength: float) -> None:
    color = LIP_COLORS.get(color_name)
    if color is None:
        return
    pts = face.pts
    height, width = frame.shape[:2]
    outer = pts[LIPS_OUTER]
    box = bounds(outer, 5, width, height)
    if box is None:
        return
    x0, y0, x1, y1 = box
    roi = frame[y0:y1, x0:x1]
    mask = region_mask(roi.shape[:2], (x0, y0), [outer], [pts[LIPS_INNER]],
                       max(0.8, face.mouth_width / 55.0))
    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY).astype(np.float32)
    weight = float(mask.sum())
    reference = float((gray * mask).sum()) / weight if weight > 1.0 else 128.0
    shade = np.clip(gray / max(reference, 40.0), 0.45, 1.6)[..., None]
    alpha = (mask * (0.25 + 0.6 * strength))[..., None]
    tinted = np.array(color, np.float32) * shade
    roi[:] = np.clip(roi * (1.0 - alpha) + tinted * alpha, 0, 255).astype(np.uint8)


def blush(frame: np.ndarray, face: Face, strength: float) -> None:
    pts = face.pts
    height, width = frame.shape[:2]
    radius = face.size * 0.17
    for cheek in ((pts[50] + pts[205]) / 2.0, (pts[280] + pts[425]) / 2.0):
        box = bounds(cheek[None, :], radius * 1.6, width, height)
        if box is None:
            continue
        x0, y0, x1, y1 = box
        roi = frame[y0:y1, x0:x1]
        ys, xs = np.indices(roi.shape[:2], dtype=np.float32)
        offset_x = xs + x0 - cheek[0]
        offset_y = ys + y0 - cheek[1]
        along = offset_x * face.right[0] + offset_y * face.right[1]
        across = offset_x * face.down[0] + offset_y * face.down[1]
        falloff = np.exp(-((along / radius) ** 2 + (across / (radius * 0.72)) ** 2) * 1.6)
        inside = region_mask(roi.shape[:2], (x0, y0), [pts[FACE_OVAL]], blur=max(2.0, face.size / 50.0))
        alpha = (falloff * inside * 0.42 * strength)[..., None]
        roi[:] = np.clip(roi * (1.0 - alpha) + BLUSH_COLOR * alpha, 0, 255).astype(np.uint8)


def whiten_teeth(frame: np.ndarray, face: Face, strength: float) -> None:
    pts = face.pts
    height, width = frame.shape[:2]
    inner = pts[LIPS_INNER]
    box = bounds(inner, 2, width, height)
    if box is None:
        return
    x0, y0, x1, y1 = box
    roi = frame[y0:y1, x0:x1]
    mask = region_mask(roi.shape[:2], (x0, y0), [inner], blur=0.8)
    if float(mask.sum()) < 4.0:
        return
    hsv = cv2.cvtColor(roi, cv2.COLOR_BGR2HSV).astype(np.float32)
    # Teeth are the bright, weakly saturated pixels inside the mouth.
    teeth = mask * np.clip((hsv[..., 2] - 70.0) / 50.0, 0.0, 1.0) * np.clip((150.0 - hsv[..., 1]) / 60.0, 0.0, 1.0)
    hsv[..., 1] *= 1.0 - 0.75 * strength * teeth
    hsv[..., 2] += 30.0 * strength * teeth
    roi[:] = cv2.cvtColor(np.clip(hsv, 0, 255).astype(np.uint8), cv2.COLOR_HSV2BGR)
