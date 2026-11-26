"""Expression-triggered effects: particles and a rainbow driven by face blendshapes."""

from __future__ import annotations

import math
from dataclasses import dataclass, field

import cv2
import numpy as np

import art
from landmarks import Face

TRIGGERS = ("hearts", "rainbow", "questions", "kisses", "storm", "applause", "idea", "fireworks")
MAX_PARTICLES = 260
# The storm needs brows this far below (or a mouth this far down from) the
# face's usual level, held for CONCERN_HOLD seconds. The rise depends on the
# sensitivity setting: CONCERN_RISE at its lowest, CONCERN_RISE_MIN at its highest.
CONCERN_RISE = 0.32
CONCERN_RISE_MIN = 0.08
CONCERN_HOLD = 0.45
IDEA_SECONDS = 1.6
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
    wobble: float = 0.0
    drag: float = 0.0
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


class Usual:
    """Tracks the level an expression score usually sits at.

    Unlike `Resting`, which hugs the lowest values, this settles in the middle
    of the everyday wobble, so only a clear change stands out. It drops back
    quickly and climbs slowly, so a held expression keeps counting.
    """

    WARMUP = 20

    def __init__(self):
        self.level = 0.0
        self.seen = 0

    def reset(self) -> None:
        self.seen = 0

    def above(self, value: float) -> float:
        self.seen += 1
        if self.seen <= self.WARMUP:
            self.level += (value - self.level) / self.seen
            return 0.0
        rate = 0.008 if value < self.level else 0.0015
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
        self.resting = {"brows": Resting(), "pucker": Resting(), "furrow": Usual(), "frown": Usual()}
        self.concern_held = 0.0
        self.idea_left = 0.0
        self.next_firework = 0.0
        # Colour names from art.MAGIC_COLORS, set by the caller.
        self.question_color = "blue"
        self.star_color = "gold"
        self.next_strike = 0.0
        self.strike_left = 0.0
        self.strike_offset = 0.0

    @property
    def busy(self) -> bool:
        """True while something is still on screen after its trigger ended."""
        return (bool(self.particles) or self.rainbow_length > 0.0 or self.storm > 0.0
                or self.idea_left > 0.0)

    def clear(self) -> None:
        self.particles.clear()
        self.rainbow_length = 0.0
        self.storm = 0.0
        self.idea_left = 0.0

    def _cloud_anchor(self, face: Face, time_s: float) -> np.ndarray:
        return face.offset(face.pts[10], math.sin(time_s * 0.9) * 0.05, -0.62)

    def _emit(self, name: str, rate: float, dt: float) -> int:
        self.budget[name] += rate * dt
        count = int(self.budget[name])
        self.budget[name] -= count
        return count

    def update(self, face: Face | None, enabled: list[str], dt: float, time_s: float = 0.0,
               gestures=None, storm_sensitivity: float = 0.5) -> None:
        rng = self.rng
        self.idea_left = max(0.0, self.idea_left - dt)
        if gestures is not None:
            self._update_gestures(face, enabled, gestures, dt)
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
            frown_rise = self.resting["frown"].above(frown)

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

            brows_up = "questions" in enabled and brows_rise > 0.3
            if brows_up:
                # Question marks rise from the top of the head, jiggling, and fade.
                count = 3 if not self.brows_were_up else self._emit("questions", 4.0, dt)
                for _ in range(count):
                    self.particles.append(Particle(
                        art.question_mark(self.question_color), face.offset(face.pts[10], rng.uniform(-0.45, 0.45), rng.uniform(-0.15, 0.0)),
                        np.array([rng.uniform(-0.08, 0.08), rng.uniform(-0.6, -0.38)]) * size,
                        rng.uniform(0.13, 0.22) * size, rng.uniform(1.4, 2.0),
                        sway=rng.uniform(0.02, 0.05) * size, phase=rng.uniform(0.0, 6.28),
                        angle=rng.uniform(-0.2, 0.2), wobble=rng.uniform(0.2, 0.35),
                        growth=0.03 * size))
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

            # Concerned: brows clearly lower, or the mouth clearly more turned
            # down, than this face usually has them, held for a moment, so a
            # neutral face or a passing twitch stays dry.
            rise = CONCERN_RISE + (CONCERN_RISE_MIN - CONCERN_RISE) * float(np.clip(storm_sensitivity, 0.0, 1.0))
            concerned = "storm" in enabled and smile < 0.3 and (furrow_rise > rise or frown_rise > rise)
            self.concern_held = self.concern_held + dt if concerned else 0.0
            brewing = self.concern_held >= CONCERN_HOLD
            self.storm += (dt / 0.5) if brewing else -(dt / 0.6)
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
            self.concern_held = 0.0
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
            particle.velocity *= max(0.0, 1.0 - particle.drag * dt)
            particle.position = particle.position + particle.velocity * dt
            particle.angle += particle.spin * dt
            x, y = particle.position
            if -200 < x < self.width + 200 and -200 < y < self.height + 200:
                alive.append(particle)
        self.particles = alive

    def _update_gestures(self, face: Face | None, enabled: list[str], gestures, dt: float) -> None:
        rng = self.rng
        size = face.size if face is not None else self.height * 0.3
        if "applause" in enabled and gestures.applauding:
            # Clapping hands rise from the bottom edge, around the person.
            middle = face.center[0] if face is not None else self.width / 2.0
            count = self._emit("applause", 7.0, dt) if gestures.applauding else 0
            for _ in range(count):
                start = np.array([middle + rng.uniform(-2.2, 2.2) * size, self.height + 0.15 * size], np.float32)
                self.particles.append(Particle(
                    art.clapping_hands(), start,
                    np.array([rng.uniform(-0.08, 0.08), rng.uniform(-1.5, -1.0)]) * size,
                    rng.uniform(0.26, 0.4) * size, rng.uniform(1.6, 2.3),
                    sway=rng.uniform(0.04, 0.1) * size, phase=rng.uniform(0.0, 6.28),
                    angle=rng.uniform(-0.25, 0.25), wobble=rng.uniform(0.1, 0.2)))
        if "fireworks" in enabled and gestures.celebrating:
            self.next_firework -= dt
            if self.next_firework <= 0.0:
                self.next_firework = rng.uniform(0.22, 0.5)
                self._firework(face, size)
        else:
            self.next_firework = 0.0
        if "idea" in enabled and gestures.idea:
            self.idea_left = IDEA_SECONDS
            if face is not None:
                origin = face.offset(face.pts[10], 0.0, -0.15)
            else:
                origin = gestures.idea_at
            for _ in range(44):
                direction = rng.uniform(math.pi * 0.95, math.pi * 2.05)
                speed = rng.uniform(1.0, 3.0) * size
                self.particles.append(Particle(
                    art.star(self.star_color) if rng.random() < 0.65 else art.sparkle(), np.array(origin, np.float32),
                    np.array([math.cos(direction), math.sin(direction)]) * speed,
                    rng.uniform(0.08, 0.2) * size, rng.uniform(0.9, 1.7),
                    gravity=2.2 * size, spin=rng.uniform(-5.0, 5.0)))

    def _firework(self, face: Face | None, size: float) -> None:
        """One shell bursting somewhere in the upper part of the picture."""
        rng = self.rng
        middle = face.center[0] if face is not None else self.width / 2.0
        top = face.pts[10][1] if face is not None else self.height * 0.4
        x = float(np.clip(middle + rng.uniform(-2.6, 2.6) * size, self.width * 0.06, self.width * 0.94))
        y = float(np.clip(top + rng.uniform(-0.9, 0.5) * size, self.height * 0.08, self.height * 0.6))
        colors = art.FIREWORK_COLORS
        main = colors[int(rng.integers(len(colors)))]
        second = colors[int(rng.integers(len(colors)))]
        count = 36
        turn = rng.uniform(0.0, 6.28)
        for index in range(count):
            direction = turn + index / count * 2.0 * math.pi
            inner = index % 3 == 0
            speed = (rng.uniform(0.5, 0.7) if inner else rng.uniform(1.15, 1.4)) * size
            self.particles.append(Particle(
                art.spark(second if inner else main), np.array([x, y], np.float32),
                np.array([math.cos(direction), math.sin(direction)]) * speed,
                rng.uniform(0.11, 0.16) * size, rng.uniform(0.9, 1.3),
                gravity=0.9 * size, growth=-0.05 * size, drag=1.6))
        for _ in range(4):
            self.particles.append(Particle(
                art.sparkle(), np.array([x, y], np.float32) + rng.uniform(-0.2, 0.2, 2) * size,
                np.zeros(2, np.float32), rng.uniform(0.1, 0.2) * size, rng.uniform(0.3, 0.6),
                spin=rng.uniform(-3.0, 3.0)))

    def draw(self, frame: np.ndarray, face: Face | None, time_s: float) -> None:
        if face is not None and self.rainbow_length > 0.02:
            self._draw_rainbow(frame, face, time_s)
        for particle in self.particles:
            progress = particle.age / particle.life
            opacity = min(1.0, progress * 6.0) * min(1.0, (1.0 - progress) * 3.0)
            position = particle.position.copy()
            position[0] += math.sin(particle.age * 3.0 + particle.phase) * particle.sway
            angle = particle.angle + math.sin(particle.age * 9.0 + particle.phase) * particle.wobble
            art.blit(frame, particle.sprite, position, particle.size + particle.growth * particle.age,
                     angle, opacity=opacity)
        if face is not None and self.idea_left > 0.0:
            # A light bulb pops up above the head, then fades.
            age = IDEA_SECONDS - self.idea_left
            pop = min(1.0, age / 0.18)
            pop = 1.0 + 0.25 * math.sin(pop * math.pi) if pop < 1.0 else 1.0
            art.blit(frame, art.light_bulb(), face.offset(face.pts[10], 0.0, -0.2),
                     0.42 * face.size * min(1.0, age / 0.12 + 0.3) * pop, face.angle * 0.5,
                     pivot=(0.5, 1.0), opacity=min(1.0, self.idea_left / 0.4))
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
