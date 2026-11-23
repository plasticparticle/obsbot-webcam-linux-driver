"""Stickers that follow the head: where each sprite sits relative to the face."""

from __future__ import annotations

import math

import numpy as np

import art
from landmarks import Face

STICKERS = ("sunglasses", "heart_glasses", "dog", "cat", "bunny", "moustache",
            "crown", "party_hat", "top_hat", "halo", "horns", "clown_nose")


def _place(frame, face: Face, sprite, anchor, width: float, pivot=(0.5, 0.5),
           right: float = 0.0, down: float = 0.0, tilt: float = 0.0, opacity: float = 1.0) -> None:
    """`width`, `right` and `down` are in head sizes; `tilt` is extra rotation in radians."""
    art.blit(frame, sprite, face.offset(anchor, right, down), width * face.size,
             face.angle + tilt, pivot, opacity)


def draw(frame: np.ndarray, face: Face, name: str, time_s: float) -> None:
    pts = face.pts
    brow_top = pts[10]
    if name == "sunglasses":
        _place(frame, face, art.sunglasses(), face.eye_center, 1.02, pivot=(0.5, 0.5))
    elif name == "heart_glasses":
        _place(frame, face, art.heart_glasses(), face.eye_center, 1.06, pivot=(0.5, 0.5))
    elif name == "dog":
        _place(frame, face, art.dog_ear(), pts[54], 0.46, pivot=(0.8, 0.3), right=-0.02, down=-0.16)
        _place(frame, face, art.dog_ear_mirrored(), pts[284], 0.46, pivot=(0.2, 0.3), right=0.02, down=-0.16)
        opening = face.shape("jawOpen")
        if opening > 0.18:
            grow = min(1.0, (opening - 0.18) / 0.3)
            _place(frame, face, art.dog_tongue(), pts[14], 0.2 + 0.1 * grow, pivot=(0.5, 0.06),
                   opacity=min(1.0, grow * 3.0))
        _place(frame, face, art.dog_nose(), pts[4], 0.3, pivot=(0.5, 0.45))
    elif name == "cat":
        _place(frame, face, art.cat_ear(), pts[54], 0.36, pivot=(0.55, 0.9), right=-0.02, down=-0.06)
        _place(frame, face, art.cat_ear_mirrored(), pts[284], 0.36, pivot=(0.45, 0.9), right=0.02, down=-0.06)
        _place(frame, face, art.cat_face(), pts[4], 1.25, pivot=(0.5, 0.5))
    elif name == "bunny":
        _place(frame, face, art.bunny_ears(), brow_top, 0.8, pivot=(0.5, 0.97), down=-0.06)
        _place(frame, face, art.bunny_nose(), pts[4], 0.26, pivot=(0.5, 0.3))
    elif name == "moustache":
        _place(frame, face, art.moustache(), (pts[2] + pts[0]) / 2.0, 0.6, pivot=(0.5, 0.42))
    elif name == "crown":
        _place(frame, face, art.crown(), brow_top, 0.82, pivot=(0.5, 0.95), down=-0.1)
    elif name == "party_hat":
        _place(frame, face, art.party_hat(), brow_top, 0.6, pivot=(0.5, 0.95), right=0.14, down=-0.06, tilt=0.2)
    elif name == "top_hat":
        _place(frame, face, art.top_hat(), brow_top, 1.05, pivot=(0.5, 0.9), down=-0.04)
    elif name == "halo":
        bob = math.sin(time_s * 2.2) * 0.012
        _place(frame, face, art.halo(), brow_top, 0.82, pivot=(0.5, 0.5), down=-0.42 + bob)
    elif name == "horns":
        _place(frame, face, art.horns(), brow_top, 0.82, pivot=(0.5, 0.95), down=-0.02)
    elif name == "clown_nose":
        _place(frame, face, art.clown_nose(), pts[4], 0.23, pivot=(0.5, 0.5))


def draw_all(frame: np.ndarray, face: Face, names: list[str], time_s: float) -> None:
    for name in STICKERS:
        if name in names:
            draw(frame, face, name, time_s)
