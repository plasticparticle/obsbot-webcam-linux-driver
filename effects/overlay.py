"""Things laid over the finished picture: name badge, logo and hold cards."""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from functools import cache, lru_cache

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ACCENT = (116, 105, 255)  # RGB


@cache
def _font_file(family: str, bold: bool) -> str | None:
    """The font file for a family name; empty means the default sans-serif."""
    try:
        pattern = (family.replace(":", " ").replace("-", "\\-") or "sans-serif") + (":bold" if bold else "")
        found = subprocess.run(["fc-match", "-f", "%{file}", pattern], capture_output=True, text=True, timeout=3)
        return found.stdout.strip() or None
    except (OSError, subprocess.SubprocessError):
        return None


@cache
def _font(size: int, bold: bool, family: str = ""):
    path = _font_file(family, bold)
    try:
        if path:
            return ImageFont.truetype(path, size)
    except OSError:
        pass
    return ImageFont.load_default(size)


def _premultiplied(image: Image.Image) -> np.ndarray:
    """RGBA PIL image -> float BGR premultiplied by alpha, plus alpha, as (h, w, 4)."""
    rgba = np.asarray(image, np.float32)
    alpha = rgba[..., 3:4] / 255.0
    return np.dstack([rgba[..., 2::-1] * alpha, alpha])


def paste(frame: np.ndarray, layer: np.ndarray, x: int, y: int, opacity: float = 1.0) -> None:
    height, width = frame.shape[:2]
    x0, y0 = max(0, x), max(0, y)
    x1, y1 = min(width, x + layer.shape[1]), min(height, y + layer.shape[0])
    if x1 <= x0 or y1 <= y0:
        return
    part = layer[y0 - y:y1 - y, x0 - x:x1 - x]
    roi = frame[y0:y1, x0:x1]
    roi[:] = np.clip(roi * (1.0 - part[..., 3:4] * opacity) + part[..., :3] * opacity, 0, 255).astype(np.uint8)


@dataclass(frozen=True)
class BadgeDesign:
    """How the name badge looks and where it sits."""

    font: str = ""
    bold: bool = True
    caps: bool = False
    size: float = 0.35
    position: str = "bottom-left"
    style: str = "bar"
    layout: str = "stacked"
    accent: tuple[int, int, int] = ACCENT
    opacity: float = 0.8
    margin: float = 0.35


PANEL = (14, 16, 26)


@lru_cache(maxsize=12)
def _badge(name: str, title: str, frame_height: int, design: BadgeDesign) -> np.ndarray:
    scale = frame_height / 720.0 * (0.6 + 1.15 * design.size)
    name_font = _font(max(10, round(36 * scale)), design.bold, design.font)
    title_size = 36 if design.layout == "inline" else 22
    title_font = _font(max(9, round(title_size * scale)), False, design.font)
    pad_x, pad_y, gap = round(22 * scale), round(14 * scale), round(5 * scale)
    bar = round(8 * scale) if design.style == "bar" else 0
    line = max(2, round(5 * scale)) if design.style == "underline" else 0
    dot = round(9 * scale) if design.style == "pill" else 0
    if design.caps:
        name = name.upper()

    probe = ImageDraw.Draw(Image.new("RGBA", (8, 8)))
    name_width = probe.textlength(name or " ", font=name_font)
    name_height = name_font.getbbox("Ag")[3]
    title_width = probe.textlength(title, font=title_font) if title else 0
    title_height = title_font.getbbox("Ag")[3] if title else 0
    inline = design.layout == "inline" and bool(title)
    spacer = round(18 * scale) if inline else 0
    text_width = name_width + spacer + title_width if inline else max(name_width, title_width)
    text_height = max(name_height, title_height) if inline else name_height + (gap + title_height if title else 0)
    lead = bar + (dot * 2 + round(10 * scale) if dot else 0)
    width = round(lead + pad_x * 2 + text_width)
    height = round(pad_y * 2 + text_height + line)

    image = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    alpha = round(255 * design.opacity)
    accent = design.accent
    # Dark text on a light accent, light text otherwise.
    on_accent = (20, 22, 30) if sum(accent) > 520 else (255, 255, 255)
    text, muted = (255, 255, 255, 255), (205, 210, 228, 255)
    radius = round(10 * scale)
    if design.style == "block":
        draw.rounded_rectangle((0, 0, width - 1, height - 1), radius, fill=(*accent, max(alpha, 60)))
        text, muted = (*on_accent, 255), (*on_accent, 215)
    elif design.style == "pill":
        draw.rounded_rectangle((0, 0, width - 1, height - 1), height // 2, fill=(*PANEL, alpha))
        center = (pad_x + dot, height // 2)
        draw.ellipse((center[0] - dot, center[1] - dot, center[0] + dot, center[1] + dot), fill=(*accent, 255))
    elif design.style == "plain":
        # No panel: a soft shadow keeps the text readable on any picture.
        shadow = Image.new("RGBA", (width, height), (0, 0, 0, 0))
        shade = ImageDraw.Draw(shadow)
        shade.text((lead + pad_x, pad_y), name, font=name_font, fill=(0, 0, 0, 230))
        if title:
            spot = (lead + pad_x + name_width + spacer, pad_y + (name_height - title_height)) if inline \
                else (lead + pad_x, pad_y + name_height + gap)
            shade.text(spot, title, font=title_font, fill=(0, 0, 0, 230))
        image = shadow.filter(ImageFilter.GaussianBlur(max(1.5, 3.0 * scale)))
        draw = ImageDraw.Draw(image)
    else:
        draw.rounded_rectangle((0, 0, width - 1, height - 1), radius, fill=(*PANEL, alpha))
        if bar:
            draw.rounded_rectangle((0, 0, bar + radius, height - 1), radius, fill=(*accent, 255))
            draw.rectangle((bar, 0, bar + radius, height - 1), fill=(*PANEL, 255))
        if line:
            draw.rounded_rectangle((radius // 2, height - line - 1, width - 1 - radius // 2, height - 1),
                                   line // 2, fill=(*accent, 255))

    draw.text((lead + pad_x, pad_y), name, font=name_font, fill=text)
    if title and inline:
        x = lead + pad_x + name_width + spacer
        draw.text((x, pad_y + (name_height - title_height)), title, font=title_font, fill=muted)
    elif title:
        draw.text((lead + pad_x, pad_y + name_height + gap), title, font=title_font, fill=muted)
    return _premultiplied(image)


def draw_badge(frame: np.ndarray, name: str, title: str, design: BadgeDesign = BadgeDesign()) -> None:
    if not name.strip() and not title.strip():
        return
    height, width = frame.shape[:2]
    layer = _badge(name.strip()[:60], title.strip()[:80], height, design)
    margin = round(height * (0.015 + 0.14 * design.margin))
    vertical, _, horizontal = design.position.partition("-")
    x = {"left": margin, "center": (width - layer.shape[1]) // 2}.get(horizontal, width - layer.shape[1] - margin)
    y = margin if vertical == "top" else height - layer.shape[0] - margin
    paste(frame, layer, x, y)


@cache
def _logo(path: str, target_height: int) -> np.ndarray | None:
    try:
        image = Image.open(path).convert("RGBA")
    except (OSError, ValueError):
        return None
    scale = target_height / image.height
    width = max(1, round(image.width * scale))
    if width > target_height * 5:
        width, target_height = target_height * 5, max(1, round(image.height * target_height * 5 / image.width))
    return _premultiplied(image.resize((width, target_height), Image.LANCZOS))


def draw_logo(frame: np.ndarray, path: str) -> None:
    height, width = frame.shape[:2]
    layer = _logo(path, max(16, round(height * 0.1)))
    if layer is not None:
        margin = round(height * 0.045)
        paste(frame, layer, width - layer.shape[1] - margin, margin, 0.92)


def hold_card(backdrop: np.ndarray, text: str) -> np.ndarray:
    """A calm "be right back" card built from a blurred, darkened still."""
    height, width = backdrop.shape[:2]
    small = cv2.resize(backdrop, (width // 8, height // 8), interpolation=cv2.INTER_AREA)
    small = cv2.GaussianBlur(small, (0, 0), max(2.0, width / 160.0))
    card = cv2.resize(small, (width, height), interpolation=cv2.INTER_LINEAR)
    card = cv2.addWeighted(card, 0.3, np.full_like(card, (38, 24, 20)), 0.7, 0.0)
    scale = height / 720.0
    image = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    text = (text.strip() or "Be right back")[:48]
    size = round(64 * scale)
    font = _font(size, True)
    while size > 14 and draw.textlength(text, font=font) > width * 0.84:
        size = round(size * 0.9)
        font = _font(size, True)
    box = draw.textbbox((0, 0), text, font=font)
    x = (width - (box[2] - box[0])) / 2 - box[0]
    y = (height - (box[3] - box[1])) / 2 - box[1]
    bar_width, bar_height = round(72 * scale), max(3, round(5 * scale))
    draw.rounded_rectangle(((width - bar_width) / 2, y + box[1] - round(30 * scale),
                            (width + bar_width) / 2, y + box[1] - round(30 * scale) + bar_height),
                           bar_height // 2, fill=(*ACCENT, 255))
    draw.text((x, y), text, font=font, fill=(255, 255, 255, 255))
    paste(card, _premultiplied(image), 0, 0)
    return card
