#!/usr/bin/env python3
"""Screenshots of the built game on an emulated tablet or phone, played with
the touch controls, for looking at the mobile layout while building it.

    python3 tools/mobile.py OUTDIR [device] [--landscape]
    device: a Playwright device name, "iPad Pro 11" (default) or "iPhone 15"

Serves dist/, opens the page in WebKit as that device (touch, its screen
size and pixel ratio), taps Go inside, holds Grab on the record, runs for the
garage with the stick, gets in, drives, and prints the HUD and any console
errors. Touches are pointer events dispatched on the controls, as a finger's
are; WebKit is Safari's engine, though an emulated device is not an iPad.
"""
import functools, http.server, pathlib, sys, threading
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = pathlib.Path(sys.argv[1])
args = [a for a in sys.argv[2:] if not a.startswith("--")]
DEVICE = args[0] if args else "iPad Pro 11"
LANDSCAPE = "--landscape" in sys.argv
OUT.mkdir(parents=True, exist_ok=True)


class H(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

    def log_message(self, *a):
        pass


srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(H, directory=str(ROOT / "dist")))
threading.Thread(target=srv.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{srv.server_address[1]}"

TOUCH = """([sel, type, x, y, id]) => {
  const node = document.querySelector(sel);
  const r = node.getBoundingClientRect();
  const cx = x === null ? r.left + r.width / 2 : x, cy = y === null ? r.top + r.height / 2 : y;
  node.dispatchEvent(new PointerEvent(type, { pointerId: id, pointerType: 'touch', isPrimary: true,
    clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
}"""

with sync_playwright() as p:
    dev = dict(p.devices[DEVICE])
    if LANDSCAPE:
        w, h = dev["viewport"]["width"], dev["viewport"]["height"]
        dev["viewport"] = {"width": max(w, h), "height": min(w, h)}
    b = p.webkit.launch()
    ctx = b.new_context(**dev)
    pg = ctx.new_page()
    logs = []
    pg.on("console", lambda m: logs.append(f"{m.type}: {m.text}"))
    pg.on("pageerror", lambda e: logs.append(f"pageerror: {e}"))
    pg.goto(base + "/index.html")
    pg.wait_for_selector("body[data-ready='true']", state="attached", timeout=60000)
    n = [0]
    vw, vh = dev["viewport"]["width"], dev["viewport"]["height"]

    def shot(name):
        n[0] += 1
        pg.screenshot(path=str(OUT / f"{n[0]:02d}-{name}.png"))
        print(name, pg.evaluate("() => document.body.dataset.mode"), pg.evaluate("() => document.getElementById('prompt').textContent"))

    def press(sel, ms, id=7):
        pg.evaluate(TOUCH, [sel, "pointerdown", None, None, id])
        pg.wait_for_timeout(ms)
        pg.evaluate(TOUCH, [sel, "pointerup", None, None, id])

    def drag(x0, y0, x1, y1, ms, steps=8, id=3, hold=True):
        pg.evaluate(TOUCH, ["#touch-pad", "pointerdown", x0, y0, id])
        for k in range(1, steps + 1):
            pg.evaluate(TOUCH, ["#touch-pad", "pointermove", x0 + (x1 - x0) * k / steps, y0 + (y1 - y0) * k / steps, id])
            pg.wait_for_timeout(ms // steps)
        if not hold:
            pg.evaluate(TOUCH, ["#touch-pad", "pointerup", x1, y1, id])

    def lift(id=3):
        pg.evaluate(TOUCH, ["#touch-pad", "pointerup", 0, 0, id])

    pg.wait_for_timeout(1500)
    shot("title")
    pg.tap("#begin")
    pg.wait_for_timeout(1500)
    shot("house")
    # look round to the record: a drag on the right half
    drag(vw * 0.75, vh * 0.5, vw * 0.75 - 120, vh * 0.5, 400, hold=False)
    pg.wait_for_timeout(300)
    shot("looked")
    press("#touch-buttons [data-id=use]", 5600)
    pg.wait_for_timeout(1500)
    shot("chase")
    # run: stick forward with the left thumb
    sx, sy = vw * 0.2, vh * 0.75
    drag(sx, sy, sx, sy - 70, 200, id=5)
    pg.wait_for_timeout(1200)
    shot("running")
    lift(5)
    for _ in range(4):
        press("#touch-buttons [data-id=use]", 120)
    pg.wait_for_timeout(500)
    shot("in-car?")
    drag(sx, sy, sx, sy - 70, 200, id=5)
    pg.wait_for_timeout(3000)
    shot("driving")
    lift(5)
    print("\n".join(logs[-20:]))
    b.close()
