#!/usr/bin/env python3
"""Regenerate the bundled .cube LUTs.

The LUTs are computed from the formulas below rather than copied from any
film stock or commercial pack, so they can be redistributed with the project.
Run `python effects/luts/generate.py` after editing a recipe.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np

SIZE = 17


def luma(rgb: np.ndarray) -> np.ndarray:
    return rgb @ np.array([0.2126, 0.7152, 0.0722], np.float32)


def saturate(rgb: np.ndarray, amount: float) -> np.ndarray:
    gray = luma(rgb)[..., None]
    return gray + (rgb - gray) * amount


def contrast(rgb: np.ndarray, amount: float) -> np.ndarray:
    smooth = rgb * rgb * (3.0 - 2.0 * rgb)
    return rgb + (smooth - rgb) * amount


def to_hsv(rgb: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    high, low = rgb.max(-1), rgb.min(-1)
    span = high - low
    safe = np.where(span > 1e-6, span, 1.0)
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    hue = np.where(high == r, (g - b) / safe % 6.0, np.where(high == g, (b - r) / safe + 2.0, (r - g) / safe + 4.0))
    hue = np.where(span > 1e-6, hue / 6.0, 0.0)
    return hue, np.where(high > 1e-6, span / np.maximum(high, 1e-6), 0.0), high


def from_hsv(hue: np.ndarray, sat: np.ndarray, value: np.ndarray) -> np.ndarray:
    k = (np.array([5.0, 3.0, 1.0], np.float32) + hue[..., None] * 6.0) % 6.0
    return value[..., None] * (1.0 - sat[..., None] * np.clip(np.minimum(k, 4.0 - k), 0.0, 1.0))


def hue_window(hue: np.ndarray, center: float, width: float) -> np.ndarray:
    """1 at `center`, fading to 0 at `width` away (hue wraps around)."""
    distance = np.abs((hue - center + 0.5) % 1.0 - 0.5)
    return np.clip(1.0 - distance / width, 0.0, 1.0)


def pull_hue(hue: np.ndarray, target: float, weight: np.ndarray) -> np.ndarray:
    delta = (target - hue + 0.5) % 1.0 - 0.5
    return (hue + delta * weight) % 1.0


def blockbuster(rgb):
    hue, sat, value = to_hsv(rgb)
    cool = np.maximum(hue_window(hue, 0.5, 0.26), hue_window(hue, 0.33, 0.12))
    hue = pull_hue(hue, 0.5, cool * 0.75)
    hue = pull_hue(hue, 0.07, hue_window(hue, 0.05, 0.16) * 0.6)
    out = from_hsv(hue, np.clip(sat * 1.12, 0, 1), value)
    shade = (1.0 - luma(out))[..., None] ** 2
    out = out + shade * np.array([-0.035, 0.012, 0.045], np.float32)
    return contrast(out, 0.28)


def bleach_bypass(rgb):
    gray = luma(rgb)[..., None]
    mixed = saturate(rgb, 0.42)
    overlay = np.where(gray < 0.5, 2.0 * mixed * gray, 1.0 - 2.0 * (1.0 - mixed) * (1.0 - gray))
    return contrast(mixed * 0.35 + overlay * 0.65, 0.2)


def cross_process(rgb):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    red = r * r * (3.0 - 2.0 * r)
    green = np.clip(g ** 0.82 * 1.04, 0, 1)
    blue = 0.10 + b * 0.78
    out = np.stack([red, green, blue], -1)
    return saturate(out + luma(out)[..., None] ** 2 * np.array([0.05, 0.04, -0.06], np.float32), 1.12)


def faded_film(rgb):
    out = 0.07 + saturate(rgb, 0.82) * 0.86
    tone = luma(out)[..., None]
    out = out + (1.0 - tone) ** 2 * np.array([-0.012, 0.022, 0.012], np.float32)
    return out + tone ** 2 * np.array([0.03, -0.006, 0.014], np.float32)


def moonlight(rgb):
    out = saturate(rgb, 0.45) * np.array([0.52, 0.66, 0.92], np.float32)
    out = contrast(np.clip(out, 0, 1), 0.3)
    return out + (1.0 - luma(out))[..., None] ** 3 * np.array([0.0, 0.012, 0.045], np.float32)


def autumn(rgb):
    hue, sat, value = to_hsv(rgb)
    foliage = hue_window(hue, 0.3, 0.2)
    hue = pull_hue(hue, 0.06, foliage * 0.85)
    sat = np.clip(sat * (1.0 + 0.25 * foliage), 0, 1)
    out = from_hsv(hue, sat, value) * np.array([1.05, 1.0, 0.9], np.float32)
    return contrast(np.clip(out, 0, 1), 0.12)


def infrared(rgb):
    hue, sat, value = to_hsv(rgb)
    foliage = hue_window(hue, 0.3, 0.2) * np.clip(sat * 2.5, 0, 1)
    sky = hue_window(hue, 0.6, 0.14) * np.clip(sat * 2.5, 0, 1)
    out = from_hsv(pull_hue(hue, 0.93, foliage), sat * (1.0 - 0.55 * foliage), value)
    pale = np.array([1.0, 0.86, 0.92], np.float32)
    out = out + (pale - out) * (foliage * 0.6)[..., None]
    return contrast(np.clip(out * (1.0 - 0.38 * sky)[..., None], 0, 1), 0.18)


def two_strip(rgb):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    cyan = g * 0.62 + b * 0.38
    out = np.stack([r * 1.04, cyan * 0.94 + r * 0.06, cyan * 0.9 + r * 0.04], -1)
    return contrast(np.clip(saturate(out, 1.15), 0, 1), 0.2)


RECIPES = {
    "Blockbuster": blockbuster,
    "Bleach Bypass": bleach_bypass,
    "Cross Process": cross_process,
    "Faded Film": faded_film,
    "Moonlight": moonlight,
    "Autumn": autumn,
    "Infrared": infrared,
    "Two Strip": two_strip,
}


def write_cube(path: Path, title: str, recipe) -> None:
    axis = np.linspace(0.0, 1.0, SIZE, dtype=np.float32)
    # .cube order: red changes fastest, then green, then blue.
    blue, green, red = np.meshgrid(axis, axis, axis, indexing="ij")
    grid = np.stack([red, green, blue], -1).reshape(-1, 3)
    out = np.clip(recipe(grid), 0.0, 1.0)
    lines = [f'TITLE "{title}"', f"LUT_3D_SIZE {SIZE}", ""]
    lines += [f"{r:.5f} {g:.5f} {b:.5f}" for r, g, b in out]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> None:
    folder = Path(__file__).parent
    for title, recipe in RECIPES.items():
        write_cube(folder / f"{title}.cube", title, recipe)
        print(f"wrote {title}.cube")


if __name__ == "__main__":
    main()
