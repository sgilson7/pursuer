# Second-order notebook

`B3 Build in self-questioning`. One row per assumption, surprise or divergence, written when it happened. *Divergence*: assumed, found, changed. *Finding*: something learned. *Worklist*: Sam's to do.

| # | Kind | Row |
|---|---|---|
| M1 | divergence | The request names "Are You Coming to the Tree" (The Hunger Games) for the chase. It is a commercial recording and composition, and this repository and its Pages site are public, so the game does not publish it or a transcription of its melody. The page plays an original tune (D minor, a hummed sine with vibrato over a drone) from `data/music.json`. Setting `file` there to the path of a recording under `web/` plays that on a loop instead. |
| M2 | divergence | The request says "a celebrity or character related to the item". The six pursuers are original characters drawn in TikZ (`tools/art.py`): a rock star, a wrestler, a teddy bear, a breakfast-TV host, a lounge singer and a queen. A real person's likeness as a monster in a public game raises publicity and defamation questions that are Sam's to weigh. Swapping in other art is one PNG per item. |
| M3 | divergence | The project lives in `LargeFlatEscape/`, the folder the session started in; the game, repository and site are named Pursuer (`sgilson7/pursuer`), as the request says. |
| M4 | divergence | "the car in the house" is a car in an attached garage at the house's east end, facing a driveway to the avenue. The pursuer rises west of the house, so the garage door faces away from it. |
| M5 | divergence | "Bridges begin to fall apart": the city has the one overpass the request asks for and no river, so the overpass is the bridge. Its spans need both pillars, so one pillar knocked out drops two spans. |
| P1 | finding | Shapes built with left-handed axes (the walls, the road dashes, the pursuer's strut) were drawn inside out: their back faces showed and the outline pass drew over them, so the fallen walls read as a plum slab. Test `every_drawn_shape_is_right_handed`; flipping one dash's axis back fails it ("house: instance 145 is mirrored"). |
| P2 | finding | The car's roll-over key first lifted the car whenever it was not upright and in the air, and it climbed 30 m. The lift now acts only within 3 m of the ground and below 2 m/s upward. A car on its roof rolled about its nose axis, since `up × UP` is zero when upside down. Test `the_flip_key_rolls_an_upside_down_car_back_onto_its_wheels`. |
| P3 | finding | The walker's step height (0.45 m) equalled the couch seat's height above the floor, so it climbed the seat and then the back and walked over the couch. Step is 0.35 m. |
| P4 | finding | Loose pieces did not sleep: resting on the ground, their speed hovered near 0.2 m/s and a tick with no contact reset the count. Contacts now bleed speed (4% a step, spin 10%) and a piece sleeps after 20 slow steps, touching or not. Test `a_falling_box_lands_on_the_ground_and_sleeps_there`. |
| P5 | finding | Headless Chromium on SwiftShader drew the city at 2 frames a second and a click timed out. On a software renderer (named in `data/display.json`) the core draws no outlines, to 320 m, with one mirror and no mountains or clouds: 16 frames a second. A person's GPU is unaffected. |
| P6 | finding | `rubble` stepped only during a chase, so `#everything` from the title measured nothing. It now steps in every mode that is not paused, which costs nothing when no piece is moving. |
| P7 | finding | Headless Chromium's synthetic mouse moves carry no `movementX`, so `tools/shots.py` turns the view by dispatching a `mousemove` with one. |
| T1 | worklist | Every number of the feel is the agent's guess until Sam plays it: car speed and grip (`car.rs` BASE), the pursuer's speeds and turn rate (`data/pursuer.json`), upgrade times (`data/upgrades.json`), the five-second hold (`data/items.json`). |
| T2 | worklist | Sam: whether the music should be the original tune, silence, or a licensed recording (M1); whether the pursuers should stay original characters (M2). |
| T3 | worklist | Sam: the words in `data/copy.json`, `data/items.json` (each pursuer's line) and `data/upgrades.json`. |
