#!/usr/bin/env python3
"""Regenerate the bundled virtual backgrounds.

The pictures are abstract and computed here, so they can be redistributed
with the project. Run `python effects/backgrounds/generate.py` to rebuild.
"""

from __future__ import annotations

from pathlib import Path

import cv2
import numpy as np

W, H = 1920, 1080


def gradient(top, bottom, angle: float = 0.0) -> np.ndarray:
    ys, xs = np.indices((H, W), dtype=np.float32)
    t = np.clip((ys / H) * np.cos(angle) + (xs / W) * np.sin(angle), 0.0, 1.0)[..., None]
    return np.array(top, np.float32) * (1.0 - t) + np.array(bottom, np.float32) * t


def glow(image: np.ndarray, center, radius: float, color, strength: float) -> None:
    ys, xs = np.indices((H, W), dtype=np.float32)
    falloff = np.exp(-(((xs - center[0] * W) / (radius * W)) ** 2 + ((ys - center[1] * H) / (radius * W)) ** 2))
    image += falloff[..., None] * np.array(color, np.float32) * strength


def vignette(image: np.ndarray, strength: float) -> None:
    ys, xs = np.indices((H, W), dtype=np.float32)
    radial = ((xs - W / 2) / (W / 2)) ** 2 + ((ys - H / 2) / (H / 2)) ** 2
    image *= (1.0 - strength * np.clip(radial - 0.2, 0.0, 1.5))[..., None]


def finish(image: np.ndarray, seed: int) -> np.ndarray:
    # A little grain hides banding in the smooth gradients.
    noise = np.random.default_rng(seed).normal(0.0, 1.6, (H, W, 1)).astype(np.float32)
    return np.clip(image + noise, 0, 255).astype(np.uint8)


# Colours are BGR.
def soft_studio() -> np.ndarray:
    image = gradient((214, 208, 204), (150, 142, 138))
    glow(image, (0.3, 0.3), 0.45, (30, 30, 30), 1.0)
    vignette(image, 0.22)
    return finish(image, 1)


def warm_wall() -> np.ndarray:
    image = gradient((176, 196, 222), (110, 132, 168), 0.3)
    glow(image, (0.75, 0.25), 0.4, (40, 60, 70), 1.0)
    vignette(image, 0.3)
    return finish(image, 2)


def cool_office() -> np.ndarray:
    image = gradient((196, 184, 166), (120, 104, 84), 0.2)
    # Soft out-of-focus window bands and shelf lines.
    layer = np.zeros((H, W, 3), np.float32)
    for x, width in ((0.12, 0.16), (0.36, 0.16), (0.60, 0.16)):
        cv2.rectangle(layer, (int(x * W), int(0.08 * H)), (int((x + width) * W), int(0.62 * H)), (70, 60, 48), -1)
    for y in (0.72, 0.86):
        cv2.rectangle(layer, (int(0.66 * W), int(y * H)), (W, int((y + 0.015) * H)), (-40, -40, -40), -1)
    image += cv2.GaussianBlur(layer, (0, 0), 46)
    vignette(image, 0.25)
    return finish(image, 3)


def dusk() -> np.ndarray:
    image = gradient((150, 84, 60), (96, 120, 232), 0.0)
    glow(image, (0.5, 0.95), 0.6, (20, 60, 90), 1.0)
    vignette(image, 0.28)
    return finish(image, 4)


def dark_stage() -> np.ndarray:
    image = gradient((34, 26, 24), (16, 12, 12))
    glow(image, (0.5, 0.2), 0.42, (70, 46, 40), 1.0)
    glow(image, (0.12, 0.8), 0.3, (60, 20, 40), 0.7)
    vignette(image, 0.35)
    return finish(image, 5)


def bokeh() -> np.ndarray:
    image = gradient((58, 40, 36), (30, 22, 26))
    rng = np.random.default_rng(6)
    layer = np.zeros((H, W, 3), np.float32)
    palette = [(120, 170, 255), (255, 170, 120), (200, 120, 255), (140, 230, 255)]
    for _ in range(46):
        center = (int(rng.uniform(0, W)), int(rng.uniform(0, H * 0.8)))
        radius = int(rng.uniform(26, 92))
        color = np.array(palette[int(rng.integers(len(palette)))], np.float32) * rng.uniform(0.12, 0.34)
        disc = np.zeros((H, W, 3), np.float32)
        cv2.circle(disc, center, radius, tuple(float(c) for c in color), -1, cv2.LINE_AA)
        layer = np.maximum(layer, disc)
    image += cv2.GaussianBlur(layer, (0, 0), 7)
    vignette(image, 0.3)
    return finish(image, 6)


BACKGROUNDS = {
    "Soft Studio": soft_studio,
    "Warm Wall": warm_wall,
    "Cool Office": cool_office,
    "Dusk": dusk,
    "Dark Stage": dark_stage,
    "Bokeh": bokeh,
}


def main() -> None:
    folder = Path(__file__).parent
    for name, build in BACKGROUNDS.items():
        cv2.imwrite(str(folder / f"{name}.jpg"), build(), [cv2.IMWRITE_JPEG_QUALITY, 88])
        print(f"wrote {name}.jpg")


if __name__ == "__main__":
    main()
