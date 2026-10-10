# Pursuer

A first-person survival driving game that runs in the browser. You start in a house in the middle of a city. Pick something up and hold on to it for five seconds, and the house folds flat like a paper diorama while a 60-metre cardboard cut-out of someone that thing belongs to stands up and comes for you, through every building in the way. Get to the car in the garage and drive. The score is how long you last.

**Play:** https://sgilson7.github.io/pursuer/

- **The house.** Six things to pick up: a teddy bear, an electric guitar, a trophy, a toaster, a microphone and a crown. Each has its own pursuer: Mr. Buttons, The Headliner, The Champion, The Morning Host, The Crooner and The Queen. Let go before five seconds and the thing goes back where it was. The house is silent apart from your shoes squeaking.
- **The pursuer.** A flat, still picture that always faces you and walks straight at you, gaining speed the longer the chase lasts, from 9 m/s to 75 m/s by about four and a half minutes, faster than the car at full turbo. It turns slowly, so a sharp turn at speed can slip past it. Far behind, it hurries to stay in your mirrors. Every 25 to 45 seconds, if it is more than 140 m away, it vanishes and pops up 120 m straight ahead of you, stands still for a second, and comes on from there.
- **The city.** One kilometre square on a Manhattan grid, with downtown towers north of the house, mid-rise blocks around it, suburbs on the hills to the south and west, five hills, and one elevated road over the widened street south of the house. About 20,000 breakable pieces: skyscrapers, apartment blocks, houses, trees, rooftop clutter and the overpass's deck, pillars and barriers.
- **Destruction.** What the pursuer walks through is thrown forward and outward, piece by piece. A piece knocked out takes whatever it held up with it, a building that loses enough of its ground floor comes down whole, and a span of the overpass falls when either of its pillars goes. Loose pieces tumble, land, pile up and come to rest.
- **The car.** First person from a high seat, with a rear-view mirror and two door mirrors that each show the pursuer behind you. It is an arcade car with some weight: the steering takes a moment to turn, the back can slide, and holding full lock for a second and a half at over 125 km/h rolls it over. It earns abilities as you survive: big wheels at 25 s (Q: eight seconds on stilts tall enough to drive over a fallen building's blocks), hover for three seconds at 55 s (Space), turbo at 85 s, wings at 120 s (F: fly for five seconds), and more turbo at 170 s and 240 s. Hold R to roll it back onto its wheels. Nothing hurts the car; there is no fall damage.

## Controls

| Input | Does |
| --- | --- |
| W A S D | Walk, or drive |
| Mouse | Look |
| E or left click (hold) | Pick something up; get in the car |
| Shift | Run, or handbrake |
| R (hold) | Roll the car back onto its wheels |
| Space | Jump, or hover once the car can |
| Q | Big wheels, once the car has them |
| F | Fly, once the car can |
| Esc | Pause |
| N | Sound on or off |

Add `#everything` to the address to knock the whole city loose at once when the page loads, which is the worst case the physics is built for.

## How it is built

Pursuer is a [Builder](https://github.com/sgilson7/builder-setting) project, and its parents are Sam's other games.

- **From Iron Vector, Food Truck Circus and Long Time Comin':** the 3D engine. Q16.16 fixed-point maths with no floats in the core, instanced draws, the core computing every matrix, the camera and the HUD, pointer-lock input, a fixed 60 ticks a second, the ink outline (the inverted hull) and toon-banded light, and the TikZ art pipeline after `Sam_Comm_final.png`.
- **From Vagrancy:** the sticky-feet stand-up, as the car's roll-over key.
- **New here:** the rubble. Every piece is a box with a slot in one instance buffer that the page reads straight from the module's memory and uploads only where a piece moved. Standing and resting pieces cost nothing; moving ones fall, tumble and collide by their corners in integer maths. The 2,600 moving pieces nearest the player get every contact and the rest only fall to the ground, so the cost has a ceiling. Each view (the windscreen and three mirrors) draws only the structures it can see.

Measured on an Apple Silicon Mac in Chromium: 60 frames a second while the pursuer crosses blocks, and 60 with all 19,857 pieces knocked loose at once (`#everything`). On a software renderer (no graphics card) the page draws without outlines, to 320 m and with one mirror, at about 16 frames a second. See `MEASUREMENTS.md`.

The layers:

- **`crates/core`** holds every rule: the city plan, the rubble, the car, the walker, the house and its fold, the pursuer, the cameras, the views and their draws, and the HUD. No input or output, no floats, no clock.
- **`crates/shim`** moves values across the WebAssembly boundary and decides nothing.
- **`web/`** is a static page with a WebGL 2 canvas. It draws what the core returns.
- **`data/`** holds every colour, item, pursuer number, upgrade, word, sound recipe, the tune and the shaders.
- **`tools/art.py`** draws every picture as TikZ; the sources are in `tools/art/`.

```bash
./scripts/setup.sh   # once: Playwright and three browser engines
./scripts/check.sh   # fmt, clippy, the core tests, the wasm build and audit, the browser check
python3 scripts/serve.py 8000
```

A push to `main` runs the same check in Chromium and deploys `dist/` to GitHub Pages if it passes.

`BRIEF.md` holds the request, `PLAN.md` the decisions, `SECOND-ORDER.md` what was found while building, and `HANDOFF.md` where the work stands.

## Music

The request asked for "Are You Coming to the Tree" from *The Hunger Games* when the pursuer appears. That recording is not published with the game, since it is not Sam's to publish. The page plays an original tune instead, synthesized from `data/music.json`. To play a recording you have the right to use, put it under `web/` and set `file` in `data/music.json` to its path.

## Credits

Made by Sam Gilson. Inspired by Crazy Taxi, Battlefield: Bad Company and Paper Mario; it uses nothing of theirs. The pursuers are original characters.

## Licence

MIT. See `LICENSE`.
