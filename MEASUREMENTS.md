# Measurements

What was measured, how, and on what. Each number is one run.

## The rubble, native (`crates/core/tests/perf_probe.rs`, release build, Apple Silicon)

| Case | Result |
| --- | --- |
| All 19,857 pieces knocked loose at once | 14,345 still moving after the first step; mean 3.5 ms a step over 600 steps, worst 4.6 ms; 1,538 still moving after 10 s |
| A pursuer 40 m wide crossing downtown for 30 s | mean 1.0 ms a step, worst 2.4 ms; 2,104 pieces knocked loose, at most 1,519 moving at once |

## The page (`tools/shots.py`, Chromium 1280 × 720)

| Renderer | Case | Frames a second |
| --- | --- | --- |
| Apple GPU through Metal (ANGLE) | the pursuer crossing two blocks toward the house, eight samples of 2 s | 60.1 to 60.5 |
| Apple GPU through Metal (ANGLE) | `#everything`, the whole city falling, six samples of 2 s | 60.0 to 60.5 |
| SwiftShader (software) | the title, low-detail tier | 16 |

Not measured: an integrated Intel or AMD laptop GPU, Firefox or Safari frame rates, a phone.
