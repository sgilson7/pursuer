"""The project's own browser interactions, run by check.py in every engine.

check.py has already loaded the page, waited for it to report ready, and let
it idle. These drive the game the way a person would and assert what the page
shows. Expected values are worked out from the data files, never copied from
the game's own output:

- the first item in items.json is the teddy bear, and the walker starts
  facing it, so the prompt is copy.json's prompt_pick with its name;
- items.json's hold_seconds is 5: two seconds of holding is not enough,
  six is, and the pursuer is that item's monster_name;
- standing still in the house, the pursuer (pursuer.json: 230 m away, 9 m/s
  and gaining) reaches you well inside two minutes of game time;
- the time survived is then more than zero and is kept as the best after a
  reload.
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
COPY = json.loads((ROOT / "data" / "copy.json").read_text())
ITEMS = json.loads((ROOT / "data" / "items.json").read_text())


def click(page, selector):
    page.click(selector, timeout=5000)
    page.wait_for_timeout(250)


def mode(page):
    return page.evaluate("() => document.body.dataset.mode")


def run(page, check, engine):
    message = page.inner_text("#message")
    check(message in ("", COPY["no_webgl"]), f"no error message is shown ({message!r})")
    check(mode(page) == "title", f"the game opens on the title ({mode(page)})")
    check(page.text_content("#title h1") == COPY["title"], "the title from data/copy.json is drawn")
    check(page.text_content("#title-best") == "0:00.0", "a first visit has no best time")

    click(page, "#begin")
    check(mode(page) == "house", f"Go inside starts in the house ({mode(page)})")
    if not page.is_hidden("#engage"):
        click(page, "#engage-btn")
    teddy = ITEMS["items"][0]
    want = COPY["prompt_pick"].replace("{item}", teddy["name"])
    page.wait_for_timeout(400)
    check(page.inner_text("#prompt") == want, f"facing the teddy bear: {page.inner_text('#prompt')!r}")

    page.keyboard.down("KeyE")
    page.wait_for_timeout(2000)
    page.keyboard.up("KeyE")
    page.wait_for_timeout(300)
    check(mode(page) == "house", f"two seconds of holding is not too long ({mode(page)})")

    page.keyboard.down("KeyE")
    try:
        page.wait_for_function("() => document.body.dataset.mode === 'chase'", timeout=60000)
        chased = True
    except Exception:
        chased = False
    page.keyboard.up("KeyE")
    check(chased, f"holding on brings the pursuer ({mode(page)})")
    page.wait_for_timeout(300)
    check(page.inner_text("#h-monster") == teddy["monster_name"], f"and it is {teddy['monster_name']}: {page.inner_text('#h-monster')!r}")
    check(page.inner_text("#prompt") == COPY["prompt_run"], f"the page says to run: {page.inner_text('#prompt')!r}")

    page.keyboard.press("Escape")
    page.wait_for_timeout(300)
    check(mode(page) == "paused", f"Esc pauses ({mode(page)})")
    click(page, "#resume")
    if not page.is_hidden("#engage"):
        click(page, "#engage-btn")
    check(mode(page) == "chase", f"and Resume goes back ({mode(page)})")

    try:
        page.wait_for_function("() => document.body.dataset.mode === 'caught'", timeout=240000)
        caught = True
    except Exception:
        caught = False
    check(caught, f"standing still, you are caught ({mode(page)}, {page.inner_text('#h-distance')} m)")
    survived = page.inner_text("#c-time")
    check(survived not in ("", "0:00.0"), f"the time survived is shown ({survived!r})")
    check(page.inner_text("#c-monster") == teddy["monster_name"], "the verdict names the pursuer")
    check(not page.is_hidden("#c-new"), "a first escape is a new best")

    click(page, "#again")
    check(mode(page) == "house", f"Go back inside starts again ({mode(page)})")

    page.reload()
    page.wait_for_selector("body[data-ready='true']", state="attached", timeout=20000)
    check(page.text_content("#title-best") == survived, f"the best was kept ({page.text_content('#title-best')!r})")


# A finger: pointer events of type touch, dispatched on a control as a
# finger's are (Playwright's own touch only taps).
FINGER = """([sel, type, dx, dy]) => {
  const node = document.querySelector(sel);
  const r = node.getBoundingClientRect();
  node.dispatchEvent(new PointerEvent(type, { pointerId: 9, pointerType: 'touch', isPrimary: true,
    clientX: r.left + r.width / 2 + dx, clientY: r.top + r.height / 2 + dy, bubbles: true, cancelable: true }));
}"""


def finger(page, sel, kind, dx=0, dy=0):
    page.evaluate(FINGER, [sel, kind, dx, dy])


def run_touch(page, check, engine):
    """The same game on a phone, played with the touch controls: the copy
    names the buttons, holding Grab brings the pursuer, the pause button
    pauses, and the stick moves you."""
    check(mode(page) == "title", f"the game opens on the title ({mode(page)})")
    check(page.evaluate("() => document.body.classList.contains('touch')"), "a touch screen shows the touch controls")
    check(page.is_visible(".mobile-line"), "the title says it plays on a phone")
    page.tap("#begin")
    page.wait_for_timeout(400)
    check(mode(page) == "house", f"tapping Go inside starts in the house ({mode(page)})")
    check(page.is_hidden("#engage"), "a touch screen needs no click to play")
    teddy = ITEMS["items"][0]
    want = COPY["prompt_pick_touch"].replace("{item}", teddy["name"])
    check(page.inner_text("#prompt") == want, f"the prompt names the Grab button: {page.inner_text('#prompt')!r}")
    grab = "#touch-buttons [data-id=use]"
    check(page.is_visible(grab), "the Grab button shows on foot")
    finger(page, grab, "pointerdown")
    try:
        page.wait_for_function("() => document.body.dataset.mode === 'chase'", timeout=60000)
        chased = True
    except Exception:
        chased = False
    finger(page, grab, "pointerup")
    check(chased, f"holding Grab brings the pursuer ({mode(page)})")
    check(page.inner_text("#prompt") == COPY["prompt_run"], f"and the page says to run: {page.inner_text('#prompt')!r}")
    page.tap("#pause-btn")
    page.wait_for_timeout(300)
    check(mode(page) == "paused", f"the pause button pauses ({mode(page)})")
    page.tap("#resume")
    page.wait_for_timeout(300)
    check(mode(page) == "chase", f"and Resume goes back ({mode(page)})")
    finger(page, "#touch-pad", "pointerdown", -page.viewport_size["width"] / 4, page.viewport_size["height"] / 4)
    finger(page, "#touch-pad", "pointermove", -page.viewport_size["width"] / 4, page.viewport_size["height"] / 4 - 70)
    page.wait_for_timeout(300)
    check(page.evaluate("() => document.getElementById('stick').classList.contains('live')"), "a left thumb is a stick")
    finger(page, "#touch-pad", "pointerup")
