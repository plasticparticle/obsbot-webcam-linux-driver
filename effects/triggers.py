"""Expression-triggered effects: particles and a rainbow driven by face blendshapes."""

from __future__ import annotations

import math
from dataclasses import dataclass, field

import cv2
import numpy as np

import art
from landmarks import Face

TRIGGERS = ("hearts", "rainbow", "stars", "kisses", "storm")
MAX_PARTICLES = 100
# BGR, top band first.
RAINBOW = [(60, 60, 240), (40, 150, 250), (60, 225, 250), (90, 200, 80), (235, 150, 60), (200, 80, 150)]


@dataclass
class Particle:
    sprite: art.Sprite
    position: np.ndarray
    velocity: np.ndarray
    size: float
    life: float
    gravity: float = 0.0
    growth: float = 0.0
    spin: float = 0.0
    angle: float = 0.0
    sway: float = 0.0
    age: float = 0.0
    phase: float = field(default=0.0)


class Resting:
    """Tracks a face's resting level for one expression score.

    Scores such as "brows down" sit at very different values from person to
    person, so triggers compare against this baseline instead of a fixed
    threshold. It follows drops quickly and rises slowly, so holding an
    expression still counts for a long while.
    """

    WARMUP = 12

    def __init__(self):
        self.level = 0.0
        self.seen = 0

    def reset(self) -> None:
        self.seen = 0

    def above(self, value: float) -> float:
        """How far `value` is above the resting level (0 while warming up)."""
        self.seen += 1
        if self.seen <= self.WARMUP:
            # The first tracked frames are unreliable; just adopt them.
            self.level = value
            return 0.0
        rate = 0.2 if value < self.level else 0.0007
        self.level += (value - self.level) * rate
        return value - self.level


class Triggers:
    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.particles: list[Particle] = []
        self.rng = np.random.default_rng()
        self.budget = {name: 0.0 for name in TRIGGERS}
        self.brows_were_up = False
        self.rainbow_length = 0.0
        self.storm = 0.0
        self.resting = {name: Resting() for name in ("brows", "pucker", "furrow")}
        self.next_strike = 0.0
        self.strike_left = 0.0
        self.strike_offset = 0.0

    @property
    def busy(self) -> bool:
        """True while something is still on screen after its trigger ended."""
        return bool(self.particles) or self.rainbow_length > 0.0 or self.storm > 0.0

    def clear(self) -> None:
        self.particles.clear()
        self.rainbow_length = 0.0
        self.storm = 0.0

    def _cloud_anchor(self, face: Face, time_s: float) -> np.ndarray:
        return face.offset(face.pts[10], math.sin(time_s * 0.9) * 0.05, -0.62)

    def _emit(self, name: str, rate: float, dt: float) -> int:
        self.budget[name] += rate * dt
        count = int(self.budget[name])
        self.budget[name] -= count
        return count

    def update(self, face: Face | None, enabled: list[str], dt: float, time_s: float = 0.0) -> None:
        rng = self.rng
        if face is not None:
            size = face.size
            smile = (face.shape("mouthSmileLeft") + face.shape("mouthSmileRight")) / 2.0
            mouth_open = face.shape("jawOpen")
            brows = max(face.shape("browInnerUp"),
                        (face.shape("browOuterUpLeft") + face.shape("browOuterUpRight")) / 2.0)
            pucker = face.shape("mouthPucker")
            furrow = (face.shape("browDownLeft") + face.shape("browDownRight")) / 2.0
            frown = (face.shape("mouthFrownLeft") + face.shape("mouthFrownRight")) / 2.0
            brows_rise = self.resting["brows"].above(brows)
            pucker_rise = self.resting["pucker"].above(pucker)
            furrow_rise = self.resting["furrow"].above(furrow)

            if "hearts" in enabled and smile > 0.5 and mouth_open < 0.5:
                for _ in range(self._emit("hearts", 7.0, dt)):
                    side = rng.choice((-1.0, 1.0))
                    start = face.offset(face.center, side * rng.uniform(0.45, 0.95), rng.uniform(-0.5, 0.35))
                    self.particles.append(Particle(
                        art.heart_red() if rng.random() < 0.6 else art.heart_pink(), start,
                        np.array([rng.uniform(-0.05, 0.05), rng.uniform(-0.55, -0.3)]) * size,
                        rng.uniform(0.09, 0.19) * size, rng.uniform(1.3, 2.0),
                        sway=rng.uniform(0.04, 0.1) * size, phase=rng.uniform(0.0, 6.28),
                        angle=rng.uniform(-0.3, 0.3)))

            brows_up = "stars" in enabled and brows_rise > 0.3
            if brows_up:
                count = 14 if not self.brows_were_up else self._emit("stars", 6.0, dt)
                for _ in range(count):
                    direction = rng.uniform(math.pi * 1.05, math.pi * 1.95)
                    speed = rng.uniform(0.7, 1.5) * size
                    self.particles.append(Particle(
                        art.star() if rng.random() < 0.7 else art.sparkle(),
                        face.offset(face.center, rng.uniform(-0.4, 0.4), -0.45),
                        np.array([math.cos(direction), math.sin(direction)]) * speed,
                        rng.uniform(0.08, 0.17) * size, rng.uniform(0.9, 1.5),
                        gravity=1.6 * size, spin=rng.uniform(-4.0, 4.0)))
            self.brows_were_up = brows_up

            if "kisses" in enabled and pucker_rise > 0.35 and pucker > 0.5 and mouth_open < 0.3:
                for _ in range(self._emit("kisses", 5.0, dt)):
                    direction = rng.uniform(-math.pi * 0.85, -math.pi * 0.15)
                    self.particles.append(Particle(
                        art.heart_pink(), face.mouth_center.copy(),
                        np.array([math.cos(direction), math.sin(direction)]) * rng.uniform(0.5, 0.9) * size,
                        0.05 * size, rng.uniform(1.0, 1.5), growth=rng.uniform(0.12, 0.22) * size,
                        angle=rng.uniform(-0.4, 0.4)))

            pouring = "rainbow" in enabled and mouth_open > 0.3
            self.rainbow_length += (dt / 0.35) if pouring else -(dt / 0.2)

            # Concerned: furrowed brows or a downturned mouth.
            concerned = "storm" in enabled and (furrow_rise > 0.22 or frown > 0.35)
            self.storm += (dt / 0.5) if concerned else -(dt / 0.6)
            if self.storm > 0.6:
                cloud = self._cloud_anchor(face, time_s)
                for _ in range(self._emit("storm", 26.0, dt)):
                    self.particles.append(Particle(
                        art.raindrop(), face.offset(cloud, rng.uniform(-0.55, 0.55), rng.uniform(0.0, 0.12)),
                        np.array([rng.uniform(-0.05, 0.05), rng.uniform(1.5, 2.0)]) * size,
                        rng.uniform(0.028, 0.042) * size, rng.uniform(0.55, 0.85)))
                self.next_strike -= dt
                if self.next_strike <= 0.0:
                    self.next_strike = rng.uniform(1.2, 2.8)
                    self.strike_left = 0.16
                    self.strike_offset = rng.uniform(-0.4, 0.4)
        else:
            for resting in self.resting.values():
                resting.reset()
            self.brows_were_up = False
            self.rainbow_length -= dt / 0.2
            self.storm -= dt / 0.6
        self.rainbow_length = float(np.clip(self.rainbow_length, 0.0, 1.0))
        self.storm = float(np.clip(self.storm, 0.0, 1.0))
        self.strike_left = max(0.0, self.strike_left - dt)

        alive = []
        for particle in self.particles[-MAX_PARTICLES:]:
            particle.age += dt
            if particle.age >= particle.life:
                continue
            particle.velocity[1] += particle.gravity * dt
            particle.position = particle.position + particle.velocity * dt
            particle.angle += particle.spin * dt
            x, y = particle.position
            if -200 < x < self.width + 200 and -200 < y < self.height + 200:
                alive.append(particle)
        self.particles = alive

    def draw(self, frame: np.ndarray, face: Face | None, time_s: float) -> None:
        if face is not None and self.rainbow_length > 0.02:
            self._draw_rainbow(frame, face, time_s)
        for particle in self.particles:
            progress = particle.age / particle.life
            opacity = min(1.0, progress * 6.0) * min(1.0, (1.0 - progress) * 3.0)
            position = particle.position.copy()
            position[0] += math.sin(particle.age * 3.0 + particle.phase) * particle.sway
            art.blit(frame, particle.sprite, position, particle.size + particle.growth * particle.age,
                     particle.angle, opacity=opacity)
        if face is not None and self.storm > 0.02:
            cloud = self._cloud_anchor(face, time_s)
            if self.strike_left > 0.0:
                # A brief, gentle flash with the bolt.
                cv2.convertScaleAbs(frame, dst=frame, alpha=1.1, beta=8)
                art.blit(frame, art.lightning_bolt(), face.offset(cloud, self.strike_offset, 0.1),
                         0.3 * face.size, face.angle, pivot=(0.5, 0.0))
            art.blit(frame, art.storm_cloud(), cloud, 1.5 * face.size * (0.85 + 0.15 * self.storm),
                     face.angle * 0.5, opacity=min(1.0, self.storm * 1.6))

    def _draw_rainbow(self, frame: np.ndarray, face: Face, time_s: float) -> None:
        pts = face.pts
        mouth_width = float(np.linalg.norm(pts[308] - pts[78]))
        opening = float(np.linalg.norm(pts[14] - pts[13]))
        band = max(2.0, min(mouth_width * 0.8, opening * 3.0) / len(RAINBOW))
        start = face.mouth_center
        reach = (self.height - start[1] + band * 4) * self.rainbow_length
        steps = np.linspace(0.0, 1.0, 28, dtype=np.float32)
        wave = np.sin(steps * 5.0 - time_s * 7.0) * steps * face.size * 0.05
        spread = 1.0 + steps * 0.9
        overlay = frame.copy()
        for index, color in enumerate(RAINBOW):
            lateral = (index - (len(RAINBOW) - 1) / 2.0) * band
            xs = start[0] + wave + lateral * spread + face.down[0] * reach * steps
            ys = start[1] + reach * steps
            path = np.round(np.stack([xs, ys], axis=1)).astype(np.int32)
            for a, b, widen in zip(path[:-1], path[1:], spread[1:]):
                cv2.line(overlay, tuple(int(v) for v in a), tuple(int(v) for v in b), color,
                         max(1, int(round(band * widen + 1.5))), cv2.LINE_AA)
        cv2.addWeighted(overlay, 0.86, frame, 0.14, 0.0, dst=frame)
