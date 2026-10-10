//! The ground: a height field over the city, five hills, and the rim that
//! rises into mountains past the edge.
//!
//! Heights are kept at the corners of 5 m cells. Each cell is two triangles,
//! split from its (−x, −z) corner to its (+x, +z) corner, and `height` reads
//! the same triangles the page draws, so a wheel rests on the drawn ground
//! exactly. Past the grid, `raw` is the height directly.

use crate::fx::{self, int, ONE, TURN};
use crate::geom::{v3, V3};

/// Metres a cell is across.
pub const CELL: i32 = 5;
/// The grid covers `[-GRID, GRID]` metres on x and z.
pub const GRID: i32 = 520;
/// Cells a side.
pub const N: usize = (2 * GRID / CELL) as usize;
/// The city is `[-CITY, CITY]` metres square.
pub const CITY: i32 = 500;
/// Nothing drives or chases past this many metres from the middle.
pub const BOUND: i32 = 530;

/// A hill: its middle, its height and its radius, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hill {
    pub x: i32,
    pub z: i32,
    pub height: i32,
    pub radius: i32,
}

/// Five hills, after Seattle's: suburbs on the tall ones, one at the edge of
/// downtown. North is −z.
pub const HILLS: [Hill; 5] = [
    Hill {
        x: -330,
        z: 330,
        height: 38,
        radius: 120,
    },
    Hill {
        x: -360,
        z: -300,
        height: 30,
        radius: 100,
    },
    Hill {
        x: 310,
        z: 340,
        height: 34,
        radius: 110,
    },
    Hill {
        x: 400,
        z: -360,
        height: 24,
        radius: 90,
    },
    Hill {
        x: -110,
        z: 330,
        height: 20,
        radius: 75,
    },
];

/// The analytic height at a point, Q16 metres in and out.
pub fn raw(x: i32, z: i32) -> i32 {
    let mut h: i64 = 0;
    for hill in HILLS {
        let dx = (x - int(hill.x)) as i64;
        let dz = (z - int(hill.z)) as i64;
        let d2 = (dx * dx + dz * dz) >> 16;
        let r2 = (int(hill.radius) as i64 * int(hill.radius) as i64) >> 16;
        if d2 < r2 {
            let t = ONE as i64 - (d2 << 16) / r2;
            h += (int(hill.height) as i64 * ((t * t) >> 16)) >> 16;
        }
    }
    // a gentle swell, flattened round the house in the middle
    let (xm, zm) = (x >> 16, z >> 16);
    let swell = fx::mul(fx::sin(xm * TURN / 340), fx::cos(zm * TURN / 290)) * 3 / 2;
    let mid = V3 { x, y: 0, z }.len_xz();
    let flat = fx::div((mid - int(70)).clamp(0, int(140)), int(140));
    h += fx::mul(swell, flat) as i64;
    // the rim: past the city the ground rises into mountains
    let edge = x.abs().max(z.abs()) - int(CITY + 5);
    if edge > 0 {
        let e = edge as i64;
        let bumps = (fx::sin(xm * TURN / 197) + fx::cos(zm * TURN / 163) + ONE * 2) as i64;
        let rise = e / 4 + ((e * e) >> 16) / 900 + ((e * bumps) >> 16) / 12;
        h += rise.min(int(450) as i64);
    }
    h as i32
}

#[derive(Debug, Clone)]
pub struct Terrain {
    /// heights at the cell corners, (N + 1)² in rows of constant z
    h: Vec<i32>,
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new()
    }
}

impl Terrain {
    pub fn new() -> Terrain {
        let mut h = Vec::with_capacity((N + 1) * (N + 1));
        for j in 0..=N {
            for i in 0..=N {
                h.push(raw(corner(i), corner(j)));
            }
        }
        Terrain { h }
    }

    fn at(&self, i: usize, j: usize) -> i32 {
        self.h[j * (N + 1) + i]
    }

    /// The cell a point is in and where in it, or `None` past the grid.
    fn locate(x: i32, z: i32) -> Option<(usize, usize, i32, i32)> {
        let gx = x + int(GRID);
        let gz = z + int(GRID);
        if gx < 0 || gz < 0 || gx >= int(2 * GRID) || gz >= int(2 * GRID) {
            return None;
        }
        let (i, j) = ((gx / int(CELL)) as usize, (gz / int(CELL)) as usize);
        let u = (gx % int(CELL)) / CELL;
        let v = (gz % int(CELL)) / CELL;
        Some((i, j, u, v))
    }

    /// The drawn ground's height under a point.
    pub fn height(&self, x: i32, z: i32) -> i32 {
        let Some((i, j, u, v)) = Self::locate(x, z) else {
            return raw(x, z);
        };
        let (h00, h10, h01, h11) = (
            self.at(i, j),
            self.at(i + 1, j),
            self.at(i, j + 1),
            self.at(i + 1, j + 1),
        );
        if u >= v {
            h00 + fx::mul(u, h10 - h00) + fx::mul(v, h11 - h10)
        } else {
            h00 + fx::mul(u, h11 - h01) + fx::mul(v, h01 - h00)
        }
    }

    /// The drawn ground's unit normal under a point.
    pub fn normal(&self, x: i32, z: i32) -> V3 {
        let Some((i, j, u, v)) = Self::locate(x, z) else {
            let d = ONE;
            let hx = raw(x + d, z) - raw(x - d, z);
            let hz = raw(x, z + d) - raw(x, z - d);
            return v3(-hx / 2, ONE, -hz / 2).norm();
        };
        let (h00, h10, h01, h11) = (
            self.at(i, j),
            self.at(i + 1, j),
            self.at(i, j + 1),
            self.at(i + 1, j + 1),
        );
        let (dx, dz) = if u >= v {
            (h10 - h00, h11 - h10)
        } else {
            (h11 - h01, h01 - h00)
        };
        v3(-dx / CELL, ONE, -dz / CELL).norm()
    }

    /// The lowest ground under a rectangle, for setting a building's floor.
    pub fn lowest(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> i32 {
        let mut lo = i32::MAX;
        for k in 0..=4 {
            for l in 0..=4 {
                let x = x0 + (x1 - x0) / 4 * k;
                let z = z0 + (z1 - z0) / 4 * l;
                lo = lo.min(self.height(x, z));
            }
        }
        lo
    }

    /// The triangles of every grid cell whose kind is `kind`, as mesh
    /// vertices (position, normal), raised by `lift`.
    pub fn cells_mesh(&self, kinds: &[u8], kind: u8, lift: i32) -> Vec<i32> {
        let mut o = Vec::new();
        for j in 0..N {
            for i in 0..N {
                if kinds[j * N + i] != kind {
                    continue;
                }
                let p = |a: usize, b: usize| v3(corner(a), self.at(a, b) + lift, corner(b));
                let (p00, p10, p01, p11) = (p(i, j), p(i + 1, j), p(i, j + 1), p(i + 1, j + 1));
                tri(&mut o, p00, p11, p10);
                tri(&mut o, p00, p01, p11);
            }
        }
        o
    }

    /// The mountains round the city: a coarse grid out to `far` metres with
    /// the city's square cut out, sunk a little so its seam hides under the
    /// fine grid.
    pub fn outskirts_mesh(&self, far: i32, step: i32) -> Vec<i32> {
        let mut o = Vec::new();
        let n = 2 * far / step;
        let at = |k: i32| int(-far + k * step);
        for b in 0..n {
            for a in 0..n {
                let (x0, z0, x1, z1) = (at(a), at(b), at(a + 1), at(b + 1));
                let inside = |v: i32| v > int(-GRID) && v < int(GRID);
                if inside(x0) && inside(x1) && inside(z0) && inside(z1) {
                    continue;
                }
                let p = |x: i32, z: i32| v3(x, raw(x, z) - ONE / 3, z);
                tri(&mut o, p(x0, z0), p(x1, z1), p(x1, z0));
                tri(&mut o, p(x0, z0), p(x0, z1), p(x1, z1));
            }
        }
        o
    }
}

/// The coordinate of corner `i`, Q16 metres.
pub fn corner(i: usize) -> i32 {
    int(-GRID + i as i32 * CELL)
}

/// The cell index of a coordinate (clamped to the grid).
pub fn cell_of(v: i32) -> usize {
    (((v + int(GRID)) / int(CELL)).clamp(0, N as i32 - 1)) as usize
}

fn tri(o: &mut Vec<i32>, a: V3, b: V3, c: V3) {
    let n = b.sub(a).cross(c.sub(a)).norm();
    for p in [a, b, c] {
        o.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_five_hills_and_each_rises_above_the_city_floor() {
        let t = Terrain::new();
        assert_eq!(HILLS.len(), 5);
        for hill in HILLS {
            let top = t.height(int(hill.x), int(hill.z));
            assert!(top > int(hill.height - 4), "{hill:?}: {}", top / ONE);
        }
    }

    #[test]
    fn the_middle_is_flat_for_the_house() {
        let t = Terrain::new();
        for (x, z) in [(0, 0), (20, 10), (-30, -20), (40, 30)] {
            assert!(t.height(int(x), int(z)).abs() < ONE / 4, "{x},{z}");
        }
    }

    #[test]
    fn the_height_agrees_with_the_corners_and_the_edge_rises() {
        let t = Terrain::new();
        for (i, j) in [(10, 10), (100, 37), (150, 190)] {
            assert_eq!(t.height(corner(i), corner(j)), t.at(i, j));
        }
        assert!(raw(int(800), 0) > int(60), "{}", raw(int(800), 0) / ONE);
    }

    #[test]
    fn normals_point_up_and_lean_away_from_a_hilltop() {
        let t = Terrain::new();
        let h = HILLS[0];
        let n = t.normal(int(h.x + 60), int(h.z));
        assert!(n.y > ONE / 2 && n.x > ONE / 10, "{n:?}");
        assert!(t.normal(0, 0).y > ONE - ONE / 50);
    }
}
