#!/usr/bin/env python3
"""The Pursuer shorts: Kanye West chasing a player who is playing on a phone.

    python tools/film/shorts.py NAME [--dry] [--out DIR]
    NAME: mirror, ruins or whole_run

- mirror: Kanye fills the rear-view mirror while the player dashes, turns on
  the spot and flies away.
- ruins: Kanye has torn down most of the town; the player weaves through what
  is left until he jumps out in front and runs them down.
- whole_run: from the start: the record picked up, Kanye standing up, the car
  reached at the last moment, a crash, and the catch.

Each films the page (film.py) and cuts it (edit.py) to ~/Movies/Pursuer/.
`--dry` runs the scene unfilmed and prints where everything went, to set a
script's timings quickly. The build must be current (scripts/build.sh).
"""
import argparse, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from film import FPS, Studio, Take, heading_to, near, steer, turn_by, weave  # noqa: E402

SITE = "sgilson7.github.io/pursuer"
MOBILE = "Plays in your phone's browser"
TAG = ("PLAYED ON A PHONE", "tag")
# the rear-view mirror, held upright: the core lays it out across the top,
# 64% of the width and 11% of the height, 2.5% down (CSS pixels of 360 x 640)
MIRROR_X = (65, 295)
MIRROR_Y = (18, 84)
INSET_W = 144


def mirror_box(p):
    """The part of the rear-view mirror to blow up: centred on Kanye's face
    where the core says it shows, kept inside the mirror."""
    face = p.get("mirror_face")
    cx = face[0] * 360 / 1000 if face else 180
    cx = max(MIRROR_X[0] + INSET_W / 2, min(MIRROR_X[1] - INSET_W / 2, cx))
    return (cx - INSET_W / 2, MIRROR_Y[0], cx + INSET_W / 2, MIRROR_Y[1])


class Scene:
    def __init__(self, studio, name, out, dry):
        self.s, self.name, self.out, self.dry = studio, name, out, dry
        self.frames = []
        self.take = None

    def stage(self, spec, label):
        if self.take:
            self.frames += self.take.frames
        self.take = Take(self.s, f"{self.name}-{label}", self.out, spec)
        return self.take.state

    def frame(self, bits=0, dx=0, dy=0, speed=1.0, shoot=True, **notes):
        p = self.take.step(bits, dx, dy, speed, shoot and not self.dry, **notes)
        if self.dry and len(self.take.frames) == 0 and shoot:
            self.ticks = getattr(self, "ticks", 0) + 1
        return p

    def done(self):
        if self.take:
            self.frames += self.take.frames
        return self.frames


def log(p, *extra):
    print(f"  car {p['car']} hdg {p['heading']} {p['speed']} m/s up {p['up']}  kanye {p['monster_distance']} m  {p['mode']}", *extra, flush=True)


# ---- 1: the mirror ----

def mirror(sc):
    s = sc.s
    p = sc.stage({"item": "vinyl", "seconds": 150, "abilities": ["dash", "spin", "wings", "turbo"],
                  "car": [50, 430, 0, 44], "behind": 72, "jump_in": 300, "seed": 5}, "chase")
    fwd = lambda: steer(s, sc.take.state, 50, -480)  # noqa: E731
    hook = [("Kanye is right behind me", "hook"), TAG]
    # 0-2 s: the hook, in the mirror
    for k in range(54):
        close = k < 36
        p = sc.frame(fwd(), inset=mirror_box(sc.take.state) if close else None,
                     captions=[("Kanye is right behind me", "hook"), TAG] + ([("in my rear-view mirror", "under")] if close and k >= 12 else []),
                     hit=True if k == 36 else None)
    log(p, "after hook")
    # 2-3.5 s: he gains, to the corner at z = 280
    for _ in range(120):
        if sc.take.state["car"][2] <= 285:
            break
        p = sc.frame(fwd(), captions=[("he's gaining", "beat"), TAG])
    log(p, "at the corner")
    # a quarter turn right, on the spot, onto the street
    p = sc.frame(s.bits("forward", "right", "spin"), speed=0.35, captions=[("INSTANT TURN", "beat"), TAG], hit=True, whoosh=True, shake=0.6)
    east = lambda: steer(s, sc.take.state, 480, 280)  # noqa: E731
    for k in range(26):
        p = sc.frame(east(), speed=0.35, captions=[("INSTANT TURN", "beat"), TAG], zoom=(180, 320, 1.2))
    log(p, "turned")
    for k in range(24):
        p = sc.frame(east(), captions=[("he cuts the corner", "beat"), TAG])
    # the dash
    p = sc.frame(east() | s.bits("dash"), captions=[("DASH", "beat"), TAG], hit=True, whoosh=True, zoom=(180, 330, 1.15))
    for k in range(30):
        p = sc.frame(east(), captions=[("DASH", "beat"), TAG], zoom=(180, 330, 1.15 - 0.15 * k / 30))
    log(p, "dashed")
    for k in range(30):
        p = sc.frame(east(), inset=mirror_box(sc.take.state) if k < 24 else None, captions=[("still there", "hook"), TAG] if k < 24 else [TAG])
    log(p, "east")
    # and away into the air
    p = sc.frame(s.bits("fly", "back"), speed=0.5, captions=[("AND FLY", "beat"), TAG], hit=True, whoosh=True, flash=0.5)
    for k in range(40):
        p = sc.frame(s.bits("back") if k < 16 else 0, speed=0.5, captions=[("AND FLY", "beat"), TAG])
        if sc.dry and k % 8 == 0:
            log(p, "fly")
    log(p, "flying")
    for k in range(30):
        p = sc.frame(0, inset=mirror_box(sc.take.state), captions=[("see you, Ye", "hook"), TAG])
    for k in range(66):
        p = sc.frame(0, dim=min(0.55, k / 12), captions=[(MOBILE, "end"), ("free · no download · " + SITE, "small")])
        if sc.dry and k % 10 == 0:
            log(p, "end card")
    log(p, "end")
    return sc.done()


# ---- 2: the ruins ----

def ravaged_town():
    """Kanye's trail through most of the town: through the middle of every
    block both ways, stopping short of the overpass (z = 120) on either side
    and walking away from it, so what he throws forward lands away from it
    and the deck stands over the ruins."""
    paths = []
    for z in (360, 240, 40, -80, -240, -400):
        paths.append([-480, z, 480, z] if z % 160 else [480, z, -480, z])
    for x in (-400, -300, -200, -100, 0, 100, 200, 300, 400):
        paths += [[x, 98, x, -480], [x, 142, x, 480]]
    return paths


def ruins(sc):
    s = sc.s
    # east of the overpass on its street, heading west for the ramp; he is
    # far off, and jumps out in front as the car crosses the deck
    p = sc.stage({"item": "vinyl", "seconds": 140, "abilities": ["spin", "dash", "turbo"], "car": [478, 120, 90, 26],
                  "monster": [-420, 450], "jump_in": 6.5, "ravage": ravaged_town(), "settle": 8, "seed": 9}, "drive")
    log(p, "staged", sc.take.state["intact"])
    west = lambda: steer(s, sc.take.state, sc.take.state["car"][0] - 60, 120, slack=3)  # noqa: E731
    # up the ramp with the ruins on both sides: a look left across them, and back
    last = p["monster_distance"]
    for k in range(600):
        p = sc.take.state
        # a jump: from far off to just ahead in one frame
        if p["mode"] != "chase" or last - p["monster_distance"] > 100:
            break
        last = p["monster_distance"]
        dx = -40 if k < 18 else 40 if k < 36 else 0
        cap = [("Kanye destroyed the whole town", "hook"), TAG] if k < 80 else [("I just have to get out", "beat"), TAG]
        # the long run along the deck, quicker: every other frame
        p = sc.frame(west(), dx=dx, shoot=k < 90 or k % 2 == 0, captions=cap)
        if sc.dry and k % 15 == 0:
            log(p, "deck")
    log(p, "he jumped", p["monster"])
    # he stands up through the overpass ahead: full speed while he rises, then slow
    jump = [("HE'S IN FRONT OF ME", "low"), TAG]
    for k in range(36):
        p = sc.frame(west(), speed=1 if k < 10 else 0.3, captions=jump, flash=max(0, 0.9 - k / 8), shake=max(0, 1 - k / 20),
                     hit=True if k == 0 else None, zoom=(180, 300, 1.25 - 0.25 * k / 36))
    log(p, "saw him", p["monster"])
    # brake hard, turn round on the spot, and floor it back east
    for k in range(10):
        p = sc.frame(s.bits("back", "shift"), captions=[("TURN AROUND", "beat"), TAG])
    p = sc.frame(s.bits("spin"), captions=[("TURN AROUND", "beat"), TAG], hit=True, whoosh=True, shake=0.6)
    log(p, "turned")
    for k in range(300):
        p = sc.take.state
        if p["mode"] != "chase":
            break
        close = (p["monster_distance"] or 999) < 70
        p = sc.frame(steer(s, p, p["car"][0] + 60, 120, slack=3), inset=mirror_box(p) if close else None,
                     captions=[("TOO SLOW", "hook"), TAG] if close else [("GO GO GO", "beat"), TAG])
        if sc.dry and k % 10 == 0:
            log(p, "flee")
    log(p, "caught?")
    for k in range(50):
        p = sc.frame(0, hit=True if k == 0 else None, flash=0.8 if k == 0 else 0, shake=max(0, 0.8 - k / 10))
    for k in range(66):
        p = sc.frame(0, dim=min(0.75, k / 10), captions=[(MOBILE, "end"), ("free · no download · " + SITE, "small")])
    return sc.done()


# ---- 3: the whole run ----

def walker_heading_to(p, x, z):
    wx, wz = p["walker"][0], p["walker"][1]
    import math
    return math.degrees(math.atan2(-(x - wx), -(z - wz)))


def look_toward(p, heading, most=40):
    """Mouse movement (CSS pixels) that turns the walker toward `heading`:
    13 pixels a degree, a positive move turning right."""
    off = turn_by(heading, p["walker"][2])
    return int(max(-most, min(most, -off * 13 / 3)))


def whole_run(sc):
    s = sc.s
    # in the house, at the record player, the record in reach
    p = sc.stage({"walker": [-0.2, -3.7, 0, -30], "seed": 21}, "run")
    log(p, "staged", p["aim"])
    hold = s.bits("use")
    # 1: grab Kanye's record and hold on (5 s, filmed at double speed)
    for k in range(160):
        if sc.take.state["mode"] == "chase":
            break
        cap = [("I picked up Kanye's record", "hook"), TAG] if k < 70 else [("and held it too long", "hook"), TAG]
        p = sc.frame(hold, speed=2 if 10 < k < 150 else 1, shoot=k < 12 or k % 2 == 0, captions=cap)
        if sc.dry and k % 20 == 0:
            log(p, "holding", p["held"], p["walker"])
    log(p, "folded", p["walker"])
    # 2: the house folds; turn to the west to watch him stand up
    for k in range(50):
        p = sc.take.state
        dx = look_toward(p, 90, most=60)
        p = sc.frame(0, dx=dx, captions=[("oh no", "beat"), TAG], hit=True if k == 0 else None, shake=max(0, 0.7 - k / 30))
    log(p, "looking west", p["walker"], p["monster_distance"])
    # he walks in: frozen, watching (fast-forward: two frames of three unseen)
    for k in range(1200):
        p = sc.take.state
        if (p["monster_distance"] or 999) < 55:
            break
        p = sc.frame(0, shoot=k % 6 == 0, captions=[("he's coming", "beat"), TAG])
    log(p, "he's close", p["monster_distance"])
    # 3: run for the car in the garage
    car = (11, 0)
    for k in range(200):
        p = sc.take.state
        if p["driving"] or p["mode"] != "chase":
            break
        dist = ((p["walker"][0] - car[0]) ** 2 + (p["walker"][1] - car[1]) ** 2) ** 0.5
        want = walker_heading_to(p, *car)
        facing = abs(turn_by(want, p["walker"][2])) < 35
        bits = (s.bits("forward", "shift") if facing else 0) if dist > 4.6 else s.bits("use")
        if k % 2 and dist <= 4.6:
            bits = 0
        p = sc.frame(bits, dx=look_toward(p, want, most=90), captions=[("RUN", "beat"), TAG])
        if sc.dry and k % 10 == 0:
            log(p, "running", p["walker"], dist)
    log(p, "in the car?", p["driving"], p["monster_distance"])
    for k in range(12):
        p = sc.frame(0, captions=[("just made it", "beat"), TAG], hit=True if k == 0 else None)
    # 4: out of the garage and north up the avenue, flat out, into the
    # debris he has thrown ahead (at z = -184 in this take): slow motion for
    # the crash
    fastest = 0
    for k in range(500):
        p = sc.take.state
        fastest = max(fastest, p["speed"])
        if fastest > 25 and p["speed"] < 2 and p["up"] < 70:
            break
        if p["car"][0] < 30 and abs(turn_by(0, p["heading"])) > 45:
            bits = steer(s, p, 50, p["car"][2])
        else:
            lined_up = abs(turn_by(0, p["heading"])) < 25 or p["speed"] < 12
            bits = steer(s, p, 50, -480, throttle="forward" if lined_up else None)
        crash = p["car"][2] < -172
        hit = crash and fastest > 25 and p["speed"] < fastest - 15 and not getattr(sc, "hit_once", False)
        if hit:
            sc.hit_once = True
        p = sc.frame(bits, speed=0.3 if crash else 1, shoot=crash or k < 20 or k % 2 == 0,
                     captions=[("NO NO NO", "beat"), TAG] if crash else ([("floor it", "beat"), TAG] if k > 20 else [TAG]),
                     hit=True if hit else None, shake=0.9 if crash and p["speed"] < 20 else 0, flash=0.5 if hit else 0)
        if sc.dry and k % 15 == 0:
            log(p, "drive")
    log(p, "crashed")
    # upside down: hold Flip while he comes (time compressed)
    for k in range(600):
        p = sc.take.state
        if p["mode"] != "chase":
            break
        close = (p["monster_distance"] or 999) < 90
        # stunned until he is near, then the Flip button, too late
        bits = s.bits("flip") if (p["monster_distance"] or 999) < 45 else 0
        p = sc.frame(bits, shoot=close or k % 4 == 0, shake=0.5 if close else 0,
                     captions=[("upside down", "beat"), TAG] if not close else [("FLIP FLIP FLIP", "beat"), TAG])
        if sc.dry and k % 20 == 0:
            log(p, "flipping")
    log(p, "end")
    for k in range(40):
        p = sc.frame(0, hit=True if k == 0 else None, flash=0.8 if k == 0 else 0, shake=max(0, 0.8 - k / 10))
    for k in range(66):
        p = sc.frame(0, dim=min(0.75, k / 10), captions=[(MOBILE, "end"), ("free · no download · " + SITE, "small")])
    return sc.done()


SCENES = {"mirror": mirror, "ruins": ruins, "whole_run": whole_run}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("name", choices=sorted(SCENES))
    ap.add_argument("--dry", action="store_true")
    ap.add_argument("--out", default=str(Path.home() / "Movies" / "Pursuer"))
    a = ap.parse_args()
    out = Path(a.out)
    studio = Studio()
    try:
        frames = SCENES[a.name](Scene(studio, a.name, out / "takes", a.dry))
    finally:
        studio.close()
    if studio.errors:
        print("page errors:", studio.errors[:5])
    if a.dry:
        return
    import edit
    edit.render(frames, out / f"pursuer-{a.name}.mp4")


if __name__ == "__main__":
    main()
