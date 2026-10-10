#!/usr/bin/env python3
"""A pursuer's standee from a photograph, for a pursuer that is a real person
and so is not drawn in TikZ (tools/art.py draws the original cast).

    python3 tools/standee.py kanye     # tools/art/photos/kanye.png -> web/art/monsters/kanye.png

The card matches the drawn standees (tools/art.py `standee` and `check`):
1024x1536 with a transparent background, the figure standing on the bottom
edge and clear of the top and sides, a white cardboard border of 24 px with
a thin cut edge round it, and the plum ink line inside. The photograph fills
an arch-topped card; below it the card fades into the photograph's own
darkest colour, so the face looms over a dark body.

Needs Pillow (a development tool only; the game does not run it).
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
W, H = 1024, 1536
MARGIN = 14  # clear of the sides and top, as `check` asks
BORDER = 24  # the white cardboard
EDGE = 3  # the cut line round the cardboard
INK = 7  # the plum line round the picture
CUTEDGE = (0xA8, 0x99, 0x8A, 255)
WHITE = (255, 255, 255, 255)
PLUM = (0x3B, 0x23, 0x40, 255)
FADE = 420  # pixels over which the photograph fades into the dark below
GROW = 116  # percent: the photograph a little wider than the card, cropped at the sides


def arch(draw, box, fill):
    """An arch-topped card: a half circle over a rectangle down to the bottom."""
    x0, y0, x1, y1 = box
    r = (x1 - x0) / 2
    draw.ellipse((x0, y0, x1, y0 + 2 * r), fill=fill)
    draw.rectangle((x0, y0 + r, x1, y1), fill=fill)


def standee(photo):
    card = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(card)
    outer = (MARGIN, MARGIN, W - MARGIN, H)
    arch(d, outer, CUTEDGE)
    arch(d, (outer[0] + EDGE, outer[1] + EDGE, outer[2] - EDGE, H), WHITE)
    ink = (outer[0] + BORDER, outer[1] + BORDER, outer[2] - BORDER, H)
    arch(d, ink, PLUM)
    inner = (ink[0] + INK, ink[1] + INK, ink[2] - INK, H)
    iw, ih = inner[2] - inner[0], inner[3] - inner[1]

    # the picture: the photograph at the top, fading into its own dark below
    src = photo.convert("RGB")
    side = iw * GROW // 100
    face = src.resize((side, side), Image.LANCZOS).crop(((side - iw) // 2, 0, (side - iw) // 2 + iw, side))
    bottom = src.crop((0, src.height * 9 // 10, src.width, src.height)).resize((1, 1), Image.BOX).getpixel((0, 0))
    dark = tuple(int(c * 0.7) for c in bottom)
    pic = Image.new("RGB", (iw, ih), dark)
    fade = Image.linear_gradient("L").resize((iw, FADE)).transpose(Image.FLIP_TOP_BOTTOM)
    mask = Image.new("L", (iw, side), 255)
    mask.paste(fade, (0, side - FADE))
    pic.paste(face, (0, 0), mask)
    pic = pic.filter(ImageFilter.UnsharpMask(radius=2, percent=60))

    shape = Image.new("L", (W, H), 0)
    arch(ImageDraw.Draw(shape), inner, 255)
    card.paste(pic, (inner[0], inner[1]), shape.crop((inner[0], inner[1], inner[0] + iw, inner[1] + ih)))
    return card


def main():
    for who in sys.argv[1:] or ["kanye"]:
        src = ROOT / "tools" / "art" / "photos" / f"{who}.png"
        out = ROOT / "web" / "art" / "monsters" / f"{who}.png"
        standee(Image.open(src)).save(out, optimize=True)
        print("wrote", out.relative_to(ROOT), f"{out.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
