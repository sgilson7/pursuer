"""A short from a recording of a person playing (record_tab.py's, OBS's or any
screen recorder's), cut the way edit.py cuts filmed takes: the game picture
found and filled to 1080 x 1920, the clips a cut list names in the order it
names them, captions burned in, the line that it plays on a phone across the
top, and an end card that loops.

    python tools/film/from_obs.py PLAN.json OUT.mp4

The plan (times are seconds in the recording):

    {"source": "recording60.mp4",          # relative to the plan's folder
     "box": [x0, y0, x1, y1],               # the game's picture; found when left out
     "tag": true, "end": true,
     "tag_seconds": 4,                      # the phone line only this long at the start (and on the end card)
     "clips": [
       {"from": 68.3, "to": 70.8,
        "speed": 0.5,                       # slow motion (sound slowed with it)
        "caps": [["Kanye came through the wall", "hook"]],
        "punch": [1.0, 1.25], "at": [0.5, 0.35],   # a zoom across the clip, centred there
        "flash": true,                      # a white flash at the cut in
        "hit": "hard",                      # an impact on the sound at the cut in
        "shake": 0.6}, ...]}

Caption styles are edit.py's (hook, beat, low, small, under, tag, end). The
recording's own sound (the game's music and its noises) is kept, softened
at each cut so the jumps do not click, with the hits laid over it.
"""
import json, random, subprocess, sys
from pathlib import Path

import imageio_ffmpeg
import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from edit import H, INK, W, caption, synth, write_wav  # noqa: E402

FPS = 30
TAG = "Free · plays in your phone's browser too"
END = ["Pursuer", "Free in your browser, on a phone or a computer", "sgilson7.github.io/pursuer"]
END_SECONDS = 2.5
FF = imageio_ffmpeg.get_ffmpeg_exe()


def probe(path):
    out = subprocess.run([FF, "-hide_banner", "-i", str(path)], capture_output=True, text=True).stderr
    line = next(l for l in out.splitlines() if "Video:" in l)
    toks = line.replace(",", " ").split()
    w, h = next(t for t in toks if "x" in t and all(p.isdigit() for p in t.split("x"))).split("x")
    fps = float(toks[toks.index("fps") - 1])
    hh, mm, ss = out.split("Duration:")[1].split(",")[0].strip().split(":")
    return int(w), int(h), fps, int(hh) * 3600 + int(mm) * 60 + float(ss), "Audio:" in out


def frame_at(path, t, w, h):
    raw = subprocess.run([FF, "-loglevel", "error", "-ss", f"{t:.3f}", "-i", str(path), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                         capture_output=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(h, w, 3)


def find_box(path, w, h, dur):
    """The game's picture: what is not black in a few frames, and the
    window's title bar (a band of flat grey at the top) cut off."""
    lit = np.zeros((h, w), bool)
    for t in np.linspace(dur * 0.1, dur * 0.9, 5):
        lit |= frame_at(path, t, w, h).max(axis=2) > 12
    ys, xs = np.nonzero(lit)
    x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
    f = frame_at(path, dur * 0.5, w, h)[y0:y1, x0:x1].astype(int)
    # rows of one flat grey from the top, judged in from the edges (the
    # window's corners are rounded): the title bar
    inner = f[:, (x1 - x0) // 10 : (x1 - x0) * 9 // 10]
    top = 0
    while top < (y1 - y0) // 8 and inner[top].std(axis=0).max() < 6 and np.ptp(inner[top].mean(axis=0)) < 8:
        top += 1
    return int(x0), int(y0 + top), int(x1), int(y1)


def to_916(box):
    """The box cut down at its sides or top and bottom to 9:16."""
    x0, y0, x1, y1 = box
    bw, bh = x1 - x0, y1 - y0
    if bw / bh > W / H:
        cut = bw - bh * W / H
        x0, x1 = x0 + cut / 2, x1 - cut / 2
    else:
        cut = bh - bw * H / W
        y0, y1 = y0 + cut / 2, y1 - cut / 2
    return int(x0), int(y0), int(x1), int(y1)


class ClipFrames:
    """The clip's frames, cropped and filled to 1080 x 1920, read in order as
    they are asked for (a long clip does not fit in memory)."""

    def __init__(self, src, box, t0, t1):
        x0, y0, x1, y1 = box
        vf = f"crop={x1 - x0}:{y1 - y0}:{x0}:{y0},scale={W}:{H}:flags=lanczos"
        self.p = subprocess.Popen([FF, "-loglevel", "error", "-ss", f"{t0:.3f}", "-i", str(src), "-t", f"{t1 - t0:.3f}", "-vf", vf,
                                   "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], stdout=subprocess.PIPE)
        self.have, self.frame = -1, None

    def get(self, idx):
        while self.have < idx:
            buf = self.p.stdout.read(W * H * 3)
            if len(buf) < W * H * 3:
                break
            self.frame, self.have = buf, self.have + 1
        return self.frame

    def close(self):
        self.p.stdout.close()
        self.p.kill()
        self.p.wait()


def punch(img, z, at, jx=0.0, jy=0.0):
    if z <= 1.0 and not (jx or jy):
        return img
    z = max(z, 1.0)
    cw, ch = W / z, H / z
    cx = min(max(at[0] * W + jx * W, cw / 2), W - cw / 2)
    cy = min(max(at[1] * H + jy * H, ch / 2), H - ch / 2)
    return img.crop((int(cx - cw / 2), int(cy - ch / 2), int(cx + cw / 2), int(cy + ch / 2))).resize((W, H), Image.LANCZOS)


def atempo(speed):
    chain, s = [], speed
    while s < 0.5:
        chain.append("atempo=0.5")
        s /= 0.5
    chain.append(f"atempo={s:.4f}")
    return ",".join(chain)


def main():
    plan_path, out = Path(sys.argv[1]), Path(sys.argv[2])
    plan = json.loads(plan_path.read_text())
    src = (plan_path.parent / plan["source"]).resolve()
    w, h, src_fps, dur, has_audio = probe(src)
    box = to_916(tuple(plan["box"]) if plan.get("box") else find_box(src, w, h, dur))
    print(f"{src.name}: {w}x{h} at {src_fps:g} fps, {dur:.1f} s; picture {box}")

    out.parent.mkdir(parents=True, exist_ok=True)
    silent = out.with_suffix(".video.mp4")
    enc = subprocess.Popen([FF, "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-preset", "medium", str(silent)], stdin=subprocess.PIPE)
    rnd = random.Random(5)
    hits, shown, age, n_out, last = [], None, 0, 0, None
    for clip in plan["clips"]:
        t0, t1, speed = clip["from"], clip["to"], clip.get("speed", 1.0)
        frames = ClipFrames(src, box, t0, t1)
        count = max(1, round((t1 - t0) / speed * FPS))
        z0, z1 = clip.get("punch", [1.0, 1.0])
        at = clip.get("at", [0.5, 0.5])
        if clip.get("hit"):
            hits.append((n_out / FPS, 0.4 if clip["hit"] == "soft" else 1.0))
        for k in range(count):
            t = k / FPS * speed
            img = Image.frombytes("RGB", (W, H), frames.get(int(t * src_fps)))
            last = img
            f = k / max(1, count - 1)
            # ease the punch-in
            z = z0 + (z1 - z0) * (1 - (1 - f) ** 2)
            s = clip.get("shake", 0) * max(0.0, 1 - k / 12)
            jx, jy = (rnd.uniform(-1, 1) * s * 0.02, rnd.uniform(-1, 1) * s * 0.02) if s else (0, 0)
            img = punch(img, z + (0.04 if s else 0), at, jx, jy)
            if clip.get("flash") and k < 6:
                img = Image.blend(img, Image.new("RGB", (W, H), (255, 255, 255)), 0.85 * (1 - k / 6))
            on = [tuple(c) for c in clip.get("caps", [])]
            if plan.get("tag", True) and n_out / FPS < plan.get("tag_seconds", 1e9):
                on.append((TAG, "tag"))
            key = json.dumps(on)
            age = age + 1 if key == shown else 0
            shown = key
            for text, style in on:
                caption(img, text, style, age)
            enc.stdin.write(img.tobytes())
            n_out += 1
        frames.close()
    end_frames = 0
    if plan.get("end", True) and last is not None:
        end_frames = int(END_SECONDS * FPS)
        for k in range(end_frames):
            img = Image.blend(last, Image.new("RGB", (W, H), INK), min(0.72, k / 8 * 0.72))
            caption(img, END[0], "beat", k)
            caption(img, END[1], "small", k)
            caption(img, END[2], "under", k)
            if plan.get("tag", True):
                caption(img, TAG, "tag", k)
            enc.stdin.write(img.tobytes())
    enc.stdin.close()
    enc.wait()
    length = (n_out + end_frames) / FPS

    if not has_audio:
        silent.rename(out)
        print(f"wrote {out}: {length:.1f} s, silent (the recording has no sound)")
        return
    # the sound: each clip's stretch of the recording (slowed with the picture), faded in and out
    # a little at each cut, then the recording running on under the end card
    parts, labels = [], []
    clips = plan["clips"]
    for k, c in enumerate(clips):
        speed = c.get("speed", 1.0)
        span = (c["to"] - c["from"]) / speed
        tempo = f",{atempo(speed)}" if speed != 1.0 else ""
        parts.append(f"[0:a]atrim={c['from']:.3f}:{c['to']:.3f},asetpts=PTS-STARTPTS{tempo},"
                     f"afade=t=in:d=0.03,afade=t=out:st={max(0, span - 0.05):.3f}:d=0.05[p{k}]")
        labels.append(f"[p{k}]")
    tail_from = clips[-1]["to"]
    parts.append(f"[0:a]atrim={tail_from:.3f}:{tail_from + END_SECONDS:.3f},asetpts=PTS-STARTPTS,afade=t=in:d=0.03[tail]")
    labels.append("[tail]")
    wav = out.with_suffix(".fx.wav")
    write_wav(wav, synth(length, hits, []))
    fade = max(0.0, length - 1.2)
    graph = (";".join(parts) + f";{''.join(labels)}concat=n={len(labels)}:v=0:a=1[rec];[1:a]volume=0.55[h];"
             f"[rec][h]amix=inputs=2:normalize=0,alimiter=limit=0.89:level=disabled,apad,atrim=0:{length:.3f},afade=t=out:st={fade:.2f}:d=1.2[a]")
    subprocess.run([FF, "-y", "-loglevel", "error", "-i", str(src), "-i", str(wav), "-i", str(silent), "-filter_complex", graph,
                    "-map", "2:v", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", str(out)], check=True)
    silent.unlink()
    wav.unlink()
    level = subprocess.run([FF, "-hide_banner", "-i", str(out), "-vn", "-af", "volumedetect", "-f", "null", "-"], capture_output=True, text=True).stderr
    mean = float(level.split("mean_volume:")[1].split("dB")[0])
    peak = float(level.split("max_volume:")[1].split("dB")[0])
    print(f"wrote {out}: {length:.1f} s, {len(clips)} clips, {len(hits)} hits, audio mean {mean:.1f} dB, peak {peak:.1f} dB")


if __name__ == "__main__":
    main()
