"""Filming the game for short vertical videos, after Vagrancy's
analysis/video/make_short.py: the real page, as a phone shows it, stepped a
frame at a time on its own clock, so a take is exact and repeatable whatever
the machine.

The page runs with `#film` (web/app.js): its loop stops and `window.film`
takes over. `stage` sets up a scene through the core (`Game::stage`), `step`
plays one frame with the actions given and returns the HUD and the core's
`probe` (where the car and the pursuer are). The touch controls light up for
the actions pressed, so the film shows a phone being played.

A `Take` keeps each frame as a PNG with what happened on it; `edit.py` cuts
takes together into a short.
"""
import functools, http.server, json, math, threading
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
FPS = 30
FRAME_MS = 1000 / FPS
# A phone held upright, drawn at four pixels a CSS pixel, so a frame can be
# cropped into and still fill 1080 x 1920.
VIEW = {"width": 360, "height": 640}
SCALE = 4


class _Quiet(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

    def log_message(self, *a):
        pass


class Studio:
    """The served build and one browser page in film mode."""

    def __init__(self):
        self.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(_Quiet, directory=str(ROOT / "dist")))
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()
        self.pw = sync_playwright().start()
        self.browser = self.pw.chromium.launch(args=["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist"])
        self.ctx = self.browser.new_context(viewport=VIEW, device_scale_factor=SCALE, has_touch=True, is_mobile=True)
        self.page = self.ctx.new_page()
        self.errors = []
        self.page.on("pageerror", lambda e: self.errors.append(str(e)))
        self.page.on("console", lambda m: m.type == "error" and self.errors.append(m.text))
        self.page.goto(f"http://127.0.0.1:{self.srv.server_address[1]}/index.html#film")
        self.page.wait_for_selector("body[data-ready='true']", state="attached", timeout=60000)
        self.actions = self.page.evaluate("window.film.actions")

    def bits(self, *names):
        b = 0
        for n in names:
            b |= self.actions[n]
        return b

    def close(self):
        self.browser.close()
        self.pw.stop()
        self.srv.shutdown()


class Take:
    """A run of frames from one staged scene. Each frame keeps its picture,
    the HUD's sounds, the probe, and the edit's notes for it."""

    def __init__(self, studio, name, out, spec=None):
        self.s = studio
        self.name = name
        self.dir = Path(out) / name
        self.dir.mkdir(parents=True, exist_ok=True)
        for f in list(self.dir.glob("*.png")) + list(self.dir.glob("*.jpg")):
            f.unlink()
        self.frames = []
        if spec is not None:
            self.s.page.evaluate("s => window.film.stage(s)", spec)
        self.state = self.s.page.evaluate("window.film.probe()")
        self.hud = {}
        # sounds from frames run unseen, kept for the next frame shot
        self.heard = []

    def step(self, bits=0, dx=0, dy=0, speed=1.0, shoot=True, **notes):
        """One frame of film: `speed` is game time per film time (0.25 is
        quarter-speed slow motion). Unshot frames run the game on unseen."""
        r = self.s.page.evaluate("([b, x, y, ms]) => window.film.step(b, x, y, ms)", [bits, dx, dy, FRAME_MS * speed])
        self.hud, self.state = r["hud"], r["probe"]
        self.heard += self.hud["sounds"]
        if shoot:
            path = self.dir / f"{len(self.frames):05d}.jpg"
            self.s.page.screenshot(path=str(path), type="jpeg", quality=93)
            # the thumbs: the centre of each control pressed, in CSS pixels
            thumbs = [[x + w / 2, y + h / 2] for x, y, w, h in r["touches"]]
            self.frames.append({"image": str(path), "sounds": self.heard, "probe": self.state, "thumbs": thumbs, **notes})
            self.heard = []
        return self.state

    def note(self, **notes):
        """Adds notes to the last frame shot (a caption starting, a hit)."""
        self.frames[-1].update(notes)

    def save(self):
        (self.dir / "take.json").write_text(json.dumps(self.frames))


# ---- a driver: what a player's thumbs would do ----

def heading_to(p, x, z):
    """The heading (degrees, the core's: 0 faces −z, positive turns left)
    from the car to a point."""
    dx, dz = x - p["car"][0], z - p["car"][2]
    return math.degrees(math.atan2(-dx, -dz))


def turn_by(want, have):
    return (want - have + 180) % 360 - 180


def steer(studio, p, x, z, slack=4, throttle="forward"):
    """The actions that drive toward (x, z): the throttle, and left or right
    when the heading is more than `slack` degrees off."""
    names = [throttle] if throttle else []
    off = turn_by(heading_to(p, x, z), p["heading"])
    if off > slack:
        names.append("left")
    elif off < -slack:
        names.append("right")
    return studio.bits(*names)


def near(p, x, z, r):
    return math.hypot(p["car"][0] - x, p["car"][2] - z) < r


RAYS = [-40, -25, -12, 0, 12, 25, 40]  # the probe's `ahead`, degrees to the right


def weave(studio, p, goal, slack=4):
    """The actions that weave toward heading `goal` (degrees) through what is
    in the way: of the probe's rays, the one with the most clear road, less
    a little for each degree it is off the goal; backing out when nothing
    ahead is clear."""
    best, pick = None, 0
    for a, clear in zip(RAYS, p["ahead"]):
        score = clear - 0.35 * abs(turn_by(goal, p["heading"] - a))
        if best is None or score > best:
            best, pick = score, a
    if max(p["ahead"]) < 8:
        return studio.bits("back", "left")
    off = -pick if abs(pick) > 0 else turn_by(goal, p["heading"])
    names = ["forward"] if p["ahead"][3] > 12 or p["speed"] < 18 else []
    if off > slack:
        names.append("left")
    elif off < -slack:
        names.append("right")
    return studio.bits(*names)
