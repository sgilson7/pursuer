"""Recording a person playing, from inside the browser, with no
screen-recording permission for any program: the picture from Chromium's
screencast (each frame the page draws, its HUD included, sent over the
debugging connection) and the sound from the page's own audio, tapped by the
extension in `tab_hook/` and recorded in the page.

The game runs in a Chromium window started with remote debugging and the
extension:

    "<Chromium>" --remote-debugging-port=9333 --load-extension=tools/film/tab_hook \\
        --app=http://127.0.0.1:8790/index.html

    python tools/film/record_tab.py run DIR    # records until DIR/STOP appears
    python tools/film/record_tab.py stop DIR   # makes DIR/STOP

`run` writes DIR/recording.mp4: the frames at a steady 60 a second (room for slow motion) (each
held until the next came) with the sound laid under them from the moment
it started. A reload of the page ends the sound's recording and loses it.
"""
import base64, subprocess, sys, time
from pathlib import Path

import imageio_ffmpeg
from playwright.sync_api import sync_playwright

PORT = 9333
FPS = 60

AUDIO_START = """async () => {
  // the sound: a context of the recorder's own, which the tap (tab_hook/tap.js)
  // feeds the page's Web Audio and media elements, now or once the page makes them
  window.__recCtx = window.__recCtx || new AudioContext();
  window.__recDest = window.__recCtx.createMediaStreamDestination();
  await window.__recCtx.resume();
  // a silent source, so the recording runs from the start, before the page makes a sound
  const hush = window.__recCtx.createConstantSource();
  hush.offset.value = 0;
  hush.connect(window.__recDest);
  hush.start();
  window.__bridge();
  const rec = new MediaRecorder(window.__recDest.stream, { mimeType: 'audio/webm;codecs=opus', audioBitsPerSecond: 192000 });
  window.__chunks = [];
  rec.ondataavailable = e => { if (e.data.size) window.__chunks.push(e.data); };
  await new Promise(r => { rec.onstart = r; rec.start(1000); });
  window.__rec = rec;
  return Date.now() / 1000;
}"""

AUDIO_STOP = """async () => {
  const rec = window.__rec;
  if (!rec) return 0;
  if (rec.state !== 'inactive') await new Promise(r => { rec.onstop = r; rec.stop(); });
  window.__recDest = null;
  window.__blob = new Blob(window.__chunks, { type: rec.mimeType });
  return window.__blob.size;
}"""

PART = """async ([at, n]) => {
  const buf = new Uint8Array(await window.__blob.slice(at, at + n).arrayBuffer());
  let s = '';
  for (let i = 0; i < buf.length; i += 32768) s += String.fromCharCode.apply(null, buf.subarray(i, i + 32768));
  return btoa(s);
}"""


def run(out):
    frames_dir = out / "frames"
    frames_dir.mkdir(parents=True, exist_ok=True)
    for f in frames_dir.glob("*.jpg"):
        f.unlink()
    (out / "STOP").unlink(missing_ok=True)
    stamps = []
    with sync_playwright() as p:
        b = p.chromium.connect_over_cdp(f"http://127.0.0.1:{PORT}")
        pg = next(pg for c in b.contexts for pg in c.pages if "index.html" in pg.url)
        s = pg.context.new_cdp_session(pg)
        w, h, dpr = pg.evaluate("[innerWidth, innerHeight, devicePixelRatio]")

        def frame(e):
            s.send("Page.screencastFrameAck", {"sessionId": e["sessionId"]})
            path = frames_dir / f"{len(stamps):06d}.jpg"
            path.write_bytes(base64.b64decode(e["data"]))
            stamps.append(e["metadata"]["timestamp"])

        s.on("Page.screencastFrame", frame)
        audio_t0 = s.send("Runtime.evaluate", {"expression": f"({AUDIO_START})()", "awaitPromise": True, "returnByValue": True})["result"]["value"]
        s.send("Page.startScreencast", {"format": "jpeg", "quality": 88, "maxWidth": int(w * dpr), "maxHeight": int(h * dpr), "everyNthFrame": 2})
        print(f"recording {w}x{h} at {dpr}x; touch {out / 'STOP'} to end", flush=True)
        last = time.time()
        while not (out / "STOP").exists():
            pg.wait_for_timeout(200)
            if time.time() - last > 10:
                last = time.time()
                print(f"  {len(stamps)} frames", flush=True)
        s.send("Page.stopScreencast")
        pg.wait_for_timeout(300)
        size = pg.evaluate(AUDIO_STOP)
        audio = out / "audio.webm"
        with open(audio, "wb") as f:
            step = 8 << 20
            for at in range(0, size, step):
                f.write(base64.b64decode(pg.evaluate(PART, [at, step])))
        pg.evaluate("() => { window.__blob = null; window.__chunks = []; window.__rec = null; }")
    if not stamps:
        raise SystemExit("no frames came")
    # the frames, each held until the next, at a steady rate
    lst = out / "frames.txt"
    with open(lst, "w") as f:
        for k, t in enumerate(stamps):
            nxt = stamps[k + 1] if k + 1 < len(stamps) else t + 1 / FPS
            f.write(f"file '{frames_dir / f'{k:06d}.jpg'}'\nduration {max(0.001, nxt - t):.4f}\n")
        f.write(f"file '{frames_dir / f'{len(stamps) - 1:06d}.jpg'}'\n")
    # the sound began at audio_t0; the first frame at stamps[0]
    offset = audio_t0 - stamps[0]
    ff = imageio_ffmpeg.get_ffmpeg_exe()
    video = out / "recording.mp4"
    cmd = [ff, "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(lst)]
    if size:
        cmd += ["-itsoffset", f"{offset:.3f}", "-i", str(audio), "-map", "0:v", "-map", "1:a", "-c:a", "aac", "-b:a", "192k"]
    cmd += ["-vf", f"fps={FPS},scale=trunc(iw/2)*2:trunc(ih/2)*2", "-c:v", "libx264", "-crf", "16", "-preset", "fast", "-pix_fmt", "yuv420p", str(video)]
    subprocess.run(cmd, check=True)
    span = stamps[-1] - stamps[0]
    print(f"wrote {video}: {span:.1f} s, {len(stamps)} frames ({len(stamps) / max(span, 0.001):.0f} a second), sound {size / 1e6:.1f} MB")


def main():
    cmd, out = sys.argv[1], Path(sys.argv[2])
    if cmd == "run":
        run(out)
    elif cmd == "stop":
        out.mkdir(parents=True, exist_ok=True)
        (out / "STOP").touch()


if __name__ == "__main__":
    main()
