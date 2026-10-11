"""Cutting filmed takes (film.py) into a vertical short, after Vagrancy's
make_short.py and the short-form notes it follows: open on the action, a
cut or a beat every one to three seconds, slow motion and a punch-in on the
big moments, captions burned in for viewers with the sound off, a hit on
each impact, and an end that loops into the start.

Each frame's notes (set by the scene's script as it films) say what the edit
does to it:

- `zoom`: [x, y, scale]: a punch-in centred on (x, y) in CSS pixels
- `shake`: 0..1, the frame jolted by up to 3% of its width
- `flash`: 0..1, a white flash over it
- `inset`: [x0, y0, x1, y1]: that part of the frame (CSS pixels) blown up
  across its middle, over a dimmed copy: the rear-view mirror's close-up
- `dim`: 0..1, the frame darkened, under an end card
- `caption`: [text, style] burned in: style `hook` (big, high), `beat`
  (big, middle), `low` (big, over the dashboard, clear of what is ahead),
  `small` (a smaller line under it), `under` (under a mirror close-up),
  `end` (the end card), `tag` (the small line at the top)
- `hit`: an impact on the sound track on this frame (`soft` for a lighter one)
- `whoosh`: a rush of air on this frame

The thumbs (where the film pressed the touch controls) are drawn as
translucent fingertips, so the short shows a phone being played.

The sound is the game's chase song from where it starts in the game
(data/music.json), with the hits and rushes synthesized over it.
"""
import json, math, random, subprocess, wave
from pathlib import Path

import imageio_ffmpeg
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

from film import FPS, ROOT, SCALE

W, H = 1080, 1920
BOLD = "/System/Library/Fonts/Supplemental/Arial Black.ttf"
PLAIN = "/System/Library/Fonts/Supplemental/Trebuchet MS Bold.ttf"
INK = (59, 35, 64)
PAPER = (255, 248, 234)
GOLD = (242, 181, 58)
RATE = 44100


def _font(path, size):
    return ImageFont.truetype(path, size)


def _wrap(draw, text, font, width):
    lines, line = [], ""
    for word in text.split():
        trial = f"{line} {word}".strip()
        if draw.textlength(trial, font=font) <= width or not line:
            line = trial
        else:
            lines.append(line)
            line = word
    lines.append(line)
    return lines


def caption(img, text, style, age):
    """A caption in the TikTok manner: heavy white words with a thick ink
    outline, popping in over its first few frames."""
    d = ImageDraw.Draw(img)
    size, y, fill = {"hook": (92, 0.30, PAPER), "beat": (104, 0.44, GOLD), "small": (54, 0.53, PAPER),
                     "end": (78, 0.36, PAPER), "tag": (40, 0.035, PAPER), "under": (54, 0.68, PAPER), "low": (100, 0.62, GOLD)}[style]
    pop = min(1.0, age / 4)
    size = int(size * (0.75 + 0.25 * pop + 0.12 * math.sin(pop * math.pi)))
    font = _font(BOLD, size)
    lines = _wrap(d, text.upper() if style != "tag" else text, font, W * 0.86)
    top = H * y - (len(lines) * size * 1.12) / 2
    for k, line in enumerate(lines):
        x = (W - d.textlength(line, font=font)) / 2
        d.text((x, top + k * size * 1.12), line, font=font, fill=fill, stroke_width=max(4, size // 9), stroke_fill=INK)


def compose(frame, rnd):
    img = Image.open(frame["image"]).convert("RGB")
    fw, fh = img.size
    x, y, z = frame.get("zoom") or (fw / SCALE / 2, fh / SCALE / 2, 1.0)
    z = max(1.0, z)
    cw, ch = fw / z, fh / z
    s = frame.get("shake", 0)
    jx = rnd.uniform(-1, 1) * s * 0.03 * cw
    jy = rnd.uniform(-1, 1) * s * 0.03 * cw
    # a shaken frame is cropped in a little so its edges never show
    if s:
        cw, ch = cw * 0.94, ch * 0.94
    left = min(max(x * SCALE - cw / 2 + jx, 0), fw - cw)
    top = min(max(y * SCALE - ch / 2 + jy, 0), fh - ch)
    img = img.crop((int(left), int(top), int(left + cw), int(top + ch))).resize((W, H), Image.LANCZOS)
    k = W / cw
    # the fingertips
    if frame.get("thumbs"):
        over = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        d = ImageDraw.Draw(over)
        r = 26 * SCALE * k / 4 * 1.3
        for tx, ty in frame["thumbs"]:
            cx, cy = (tx * SCALE - left) * k, (ty * SCALE - top) * k
            d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(255, 255, 255, 110), outline=(255, 255, 255, 200), width=5)
        img = Image.alpha_composite(img.convert("RGBA"), over).convert("RGB")
    if frame.get("inset"):
        img = inset(Image.open(frame["image"]).convert("RGB"), img, frame["inset"])
    if frame.get("dim"):
        img = Image.blend(img, Image.new("RGB", (W, H), INK), frame["dim"])
    if frame.get("flash"):
        img = Image.blend(img, Image.new("RGB", (W, H), (255, 255, 255)), min(1.0, frame["flash"]))
    return img


def inset(raw, img, box):
    """A close-up of part of the frame (the mirror: `box` is x0, y0, x1, y1
    in CSS pixels), blown up across the middle of the picture over a dimmed,
    blurred copy of it, in the game's plum frame."""
    x0, y0, x1, y1 = (v * SCALE for v in box)
    part = raw.crop((int(x0), int(y0), int(x1), int(y1)))
    w = int(W * 0.94)
    h = int(w * part.height / part.width)
    part = part.resize((w, h), Image.LANCZOS)
    back = img.filter(ImageFilter.GaussianBlur(14))
    back = Image.blend(back, Image.new("RGB", (W, H), INK), 0.45)
    d = ImageDraw.Draw(back)
    left, top = (W - w) // 2, int(H * 0.5 - h / 2)
    d.rounded_rectangle((left - 22, top - 22, left + w + 22, top + h + 22), radius=34, fill=PAPER)
    d.rounded_rectangle((left - 12, top - 12, left + w + 12, top + h + 12), radius=28, fill=INK)
    back.paste(part, (left, top))
    return back


def synth(length, hits, whooshes):
    """The hits (a falling low thump over a burst of noise, as Vagrancy's) and
    the rushes of air (filtered noise swelling and falling)."""
    n = int(length * RATE) + RATE
    buf = np.zeros(n)
    rng = np.random.default_rng(7)
    t = np.arange(int(0.6 * RATE)) / RATE
    for at, gain in hits:
        s = int(at * RATE)
        thump = np.sin(2 * np.pi * (110 - 55 * t) * t) * np.exp(-t * 7) * 0.9 + rng.uniform(-1, 1, t.size) * np.exp(-t * 35) * 0.4
        e = min(n, s + t.size)
        buf[s:e] += gain * thump[: e - s]
    tw = np.arange(int(0.55 * RATE)) / RATE
    for at in whooshes:
        s = int(at * RATE)
        noise = rng.uniform(-1, 1, tw.size)
        # a one-pole low-pass sweeping up then down, and a swell
        out = np.zeros_like(noise)
        acc = 0.0
        for i in range(tw.size):
            a = 0.04 + 0.25 * math.sin(math.pi * i / tw.size)
            acc += a * (noise[i] - acc)
            out[i] = acc
        out *= np.sin(np.pi * tw / tw[-1]) ** 2 * 2.2
        e = min(n, s + tw.size)
        buf[s:e] += out[: e - s]
    return buf


def write_wav(path, buf):
    peak = max(1e-9, np.abs(buf).max())
    data = (buf / peak * 0.8 * 32767).astype("<i2")
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(data.tobytes())


def render(frames, out, music_gain=0.75, seed=11):
    """Encodes the frames, in order, with their notes, to `out` (an .mp4)."""
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    ff = imageio_ffmpeg.get_ffmpeg_exe()
    silent = out.with_suffix(".video.mp4")
    enc = subprocess.Popen([ff, "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-preset", "medium", str(silent)], stdin=subprocess.PIPE)
    rnd = random.Random(seed)
    hits, whooshes = [], []
    shown, age = None, 0
    for k, f in enumerate(frames):
        img = compose(f, rnd)
        caps = f.get("captions") or ([f["caption"]] if f.get("caption") else [])
        key = json.dumps(caps)
        age = age + 1 if key == shown else 0
        shown = key
        for text, style in caps:
            caption(img, text, style, age)
        enc.stdin.write(img.tobytes())
        at = k / FPS
        if f.get("hit"):
            hits.append((at, 0.4 if f["hit"] == "soft" else 1.0))
        if f.get("whoosh"):
            whooshes.append(at)
    enc.stdin.close()
    enc.wait()
    length = len(frames) / FPS
    wav = out.with_suffix(".fx.wav")
    write_wav(wav, synth(length, hits, whooshes))
    music = json.loads((ROOT / "data" / "music.json").read_text())
    song = next(t for t in music["tracks"] if t.get("file"))
    mp3 = ROOT / "web" / song["file"]
    begin = song["start"]
    mix = (f"[2:a]atrim={begin:.3f}:{begin + length:.3f},asetpts=PTS-STARTPTS,"
           f"afade=t=out:st={max(0, length - 0.8):.2f}:d=0.8,volume={music_gain}[m];"
           f"[1:a]volume=0.7[h];[m][h]amix=inputs=2:normalize=0,alimiter=limit=0.89:level=disabled,atrim=0:{length:.2f}[a]")
    subprocess.run([ff, "-y", "-loglevel", "error", "-i", str(silent), "-i", str(wav), "-i", str(mp3), "-filter_complex", mix,
                    "-map", "0:v", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", str(out)], check=True)
    level = subprocess.run([ff, "-hide_banner", "-i", str(out), "-vn", "-af", "volumedetect", "-f", "null", "-"], capture_output=True, text=True).stderr
    mean = float(level.split("mean_volume:")[1].split("dB")[0])
    peak = float(level.split("max_volume:")[1].split("dB")[0])
    if mean < -40:
        raise SystemExit(f"{out}: the audio is silent (mean {mean} dB)")
    if peak > -0.1:
        raise SystemExit(f"{out}: the audio clips (peak {peak} dB)")
    silent.unlink()
    wav.unlink()
    print(f"wrote {out}: {length:.1f} s, {len(hits)} hits, audio mean {mean:.1f} dB, peak {peak:.1f} dB")
