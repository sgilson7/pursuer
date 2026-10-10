//! How long the rubble takes, with the city standing and with all of it
//! knocked loose at once. Ignored by default (it prints timings, which are
//! not a pass or a fail):
//!
//!     cargo test --release -p pursuer_core --test perf_probe -- --ignored --nocapture

use pursuer_core::city;
use pursuer_core::content::Palette;
use pursuer_core::fx::{self, int, ONE};
use pursuer_core::geom::{v3, V3};
use pursuer_core::rubble::Rubble;
use pursuer_core::terrain::Terrain;
use std::time::Instant;

fn city() -> (Terrain, Rubble) {
    let t = Terrain::new();
    let pal = Palette::parse(include_str!("../../../data/palette.json")).unwrap();
    let bp = city::plan(&t, &pal);
    let r = Rubble::new(&bp);
    (t, r)
}

#[test]
#[ignore]
fn the_whole_city_knocked_loose() {
    let (t, mut r) = city();
    let n = r.len();
    let start = Instant::now();
    for i in 0..n {
        let k = (i as i32 % 7) - 3;
        r.loose(i, v3(k * ONE, ONE, -k * ONE), V3::ZERO);
    }
    println!("pieces {n}, loosing all took {:?}", start.elapsed());
    let mut worst = 0u128;
    let mut total = 0u128;
    let steps = 600;
    for s in 0..steps {
        let t0 = Instant::now();
        r.step(&t, fx::ratio(1, 60), v3(0, 0, 0));
        let us = t0.elapsed().as_micros();
        worst = worst.max(us);
        total += us;
        if s % 60 == 0 {
            println!("step {s}: {us} µs, awake {}", r.awake_count());
        }
    }
    println!(
        "mean {} µs, worst {} µs, awake at end {}",
        total / steps as u128,
        worst,
        r.awake_count()
    );
    let t0 = Instant::now();
    let d = r.take_dirty();
    println!("dirty runs {} in {:?}", d.len() / 3, t0.elapsed());
}

#[test]
#[ignore]
fn a_pursuer_crossing_downtown() {
    let (t, mut r) = city();
    let mut at = v3(int(-450), 0, int(-300));
    let fwd = v3(ONE, 0, 0);
    let mut worst = 0u128;
    let mut total = 0u128;
    let steps = 60 * 30;
    for _ in 0..steps {
        let t0 = Instant::now();
        at = at.add(fwd.scale(ONE / 2));
        at.y = t.height(at.x, at.z);
        r.sweep(at, fwd, int(20), int(60), -ONE, ONE * 3, int(30));
        r.step(&t, fx::ratio(1, 60), at);
        let us = t0.elapsed().as_micros();
        worst = worst.max(us);
        total += us;
    }
    println!(
        "30 s crossing: mean {} µs, worst {} µs, loosed {}, awake peak {}",
        total / steps as u128,
        worst,
        r.stats.loosed,
        r.stats.awake_peak
    );
}
