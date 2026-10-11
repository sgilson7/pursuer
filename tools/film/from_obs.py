"""A short from a recording of a person playing (OBS or any screen
recorder), cut the way edit.py cuts filmed takes: the game picture found
and filled to 1080 x 1920, the parts kept, captions burned in, the line that
it plays on a phone across the top, and an end card that loops.

    python tools/film/from_obs.py RECORDING OUT.mp4 [--keep 3-14,22.5-31]
        [--cap "0-2.5:Kanye is right behind me:hook"] ... [--slow 9-10.5]
        [--box x0,y0,x1,y1] [--no-tag] [--no-end]

Times are seconds in the recording. `--keep` lists the stretches to use, in
order (all of it when left out). `--cap` is START-END:TEXT[:STYLE], the
styles edit.py's (hook, beat, low, small, under, tag, end), times in the
recording. `--slow` plays a stretch at half speed, its sound slowed with it.
`--box` is the game's picture in the recording's pixels; when left out it is
found: the part of the frame that is not black, less the window's title bar.

The recording's own sound (the game's music and its noises) is kept.
"""
import argparse, json, subprocess, sys
from pathlib import Path

import imageio_ffmpeg
import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from edit import H, INK, W, caption  # noqa: E402

FPS = 30
TAG = "Free · plays in your phone's browser too"
END = ["Pursuer", "Free in your browser, on a phone or a computer", "sgilson7.github.io/pursuer"]
FF = imageio_ffmpeg.get_ffmpeg_exe()


def probe(path):
    out = subprocess.run([FF, "-hide_banner", "-i", str(path)], capture_output=True, text=True).stderr
    line = next(l for l in out.splitlines() if "Video:" in l)
    w, h = next(tok for tok in line.replace(",", " ").split() if "x" in tok and tok.split("x")[0].isdigit() and tok.split("x")[1].isdigit()).split("x")
    dur = out.split("Duration:")[1].split(",")[0].strip()
    hh, mm, ss = dur.split(":")
    return int(w), int(h), int(hh) * 3600 + int(mm) * 60 + float(ss)


def frame_at(path, t, w, h):
    raw = subprocess.run([FF, "-loglevel", "error", "-ss", f"{t:.3f}", "-i", str(path), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                         capture_output=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(h, w, 3)


def find_box(path, w, h, dur):
    """The game's picture: what is not black in a few frames, and the
    window's title bar (a band of flat grey at the top) cut off, leaving
    the tallest 9:16-or-narrower picture under it."""
    lit = np.zeros((h, w), bool)
    for t in np.linspace(dur * 0.1, dur * 0.9, 5):
        lit |= frame_at(path, t, w, h).max(axis=2) > 12
    ys, xs = np.nonzero(lit)
    x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
    f = frame_at(path, dur * 0.5, w, h)[y0:y1, x0:x1].astype(int)
    # rows of one flat colour from the top, judged in from the edges (the
    # window's corners are rounded): the title bar
    inner = f[:, (x1 - x0) // 10 : (x1 - x0) * 9 // 10]
    top = 0
    while top < (y1 - y0) // 8 and inner[top].std(axis=0).max() < 6:
        top += 1
    return x0, y0 + top, x1, y1


def parse_spans(text):
    return [tuple(float(v) for v in part.split("-")) for part in text.split(",")] if text else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("recording")
    ap.add_argument("out")
    ap.add_argument("--keep")
    ap.add_argument("--cap", action="append", default=[])
    ap.add_argument("--slow", action="append", default=[])
    ap.add_argument("--box")
    ap.add_argument("--no-tag", action="store_true")
    ap.add_argument("--no-end", action="store_true")
    a = ap.parse_args()

    src, out = Path(a.recording), Path(a.out)
    w, h, dur = probe(src)
    box = tuple(int(v) for v in a.box.split(",")) if a.box else find_box(src, w, h, dur)
    bx0, by0, bx1, by1 = box
    # fill 1080 x 1920: crop the box's sides or top to 9:16
    bw, bh = bx1 - bx0, by1 - by0
    if bw / bh > W / H:
        cut = bw - bh * W / H
        bx0, bx1 = bx0 + cut / 2, bx1 - cut / 2
    else:
        cut = bh - bw * H / W
        by0, by1 = by0 + cut / 2, by1 - cut / 2
    print(f"recording {w}x{h}, {dur:.1f} s; game picture {box}, used {int(bx0)},{int(by0)},{int(bx1)},{int(by1)}")

    keep = parse_spans(a.keep) or [(0.0, dur)]
    slow = [tuple(float(v) for v in s.split("-")) for s in a.slow]
    caps = []
    for c in a.cap:
        span, rest = c.split(":", 1)
        text, _, style = rest.rpartition(":") if rest.rsplit(":", 1)[-1] in ("hook", "beat", "low", "small", "under", "tag", "end") else (rest, "", "hook")
        t0, t1 = (float(v) for v in span.split("-"))
        caps.append((t0, t1, text, style))

    # the film's frames: each a time in the recording (slow stretches give two frames a source frame)
    times = []
    for t0, t1 in keep:
        t = t0
        while t < t1:
            times.append(t)
            t += 1 / FPS / (2 if any(s0 <= t < s1 for s0, s1 in slow) else 1)

    # decode the whole recording once, at 30 fps, cropped and scaled
    crop = f"crop={int(bx1 - bx0)}:{int(by1 - by0)}:{int(bx0)}:{int(by0)},scale={W}:{H}:flags=lanczos,fps={FPS}"
    dec = subprocess.Popen([FF, "-loglevel", "quiet", "-i", str(src), "-vf", crop, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], stdout=subprocess.PIPE)
    silent = out.with_suffix(".video.mp4")
    out.parent.mkdir(parents=True, exist_ok=True)
    enc = subprocess.Popen([FF, "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-preset", "medium", str(silent)], stdin=subprocess.PIPE)
    size = W * H * 3
    have, frame, last = -1, None, None
    shown, age = None, 0
    for t in times:
        want = int(t * FPS)
        while have < want:
            buf = dec.stdout.read(size)
            if len(buf) < size:
                break
            frame, have = buf, have + 1
        if frame is None:
            break
        img = Image.frombytes("RGB", (W, H), frame)
        on = [(text, style) for t0, t1, text, style in caps if t0 <= t < t1]
        if not a.no_tag:
            on.append((TAG, "tag"))
        key = json.dumps(on)
        age = age + 1 if key == shown else 0
        shown = key
        for text, style in on:
            caption(img, text, style, age)
        enc.stdin.write(img.tobytes())
        last = Image.frombytes("RGB", (W, H), frame)
    dec.stdout.close()
    dec.kill()
    dec.wait()
    end_frames = 0
    if not a.no_end and last is not None:
        end_frames = int(2.5 * FPS)
        for k in range(end_frames):
            img = Image.blend(last, Image.new("RGB", (W, H), INK), min(0.72, k / 8 * 0.72))
            caption(img, END[0], "beat", k)
            caption(img, END[1], "small", k)
            caption(img, END[2], "under", k)
            enc.stdin.write(img.tobytes())
    enc.stdin.close()
    enc.wait()

    # the sound: the same stretches of the recording's audio, slowed where the picture is
    parts, labels = [], []
    pieces = []
    for t0, t1 in keep:
        cuts = sorted({t0, t1, *[v for s in slow for v in s if t0 < v < t1]})
        for c0, c1 in zip(cuts, cuts[1:]):
            pieces.append((c0, c1, any(s0 <= c0 < s1 for s0, s1 in slow)))
    for k, (c0, c1, slowed) in enumerate(pieces):
        parts.append(f"[1:a]atrim={c0:.3f}:{c1:.3f},asetpts=PTS-STARTPTS{',atempo=0.5' if slowed else ''}[p{k}]")
        labels.append(f"[p{k}]")
    length = len(times) / FPS + end_frames / FPS
    fade = max(0.0, length - 1.2)
    graph = ";".join(parts) + f";{''.join(labels)}concat=n={len(labels)}:v=0:a=1,apad,atrim=0:{length:.3f},afade=t=out:st={fade:.2f}:d=1.2[a]"
    has_audio = "Audio:" in subprocess.run([FF, "-hide_banner", "-i", str(src)], capture_output=True, text=True).stderr
    if has_audio:
        subprocess.run([FF, "-y", "-loglevel", "error", "-i", str(silent), "-i", str(src), "-filter_complex", graph,
                        "-map", "0:v", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", str(out)], check=True)
        silent.unlink()
    else:
        silent.rename(out)
        print("the recording has no sound: the short is silent (add a sound in TikTok)")
    print(f"wrote {out}: {length:.1f} s")


if __name__ == "__main__":
    main()
