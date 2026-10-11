# Handoff

10 October 2026. Where Pursuer stands.

## What exists

| What | Where |
| --- | --- |
| Repository | https://github.com/sgilson7/pursuer (public, MIT) |
| The game | https://sgilson7.github.io/pursuer/ (Pages, deployed by `.github/workflows/deploy.yml` after the check passes) |
| Website page | https://sgilson7.github.io/projects/pursuer/, from `~/Documents/website` commit `932e6bc` (`content/projects/pursuer.md`, an entry in `content/projects.yaml`, `/pursuer/` in `OFF_BUILD_ROUTES`) |

## Done, against the brief

- The house: seven items, each with a pursuer (the record brings Kanye West); a record player picks the chase music, with its song's name floating over it; let go before five seconds and it goes back; squeaky footsteps and pick-up sounds and nothing else.
- The fold: the walls fall outward flat, the roof blows away, the pursuer stands up 230 m west and comes.
- The car: in the garage, entered with E, driven in first person with a rear-view mirror and two door mirrors.
- The city: 1 km square, Manhattan grid, downtown, mid-rise, suburbs, five hills, one overpass; 19,857 breakable pieces.
- Destruction: the sweep, supports, whole-building collapse, spans that need both pillars, loose pieces that tumble and sleep.
- Abilities by time survived, one drawn at random at each of seven times (`data/upgrades.json`): big wheels (Q), wings (F), dash (C), instant turn (X), nitrous (V), leap (Space) and three turbos in order; R rolls the car over (4.1 s from roof to wheels); no fall damage.
- The car rolls over in too hard a turn at speed. The pursuer speeds up to 75 m/s and, every 25 to 45 s, jumps to 120 m ahead of you.
- Score: time survived, best kept in local storage.
- Phones and tablets: touch controls (`web/touch.js`, `data/touch.json`): a stick, a look drag, and buttons for Grab, Run, Jump, Brake, Flip and each ability drawn; a pause button; words that name the buttons; no pointer lock; drawn at no more than 2x; a view that stays about 70° wide held upright, with the mirrors across the top; the rear-view mirror tilts up to the pursuer's face when it is close; the recording unlocked for iOS by the first tap. Checked in WebKit as an iPad Pro 11 and an iPhone 15 (`tools/mobile.py`, and `webkit-iphone` in the browser check); not on a real device.
- Three TikTok shorts of Kanye West chasing a phone player, filmed from the real page by `tools/film/` (after Vagrancy's): `~/Movies/Pursuer/pursuer-mirror.mp4`, `pursuer-ruins.mp4`, `pursuer-whole_run.mp4`, with posting notes in `~/Movies/Pursuer/tiktok-post.txt`. Not posted.
- Performance: 60 frames a second in Chromium on an Apple Silicon Mac during a chase and with the whole city loose (`MEASUREMENTS.md`).

## Not done, or done differently

- The Hanging Tree (sped up, from 0:54.85) is the default chase music, deployed with the game at Sam's decision (SECOND-ORDER M6, T5).
- Six pursuers are original characters; Kanye West is Sam's addition, from a photograph (M2, M7).
- No river: the overpass is the bridge (M5).
- Frame rates on integrated Intel or AMD graphics, and in Firefox and Safari, were not measured.

## Commands run, and results

| Command | Result |
| --- | --- |
| `./scripts/check.sh` (five runs) | BUILDER CHECK PASSED: fmt, clippy, 95 core tests (the fifth run added the still house after a catch) and 11 boundary tests (fifth round: touch controls, the tall view, the tilting mirror, staging for film), wasm build, import audit, browser check in Chromium, Firefox, WebKit and WebKit as an iPhone 15 played by touch |
| `python tools/mobile.py OUT "iPad Pro 11" --landscape`, and `"iPhone 15"` both ways | screenshots of touch play in WebKit |
| `python tools/film/shorts.py mirror|ruins|whole_run` | the three shorts in `~/Movies/Pursuer/` |
| `cargo test --release -p pursuer_core --test perf_probe -- --ignored --nocapture` | see `MEASUREMENTS.md` |
| `python3 tools/shots.py OUT drive|watch|everything` | screenshots and frame rates, Metal GPU |

Live checks: the Pursuer browser check against https://sgilson7.github.io/pursuer passed in Chromium; the website's `make gate-live` passed in three engines.

The website page went live at 12:41 inside build `915132a3`, published by another session's deploy that built from the working tree while this change was still uncommitted; this session's `make deploy` at 12:45 found the same build live, and commit `932e6bc` records the change.

## Waiting on Sam

See the worklist rows in `SECOND-ORDER.md` (T1 to T3): the feel numbers, the music and the characters, and the words. T7: play it on a real iPad and iPhone. T8: whether and how to post the shorts.
