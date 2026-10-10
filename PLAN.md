# Plan

Stage 2. Approved in advance (BRIEF.md, Autonomy).

## The game in one paragraph

**Pursuer.** You start in a house in the middle of a city. Pick up one of six things and hold it for five seconds, and the house folds flat as a Paper Mario house opens, the roof blows away, and a giant flat cut-out of a character tied to that thing stands up 230 m west and walks at you. It goes straight through whatever is in the way and throws the pieces forward. Run to the car in the garage, drive, and watch it in the mirrors. The car gains abilities as you last. The score is the time from the pursuer standing up to it catching you.

## Inspirations, and what each contributes

| Source | What it gives this game |
| --- | --- |
| Crazy Taxi | the arcade car: planted, fast, forgiving, steering that eases off with speed |
| Battlefield: Bad Company | buildings that come apart piece by piece and come down whole when their ground floor goes |
| Paper Mario | the house as a paper diorama that opens by folding its walls flat |
| New York, Seattle | the Manhattan grid of avenues and long blocks; Seattle's hills with houses on them |
| Iron Vector, Food Truck Circus, Long Time Comin' | the engine: Q16.16 core, instanced draws, the core computing every matrix, ink outlines, TikZ art |
| Vagrancy | the sticky-feet stand-up, as the car's roll-over key |
| Sam_Comm_final.png | the palette and the look: a monster too big for the street, debris in the air |

## Architecture

- `crates/core` (`pursuer_core`): every rule, Q16.16 integers, no floats, clocks or I/O. `fx`, `geom`, `rng` and `mesh` from Long Time Comin'.
- `crates/shim` (`pursuer_shim`): one call per function, no branches. The city's instance buffer is read by the page straight from the module's memory through a pointer; nothing is copied.
- `web/`: WebGL 2. Instances and meshes are the core's integers, read as integer attributes and scaled in the shader. Up to four views a frame, each with its own list of draws from the core.
- `data/`: palette, items, pursuer, upgrades, copy, controls, sounds, music, display, shaders.

## Modules

| Module | Holds |
| --- | --- |
| `terrain` | the height field, five hills, the rim of mountains, the ground meshes |
| `city` | roads, blocks, districts, and every building, tree and span of the overpass as pieces with their supports |
| `rubble` | the pieces' states, the grid, support and collapse, the physics of loose pieces, the upload runs |
| `car` | ray-cast wheels, tyre forces, body collisions, hover, flight, the roll-over key, the kit |
| `walker` | walking, stepping up, squeaks |
| `house` | walls with openings, furniture, items, the fold |
| `monster` | rising, turning, speeding up, the sweep, the catch |
| `render` | instance layout, views, frustum culling of structures, mirrors |
| `game` | modes, the 60 Hz step, cameras, the cockpit, the HUD |

## Decisions

| Decision | Choice | Why |
| --- | --- | --- |
| Smallest piece | 8 m towers, 6 × 7 m mid-rise, 4 × 3 m house modules; shells with full roofs | 20,000 pieces holds the frame on a laptop GPU with outlines and three mirrors |
| Cost ceiling | 2,600 fully simulated moving pieces, nearest the player; the rest fall only | the whole city loose at once has to stay playable |
| Collapse | a piece needs its supporter (or both, for a span); a building goes whole at 40% of its ground floor | gives Bad Company's sudden collapses without a stress solver |
| Pursuer size | 40 × 60 m, the art's 2:3 | big enough to fill a mirror from 200 m |
| Pursuer speed | 9 m/s rising 0.16 m/s each second to 46; 60% faster beyond 240 m | the car (40 m/s, more with turbo) outruns it on a straight; it cuts corners |
| Music | an original synthesized tune; a recording can be dropped in | see SECOND-ORDER M1 |
| Characters | six original characters | see SECOND-ORDER M2 |
