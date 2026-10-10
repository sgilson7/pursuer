#!/usr/bin/env python3
"""Pursuer's 2D art, drawn as TikZ and compiled, not generated as pixels
(Sam's diagrams-as-code method, as LongTimeComin's tools/art.py and Food
Truck Circus's postcards are made).

Each picture is composed from sub-pictures defined once below (a building
with its windows and rooftop clutter, a cloud, the car, the paper house,
each monster and each item) and placed by the picture's own function. The
look follows Sam_Comm_final.png in the vagrancy repository: a warm yellow
sky with outlined clouds, salmon, coral and pink buildings, sage and teal
greens, flat fills, everything outlined in one plum ink, rooftop clutter
(air-conditioning boxes, antennas, tanks, billboards) and a giant monster
in the street with debris flying.

    python3 tools/art.py            # every picture: web/art/**.png, tools/art/**.tex
    python3 tools/art.py title      # one picture
    python3 tools/art.py monsters   # one folder

Facts each picture must get right (the list of what an image generator gets
wrong), and where this file enforces each:

- text: no words or letters in any picture; the page sets every word over
  the art. `document` refuses any TeX that could typeset text (\\node,
  \\text, \\put, \\char ...), so no picture can carry a letter.
- sizes: title and caught are 1920x1080 and opaque; each monster is
  1024x1536, each item 256x256, both with a transparent background (`SIZE`;
  `check` reads every PNG's header and refuses a wrong size, a missing alpha
  channel on a cutout, or a file over 1.5 MB).
- the standee: a monster is a flat, still cut-out photo, so every monster is
  drawn through `standee`, which lays a white cardboard border (24 px at
  full size) and a thin cut edge round the whole silhouette before the
  character; in the scenes the same `standee` also draws the cardboard
  strut behind it and the ground plate at its feet.
- framing: each monster stands full-body facing the viewer with its feet on
  the bottom edge and nothing cut off: `check` renders a small preview and
  refuses one whose top, left or right edge is touched, whose bottom edge is
  not touched, or which fills less than 90% of the height. Items must clear
  all four edges and be centred.
- the cast: one original character per item (`MONSTER_OF`): the guitar's
  Headliner, the trophy's Champion, the teddy's Mr. Buttons, the toaster's
  Morning Host, the microphone's Crooner, the crown's Queen. No real person
  and no known character. Each has staring eyes and a fixed, too-wide grin
  (`stare`, `grin`, or the bear's stitched grin), and nothing gory.
- the item matches its monster: the guitar, the ribbon microphone, the
  crown and the toast are each drawn by one function used both by the item
  icon and by the monster who carries it (`guitar`, `ribbon_mic`, `crown`,
  `toast`); the teddy icon is Mr. Buttons himself; the trophy and the belt
  share the star (`star`).
- calm left third: the title text sits there, so `debris` refuses to throw
  a piece into the left third of title and caught.
- colour is never the only cue: every filled shape has the plum outline.
- repeatable: every random choice comes from a seeded generator, so the
  same script draws the same pictures.
"""
import math, random, shutil, struct, subprocess, sys, tempfile, zlib
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "web" / "art"
SRC = ROOT / "tools" / "art"
PDFLATEX = shutil.which("pdflatex") or "/Library/TeX/texbin/pdflatex"
PDFTOCAIRO = shutil.which("pdftocairo") or "/opt/homebrew/bin/pdftocairo"

# Sam_Comm_final's palette, by eye from the picture, plus the cast's own.
COLOURS = {
    "ink": "3B2340", "sky": "EFC46A", "skyhi": "F5DA92", "skylow": "E9B060", "cloud": "F2D58A",
    "wallA": "E88A73", "wallB": "EE9F7E", "wallC": "E47A7E", "wallD": "F0B07E", "wallE": "D9765F", "wallF": "E99A8A",
    "greenA": "8DBB7E", "greenB": "6FA77A", "greenC": "A9C97F", "greenD": "5E9C6A",
    "window": "F6E7A0", "windark": "8C5A64", "metal": "8D9A93", "metaldark": "5E6B66", "wood": "B9835A",
    "paving": "E2BD8C", "ground": "D3A77D", "stone": "E7C9A0", "road": "C9957A", "roadline": "F3DFA8",
    "paper": "F6E7C4", "paperdark": "E3CC98", "cardboard": "C9A06A", "cardboarddark": "A47C4C", "cutedge": "A8998A",
    "magenta": "FF2E88", "cyan": "13CFE3", "gold": "FFB000", "trim": "F26B38", "violet": "7B2FF7",
    "skin": "E8B07C", "skinB": "C98B5E", "skinC": "F2C9A0", "skinshade": "D4956A",
    "eyewhite": "FBF4E4", "mouth": "5A2238", "teeth": "FBF4E4", "blush": "F08A8A",
    "hairdark": "2A1E33", "leather": "33293D", "leatherhi": "54466A", "denim": "46597A",
    "guitar": "D2463C", "goldbelt": "F2B53A", "golddark": "C98A1E", "red": "B3262E", "royal": "5E3A87",
    "fur": "C98A55", "furlight": "EBC592", "bow": "E2577A", "patch": "6FA77A",
    "chef": "F6EBD2", "chefdark": "D9C7A0", "shirt": "5FA8A0", "check": "9AA3A0", "toast": "E9B55E", "crust": "A8693C", "butter": "F7DE6A",
    "tux": "2E2638", "tuxhi": "4F4460", "shirtwhite": "F8F0DE", "chrome": "C9D3CF", "chromedark": "7F8C88",
    "velvet": "A8324A", "ermine": "F7F0DF", "gown": "5E9C6A", "hairq": "CFC6D8", "jewelB": "13A6C9",
    "car": "4FA3A5", "cardark": "2F7476", "tyre": "3E3A46", "glass": "A9D4CB", "lamp": "FFE6A3",
    "smoke": "E8D3AE", "shingle": "C9604F",
}
WALLS = ["wallA", "wallB", "wallC", "wallD", "wallE", "wallF"]
GREENS = ["greenA", "greenB", "greenC", "greenD"]
SIGNS = ["magenta", "cyan", "gold", "trim", "greenB"]

PREAMBLE = r"""\documentclass[tikz,border=0pt]{standalone}
\usepackage{xcolor}
% Palette: Sam_Comm_final.png by eye, and the cast's own colours.
""" + "\n".join(f"\\definecolor{{{k}}}{{HTML}}{{{v}}}" for k, v in COLOURS.items()) + "\n"

# Canvas sizes in cm and in pixels. Monsters are drawn at 100 px per cm.
SIZE = {"scene": ((32.0, 18.0), (1920, 1080)), "monsters": ((10.24, 15.36), (1024, 1536)), "items": ((4.0, 4.0), (256, 256))}
CX = 5.12  # a monster's centre line
BORDER_CM = 0.24  # the standee's white border: 24 px at 100 px/cm
PT = 0.0351459  # cm per pt


def f(x):
    return f"{x:.3f}".rstrip("0").rstrip(".") if abs(x) > 0.0005 else "0"


def mirror(pts, c=CX):
    return [(2 * c - x, y) for x, y in pts]


class Fig:
    """A list of inked shapes under an affine transform, so a character can
    be drawn once and placed anywhere, and its silhouette drawn again as the
    standee's white border."""

    def __init__(self, ink=1.1, T=(1, 0, 0, 1, 0, 0)):
        self.items = []
        self.ink = ink
        self.T = T

    # transforms -------------------------------------------------------
    def pt(self, x, y):
        a, b, c, d, e, g = self.T
        return (a * x + b * y + e, c * x + d * y + g)

    def k(self):
        a, b, c, d, _, _ = self.T
        return math.sqrt(abs(a * d - b * c))

    @contextmanager
    def frame(self, dx=0.0, dy=0.0, s=1.0, rot=0.0, sx=1.0, sy=1.0):
        old = self.T
        r = math.radians(rot)
        la, lb, lc, ld = s * sx * math.cos(r), -s * sy * math.sin(r), s * sx * math.sin(r), s * sy * math.cos(r)
        a, b, c, d, e, g = old
        self.T = (a * la + b * lc, a * lb + b * ld, c * la + d * lc, c * lb + d * ld, a * dx + b * dy + e, c * dx + d * dy + g)
        try:
            yield
        finally:
            self.T = old

    def P(self, x, y):
        X, Y = self.pt(x, y)
        return f"({f(X)},{f(Y)})"

    # shapes ------------------------------------------------------------
    def _add(self, kind, path, colour, opts="", sil=True, outline=True, lw=None):
        self.items.append({"kind": kind, "path": path, "colour": colour, "opts": opts, "sil": sil, "outline": outline, "lw": lw})

    def poly(self, pts, fill, **kw):
        self._add("fill", " -- ".join(self.P(x, y) for x, y in pts) + " -- cycle", fill, **kw)

    def blob(self, pts, fill, tension=0.6, **kw):
        coords = " ".join(self.P(x, y) for x, y in pts)
        self._add("fill", f"plot[smooth cycle, tension={tension}] coordinates {{{coords}}}", fill, **kw)

    def circle(self, x, y, r, fill, **kw):
        self._add("fill", f"{self.P(x, y)} circle ({f(r * self.k())})", fill, **kw)

    def ellipse(self, x, y, rx, ry, fill, **kw):
        n = 20
        self.blob([(x + rx * math.cos(2 * math.pi * i / n), y + ry * math.sin(2 * math.pi * i / n)) for i in range(n)], fill, tension=0.55, **kw)

    def rect(self, x0, y0, x1, y1, fill, **kw):
        self.poly([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], fill, **kw)

    def stroke(self, pts, lw=1.0, colour="ink", smooth=False, opts="", sil=False):
        if smooth:
            path = "plot[smooth, tension=0.6] coordinates {" + " ".join(self.P(x, y) for x, y in pts) + "}"
        else:
            path = " -- ".join(self.P(x, y) for x, y in pts)
        self._add("stroke", path, colour, opts=opts, sil=sil, lw=lw * self.k())

    def raw(self, tex):
        self.items.append({"kind": "raw", "tex": tex, "sil": False})

    # output ------------------------------------------------------------
    def render(self):
        out = []
        for it in self.items:
            if it["kind"] == "raw":
                out.append(it["tex"])
            elif it["kind"] == "fill":
                if it["outline"]:
                    lw = self.ink * (it["lw"] or 1.0)
                    out.append(f"\\filldraw[draw=ink, line width={f(lw)}pt, line join=round, fill={it['colour']}{it['opts']}] {it['path']};\n")
                else:
                    out.append(f"\\fill[{it['colour']}{it['opts']}] {it['path']};\n")
            else:
                out.append(f"\\draw[{it['colour']}, line width={f(it['lw'])}pt, line cap=round, line join=round{it['opts']}] {it['path']};\n")
        return "".join(out)

    def silhouette(self, border_pt, edge_pt):
        """The standee's border: every shape again, stroked fat, first in the
        cut-edge colour and then in white, so a white band of `border_pt`
        with a thin cut line round it follows the whole character."""
        out = []
        for colour, extra in (("cutedge", 2 * edge_pt), ("white", 0.0)):
            for it in self.items:
                if not it["sil"]:
                    continue
                if it["kind"] == "fill":
                    w = 2 * border_pt + extra + self.ink
                    out.append(f"\\filldraw[draw={colour}, fill={colour}, line width={f(w)}pt, line join=round] {it['path']};\n")
                else:
                    w = it["lw"] + 2 * border_pt + extra
                    out.append(f"\\draw[{colour}, line width={f(w)}pt, line cap=round, line join=round] {it['path']};\n")
        return "".join(out)


# ---- shared parts ---------------------------------------------------------

def limb(g, pts, w, fill, **kw):
    """An arm, leg or strap: a polyline thickened to width w (a number or one
    per point), rounded."""
    ws = w if isinstance(w, (list, tuple)) else [w] * len(pts)
    left, right = [], []
    n = len(pts)
    for i, (x, y) in enumerate(pts):
        if i == 0:
            dx, dy = pts[1][0] - x, pts[1][1] - y
        elif i == n - 1:
            dx, dy = x - pts[i - 1][0], y - pts[i - 1][1]
        else:
            dx, dy = pts[i + 1][0] - pts[i - 1][0], pts[i + 1][1] - pts[i - 1][1]
        L = math.hypot(dx, dy) or 1
        nx, ny = -dy / L, dx / L
        left.append((x + nx * ws[i] / 2, y + ny * ws[i] / 2))
        right.append((x - nx * ws[i] / 2, y - ny * ws[i] / 2))
    g.blob(left + right[::-1], fill, tension=0.3, **kw)


def stare(g, x, y, r, pupil=0.28):
    """A wide, unblinking eye: a white disc, a pinprick pupil dead centre."""
    g.circle(x, y, r, "eyewhite")
    g.circle(x, y, r * pupil, "ink", outline=False)
    g.circle(x + r * 0.1, y + r * 0.12, r * 0.07, "eyewhite", outline=False)


def grin(g, cx, cy, w, h, lip=None):
    """The fixed, too-wide smile: a crescent of teeth running nearly ear to
    ear, the corners creased."""
    N = 16
    us = [-1 + 2 * i / N for i in range(N + 1)]
    top = [(cx + u * w / 2, cy + 0.2 * h * u * u) for u in us]
    bot = [(cx + u * w / 2, cy + 0.2 * h * u * u - h * (1 - u * u)) for u in us]
    if lip:
        g.blob([(x, y + 0.08) for x, y in top[::2]] + [(x, y - 0.1) for x, y in bot[::-2]], lip, tension=0.4)
    g.poly(top + bot[::-1], "mouth")
    up = [(x, yt - 0.42 * (yt - yb)) for (x, yt), (_, yb) in zip(top, bot)]
    lo = [(x, yb + 0.3 * (yt - yb)) for (x, yt), (_, yb) in zip(top, bot)]
    g.poly(top[1:-1] + up[1:-1][::-1], "teeth", lw=0.6)
    g.poly(bot[2:-2] + lo[2:-2][::-1], "teeth", lw=0.6)
    for i in range(2, N - 1, 2):
        g.stroke([top[i], up[i]], lw=0.8)
    for i in range(3, N - 2, 2):
        g.stroke([bot[i], lo[i]], lw=0.8)
    for side in (-1, 1):
        x0, y0 = cx + side * w / 2, cy + 0.2 * h
        g.stroke([(x0 - side * 0.05, y0 + 0.18), (x0 + side * 0.12, y0 + 0.02), (x0 + side * 0.02, y0 - 0.22)], lw=1.4, smooth=True)


def star(g, x, y, r, fill, points=5, inner=0.45, rot=90):
    pts = []
    for i in range(points * 2):
        a = math.radians(rot + i * 180 / points)
        rr = r if i % 2 == 0 else r * inner
        pts.append((x + rr * math.cos(a), y + rr * math.sin(a)))
    g.poly(pts, fill)


def guitar(g):
    """An electric guitar, body round the origin, neck up +y (about 7.5 tall)."""
    g.blob([(0, -1.55), (1.05, -1.3), (1.4, -0.5), (1.2, 0.35), (1.15, 1.25), (0.8, 1.45), (0.5, 0.8), (-0.45, 0.8),
            (-0.75, 1.2), (-1.1, 1.05), (-1.25, 0.3), (-1.45, -0.5), (-1.1, -1.3)], "guitar", tension=0.55)
    g.blob([(-0.25, 0.75), (0.35, 0.72), (0.85, -0.15), (0.55, -0.95), (-0.55, -0.7), (-0.95, 0.15)], "eyewhite", tension=0.5)
    for y0 in (0.12, -0.38):
        g.rect(-0.32, y0, 0.32, y0 + 0.22, "hairdark", lw=0.7)
    g.rect(-0.36, -1.0, 0.36, -0.82, "chrome", lw=0.7)
    for (kx, ky) in ((0.72, -0.72), (0.98, -0.35)):
        g.circle(kx, ky, 0.13, "chrome", lw=0.7)
    g.poly([(-0.2, 0.7), (0.2, 0.7), (0.17, 4.75), (-0.17, 4.75)], "wood")
    for k in range(9):
        y = 1.25 + k * 0.4
        g.stroke([(-0.18, y), (0.18, y)], lw=0.6, colour="chromedark")
    g.poly([(-0.25, 4.7), (0.25, 4.7), (0.42, 5.8), (0.05, 6.05), (-0.32, 5.5)], "guitar")
    for k in range(3):
        g.circle(-0.38, 4.95 + k * 0.32, 0.09, "chrome", lw=0.6)
        g.circle(0.38 + k * 0.03, 5.0 + k * 0.32, 0.09, "chrome", lw=0.6)
    for k in range(4):
        x = -0.12 + 0.08 * k
        g.stroke([(x, -0.9), (x, 4.72)], lw=0.45, colour="eyewhite")


def ribbon_mic(g):
    """The old ribbon microphone: a chrome pill of slats in a yoke on a short
    stem. Centre of the pill at the origin, about 1 wide and 1.7 tall."""
    g.stroke([(-0.62, 0.1), (-0.62, -0.75), (0, -1.05), (0.62, -0.75), (0.62, 0.1)], lw=3.2, colour="chromedark", smooth=True, sil=True)
    g.rect(-0.1, -1.05, 0.1, -1.65, "chromedark")
    r = 0.5
    pill = [(-r, -0.35), (-r, 0.35)] + [(r * math.cos(math.radians(a)), 0.35 + r * math.sin(math.radians(a))) for a in range(150, 29, -30)] + \
           [(r, 0.35), (r, -0.35)] + [(r * math.cos(math.radians(a)), -0.35 + r * math.sin(math.radians(a))) for a in range(-30, -151, -30)]
    g.blob(pill, "chrome", tension=0.35)
    for k in range(-5, 6):
        y = k * 0.13
        half = r * 0.82 if abs(y) < 0.35 else r * 0.82 * math.sqrt(max(0, 1 - ((abs(y) - 0.35) / r) ** 2))
        g.stroke([(-half, y), (half, y)], lw=0.7, colour="chromedark")
    g.rect(-r - 0.02, -0.08, r + 0.02, 0.08, "metaldark", lw=0.7)
    g.circle(-0.62, 0.0, 0.1, "chromedark", lw=0.6)
    g.circle(0.62, 0.0, 0.1, "chromedark", lw=0.6)
    g.stroke([(-0.25, 0.75), (-0.33, 0.35)], lw=1.6, colour="white", opts=", opacity=0.8")


def crown(g):
    """A tall crown: a jewelled band, five points with pearls, a velvet cap
    and a little orb and cross on top. Band from y 0 to 0.55, 2.6 wide."""
    g.blob([(-1.15, 0.5), (-1.0, 1.25), (-0.4, 1.65), (0.4, 1.65), (1.0, 1.25), (1.15, 0.5)], "velvet", tension=0.6)
    tips = [(-1.3, 1.45), (-0.65, 1.7), (0, 1.95), (0.65, 1.7), (1.3, 1.45)]
    outline = [(-1.3, 0.5)]
    for i, (tx, ty) in enumerate(tips):
        outline.append((tx, ty))
        if i < len(tips) - 1:
            nx = (tx + tips[i + 1][0]) / 2
            outline.append((nx, 0.95))
    outline.append((1.3, 0.5))
    g.poly(outline, "goldbelt")
    for tx, ty in tips:
        g.circle(tx, ty + 0.08, 0.13, "eyewhite", lw=0.7)
    g.rect(-1.38, 0.0, 1.38, 0.58, "goldbelt")
    for k, col in enumerate(["red", "jewelB", "greenD", "jewelB", "red"]):
        x = -1.0 + k * 0.5
        if k % 2 == 0:
            g.poly([(x, 0.12), (x + 0.15, 0.29), (x, 0.46), (x - 0.15, 0.29)], col, lw=0.7)
        else:
            g.circle(x, 0.29, 0.13, col, lw=0.7)
    g.circle(0, 2.12, 0.17, "goldbelt", lw=0.8)
    g.stroke([(0, 2.28), (0, 2.6)], lw=2.2, colour="golddark")
    g.stroke([(-0.13, 2.48), (0.13, 2.48)], lw=2.2, colour="golddark")


def toast(g):
    """A slice of toast standing up: crust, golden face, a pat of butter.
    Bottom centre at the origin, about 1 wide and 1.1 tall."""
    shape = [(-0.5, 0), (0.5, 0), (0.52, 0.7), (0.62, 0.92), (0.4, 1.12), (0.12, 1.05), (-0.12, 1.08), (-0.42, 1.12), (-0.62, 0.92), (-0.52, 0.7)]
    g.blob(shape, "crust", tension=0.35)
    g.blob([(x * 0.78, 0.1 + y * 0.8) for x, y in shape], "toast", tension=0.35, lw=0.6)
    g.poly([(-0.18, 0.5), (0.16, 0.56), (0.12, 0.8), (-0.2, 0.74)], "butter", lw=0.7)


# ---- the cast --------------------------------------------------------------

def headliner(g):
    """The Headliner: teased hair, shades slid down over staring eyes,
    leather jacket, a guitar slung low."""
    c = CX
    mane = [(c - 2.5, 9.3)]
    n = 15
    for i in range(n + 1):
        a = math.radians(205 - i * (230 / n))
        r = 3.0 if i % 2 == 0 else 2.2
        mane.append((c + r * math.cos(a) * 1.05, 12.05 + r * math.sin(a) * 0.92))
    mane += [(c + 2.5, 9.3), (c + 1.1, 10.4), (c - 1.1, 10.4)]
    g.poly(mane, "hairdark")
    g.poly([(c + 1.2, 12.9), (c + 2.0, 14.1), (c + 2.1, 13.0), (c + 2.9, 12.6), (c + 2.0, 12.2)], "wallC")
    # legs and boots
    for s in (-1, 1):
        limb(g, [(c + s * 0.75, 6.4), (c + s * 1.15, 3.5), (c + s * 1.4, 1.3)], [1.45, 1.05, 0.85], "denim")
        g.poly([(c + s * 0.95, 3.7), (c + s * 1.45, 3.75), (c + s * 1.35, 3.35), (c + s * 1.0, 3.4)], "skin", lw=0.6)
        boot = [(c - 2.55, 0.24), (c - 0.85, 0.24), (c - 0.9, 0.75), (c - 1.0, 1.7), (c - 1.9, 1.7), (c - 2.0, 0.85), (c - 2.45, 0.6)]
        g.poly(boot if s < 0 else mirror(boot), "leather")
        g.stroke([(c + s * 1.0, 1.25), (c + s * 1.9, 1.25)], lw=1.4, colour="chrome")
    # shirt, belt, jacket
    g.poly([(c - 1.9, 6.2), (c + 1.9, 6.2), (c + 2.15, 10.3), (c - 2.15, 10.3)], "wallC")
    g.rect(c - 1.95, 6.05, c + 1.95, 6.6, "leather")
    g.rect(c - 0.35, 6.0, c + 0.35, 6.65, "chrome", lw=0.8)
    for s in (-1, 1):
        half = [(c - 2.35, 10.4), (c - 0.6, 10.3), (c - 0.95, 8.6), (c - 0.8, 5.9), (c - 2.25, 5.85), (c - 2.4, 8.0)]
        g.poly(half if s < 0 else mirror(half), "leather")
        lapel = [(c - 0.6, 10.35), (c - 1.6, 10.55), (c - 1.45, 9.5), (c - 0.95, 8.6)]
        g.poly(lapel if s < 0 else mirror(lapel), "leatherhi")
        for k in range(3):
            g.circle(c + s * (1.35 - k * 0.15), 10.2 - k * 0.45, 0.07, "chrome", lw=0.5)
        g.stroke([(c + s * 0.9, 8.4), (c + s * 0.82, 6.1)], lw=1.2, colour="chrome")
    # the guitar's placement, and where the hands fall on it
    gx, gy, gs, gr = c + 0.5, 5.0, 1.25, 40

    def gw(lx, ly):
        a = math.radians(gr)
        return (gx + gs * (lx * math.cos(a) - ly * math.sin(a)), gy + gs * (lx * math.sin(a) + ly * math.cos(a)))

    fret, strum = gw(0, 3.6), gw(0.25, 0.05)
    limb(g, [(c - 2.05, 10.0), (c - 3.35, 8.9), fret], [1.0, 0.9, 0.8], "leather")
    limb(g, [(c + 2.05, 10.0), (c + 2.95, 7.6), (strum[0] + 0.3, strum[1] + 0.2)], [1.0, 0.9, 0.8], "leather")
    limb(g, [(c - 1.7, 10.25), (c + 2.0, 5.6)], 0.38, "trim")
    with g.frame(gx, gy, gs, gr):
        guitar(g)
    g.circle(*fret, 0.36, "skin")
    g.circle(*strum, 0.36, "skin")
    # head
    g.rect(c - 0.5, 9.9, c + 0.5, 10.9, "skin")
    for s in (-1, 1):
        g.circle(c + s * 1.32, 11.95, 0.32, "skin")
        g.circle(c + s * 1.36, 11.7, 0.08, "goldbelt", lw=0.5)
    g.ellipse(c, 12.0, 1.35, 1.62, "skin")
    stare(g, c - 0.6, 12.42, 0.34)
    stare(g, c + 0.6, 12.42, 0.34)
    for s in (-1, 1):
        lens = [(c + s * 0.12, 12.38), (c + s * 1.12, 12.4), (c + s * 1.08, 11.95), (c + s * 0.92, 11.78), (c + s * 0.3, 11.78), (c + s * 0.15, 11.95)]
        g.blob(lens, "hairdark", tension=0.3)
        g.stroke([(c + s * 0.45, 12.05), (c + s * 0.62, 12.25)], lw=1.4, colour="eyewhite")
    g.stroke([(c - 0.15, 12.25), (c + 0.15, 12.25)], lw=1.6)
    g.stroke([(c - 0.03, 11.95), (c - 0.12, 11.62), (c + 0.06, 11.58)], lw=1.0)
    grin(g, c, 11.25, 2.15, 0.6)
    bangs = [(c - 1.5, 12.5), (c - 1.15, 13.2), (c - 0.85, 12.75), (c - 0.45, 13.3), (c - 0.05, 12.75), (c + 0.35, 13.3),
             (c + 0.75, 12.75), (c + 1.15, 13.25), (c + 1.5, 12.5), (c + 1.75, 13.9), (c, 14.35), (c - 1.75, 13.9)]
    g.poly(bangs, "hairdark")
    for x, y in ((-2.2, 12.2), (-1.7, 13.4), (1.9, 11.6), (-2.1, 10.9)):
        g.stroke([(c + x, y), (c + x + 0.3, y + 0.6)], lw=1.4, colour="leatherhi")


def champion(g):
    """The Champion: a double-biceps flex, a cape, an enormous gold belt,
    knee pads and boots."""
    c = CX
    cape = [(c - 2.4, 10.8), (c + 2.4, 10.8), (c + 3.5, 8.0), (c + 4.3, 4.0), (c + 4.55, 0.45), (c + 3.6, 0.95), (c + 2.6, 0.5),
            (c - 2.6, 0.5), (c - 3.6, 0.95), (c - 4.55, 0.45), (c - 4.3, 4.0), (c - 3.5, 8.0)]
    g.blob(cape, "royal", tension=0.35)
    for s in (-1, 1):
        g.stroke([(c + s * 3.2, 7.5), (c + s * 3.75, 3.0), (c + s * 3.6, 1.1)], lw=1.2, smooth=True)
        g.stroke([(c + s * 2.6, 9.6), (c + s * 4.0, 1.3)], lw=1.0, smooth=True, colour="leatherhi")
    for s in (-1, 1):
        limb(g, [(c + s * 1.0, 5.6), (c + s * 1.3, 3.2), (c + s * 1.35, 2.6)], [1.7, 1.3, 1.2], "skinB")
        g.blob([(c + s * 0.7, 3.4), (c + s * 1.95, 3.4), (c + s * 2.0, 2.6), (c + s * 0.7, 2.6)], "red", tension=0.4)
        g.stroke([(c + s * 0.85, 3.0), (c + s * 1.85, 3.0)], lw=2.0, colour="eyewhite")
        boot = [(c - 2.25, 0.24), (c - 0.5, 0.24), (c - 0.55, 0.85), (c - 0.75, 2.75), (c - 2.0, 2.75), (c - 2.1, 0.9)]
        g.poly(boot if s < 0 else mirror(boot), "red")
        g.rect(c + s * 0.72, 2.45, c + s * 2.03, 2.8, "eyewhite")
        for k in range(4):
            y = 0.9 + k * 0.38
            g.stroke([(c + s * 1.05, y), (c + s * 1.5, y + 0.2)], lw=1.4, colour="eyewhite")
            g.stroke([(c + s * 1.5, y), (c + s * 1.05, y + 0.2)], lw=1.4, colour="eyewhite")
        g.poly([(c + s * 0.5, 0.24), (c + s * 2.25, 0.24), (c + s * 2.15, 0.55), (c + s * 0.55, 0.55)], "eyewhite", lw=0.8)
    g.poly([(c - 2.0, 6.6), (c + 2.0, 6.6), (c + 2.25, 4.9), (c + 0.45, 4.55), (c, 4.95), (c - 0.45, 4.55), (c - 2.25, 4.9)], "red")
    g.poly([(c - 1.9, 6.4), (c + 1.9, 6.4), (c + 2.75, 10.0), (c + 2.1, 10.95), (c - 2.1, 10.95), (c - 2.75, 10.0)], "skinB")
    for s in (-1, 1):
        g.stroke([(c, 9.0), (c + s * 0.9, 8.8), (c + s * 1.8, 9.3)], lw=1.4, smooth=True)
        for k in range(3):
            g.stroke([(c + s * 0.15, 8.3 - k * 0.55), (c + s * 0.6, 8.25 - k * 0.55)], lw=1.2)
    g.stroke([(c, 8.5), (c, 7.3)], lw=1.2)
    # the arms, flexed
    for s in (-1, 1):
        limb(g, [(c + s * 2.4, 10.35), (c + s * 4.05, 10.2)], [1.25, 1.05], "skinB")
        limb(g, [(c + s * 4.05, 10.0), (c + s * 4.0, 12.4)], [1.05, 0.85], "skinB")
        g.circle(c + s * 3.35, 10.75, 0.72, "skinB")
        g.rect(c + s * 3.55, 12.15, c + s * 4.45, 12.55, "red")
        g.circle(c + s * 4.0, 12.95, 0.58, "skinB")
        g.stroke([(c + s * 3.7, 13.05), (c + s * 4.3, 13.05)], lw=1.0)
    # the belt
    g.rect(c - 2.1, 6.25, c + 2.1, 7.45, "leather")
    for s in (-1, 1):
        g.blob([(c + s * 1.35, 6.4), (c + s * 2.05, 6.45), (c + s * 2.05, 7.25), (c + s * 1.35, 7.3)], "goldbelt", tension=0.3)
        g.circle(c + s * 1.7, 6.85, 0.16, "red", lw=0.7)
    g.ellipse(c, 6.85, 1.5, 1.2, "goldbelt")
    g.ellipse(c, 6.85, 1.12, 0.86, "golddark")
    star(g, c, 6.88, 0.75, "goldbelt")
    g.circle(c, 6.88, 0.2, "red", lw=0.8)
    for k in range(12):
        a = 2 * math.pi * k / 12
        g.circle(c + 1.32 * math.cos(a), 6.85 + 1.04 * math.sin(a), 0.07, "eyewhite", outline=False)
    # head
    g.poly([(c - 0.8, 11.6), (c - 2.1, 10.85), (c + 2.1, 10.85), (c + 0.8, 11.6)], "skinB")
    g.rect(c - 0.75, 10.8, c + 0.75, 11.9, "skinB")
    for s in (-1, 1):
        g.circle(c + s * 1.15, 12.95, 0.3, "skinB")
    g.ellipse(c, 13.0, 1.18, 1.45, "skinB")
    g.stroke([(c - 0.7, 14.05), (c - 0.2, 14.3)], lw=2.0, colour="skinC")
    for s in (-1, 1):
        stare(g, c + s * 0.48, 13.35, 0.31)
        g.poly([(c + s * 0.15, 13.8), (c + s * 0.9, 13.95), (c + s * 0.9, 13.75), (c + s * 0.15, 13.62)], "hairdark", lw=0.6)
        limb(g, [(c + s * 0.05, 12.62), (c + s * 0.75, 12.55), (c + s * 1.25, 12.75), (c + s * 1.35, 13.1)], [0.38, 0.32, 0.2, 0.1], "hairdark")
    g.stroke([(c - 0.05, 13.2), (c - 0.12, 12.85), (c + 0.08, 12.8)], lw=1.0)
    grin(g, c, 12.38, 1.75, 0.55)


def buttons(g):
    """Mr. Buttons: a giant teddy bear, mismatched button eyes, a stitched
    grin too wide for his face, a ribbon bow."""
    c = CX
    for s in (-1, 1):
        g.circle(c + s * 2.35, 13.4, 1.0, "fur")
        g.circle(c + s * 2.35, 13.4, 0.55, "furlight")
    for s in (-1, 1):
        limb(g, [(c + s * 2.5, 8.1), (c + s * 3.75, 6.7), (c + s * 3.95, 5.7)], [1.75, 1.55, 1.4], "fur")
        g.circle(c + s * 3.95, 5.55, 0.72, "fur")
        g.circle(c + s * 3.95, 5.45, 0.4, "furlight")
    for s in (-1, 1):
        g.ellipse(c + s * 1.75, 1.55, 1.35, 1.31, "fur")
    g.ellipse(c, 5.7, 3.15, 3.5, "fur")
    g.ellipse(c, 5.3, 1.95, 2.35, "furlight")
    g.stroke([(c, 7.6), (c, 3.0)], lw=1.3, opts=", dash pattern=on 4pt off 3pt")
    for s in (-1, 1):
        g.ellipse(c + s * 1.75, 1.15, 0.9, 0.72, "furlight")
        for k in (-1, 0, 1):
            g.circle(c + s * 1.75 + k * 0.38, 1.82 - abs(k) * 0.08, 0.13, "fur", lw=0.6)
    with g.frame(c + 1.7, 3.6, 1.0, 12):
        g.rect(-0.55, -0.5, 0.55, 0.5, "patch")
        for x, y in ((-0.55, -0.5), (0.55, -0.5), (0.55, 0.5), (-0.55, 0.5)):
            g.stroke([(x - 0.12, y - 0.12), (x + 0.12, y + 0.12)], lw=1.2)
            g.stroke([(x - 0.12, y + 0.12), (x + 0.12, y - 0.12)], lw=1.2)
    # head
    g.ellipse(c, 11.45, 3.0, 2.5, "fur")
    g.stroke([(c, 13.95), (c, 12.85)], lw=1.3, opts=", dash pattern=on 4pt off 3pt")
    g.ellipse(c, 10.5, 1.6, 1.08, "furlight")
    g.blob([(c - 0.55, 11.35), (c + 0.55, 11.35), (c + 0.1, 10.85), (c - 0.1, 10.85)], "hairdark", tension=0.5)
    g.stroke([(c, 10.88), (c, 9.78)], lw=2.2)
    pts = [(c + dx, 9.75 + 1.0 * (dx / 2.35) ** 2) for dx in [i * 0.235 for i in range(-10, 11)]]
    g.stroke(pts, lw=3.2, smooth=True)
    for i in range(1, 20, 2):
        (x0, y0), (x1, y1) = pts[i - 1], pts[i + 1]
        L = math.hypot(x1 - x0, y1 - y0)
        nx, ny = -(y1 - y0) / L, (x1 - x0) / L
        x, y = pts[i]
        g.stroke([(x - nx * 0.22, y - ny * 0.22), (x + nx * 0.22, y + ny * 0.22)], lw=2.0)
    # the eyes: one big teal four-hole button, one small pink two-hole one
    g.circle(c - 1.25, 12.25, 0.7, "patch")
    g.circle(c - 1.25, 12.25, 0.52, "patch", lw=0.7)
    holes = [(-0.18, 0.18), (0.18, 0.18), (0.18, -0.18), (-0.18, -0.18)]
    for hx, hy in holes:
        g.circle(c - 1.25 + hx, 12.25 + hy, 0.09, "hairdark", outline=False)
    g.stroke([(c - 1.43, 12.43), (c - 1.07, 12.07)], lw=1.6, colour="bow")
    g.stroke([(c - 1.07, 12.43), (c - 1.43, 12.07)], lw=1.6, colour="bow")
    g.stroke([(c - 1.6, 12.6), (c - 1.4, 12.75)], lw=1.6, colour="white", opts=", opacity=0.8")
    g.circle(c + 1.2, 12.45, 0.42, "bow")
    g.circle(c + 1.2, 12.45, 0.3, "bow", lw=0.6)
    for hx in (-0.11, 0.11):
        g.circle(c + 1.2 + hx, 12.45, 0.07, "hairdark", outline=False)
    g.stroke([(c + 1.09, 12.45), (c + 1.31, 12.45)], lw=1.4, colour="eyewhite")
    g.stroke([(c + 1.31, 12.45), (c + 1.45, 12.2), (c + 1.35, 11.8), (c + 1.5, 11.5)], lw=1.0, colour="eyewhite", smooth=True)
    # the bow
    g.blob([(c - 0.15, 9.05), (c - 1.2, 9.7), (c - 1.85, 9.55), (c - 1.8, 8.4), (c - 1.1, 8.45)], "bow", tension=0.5)
    g.blob([(c + 0.15, 9.05), (c + 1.2, 9.7), (c + 1.85, 9.55), (c + 1.8, 8.4), (c + 1.1, 8.45)], "bow", tension=0.5)
    g.poly([(c - 0.25, 8.85), (c - 0.95, 7.55), (c - 0.6, 7.7), (c - 0.45, 7.45), (c + 0.05, 8.75)], "bow")
    g.poly([(c + 0.25, 8.85), (c + 0.95, 7.55), (c + 0.6, 7.7), (c + 0.45, 7.45), (c - 0.05, 8.75)], "bow")
    g.circle(c, 9.05, 0.38, "bow")
    for s in (-1, 1):
        g.stroke([(c + s * 0.5, 9.1), (c + s * 1.3, 9.25)], lw=1.0)
        g.stroke([(c + s * 0.5, 8.95), (c + s * 1.3, 8.7)], lw=1.0)


def host(g):
    """The Morning Host: a TV breakfast chef in a tall toque and apron, a
    spatula raised, a plate of toast held out, the camera smile."""
    c = CX
    for s in (-1, 1):
        limb(g, [(c + s * 0.95, 3.8), (c + s * 1.2, 1.0)], [1.25, 1.05], "check")
        for k in range(4):
            g.stroke([(c + s * 0.6, 1.4 + k * 0.6), (c + s * 1.75, 1.4 + k * 0.6)], lw=0.8, colour="metaldark")
        clog = [(c - 2.15, 0.24), (c - 0.45, 0.24), (c - 0.55, 1.05), (c - 1.95, 1.05)]
        g.blob(clog if s < 0 else mirror(clog), "hairdark", tension=0.3)
    g.poly([(c - 1.9, 6.0), (c + 1.9, 6.0), (c + 2.15, 10.2), (c - 2.15, 10.2)], "shirt")
    for k in range(-3, 4):
        g.stroke([(c + k * 0.55, 6.2), (c + k * 0.6, 10.0)], lw=1.2, colour="greenC")
    g.poly([(c - 1.95, 7.1), (c + 1.95, 7.1), (c + 2.35, 3.4), (c - 2.35, 3.4)], "chef")
    g.poly([(c - 1.15, 7.0), (c + 1.15, 7.0), (c + 1.05, 9.75), (c - 1.05, 9.75)], "chef")
    g.rect(c - 2.0, 6.85, c + 2.0, 7.25, "chefdark")
    g.rect(c - 0.8, 4.4, c + 0.8, 5.6, "chef")
    g.stroke([(c - 0.75, 5.35), (c + 0.75, 5.35)], lw=0.8)
    for s in (-1, 1):
        g.stroke([(c + s * 1.0, 9.7), (c + s * 0.5, 10.4)], lw=2.0, colour="chefdark")
    # the raised arm and the spatula
    limb(g, [(c + 2.0, 9.9), (c + 3.4, 9.6), (c + 3.7, 11.5)], [1.0, 0.9, 0.85], "shirt")
    g.rect(c + 3.25, 11.35, c + 4.15, 11.65, "chef")
    limb(g, [(c + 3.72, 11.7), (c + 3.95, 13.3)], 0.3, "wood")
    with g.frame(c + 3.98, 13.2, 1.0, -4):
        g.blob([(-0.45, 0), (0.45, 0), (0.55, 1.4), (-0.5, 1.45)], "chrome", tension=0.15)
        for k in (-0.22, 0, 0.22):
            g.stroke([(k, 0.35), (k, 1.15)], lw=2.0, colour="chromedark")
    g.circle(c + 3.72, 11.95, 0.42, "skin")
    # the plate of toast
    limb(g, [(c - 2.0, 9.9), (c - 3.2, 8.2), (c - 3.45, 8.75)], [1.0, 0.9, 0.85], "shirt")
    g.circle(c - 3.4, 8.75, 0.4, "skin")
    g.ellipse(c - 3.25, 9.15, 1.3, 0.34, "eyewhite")
    g.ellipse(c - 3.25, 9.17, 0.88, 0.2, "eyewhite", lw=0.6)
    with g.frame(c - 3.85, 9.1, 0.85, 8):
        toast(g)
    with g.frame(c - 2.9, 9.1, 0.85, -10):
        toast(g)
    for k in range(3):
        x = c - 4.0 + k * 0.6
        g.stroke([(x, 10.25), (x + 0.15, 10.55), (x - 0.05, 10.85), (x + 0.1, 11.15)], lw=1.4, colour="eyewhite", smooth=True, sil=True)
    # head and toque
    g.rect(c - 0.45, 9.9, c + 0.45, 10.6, "skin")
    g.poly([(c - 0.85, 10.45), (c + 0.85, 10.45), (c, 9.55)], "red")
    g.circle(c, 10.35, 0.2, "red")
    for s in (-1, 1):
        g.circle(c + s * 1.28, 11.75, 0.3, "skin")
    g.ellipse(c, 11.8, 1.32, 1.48, "skin")
    for s in (-1, 1):
        g.poly([(c + s * 1.32, 12.9), (c + s * 1.32, 11.9), (c + s * 1.1, 12.2), (c + s * 1.05, 12.9)], "crust", lw=0.7)
        stare(g, c + s * 0.5, 12.08, 0.33)
        g.stroke([(c + s * 0.2, 12.6), (c + s * 0.55, 12.85), (c + s * 0.9, 12.62)], lw=1.8, smooth=True)
        g.circle(c + s * 0.95, 11.35, 0.25, "blush", outline=False, opts=", opacity=0.6")
    g.stroke([(c, 11.85), (c - 0.12, 11.5), (c + 0.08, 11.45)], lw=1.0)
    grin(g, c, 11.15, 2.25, 0.62)
    g.poly([(c - 1.35, 13.4), (c + 1.35, 13.4), (c + 1.6, 14.35), (c - 1.6, 14.35)], "chef")
    for x in (-0.8, -0.25, 0.3, 0.85):
        g.stroke([(c + x, 13.55), (c + x * 1.1, 14.25)], lw=0.9, colour="chefdark")
    for x, r in ((-1.15, 0.5), (-0.4, 0.58), (0.4, 0.58), (1.15, 0.5)):
        g.circle(c + x, 14.38, r, "chef")
    g.rect(c - 1.35, 12.95, c + 1.35, 13.5, "chef")


def crooner(g):
    """The Crooner: slicked hair, a tuxedo with a carnation, an old ribbon
    microphone at his cheek, one hand out to the crowd."""
    c = CX
    for s in (-1, 1):
        limb(g, [(c + s * 0.85, 6.6), (c + s * 1.15, 1.0)], [1.4, 1.05], "tux")
        g.stroke([(c + s * 1.5, 6.3), (c + s * 1.63, 1.2)], lw=1.8, colour="tuxhi")
        shoe = [(c - 2.15, 0.24), (c - 0.55, 0.24), (c - 0.65, 0.95), (c - 1.75, 0.95), (c - 2.1, 0.6)]
        g.blob(shoe if s < 0 else mirror(shoe), "hairdark", tension=0.3)
        g.stroke([(c + s * 1.85, 0.5), (c + s * 1.5, 0.7)], lw=1.4, colour="eyewhite")
    g.poly([(c - 1.9, 5.9), (c + 1.9, 5.9), (c + 2.25, 10.6), (c - 2.25, 10.6)], "tux")
    g.poly([(c - 0.85, 10.6), (c + 0.85, 10.6), (c + 0.3, 7.3), (c - 0.3, 7.3)], "shirtwhite")
    for k in range(3):
        g.circle(c, 9.6 - k * 0.6, 0.07, "ink", outline=False)
    for s in (-1, 1):
        lapel = [(c - 0.85, 10.6), (c - 1.6, 10.65), (c - 1.25, 9.0), (c - 0.3, 7.3), (c - 0.62, 8.6)]
        g.poly(lapel if s < 0 else mirror(lapel), "tuxhi")
    g.stroke([(c, 7.3), (c, 5.95)], lw=1.2, colour="tuxhi")
    g.circle(c, 6.95, 0.1, "tuxhi", lw=0.6)
    g.poly([(c + 1.0, 9.0), (c + 1.6, 9.0), (c + 1.45, 9.35), (c + 1.3, 9.2), (c + 1.15, 9.35)], "eyewhite", lw=0.7)
    g.poly([(c - 1.35, 9.85), (c - 1.0, 9.75), (c - 1.25, 9.35)], "greenB", lw=0.7)
    g.circle(c - 1.3, 9.95, 0.28, "red")
    g.stroke([(c - 1.42, 10.02), (c - 1.25, 9.85), (c - 1.15, 10.05)], lw=0.8)
    g.rect(c - 0.45, 10.3, c + 0.45, 11.0, "skin")
    g.poly([(c, 10.5), (c - 0.6, 10.75), (c - 0.6, 10.2)], "hairdark")
    g.poly([(c, 10.5), (c + 0.6, 10.75), (c + 0.6, 10.2)], "hairdark")
    g.circle(c, 10.5, 0.13, "hairdark")
    # the open hand
    limb(g, [(c + 2.1, 10.2), (c + 3.35, 9.35), (c + 4.15, 10.25)], [1.0, 0.9, 0.85], "tux")
    with g.frame(c + 4.15, 10.25, 1.0, 40):
        g.rect(-0.45, -0.18, 0.45, 0.18, "shirtwhite")
        for x, a in ((-0.27, 14), (-0.09, 5), (0.09, -4), (0.27, -13)):
            limb(g, [(x, 0.6), (x - 0.75 * math.sin(math.radians(a)), 0.6 + 0.75 * math.cos(math.radians(a)))], [0.2, 0.17], "skin")
        limb(g, [(-0.3, 0.35), (-0.75, 0.75)], [0.22, 0.18], "skin")
        g.blob([(-0.4, 0.15), (0.4, 0.15), (0.43, 0.7), (-0.42, 0.7)], "skin", tension=0.3)
        g.rect(0.18, 0.95, 0.38, 1.05, "goldbelt", lw=0.5)
    # the microphone arm
    limb(g, [(c - 2.1, 10.2), (c - 3.15, 8.4), (c - 2.05, 9.45)], [1.0, 0.9, 0.85], "tux")
    g.rect(c - 2.3, 9.2, c - 1.75, 9.6, "shirtwhite")
    with g.frame(c - 1.95, 11.3, 1.05):
        ribbon_mic(g)
    g.circle(c - 1.95, 9.6, 0.4, "skin")
    # head
    for s in (-1, 1):
        g.circle(c + s * 1.28, 12.35, 0.3, "skin")
    g.ellipse(c, 12.4, 1.3, 1.55, "skin")
    g.blob([(c - 1.4, 12.2), (c - 1.5, 13.2), (c - 1.0, 13.95), (c + 0.1, 14.45), (c + 1.35, 14.1), (c + 1.55, 13.2), (c + 1.38, 12.2),
            (c + 1.15, 13.0), (c + 0.4, 13.4), (c - 0.5, 13.3), (c - 1.15, 12.85)], "hairdark", tension=0.45)
    g.stroke([(c - 0.9, 13.6), (c - 0.2, 14.05), (c + 0.8, 13.85)], lw=2.0, colour="leatherhi", smooth=True)
    g.stroke([(c - 0.5, 13.55), (c - 0.55, 13.3)], lw=1.2, colour="leatherhi")
    for s in (-1, 1):
        stare(g, c + s * 0.5, 12.65, 0.32)
        g.stroke([(c + s * 0.2, 13.15), (c + s * 0.55, 13.3), (c + s * 0.9, 13.1)], lw=1.8, smooth=True)
    g.stroke([(c, 12.45), (c - 0.12, 12.1), (c + 0.08, 12.05)], lw=1.0)
    g.stroke([(c - 0.6, 11.88), (c + 0.6, 11.88)], lw=1.6, colour="hairdark")
    grin(g, c, 11.68, 2.15, 0.6)


def queen(g):
    """The Queen: a tall crown, an ermine cape over velvet, a sceptre, the
    painted smile."""
    c = CX
    cape = [(c - 2.3, 10.3), (c + 2.3, 10.3), (c + 3.4, 6.5), (c + 4.2, 2.6), (c + 4.55, 0.24), (c - 4.55, 0.24), (c - 4.2, 2.6), (c - 3.4, 6.5)]
    g.poly(cape, "velvet")
    g.poly([(c - 4.55, 0.24), (c - 2.9, 0.24), (c - 2.9, 0.85), (c - 4.42, 0.85)], "ermine")
    g.poly([(c + 4.55, 0.24), (c + 2.9, 0.24), (c + 2.9, 0.85), (c + 4.42, 0.85)], "ermine")
    for x in (-4.3, -3.5, 3.5, 4.3):
        ermine_spot(g, c + x, 0.55, 0.12)
    g.poly([(c - 1.4, 7.3), (c + 1.4, 7.3), (c + 3.4, 0.24), (c - 3.4, 0.24)], "gown")
    g.poly([(c - 3.3, 0.24), (c + 3.3, 0.24), (c + 3.15, 0.75), (c - 3.15, 0.75)], "goldbelt")
    g.poly([(c - 0.4, 7.3), (c + 0.4, 7.3), (c + 0.9, 0.75), (c - 0.9, 0.75)], "greenC")
    for k in range(5):
        star(g, c, 1.4 + k * 1.2, 0.18 + k * -0.01, "goldbelt", inner=0.5)
    for s in (-1, 1):
        g.stroke([(c + s * 1.2, 6.5), (c + s * 2.2, 1.0)], lw=1.2)
    g.poly([(c - 1.4, 7.2), (c + 1.4, 7.2), (c + 1.8, 10.0), (c - 1.8, 10.0)], "gown")
    g.rect(c - 1.45, 7.0, c + 1.45, 7.4, "goldbelt")
    # the sceptre hand and the resting hand
    limb(g, [(c - 2.2, 9.8), (c - 2.95, 8.0), (c - 2.1, 7.2)], [0.95, 0.85, 0.8], "gown")
    g.circle(c - 1.95, 7.15, 0.36, "eyewhite")
    limb(g, [(c + 2.2, 9.8), (c + 3.25, 8.2), (c + 3.3, 9.2)], [0.95, 0.85, 0.8], "gown")
    limb(g, [(c + 3.4, 4.6), (c + 3.3, 13.6)], 0.24, "goldbelt")
    for y in (6.5, 11.0, 13.2):
        g.rect(c + 3.15, y, c + 3.55, y + 0.22, "goldbelt", lw=0.7)
    g.circle(c + 3.3, 13.95, 0.42, "goldbelt")
    g.poly([(c + 3.3, 13.7), (c + 3.52, 13.95), (c + 3.3, 14.2), (c + 3.08, 13.95)], "red", lw=0.7)
    g.stroke([(c + 3.3, 14.37), (c + 3.3, 14.85)], lw=2.6, colour="golddark")
    g.stroke([(c + 3.1, 14.65), (c + 3.5, 14.65)], lw=2.6, colour="golddark")
    g.circle(c + 3.3, 9.4, 0.4, "eyewhite")
    # the ermine capelet
    g.blob([(c - 2.6, 10.4), (c - 2.45, 9.0), (c - 1.0, 8.55), (c, 8.85), (c + 1.0, 8.55), (c + 2.45, 9.0), (c + 2.6, 10.4),
            (c + 1.2, 10.95), (c - 1.2, 10.95)], "ermine", tension=0.5)
    for x, y in ((-2.0, 9.6), (-1.2, 9.2), (-0.4, 9.7), (0.5, 9.3), (1.3, 9.65), (2.0, 9.4), (-1.7, 10.3), (1.7, 10.3)):
        ermine_spot(g, c + x, y, 0.13)
    g.rect(c - 0.42, 10.3, c + 0.42, 10.95, "skinC")
    for k in range(9):
        a = math.radians(200 + k * 17.5)
        g.circle(c + 0.62 * math.cos(a), 10.95 + 0.5 * math.sin(a), 0.09, "eyewhite", lw=0.5)
    # head, hair, crown
    for x, y in ((-1.3, 11.0), (-1.45, 11.7), (-1.4, 12.4), (-1.15, 12.95), (1.3, 11.0), (1.45, 11.7), (1.4, 12.4), (1.15, 12.95)):
        g.circle(c + x, y, 0.42, "hairq")
    g.ellipse(c, 11.95, 1.2, 1.45, "skinC")
    for s in (-1, 1):
        stare(g, c + s * 0.46, 12.2, 0.31)
        for k in range(3):
            g.stroke([(c + s * (0.3 + k * 0.15), 12.48), (c + s * (0.28 + k * 0.2), 12.68)], lw=1.2)
        g.stroke([(c + s * 0.2, 12.75), (c + s * 0.55, 12.9), (c + s * 0.85, 12.72)], lw=1.4, smooth=True)
        g.circle(c + s * 0.85, 11.5, 0.24, "blush", outline=False, opts=", opacity=0.7")
    g.stroke([(c, 11.95), (c - 0.1, 11.65), (c + 0.07, 11.6)], lw=1.0)
    grin(g, c, 11.35, 1.95, 0.55, lip="red")
    with g.frame(c, 12.75, 0.86):
        crown(g)


def ermine_spot(g, x, y, r):
    g.blob([(x, y + r * 1.4), (x + r * 0.6, y), (x, y - r * 0.6), (x - r * 0.6, y)], "ink", tension=0.6, outline=False)


MONSTERS = {"headliner": headliner, "champion": champion, "buttons": buttons, "host": host, "crooner": crooner, "queen": queen}
MONSTER_OF = {"guitar": "headliner", "trophy": "champion", "teddy": "buttons", "toaster": "host", "microphone": "crooner", "crown": "queen"}


def standee(fig, who, x=None, y=None, s=1.0, ink=2.6, border_cm=BORDER_CM, stand=False):
    """Draw monster `who` as a cardboard standee into fig: the white border
    and cut edge round its silhouette, then the character. With `stand`,
    the cardboard strut behind it and a ground plate at its feet, as a
    standee stands in a shop. (x, y) is the middle of its feet."""
    x = CX if x is None else x
    y = 0.0 if y is None else y
    T = (s, 0, 0, s, x - CX * s, y)
    if stand:
        h = 15.36 * s
        strut = [(x + 0.8 * s, y + 0.45 * h), (x + 1.4 * s, y + 0.47 * h), (x + 4.6 * s, y - 0.2 * s), (x + 3.6 * s, y - 0.3 * s)]
        fig.poly(strut, "cardboarddark")
        for k in range(1, 4):
            t = k / 4
            fig.stroke([(x + (0.8 + 2.8 * t) * s, y + 0.45 * h * (1 - t)), (x + (1.4 + 3.2 * t) * s, y + 0.47 * h * (1 - t))], lw=0.6)
        fig.poly([(x - 3.6 * s, y - 0.5 * s), (x + 3.6 * s, y - 0.5 * s), (x + 3.2 * s, y + 0.15 * s), (x - 3.2 * s, y + 0.15 * s)], "cardboard")
    ch = Fig(ink=ink, T=T)
    MONSTERS[who](ch)
    b = border_cm * s / PT
    fig.raw(ch.silhouette(b, max(0.6, 0.9 * s)))
    fig.raw(ch.render())


# ---- the city --------------------------------------------------------------

def sky(g, W, H, low="skylow"):
    g.raw(f"\\fill[sky] (0,0) rectangle ({f(W)},{f(H)});\n")
    g.raw(f"\\fill[skyhi] (0,{f(H * 0.8)}) rectangle ({f(W)},{f(H)});\n")
    g.raw(f"\\fill[{low}] (0,0) rectangle ({f(W)},{f(H * 0.32)});\n")


def cloud(g, x, y, s):
    """Puffs, outlined, as the clouds over Sam_Comm_final's city."""
    for dx, dy, r in [(-1.0, 0, 0.55), (-0.45, 0.25, 0.7), (0.2, 0.35, 0.8), (0.85, 0.15, 0.6), (1.35, -0.05, 0.45)]:
        g.circle(x + dx * s, y + dy * s, r * s, "cloud")
    g.rect(x - 1.0 * s, y - 0.15 * s, x + 1.35 * s, y + 0.05 * s, "cloud", outline=False)
    g.stroke([(x - 1.25 * s, y - 0.15 * s), (x + 1.6 * s, y - 0.15 * s)], lw=1.1)


def roof(g, rng, x, y, w):
    for _ in range(rng.randint(1, 3)):
        kind = rng.choice(["ac", "ac", "tank", "antenna", "billboard"])
        px = x + rng.uniform(0.2, max(0.3, w - 1.2))
        if kind == "ac":
            g.rect(px, y, px + 0.6, y + 0.4, "metal", lw=0.8)
            g.circle(px + 0.3, y + 0.2, 0.13, "metaldark", lw=0.5)
        elif kind == "tank":
            for lx in (0.08, 0.62):
                g.stroke([(px + lx, y), (px + lx, y + 0.5)], lw=0.9)
            g.rect(px, y + 0.5, px + 0.7, y + 1.15, "wood", lw=0.8)
            g.poly([(px - 0.05, y + 1.15), (px + 0.75, y + 1.15), (px + 0.35, y + 1.45)], "wallE", lw=0.8)
        elif kind == "antenna":
            top = y + rng.uniform(1.0, 2.2)
            g.stroke([(px + 0.3, y), (px + 0.3, top)], lw=0.9)
            for k in range(1, 4):
                yy = y + (top - y) * k / 4
                g.stroke([(px + 0.05, yy), (px + 0.55, yy)], lw=0.7)
        else:
            col = rng.choice(SIGNS)
            g.stroke([(px + 0.2, y), (px + 0.2, y + 0.6)], lw=1)
            g.stroke([(px + 1.2, y), (px + 1.2, y + 0.6)], lw=1)
            g.rect(px, y + 0.6, px + 1.4, y + 1.3, col)
            g.stroke([(px + 0.25, y + 0.95), (px + 0.55, y + 1.1), (px + 0.85, y + 0.85), (px + 1.15, y + 1.05)], lw=1.5, colour="paper")


def building(g, rng, x, y0, w, h, wall=None, detail=True, lit_pct=45, broken=None):
    """A block with windows and rooftop clutter. `broken` is a list of
    points along a jagged top: the block is cut there, its top torn off."""
    wall = wall or rng.choice(WALLS)
    if broken:
        clip = [(x - 0.2, y0 - 0.1), (x + w + 0.2, y0 - 0.1)] + [(px, py) for px, py in reversed(broken)]
        g.raw("\\begin{scope}\\clip " + " -- ".join(f"({f(px)},{f(py)})" for px, py in clip) + " -- cycle;\n")
    g.rect(x, y0, x + w, y0 + h, wall)
    if not broken:
        g.rect(x - 0.08, y0 + h, x + w + 0.08, y0 + h + 0.22, wall)
    cols = max(1, int((w - 0.3) / 0.55))
    rows = max(1, int((h - 0.6) / 0.75))
    gap = (w - cols * 0.32) / (cols + 1)
    for i in range(cols):
        for j in range(rows):
            wx = x + gap + i * (0.32 + gap)
            wy = y0 + 0.45 + j * 0.75
            if wy + 0.42 > y0 + h - 0.1:
                continue
            fill = "window" if rng.random() * 100 < lit_pct else "windark"
            g.rect(wx, wy, wx + 0.32, wy + 0.42, fill, lw=0.55)
            if detail and rng.random() < 0.06:
                g.rect(wx - 0.05, wy - 0.28, wx + 0.37, wy - 0.02, "metal", lw=0.6)
    if broken:
        g.raw("\\end{scope}\n")
        g.stroke(broken, lw=1.3)
        g.stroke([(x, y0), (x, max(py for px, py in broken if px <= x + 0.3) if any(px <= x + 0.3 for px, py in broken) else y0 + h)], lw=1.1)
        return
    if detail:
        roof(g, rng, x, y0 + h + 0.22, w)
        if rng.random() < 0.35:
            vx = x + rng.uniform(0.2, w - 0.5)
            g.poly([(vx, y0 + h + 0.1), (vx + 0.35, y0 + h + 0.1), (vx + 0.25, y0 + h - rng.uniform(1.5, 4)), (vx + 0.05, y0 + h - rng.uniform(1, 3))], rng.choice(GREENS), lw=0.8)


def broken_top(rng, x, w, y, depth=1.2, n=7):
    pts = []
    for i in range(n + 1):
        px = x + w * i / n
        pts.append((px, y - rng.uniform(0, depth) if 0 < i < n else y - rng.uniform(0, depth * 0.5)))
    return pts


def tree(g, rng, x, y, s):
    g.rect(x - 0.08 * s, y, x + 0.08 * s, y + 0.9 * s, "wood", lw=0.8)
    for dx, dy, r in [(-0.3, 1.0, 0.45), (0.25, 1.1, 0.5), (0, 1.45, 0.45)]:
        g.circle(x + dx * s, y + dy * s, r * s, rng.choice(GREENS))


def lamp_post(g, x, y, h):
    g.stroke([(x, y), (x, y + h), (x + 0.5, y + h + 0.2)], lw=2.2, colour="metaldark")
    g.poly([(x + 0.35, y + h + 0.1), (x + 0.85, y + h + 0.25), (x + 0.75, y + h - 0.05)], "lamp", lw=0.8)


def debris(g, rng, W, n, box, sizes=(0.15, 0.5), streak=True):
    """Chunks thrown through the air. Never into the left third, where the
    page sets its words."""
    x0, y0, x1, y1 = box
    if x0 < W / 3:
        raise SystemExit(f"debris box {box} reaches into the calm left third")
    for _ in range(n):
        x, y = rng.uniform(x0, x1), rng.uniform(y0, y1)
        s = rng.uniform(*sizes)
        kind = rng.random()
        rot = rng.uniform(0, 360)
        with g.frame(x, y, s, rot):
            if kind < 0.45:
                k = rng.randint(4, 6)
                pts = [(math.cos(2 * math.pi * i / k + rng.uniform(-0.3, 0.3)) * rng.uniform(0.6, 1.0),
                        math.sin(2 * math.pi * i / k + rng.uniform(-0.3, 0.3)) * rng.uniform(0.6, 1.0)) for i in range(k)]
                g.poly(pts, rng.choice(WALLS + ["stone", "paving", "road"]), lw=0.9)
            elif kind < 0.65:
                g.rect(-1.4, -0.25, 1.4, 0.25, "wood", lw=0.9)
                g.stroke([(-1.1, 0), (1.1, 0)], lw=0.5)
            elif kind < 0.8:
                g.rect(-0.8, -0.45, 0.8, 0.45, rng.choice(["wallE", "wallA", "shingle"]), lw=0.9)
            elif kind < 0.9:
                g.rect(-0.6, -0.6, 0.6, 0.6, "windark", lw=0.9)
                g.rect(-0.45, -0.45, 0.45, 0.45, "window", lw=0.6)
            else:
                g.rect(-0.9, -0.35, 0.9, 0.35, "metal", lw=0.9)
                g.circle(0, 0, 0.2, "metaldark", lw=0.5)
        if streak and s > 0.3 and rng.random() < 0.5:
            for d in (-0.15, 0.15):
                g.stroke([(x - 0.4 - s, y - 0.5 - s + d), (x - 0.2 - s * 0.6, y - 0.25 - s * 0.6 + d)], lw=0.9)


def puff(g, x, y, r, fill="smoke"):
    for dx, dy, k in ((-0.6, 0, 0.7), (0, 0.3, 0.9), (0.6, 0.05, 0.75), (0.2, -0.25, 0.6)):
        g.circle(x + dx * r, y + dy * r, k * r, fill)


def car_front(g):
    """The player's little car, from the front: bottom centre at the origin,
    1 wide."""
    for s in (-1, 1):
        g.rect(s * 0.47, -0.02, s * 0.32, 0.17, "tyre")
    g.poly([(-0.52, 0.1), (0.52, 0.1), (0.54, 0.36), (0.44, 0.47), (-0.44, 0.47), (-0.54, 0.36)], "car")
    g.poly([(-0.39, 0.46), (0.39, 0.46), (0.3, 0.74), (-0.3, 0.74)], "car")
    g.poly([(-0.34, 0.48), (0.34, 0.48), (0.27, 0.7), (-0.27, 0.7)], "glass")
    g.circle(-0.1, 0.6, 0.065, "hairdark", lw=0.6)
    g.stroke([(-0.2, 0.52), (0.0, 0.52)], lw=2.5, colour="hairdark")
    g.stroke([(0.12, 0.66), (0.24, 0.53)], lw=2.2, colour="white", opts=", opacity=0.7")
    for s in (-1, 1):
        g.rect(s * 0.4, 0.5, s * 0.5, 0.56, "cardark", lw=0.6)
        g.circle(s * 0.39, 0.29, 0.16, "lamp", outline=False, opts=", opacity=0.5")
        g.circle(s * 0.39, 0.29, 0.075, "lamp")
    g.rect(-0.2, 0.18, 0.2, 0.33, "metaldark")
    for k in range(3):
        g.stroke([(-0.17, 0.215 + k * 0.045), (0.17, 0.215 + k * 0.045)], lw=0.5, colour="metal")
    g.rect(-0.55, 0.06, 0.55, 0.14, "chrome")


def car_side(g):
    """The car side on, nose to the right: bottom centre at the origin, 1 long."""
    g.poly([(-0.5, 0.1), (0.5, 0.1), (0.52, 0.27), (0.33, 0.32), (0.18, 0.5), (-0.22, 0.5), (-0.36, 0.32), (-0.5, 0.3)], "car")
    g.poly([(-0.3, 0.32), (-0.2, 0.46), (-0.03, 0.46), (-0.03, 0.32)], "glass", lw=0.8)
    g.poly([(0.01, 0.32), (0.01, 0.46), (0.16, 0.46), (0.27, 0.32)], "glass", lw=0.8)
    g.stroke([(-0.45, 0.2), (0.45, 0.2)], lw=1.0, colour="cardark")
    g.rect(0.46, 0.2, 0.52, 0.25, "lamp", lw=0.6)
    for x in (-0.3, 0.3):
        g.circle(x, 0.1, 0.11, "tyre")
        g.circle(x, 0.1, 0.05, "chrome", lw=0.6)


def paper_panel(g, pts, fill, edge=(0.0, -0.13)):
    """A wall of the paper house: the panel and, under it, its cardboard edge."""
    g.poly([(x + edge[0], y + edge[1]) for x, y in pts], "paperdark")
    g.poly(pts, fill)


def falling_house(g, x, y):
    """The house as it falls away flat like a paper diorama: the floor with
    its furniture, four walls hinged at the base and dropping outward, the
    roof flung up. (x, y) is the middle of the floor's front edge."""
    # roof pieces and chimney, flung
    with g.frame(x - 4.7, y + 4.4, 0.9, 28):
        paper_panel(g, [(-1.6, 0), (1.5, 0), (0.9, 1.2), (-1.0, 1.2)], "shingle")
        for k in range(1, 3):
            g.stroke([(-1.6 + k * 0.25, k * 0.4), (1.5 - k * 0.2, k * 0.4)], lw=0.8)
    with g.frame(x + 4.9, y + 4.9, 0.9, -32):
        paper_panel(g, [(-1.5, 0), (1.6, 0), (1.0, 1.2), (-0.9, 1.2)], "shingle")
        for k in range(1, 3):
            g.stroke([(-1.5 + k * 0.2, k * 0.4), (1.6 - k * 0.25, k * 0.4)], lw=0.8)
    with g.frame(x + 3.3, y + 6.6, 0.9, 35):
        g.rect(-0.25, -0.4, 0.25, 0.4, "wallE")
        g.rect(-0.32, 0.4, 0.32, 0.55, "wallE")
    for s in (-1, 1):
        for k in range(3):
            g.stroke([(x + s * (3.0 + k * 0.3), y + 2.6 + k * 0.35), (x + s * (3.6 + k * 0.3), y + 3.5 + k * 0.35)], lw=1.0)
    # the back wall, tipping away
    paper_panel(g, [(x - 2.0, y + 1.0), (x + 2.0, y + 1.0), (x + 2.25, y + 2.4), (x - 2.25, y + 2.4)], "greenA")
    g.rect(x - 1.3, y + 1.4, x - 0.5, y + 2.05, "glass", lw=0.8)
    g.stroke([(x - 0.9, y + 1.4), (x - 0.9, y + 2.05)], lw=0.8)
    g.rect(x + 0.6, y + 1.5, x + 1.3, y + 2.0, "wallD", lw=0.8)
    # the floor and the room
    g.poly([(x - 2.6, y), (x + 2.6, y), (x + 2.0, y + 1.0), (x - 2.0, y + 1.0)], "wood")
    for k in range(1, 5):
        t = k / 5
        g.stroke([(x - 2.6 + 0.6 * t, y + t), (x + 2.6 - 0.6 * t, y + t)], lw=0.5, colour="crust")
    g.ellipse(x - 0.2, y + 0.45, 1.3, 0.3, "wallC", lw=0.8)
    g.rect(x - 1.8, y + 0.4, x - 0.9, y + 1.0, "patch")
    g.rect(x - 1.85, y + 0.4, x - 1.65, y + 1.15, "patch", lw=0.8)
    g.rect(x + 0.9, y + 0.55, x + 1.5, y + 0.6, "wood", lw=0.8)
    g.stroke([(x + 1.0, y + 0.55), (x + 1.0, y + 0.25)], lw=1.0)
    g.stroke([(x + 1.4, y + 0.55), (x + 1.4, y + 0.25)], lw=1.0)
    g.circle(x + 1.2, y + 0.75, 0.16, "fur", lw=0.7)
    g.circle(x + 1.2, y + 0.95, 0.12, "fur", lw=0.7)
    # the side walls, dropping outward
    for s in (-1, 1):
        pts = [(x + s * 2.6, y), (x + s * 2.0, y + 1.0), (x + s * 3.6, y + 2.3), (x + s * 4.4, y + 1.2)]
        paper_panel(g, pts, "wallB" if s < 0 else "wallF", edge=(s * 0.12, -0.08))
        g.poly([(x + s * 2.95, y + 0.75), (x + s * 2.7, y + 1.1), (x + s * 3.3, y + 1.6), (x + s * 3.55, y + 1.2)], "glass", lw=0.8)
    # the front wall, flat on the ground
    front = [(x - 2.6, y), (x + 2.6, y), (x + 3.1, y - 1.1), (x - 3.1, y - 1.1)]
    paper_panel(g, front, "wallD", edge=(0, -0.15))
    g.poly([(x - 0.35, y - 0.05), (x + 0.35, y - 0.05), (x + 0.4, y - 0.95), (x - 0.4, y - 0.95)], "wallE", lw=0.8)
    for s in (-1, 1):
        g.poly([(x + s * 1.2, y - 0.25), (x + s * 2.0, y - 0.25), (x + s * 2.1, y - 0.8), (x + s * 1.25, y - 0.8)], "glass", lw=0.8)


# ---- pictures --------------------------------------------------------------

def title():
    """The city in Sam_Comm_final's light. In the middle a small house falls
    open flat like a paper diorama; behind it Mr. Buttons, a flat cardboard
    standee taller than the towers, rises on his strut; the player's car
    races toward us down a cracked road. The left third stays calm for the
    title."""
    W, H = 32.0, 18.0
    rng = random.Random(1984)
    g = Fig(ink=1.1)
    sky(g, W, H)
    for cx, cy, cs in [(3.5, 15.6, 1.3), (8.6, 13.4, 0.85), (13.0, 16.4, 1.0), (27.2, 16.0, 1.4), (30.8, 12.8, 0.9)]:
        cloud(g, cx, cy, cs)
    # the far skyline, low and quiet in the left third
    x = -0.5
    while x < W:
        w = rng.uniform(1.4, 2.5)
        hmax = 4.0 if x < W / 3 else 6.5
        building(g, rng, x, 5.0, w, rng.uniform(2.2, hmax), detail=x > W / 3 - 1, lit_pct=35)
        x += w + rng.uniform(-0.15, 0.2)
    standee(g, "buttons", x=21.0, y=5.0, s=0.82, ink=1.5, stand=True)
    # towers either side of it, one torn open
    building(g, rng, 25.3, 5.0, 2.8, 6.2, wall="wallE", lit_pct=40)
    building(g, rng, 28.3, 5.0, 2.4, 4.6, wall="wallB", lit_pct=50)
    building(g, rng, 30.8, 5.0, 2.0, 7.2, wall="wallC", lit_pct=40)
    building(g, rng, 14.6, 5.0, 2.6, 7.6, wall="wallA", lit_pct=40, broken=broken_top(rng, 14.6, 2.6, 12.6, depth=1.6))
    building(g, rng, 11.4, 5.0, 2.9, 5.0, wall="wallF", lit_pct=40)
    building(g, rng, 6.0, 5.0, 2.6, 4.6, wall="wallD", detail=False, lit_pct=30)
    building(g, rng, 0.6, 5.0, 3.0, 5.4, wall="wallE", detail=False, lit_pct=30)
    # the ground: paving, the road running from the house to us
    g.raw("\\filldraw[draw=ink, line width=1.1pt, fill=paving] (-0.5,-0.5) rectangle (32.5,5.0);\n")
    g.raw("\\fill[ground] (-0.5,4.6) rectangle (32.5,5.0);\n")
    g.poly([(19.0, 4.6), (23.0, 4.6), (31.0, -0.5), (11.0, -0.5)], "road")
    for k in range(7):
        t0, t1 = k / 7, k / 7 + 0.07
        y0, y1 = 4.4 - t0 * 4.9, 4.4 - t1 * 4.9
        g.poly([(21 - 0.04 - 0.2 * t0, y0), (21 + 0.04 + 0.2 * t0, y0), (21 + 0.04 + 0.2 * t1, y1), (21 - 0.04 - 0.2 * t1, y1)], "roadline", lw=0.6)
    for pts in ([(13.5, 0.4), (14.2, 1.0), (14.0, 1.6), (15.1, 2.2), (15.0, 2.7)], [(27.5, 0.2), (26.8, 0.9), (27.2, 1.5), (26.1, 2.1)],
                [(17.0, 3.6), (17.6, 3.2), (18.4, 3.4)], [(24.5, 3.5), (25.1, 3.0), (24.8, 2.5), (25.6, 2.0)]):
        g.stroke(pts, lw=1.3)
    for (cx, cy, rot) in [(15.4, 1.3, 14), (26.6, 2.6, -18), (13.0, 0.2, -8)]:
        with g.frame(cx, cy, 1.0, rot):
            g.poly([(-0.8, -0.1), (0.7, -0.15), (0.8, 0.35), (-0.7, 0.4)], "road")
            g.poly([(-0.8, -0.1), (0.7, -0.15), (0.72, -0.35), (-0.78, -0.3)], "crust", lw=0.8)
    for tx in (2.0, 7.5):
        tree(g, rng, tx, 4.7, 1.3)
    lamp_post(g, 9.6, 4.7, 2.6)
    falling_house(g, 21.0, 4.0)
    # the car, dust behind it
    for (px, py, r) in [(19.3, 1.9, 0.45), (22.8, 2.0, 0.5), (19.9, 2.5, 0.32), (22.2, 2.6, 0.36)]:
        puff(g, px, py, r)
    with g.frame(21.0, 0.3, 3.2):
        car_front(g)
    for s in (-1, 1):
        for k in range(3):
            g.stroke([(21 + s * (2.0 + k * 0.2), 0.6 + k * 0.45), (21 + s * (2.8 + k * 0.3), 0.4 + k * 0.45)], lw=1.4)
    debris(g, rng, W, 26, (13.0, 6.0, 29.0, 13.0))
    debris(g, rng, W, 14, (13.5, 12.0, 18.5, 16.5), sizes=(0.15, 0.4))
    debris(g, rng, W, 10, (14.0, 1.5, 28.5, 4.0), sizes=(0.1, 0.28), streak=False)
    return g, "scene"


def caught():
    """Game over, from the gutter: the car upside down in the rubble, the
    standee's legs and belt filling the sky behind, debris still falling.
    The left third stays calm for the words."""
    W, H = 32.0, 18.0
    rng = random.Random(404)
    g = Fig(ink=1.1)
    sky(g, W, H)
    for cx, cy, cs in [(4.0, 15.2, 1.2), (9.0, 16.6, 0.8), (29.5, 16.6, 1.0)]:
        cloud(g, cx, cy, cs)
    x = -0.5
    while x < W:
        w = rng.uniform(1.5, 2.6)
        h = rng.uniform(2.0, 3.6) if x < W / 3 else rng.uniform(3.5, 8.0)
        broken = None if x < W / 3 + 1 else broken_top(rng, x, w, 3.6 + h, depth=1.5)
        building(g, rng, x, 3.6, w, h, detail=False, lit_pct=30, broken=broken)
        x += w + rng.uniform(-0.1, 0.2)
    for (px, py, r) in [(14.2, 9.0, 1.0), (29.0, 10.5, 1.2), (31.0, 8.0, 0.9)]:
        puff(g, px, py, r)
    standee(g, "champion", x=21.5, y=3.6, s=1.95, ink=2.2, stand=True)
    building(g, rng, 29.6, 3.6, 3.0, 9.0, wall="wallE", lit_pct=30, broken=broken_top(rng, 29.6, 3.0, 12.6, depth=2.0))
    # the street, broken into slabs
    g.raw("\\filldraw[draw=ink, line width=1.1pt, fill=road] (-0.5,-0.5) rectangle (32.5,3.6);\n")
    g.raw("\\fill[paving] (-0.5,3.2) rectangle (32.5,3.6);\n")
    for k in range(9):
        g.rect(0.6 + k * 1.2, 1.6, 1.3 + k * 1.2, 1.75, "roadline", lw=0.5)
    for (cx, cy, w, h, rot) in [(13.5, 2.2, 2.2, 0.7, 12), (16.0, 0.6, 3.0, 0.9, -9), (25.5, 2.6, 2.4, 0.8, -15), (28.5, 0.9, 3.2, 1.0, 10), (22.6, 0.25, 2.0, 0.7, 6)]:
        with g.frame(cx, cy, 1.0, rot):
            g.poly([(-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, -h / 2 - 0.3), (-w / 2, -h / 2 - 0.3)], "crust", lw=0.9)
            g.poly([(-w / 2, -h / 2), (w / 2, -h / 2), (w / 2 - 0.1, h / 2), (-w / 2 + 0.15, h / 2)], "stone")
            g.stroke([(-w / 4, -h / 2), (0, 0), (w / 5, h / 2)], lw=0.9)
    for pts in ([(11.5, 3.0), (12.4, 2.4), (12.2, 1.6), (13.2, 0.8)], [(30.5, 3.2), (31.2, 2.4), (30.8, 1.5)], [(18.0, 3.4), (18.8, 2.9), (19.9, 3.1)]):
        g.stroke(pts, lw=1.3)
    lamp_post(g, 3.0, 2.8, 4.0)
    tree(g, rng, 6.8, 2.9, 1.5)
    debris(g, rng, W, 18, (12.0, 0.2, 31.0, 3.4), sizes=(0.12, 0.35), streak=False)
    # the car, on its roof
    for (px, py, r) in [(17.0, 3.2, 0.55), (16.4, 4.2, 0.75), (17.1, 5.4, 0.9), (16.2, 6.8, 1.0)]:
        puff(g, px, py, r)
    with g.frame(19.2, 3.75, 5.6, 176):
        car_side(g)
        wheels = [g.pt(-0.3, 0.1), g.pt(0.3, 0.1)]
    for wx, wy in wheels:
        for k in range(3):
            a = math.radians(20 + k * 55)
            g.stroke([(wx + 0.85 * math.cos(a), wy + 0.85 * math.sin(a)), (wx + 1.15 * math.cos(a + 0.45), wy + 1.15 * math.sin(a + 0.45))], lw=1.4)
    with g.frame(24.4, 1.0, 0.6, 20):
        g.circle(0, 0, 0.5, "chrome")
        g.circle(0, 0, 0.2, "chromedark", lw=0.7)
    debris(g, rng, W, 34, (11.5, 4.5, 31.5, 17.0), sizes=(0.15, 0.55))
    return g, "scene"


def monster_picture(who):
    g = Fig()
    standee(g, who, x=CX, y=0.0, s=1.0, ink=2.6)
    return g, "monsters"


def item_picture(item):
    """An item as the player finds it in the house: one object, centred."""
    g = Fig(ink=1.6)
    if item == "guitar":
        with g.frame(1.26, 1.18, 0.5, -42):
            guitar(g)
    elif item == "trophy":
        g.rect(1.0, 0.3, 3.0, 0.75, "wood")
        g.poly([(1.25, 0.75), (2.75, 0.75), (2.45, 1.15), (1.55, 1.15)], "goldbelt")
        g.rect(1.85, 1.15, 2.15, 1.9, "goldbelt")
        g.ellipse(2.0, 1.95, 0.35, 0.14, "golddark")
        for s in (-1, 1):
            g.stroke([(2 + s * 0.85, 3.25), (2 + s * 1.35, 3.1), (2 + s * 1.3, 2.5), (2 + s * 0.7, 2.25)], lw=5.4, smooth=True, colour="ink")
            g.stroke([(2 + s * 0.85, 3.25), (2 + s * 1.35, 3.1), (2 + s * 1.3, 2.5), (2 + s * 0.7, 2.25)], lw=2.6, smooth=True, colour="goldbelt")
        g.blob([(0.9, 3.55), (3.1, 3.55), (2.95, 2.8), (2.45, 2.1), (2.0, 1.95), (1.55, 2.1), (1.05, 2.8)], "goldbelt", tension=0.45)
        g.ellipse(2.0, 3.55, 1.1, 0.18, "golddark")
        star(g, 2.0, 2.75, 0.45, "golddark")
        g.circle(2.0, 2.75, 0.12, "red", lw=0.7)
        g.stroke([(1.3, 3.2), (1.45, 2.6)], lw=2.2, colour="white", opts=", opacity=0.7")
    elif item == "teddy":
        with g.frame(2.0 - CX * 0.245, 0.2, 0.245):
            buttons(g)
    elif item == "toaster":
        with g.frame(1.55, 2.3, 0.8, -6):
            toast(g)
        with g.frame(2.45, 2.4, 0.8, 7):
            toast(g)
        g.blob([(0.55, 0.65), (3.45, 0.65), (3.5, 2.5), (3.2, 2.75), (0.8, 2.75), (0.5, 2.5)], "chrome", tension=0.3)
        g.rect(1.05, 2.5, 2.05, 2.62, "hairdark", lw=0.6)
        g.rect(1.95, 2.5, 2.95, 2.62, "hairdark", lw=0.6)
        g.rect(0.55, 1.0, 3.45, 1.15, "chromedark", lw=0.6)
        g.stroke([(0.8, 2.3), (0.9, 1.5)], lw=2.6, colour="white", opts=", opacity=0.7")
        g.rect(3.45, 1.7, 3.75, 1.9, "hairdark")
        g.circle(2.9, 1.65, 0.2, "red", lw=0.8)
        for x in (0.8, 3.2):
            g.rect(x - 0.18, 0.45, x + 0.18, 0.65, "hairdark", lw=0.7)
    elif item == "microphone":
        g.ellipse(2.0, 0.55, 1.0, 0.25, "metaldark")
        g.rect(1.92, 0.55, 2.08, 1.3, "chromedark", lw=0.8)
        with g.frame(2.0, 2.35, 1.05):
            ribbon_mic(g)
    elif item == "crown":
        with g.frame(2.0, 0.8, 1.0):
            crown(g)
    return g, "items"


PICTURES = {"title": title, "caught": caught}
for _who in MONSTERS:
    PICTURES[f"monsters/{_who}"] = (lambda w=_who: monster_picture(w))
for _item in MONSTER_OF:
    PICTURES[f"items/{_item}"] = (lambda i=_item: item_picture(i))

FORBIDDEN = ["\\node", "\\text", "\\put", "\\char", "\\pgftext", "label=", "\\Large", "\\huge", "\\LaTeX"]


def document(fig, kind):
    (W, H), _ = SIZE[kind]
    body = fig.render()
    for bad in FORBIDDEN:
        if bad in body:
            raise SystemExit(f"text-setting command {bad!r} in a picture: no words in the art")
    return (PREAMBLE + "\\begin{document}\n\\begin{tikzpicture}\n" + f"\\clip (0,0) rectangle ({f(W)},{f(H)});\n" + body + "\\end{tikzpicture}\n\\end{document}\n")


# ---- checks ----------------------------------------------------------------

def png_read(path):
    """Width, height, colour type and the RGBA rows of a PNG (stdlib only)."""
    data = Path(path).read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat = 8, b""
    while pos < len(data):
        n, = struct.unpack(">I", data[pos:pos + 4])
        tag = data[pos + 4:pos + 8]
        chunk = data[pos + 8:pos + 8 + n]
        if tag == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", chunk[:10])
        elif tag == b"IDAT":
            idat += chunk
        pos += 12 + n
    bpp = {6: 4, 2: 3}.get(ctype)
    if depth != 8 or bpp is None:
        return w, h, ctype, None
    raw = zlib.decompress(idat)
    stride = w * bpp
    rows, prev = [], bytearray(stride)
    for y in range(h):
        ft = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if ft == 1:
                line[i] = (line[i] + a) & 255
            elif ft == 2:
                line[i] = (line[i] + b) & 255
            elif ft == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return w, h, ctype, rows


def check(name, kind, png, preview):
    """Refuse a picture that breaks the facts in the docstring."""
    _, (PW, PH) = SIZE[kind]
    w, h, ctype, _ = png_read(png)
    problems = []
    if (w, h) != (PW, PH):
        problems.append(f"size {w}x{h}, wanted {PW}x{PH}")
    if Path(png).stat().st_size > 1.5e6:
        problems.append(f"{Path(png).stat().st_size} bytes, over 1.5 MB")
    if kind != "scene":
        if ctype != 6:
            problems.append("no alpha channel on a cutout")
        pw, ph, _, rows = png_read(preview)
        alpha = [[row[i * 4 + 3] for i in range(pw)] for row in rows]
        solid = [(x, y) for y in range(ph) for x in range(pw) if alpha[y][x] > 40]
        if any(alpha[y][x] for y in (0, ph - 1) for x in (0, pw - 1)) and kind == "items":
            problems.append("a corner is not transparent")
        if max(alpha[0]) > 0:
            problems.append("something touches the top edge (a head or prop is cropped)")
        if max(r[0] for r in alpha) > 0 or max(r[-1] for r in alpha) > 0:
            problems.append("something touches a side edge")
        ys = [y for _, y in solid]
        xs = [x for x, _ in solid]
        if kind == "monsters":
            if max(alpha[-1]) == 0:
                problems.append("the feet do not reach the bottom edge")
            if (max(ys) - min(ys) + 1) < 0.9 * ph:
                problems.append("the figure fills less than 90% of the height")
            if alpha[0][0] or alpha[0][-1]:
                problems.append("a top corner is not transparent")
        else:
            if max(alpha[-1]) > 0:
                problems.append("something touches the bottom edge")
            cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
            if abs(cx - pw / 2) > pw * 0.08 or abs(cy - ph / 2) > ph * 0.08:
                problems.append(f"not centred (centre at {cx:.0f},{cy:.0f} of {pw}x{ph})")
    return [f"{name}: {p}" for p in problems]


def main():
    only = set(sys.argv[1:])
    failures = []
    for name, make in PICTURES.items():
        if only and name not in only and name.split("/")[0] not in only:
            continue
        fig, kind = make()
        tex = document(fig, kind)
        _, (PW, PH) = SIZE[kind]
        clear = [] if kind == "scene" else ["-transp"]
        with tempfile.TemporaryDirectory() as tmp:
            t = Path(tmp)
            (t / "art.tex").write_text(tex)
            r = subprocess.run([PDFLATEX, "-interaction=nonstopmode", "-halt-on-error", "art.tex"], cwd=t, capture_output=True, text=True)
            if r.returncode != 0:
                print(r.stdout[-3000:])
                raise SystemExit(f"{name}: pdflatex failed")
            subprocess.run([PDFTOCAIRO, "-png", *clear, "-scale-to-x", str(PW), "-scale-to-y", str(PH), "-singlefile", str(t / "art.pdf"), str(t / "art")], check=True)
            subprocess.run([PDFTOCAIRO, "-png", *clear, "-scale-to-x", str(PW // 8), "-scale-to-y", str(PH // 8), "-singlefile", str(t / "art.pdf"), str(t / "small")], check=True)
            out = OUT / f"{name}.png"
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(t / "art.png", out)
            src = SRC / f"{name}.tex"
            src.parent.mkdir(parents=True, exist_ok=True)
            src.write_text(tex)
            # written first so a refused picture can be looked at; the run still fails
            problems = check(name, kind, out, t / "small.png")
        print("wrote", out.relative_to(ROOT), f"{out.stat().st_size // 1024} KB", "REFUSED" if problems else "")
        failures += problems
    if failures:
        raise SystemExit("\n".join(failures))


if __name__ == "__main__":
    main()
