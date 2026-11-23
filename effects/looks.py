"""Colour looks and film finishes: fast LUT/matrix grading for whole frames."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import cv2
import numpy as np

# Luminance weights in OpenCV's BGR channel order.
_LUMA_BGR = np.array([0.114, 0.587, 0.299], np.float32)
_IDENTITY_LUT = np.repeat(np.arange(256, dtype=np.float32)[:, None], 3, axis=1)


@dataclass(frozen=True)
class Look:
    """A grade described in RGB terms; converted to BGR tables on demand."""

    saturation: float = 1.0
    contrast: float = 0.0
    fade: float = 0.0
    gamma: float = 1.0
    gain: tuple[float, float, float] = (1.0, 1.0, 1.0)
    shadows: tuple[float, float, float] = (0.0, 0.0, 0.0)
    highlights: tuple[float, float, float] = (0.0, 0.0, 0.0)
    sepia: float = 0.0
    glow: float = 0.0


LOOKS: dict[str, Look] = {
    "golden": Look(saturation=1.12, contrast=0.12, gain=(1.08, 1.02, 0.90),
                   highlights=(0.03, 0.01, -0.03)),
    "arctic": Look(saturation=0.95, contrast=0.10, gain=(0.92, 1.0, 1.10),
                   shadows=(-0.01, 0.0, 0.03)),
    "pop": Look(saturation=1.45, contrast=0.25),
    "vintage": Look(saturation=0.78, contrast=0.10, fade=0.08, gain=(1.06, 1.0, 0.86),
                    shadows=(0.0, 0.02, 0.05)),
    "cinema": Look(saturation=1.10, contrast=0.22, shadows=(-0.03, 0.02, 0.05),
                   highlights=(0.05, 0.01, -0.05)),
    "sunset": Look(saturation=1.20, contrast=0.10, gain=(1.12, 0.95, 0.85),
                   shadows=(0.04, -0.01, 0.05), highlights=(0.06, 0.0, -0.04)),
    "dream": Look(saturation=1.10, fade=0.10, gamma=0.90, gain=(1.05, 0.98, 1.04), glow=0.35),
    "neon": Look(saturation=1.30, contrast=0.30, gain=(1.0, 0.93, 1.10),
                 shadows=(0.03, -0.02, 0.08), highlights=(0.02, -0.01, 0.03)),
    "mono": Look(saturation=0.0, contrast=0.10, fade=0.06),
    "noir": Look(saturation=0.0, contrast=0.45),
    "sepia": Look(saturation=0.0, contrast=0.08, fade=0.04, sepia=1.0),
}


def _tables(look: Look, strength: float) -> tuple[np.ndarray, np.ndarray]:
    """Return (3x3 BGR matrix, 256x1x3 BGR LUT) for a look at the given strength."""
    saturation = look.saturation
    matrix = np.eye(3, dtype=np.float32) * saturation + (1.0 - saturation) * np.tile(_LUMA_BGR, (3, 1))
    if look.sepia > 0.0:
        # Luminance is already in every channel; tint it towards warm brown.
        tint_bgr = np.array([0.74, 0.92, 1.04], np.float32)
        matrix = matrix * (1.0 + (tint_bgr[:, None] - 1.0) * look.sepia)
    matrix = np.eye(3, dtype=np.float32) * (1.0 - strength) + matrix * strength

    x = np.linspace(0.0, 1.0, 256, dtype=np.float32)
    smooth = x * x * (3.0 - 2.0 * x)
    lut = np.empty((256, 3), np.float32)
    for bgr_index, rgb_index in enumerate((2, 1, 0)):
        y = x + (smooth - x) * look.contrast
        y = np.clip(y, 0.0, 1.0) ** look.gamma
        y = y * look.gain[rgb_index]
        y = look.fade + y * (1.0 - look.fade)
        y = y + look.shadows[rgb_index] * (1.0 - x) ** 2 + look.highlights[rgb_index] * x ** 2
        lut[:, bgr_index] = np.clip(y, 0.0, 1.0) * 255.0
    lut = _IDENTITY_LUT * (1.0 - strength) + lut * strength
    return matrix, np.round(lut).astype(np.uint8).reshape(256, 1, 3)


class LutError(ValueError):
    pass


def load_cube(path: Path) -> np.ndarray:
    """Read a 3D `.cube` LUT as an array indexed [blue][green][red] of RGB in 0..1."""
    size = 0
    low, high = 0.0, 1.0
    rows: list[str] = []
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            head = line.split(None, 1)[0].upper()
            if head == "LUT_3D_SIZE":
                size = int(line.split()[1])
            elif head == "LUT_1D_SIZE":
                raise LutError("1D LUTs are not supported")
            elif head == "DOMAIN_MIN":
                low = float(line.split()[1])
            elif head == "DOMAIN_MAX":
                high = float(line.split()[1])
            elif head[0].isdigit() or head[0] in "-+.":
                rows.append(line)
    if not 2 <= size <= 129:
        raise LutError("missing or unsupported LUT_3D_SIZE")
    data = np.array(" ".join(rows).split(), np.float32)
    if data.size != size ** 3 * 3:
        raise LutError("wrong number of entries")
    data = (data - low) / max(high - low, 1e-6)
    return np.clip(data.reshape(size, size, size, 3), 0.0, 1.0)


def pack_lut(lut: np.ndarray) -> np.ndarray:
    """Expand a LUT to one packed BGR entry for each of the 2^24 input colours.

    A frame viewed as packed pixels then grades with a single table lookup.
    """
    size = lut.shape[0]
    position = np.linspace(0.0, size - 1.0, 256, dtype=np.float32)
    lower = np.minimum(position.astype(int), size - 2)
    weights = np.zeros((256, size), np.float32)
    weights[np.arange(256), lower] = 1.0 - (position - lower)
    weights[np.arange(256), lower + 1] += position - lower
    by_red = np.tensordot(weights, lut.transpose(2, 1, 0, 3), axes=(1, 0))
    table = np.empty((256, 256, 256), np.uint32)
    for red in range(256):
        plane = np.tensordot(np.tensordot(weights, by_red[red], axes=(1, 0)), weights, axes=(1, 1))
        value = np.clip(plane * 255.0 + 0.5, 0, 255).astype(np.uint32)
        table[red] = value[:, 2, :] | (value[:, 1, :] << 8) | (value[:, 0, :] << 16)
    return table.reshape(-1)


class Grader:
    """Applies the selected look plus grain, vignette and glow."""

    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self._cache_key: tuple[str, int] | None = None
        self._tables: tuple[np.ndarray, np.ndarray] | None = None
        rng = np.random.default_rng(7)
        self._grain = []
        for _ in range(3):
            noise = np.clip(rng.normal(128.0, 36.0, (height, width, 1)), 0, 255).astype(np.uint8).repeat(3, axis=2)
            self._grain += [noise, np.ascontiguousarray(noise[:, ::-1]), np.ascontiguousarray(noise[::-1])]
        self._frame = 0
        ys, xs = np.indices((height, width), dtype=np.float32)
        radial = ((xs - width / 2.0) / (width / 2.0)) ** 2 + ((ys - height / 2.0) / (height / 2.0)) ** 2
        self._vignette = np.clip((radial - 0.25) / 1.45, 0.0, 1.0) ** 1.3
        self._vignette_key = -1
        self._vignette_gain: np.ndarray | None = None
        self._lut_key: tuple[str, int] | None = None
        self._lut_table: np.ndarray | None = None

    def _lut(self, path: str) -> np.ndarray | None:
        try:
            key = (path, Path(path).stat().st_mtime_ns)
        except OSError:
            key = (path, -1)
        if key != self._lut_key:
            self._lut_key = key
            try:
                self._lut_table = pack_lut(load_cube(Path(path)))
            except (OSError, ValueError) as error:
                print(f"could not load LUT {path}: {error}", flush=True)
                self._lut_table = None
        return self._lut_table

    def _apply_lut(self, frame: np.ndarray, table: np.ndarray, strength: float) -> np.ndarray:
        height, width = frame.shape[:2]
        packed = cv2.cvtColor(frame, cv2.COLOR_BGR2BGRA).view(np.uint32).reshape(height, width)
        graded = table[packed & 0xFFFFFF].view(np.uint8).reshape(height, width, 4)
        graded = cv2.cvtColor(graded, cv2.COLOR_BGRA2BGR)
        if strength >= 0.995:
            return graded
        return cv2.addWeighted(graded, strength, frame, 1.0 - strength, 0.0)

    def apply(self, frame: np.ndarray, name: str, strength: float,
              grain: float, vignette: float, glow: float,
              lut_path: str = "", lut_strength: float = 1.0) -> np.ndarray:
        if lut_path and lut_strength > 0.01:
            table = self._lut(lut_path)
            if table is not None:
                frame = self._apply_lut(frame, table, lut_strength)
        look = LOOKS.get(name)
        if look is not None and strength > 0.01:
            key = (name, int(round(strength * 100)))
            if key != self._cache_key:
                self._tables = _tables(look, key[1] / 100.0)
                self._cache_key = key
            matrix, lut = self._tables
            frame = cv2.LUT(cv2.transform(frame, matrix), lut)
            glow = max(glow, look.glow * strength)
        if glow > 0.01:
            frame = self._glow(frame, glow)
        if vignette > 0.01:
            frame = self._apply_vignette(frame, vignette)
        if grain > 0.01:
            self._frame += 1
            noise = self._grain[(self._frame * 4) % len(self._grain)]
            amount = 0.26 * grain
            frame = cv2.addWeighted(frame, 1.0, noise, amount, -128.0 * amount)
        return frame

    def _glow(self, frame: np.ndarray, amount: float) -> np.ndarray:
        small = cv2.resize(frame, (self.width // 4, self.height // 4), interpolation=cv2.INTER_AREA)
        small = cv2.GaussianBlur(small, (0, 0), max(2.0, self.width / 160.0))
        bloom = cv2.resize(small, (self.width, self.height), interpolation=cv2.INTER_LINEAR)
        # Screen blend: brightens highlights without clipping them flat.
        lift = cv2.multiply(cv2.bitwise_not(frame), bloom, scale=0.6 * amount / 255.0)
        return cv2.add(frame, lift)

    def _apply_vignette(self, frame: np.ndarray, amount: float) -> np.ndarray:
        key = int(round(amount * 100))
        if key != self._vignette_key:
            gain = 1.0 - 0.62 * (key / 100.0) * self._vignette
            self._vignette_gain = cv2.merge([gain, gain, gain])
            self._vignette_key = key
        return cv2.multiply(frame, self._vignette_gain, dtype=cv2.CV_8U)
