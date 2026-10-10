#!/usr/bin/env python3
"""Screenshots of the built game, for looking at it while building.

    python3 tools/shots.py OUTDIR [script]

Serves dist/, opens the page in Chromium with a real GPU (Metal through
ANGLE), and runs a named script of key presses, saving a picture at each
step and printing the HUD and any console errors.
"""
import functools, http.server, json, pathlib, sys, threading
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = pathlib.Path(sys.argv[1])
SCRIPT = sys.argv[2] if len(sys.argv) > 2 else "intro"
OUT.mkdir(parents=True, exist_ok=True)


class H(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

    def log_message(self, *a):
        pass


srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(H, directory=str(ROOT / "dist")))
threading.Thread(target=srv.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{srv.server_address[1]}"

with sync_playwright() as p:
    b = p.chromium.launch(args=["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist"])
    pg = b.new_page(viewport={"width": 1280, "height": 720})
    logs = []
    pg.on("console", lambda m: logs.append(f"{m.type}: {m.text}"))
    pg.on("pageerror", lambda e: logs.append(f"pageerror: {e}"))
    pg.goto(base + "/index.html" + ("#everything" if SCRIPT == "everything" else ""))
    pg.wait_for_selector("body[data-ready='true']", state="attached", timeout=30000)
    n = [0]

    def shot(name):
        n[0] += 1
        pg.screenshot(path=str(OUT / f"{n[0]:02d}-{name}.png"))
        print(name, pg.evaluate("() => document.body.dataset.mode"))

    def fps(label):
        n = pg.evaluate("""() => new Promise(r => { let n = 0; const t0 = performance.now();
            function f(t) { n++; if (t - t0 < 2000) requestAnimationFrame(f); else r(n / ((t - t0) / 1000)); }
            requestAnimationFrame(f); })""")
        print(f"fps {label}: {n:.1f}")

    def hold(key, ms):
        pg.keyboard.down(key)
        pg.wait_for_timeout(ms)
        pg.keyboard.up(key)

    def turn(px, py=0):
        # headless Chromium gives synthetic moves no movementX, so send one
        pg.evaluate(f"() => window.dispatchEvent(new MouseEvent('mousemove', {{movementX: {px}, movementY: {py}}}))")
        pg.wait_for_timeout(100)

    pg.wait_for_timeout(1500)
    shot("title")
    if SCRIPT == "everything":
        # the title's own camera circles the city: hide the card to see it fall
        pg.evaluate("() => { document.getElementById('title').style.display = 'none'; }")
    else:
        pg.click("#begin")
    pg.wait_for_timeout(1200)
    shot("house")
    if SCRIPT in ("intro", "chase", "drive"):
        pg.keyboard.down("KeyE")
        pg.wait_for_timeout(2500)
        shot("holding")
        pg.wait_for_timeout(3200)
        pg.keyboard.up("KeyE")
        shot("fold")
        pg.wait_for_timeout(2500)
        shot("folded")
    if SCRIPT in ("chase", "drive"):
        turn(-2041, -150)
        shot("turned")
        pg.wait_for_timeout(3000)
        shot("risen")
    if SCRIPT == "everything":
        for k in range(6):
            pg.wait_for_timeout(1500)
            shot(f"all{k}")
            fps(f"all{k}")
    if SCRIPT == "watch":
        pg.keyboard.down("KeyE")
        pg.wait_for_timeout(5600)
        pg.keyboard.up("KeyE")
        turn(559, -250)
        for k in range(8):
            pg.wait_for_timeout(2500)
            shot(f"watch{k}")
            fps(f"watch{k}")
    if SCRIPT == "drive":
        hold("KeyW", 2600)
        shot("walked")
        for _ in range(4):
            pg.keyboard.press("KeyE")
            pg.wait_for_timeout(200)
        shot("in-car")
        hold("KeyW", 3000)
        shot("driving")
        hold("KeyW", 4000)
        shot("driving2")
        fps("driving")
        pg.wait_for_timeout(8000)
        shot("later")
    print("\n".join(logs[-30:]))
    hud = pg.evaluate("() => document.body.dataset.mode")
    b.close()
