# Handoff

10 October 2026. Where Pursuer stands.

## What exists

| What | Where |
| --- | --- |
| Repository | https://github.com/sgilson7/pursuer (public, MIT) |
| The game | https://sgilson7.github.io/pursuer/ (Pages, deployed by `.github/workflows/deploy.yml` after the check passes) |
| Website page | `~/Documents/website`: `content/projects/pursuer.md`, an entry in `content/projects.yaml`, `/pursuer/` in `OFF_BUILD_ROUTES` |

## Done, against the brief

- The house: six items, each with a pursuer; let go before five seconds and it goes back; squeaky footsteps and pick-up sounds and nothing else.
- The fold: the walls fall outward flat, the roof blows away, the pursuer stands up 230 m west and comes.
- The car: in the garage, entered with E, driven in first person with a rear-view mirror and two door mirrors.
- The city: 1 km square, Manhattan grid, downtown, mid-rise, suburbs, five hills, one overpass; 19,857 breakable pieces.
- Destruction: the sweep, supports, whole-building collapse, spans that need both pillars, loose pieces that tumble and sleep.
- Upgrades by time survived: big wheels, hover (3 s), turbo, wings (5 s), more turbo; R rolls the car over; no fall damage.
- Score: time survived, best kept in local storage.
- Performance: 60 frames a second in Chromium on an Apple Silicon Mac during a chase and with the whole city loose (`MEASUREMENTS.md`).

## Not done, or done differently

- The song is not published (SECOND-ORDER M1); an original tune plays, and a recording can be dropped in through `data/music.json`.
- The pursuers are original characters, not celebrities (M2).
- No river: the overpass is the bridge (M5).
- Frame rates on integrated Intel or AMD graphics, and in Firefox and Safari, were not measured.

## Commands run, and results

| Command | Result |
| --- | --- |
| `./scripts/check.sh` (three engines) | BUILDER CHECK PASSED: fmt, clippy, 76 core tests and 11 boundary tests, wasm build, import audit, browser check in Chromium, Firefox and WebKit |
| `cargo test --release -p pursuer_core --test perf_probe -- --ignored --nocapture` | see `MEASUREMENTS.md` |
| `python3 tools/shots.py OUT drive|watch|everything` | screenshots and frame rates, Metal GPU |

## Waiting on Sam

See the worklist rows in `SECOND-ORDER.md` (T1 to T3): the feel numbers, the music and the characters, and the words.
