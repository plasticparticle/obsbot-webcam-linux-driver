"""Hand gestures: clapping and a raised index finger, from hand landmarks."""

from __future__ import annotations

import numpy as np

GESTURE_TRIGGERS = ("applause", "idea", "fireworks")

# Distances between the two palms, in palm lengths. Real clapping is a small
# movement: the hands part by about one palm length, and they often hide each
# other when they meet. So a clap is the palms closing by CLAP_TRAVEL to
# within CLAP_TOGETHER (or one hand vanishing while they were close), and the
# next one counts once they have opened again by CLAP_REOPEN.
CLAP_TOGETHER = 1.3
CLAP_TRAVEL = 0.3
CLAP_REOPEN = 0.25
CLAP_NEAR = 2.0
# Close to the camera, clapping hands overlap and are tracked as ONE hand that
# shakes: it drifts one way and snaps back with every clap. A snap is a change
# of direction after travelling between BEAT_MIN and BEAT_MAX palm lengths,
# within BEAT_SECONDS of the previous one (slower is waving, larger is moving).
BEAT_MIN = 0.22
BEAT_MAX = 2.5
BEAT_SECONDS = 0.45
# Claps closer together than this are the same clap seen twice.
CLAP_REPEAT = 0.12
# This many claps, each within CLAP_GAP of the last, count as applause, which then lasts this long
# after the most recent clap.
CLAP_STREAK = 3
CLAP_GAP = 0.8
APPLAUSE_TAIL = 0.9
# How long the finger has to be up, and down again before it counts anew.
POINT_HOLD = 0.25
POINT_RELEASE = 0.4
# Shaking a fist: this many shakes, each within FIST_GAP of the last, start
# the party, which goes on for FIST_TAIL after the last shake. A fist that is
# merely held up (or a hand at your face) does nothing.
FIST_SHAKES = 2
FIST_GAP = 0.7
FIST_TAIL = 1.2


class Hand:
    """Pixel-space landmarks of one hand (21 points, wrist first)."""

    def __init__(self, pts: np.ndarray):
        self.pts = pts
        self.palm = pts[[0, 5, 9, 13, 17]].mean(axis=0)
        self.size = max(float(np.linalg.norm(pts[9] - pts[0])), 1.0)

    def _reach(self, index: int) -> float:
        return float(np.linalg.norm(self.pts[index] - self.pts[0]))

    @property
    def pointing_up(self) -> bool:
        """Index finger straight and upright, the other three fingers curled."""
        pts = self.pts
        finger = pts[8] - pts[5]
        length = float(np.linalg.norm(finger))
        if length < 0.7 * self.size or self._reach(8) < self._reach(6) * 1.2:
            return False
        if finger[1] > -0.75 * length or pts[8][1] > pts[0][1]:
            return False
        return all(self._reach(tip) < self._reach(joint) * 1.15
                   for tip, joint in ((12, 10), (16, 14), (20, 18)))

    @property
    def fist(self) -> bool:
        """All four fingers curled into the palm."""
        return all(self._reach(tip) < self._reach(joint) * 1.15
                   for tip, joint in ((8, 6), (12, 10), (16, 14), (20, 18)))


class Shake:
    """Counts back-and-forth beats of one coordinate (in palm lengths)."""

    def __init__(self):
        self.direction = 0
        self.extreme = 0.0
        self.turned_at = 0.0
        self.since_turn = 10.0

    def beat(self, value: float, dt: float) -> bool:
        """Feed the next position; True when a clap-sized stroke just ended."""
        self.since_turn += dt
        if self.direction == 0:
            self.direction, self.extreme, self.turned_at = 1, value, value
            return False
        ahead = (value - self.extreme) * self.direction
        if ahead >= 0.0:
            self.extreme = value
            return False
        if -ahead < BEAT_MIN:
            return False
        stroke = abs(self.extreme - self.turned_at)
        quick = self.since_turn < BEAT_SECONDS
        self.turned_at = self.extreme
        self.direction = -self.direction
        self.extreme = value
        self.since_turn = 0.0
        # One clap per full cycle: count the turns in one direction only.
        return quick and self.direction < 0 and BEAT_MIN <= stroke <= BEAT_MAX


class Gestures:
    """Turns the hands seen in each frame into clap and "idea" events."""

    def __init__(self):
        self.clap = False
        self.applauding = False
        self.idea = False
        self.idea_at: np.ndarray | None = None
        self.center: np.ndarray | None = None
        self.celebrating = False
        self._fist_level = 0.0
        self._fist_streak = 0
        self._since_fist = 10.0
        self._armed_clap = True
        self._shake = (Shake(), Shake())
        self._since_hand = 10.0
        self._high = 0.0
        self._low = 0.0
        self._gap = 10.0
        self._since_pair = 10.0
        self._since_clap = 10.0
        self._streak = 0
        self._pointing = 0.0
        self._released = POINT_RELEASE
        self._armed = True

    def reset(self) -> None:
        self.__init__()

    def update(self, hands: list[Hand], dt: float) -> None:
        self._update_clap(hands, dt)
        self._update_point(hands, dt)

    def _update_clap(self, hands: list[Hand], dt: float) -> None:
        self.clap = False
        self._since_clap += dt
        self._since_pair += dt
        clapped = False
        shaken = False
        self._since_fist += dt
        if len(hands) >= 2:
            first, second = hands[0], hands[1]
            gap = float(np.linalg.norm(first.palm - second.palm)) / ((first.size + second.size) / 2.0)
            if self._since_pair > 0.5:
                # A fresh pair of hands: start measuring from here.
                self._armed_clap, self._high = True, gap
            self._gap = gap
            self._since_pair = 0.0
            self.center = (first.palm + second.palm) / 2.0
            if self._armed_clap:
                self._high = max(self._high, gap)
                clapped = gap < CLAP_TOGETHER and self._high - gap > CLAP_TRAVEL
                if clapped:
                    self._low = gap
            else:
                self._low = min(self._low, gap)
                if gap - self._low > CLAP_REOPEN:
                    self._armed_clap, self._high = True, gap
        elif self._armed_clap and self._since_pair < 0.2 and self._gap < CLAP_NEAR:
            # Palms that meet hide each other, so the tracker drops a hand
            # right at the moment of the clap.
            clapped = True
            self._low = min(self._gap, CLAP_TOGETHER) - CLAP_REOPEN
        if clapped:
            self._armed_clap = False
        # The single shaking "hand" that clapping close to the camera shows.
        if hands:
            lead = max(hands, key=lambda hand: hand.size)
            if self._since_hand > 0.25:
                self._shake = (Shake(), Shake())
            step = dt + (self._since_hand if self._since_hand < 0.25 else 0.0)
            self._since_hand = 0.0
            beat = False
            for axis, shake in enumerate(self._shake):
                beat = shake.beat(float(lead.palm[axis]) / lead.size, step) or beat
            # The same shaking is a clap with open hands and a cheer with a fist.
            self._fist_level += (float(lead.fist) - self._fist_level) * 0.25
            if beat and self._fist_level > 0.6:
                shaken = True
            elif beat and self._fist_level < 0.4:
                clapped = True
        else:
            self._since_hand += dt
        if shaken:
            self._fist_streak = self._fist_streak + 1 if self._since_fist < FIST_GAP else 1
            self._since_fist = 0.0
        if self._since_fist > FIST_GAP and self._fist_streak < FIST_SHAKES:
            self._fist_streak = 0
        if self._since_fist > FIST_TAIL:
            self._fist_streak = 0
        self.celebrating = self._fist_streak >= FIST_SHAKES and self._since_fist < FIST_TAIL
        if clapped and self._since_clap < CLAP_REPEAT:
            clapped = False
        if clapped:
            self.clap = True
            self._streak = self._streak + 1 if self._since_clap < CLAP_GAP else 1
            self._since_clap = 0.0
        if self._since_clap > CLAP_GAP:
            self._streak = 0
        self.applauding = self._streak >= CLAP_STREAK and self._since_clap < APPLAUSE_TAIL

    def _update_point(self, hands: list[Hand], dt: float) -> None:
        self.idea = False
        pointing = next((hand for hand in hands if hand.pointing_up), None)
        if pointing is None:
            self._pointing = 0.0
            self._released += dt
            if self._released >= POINT_RELEASE:
                self._armed = True
            return
        self._released = 0.0
        self._pointing += dt
        if self._armed and self._pointing >= POINT_HOLD:
            self._armed = False
            self.idea = True
            self.idea_at = pointing.pts[8].copy()
