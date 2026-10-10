//! The city's plan: roads, blocks, districts, and every building, tree and
//! span of the overpass as a list of breakable pieces.
//!
//! The grid is Manhattan's: avenues run north and south 100 m apart, streets
//! east and west 80 m apart, and a ring road closes the edge. Downtown's
//! towers stand north of the middle; the suburbs climb the hills to the south
//! and west, as Seattle's do. The house is in the middle block, a park. One
//! elevated road runs east and west over the widened street at z = 120.
//!
//! Each structure is a run of pieces. A piece names the pieces holding it up
//! (`sup`): with `all` false one is enough, with `all` true it needs every one.
//! A piece with none stands on the ground.

use crate::content::{Palette, Rgb};
use crate::fx::{self, int, ONE};
use crate::geom::{v3, V3};
use crate::quat::Quat;
use crate::rng::Rng;
use crate::terrain::{Terrain, CELL, CITY, GRID, N};

pub const KIND_GRASS: u8 = 0;
pub const KIND_ROAD: u8 = 1;
pub const KIND_WALK: u8 = 2;
pub const KINDS: [u8; 3] = [KIND_GRASS, KIND_ROAD, KIND_WALK];

/// How the shader draws a piece's sides.
pub const STYLE_PLAIN: i32 = 0;
pub const STYLE_TOWER: i32 = 1;
pub const STYLE_MID: i32 = 2;
pub const STYLE_HOUSE: i32 = 3;
pub const STYLE_DECK: i32 = 4;
/// lit from inside: drawn at full brightness
pub const STYLE_GLOW: i32 = 5;

/// Avenues (constant x) and streets (constant z): where, and half their width.
pub const AVENUES: [(i32, i32); 12] = [
    (-495, 5),
    (-450, 5),
    (-350, 5),
    (-250, 5),
    (-150, 5),
    (-50, 5),
    (50, 5),
    (150, 5),
    (250, 5),
    (350, 5),
    (450, 5),
    (495, 5),
];
pub const STREETS: [(i32, i32); 14] = [
    (-495, 5),
    (-440, 5),
    (-360, 5),
    (-280, 5),
    (-200, 5),
    (-120, 5),
    (-40, 5),
    (40, 5),
    (OVERPASS_Z, 10),
    (200, 5),
    (280, 5),
    (360, 5),
    (440, 5),
    (495, 5),
];

/// The overpass runs over this street.
pub const OVERPASS_Z: i32 = 120;
/// Its deck is this high over the street between the ramps.
pub const DECK_HEIGHT: i32 = 11;
/// The deck's level stretch, and where each ramp meets the street.
pub const DECK_LEVEL: i32 = 320;
pub const DECK_END: i32 = 430;
const SEGMENT: i32 = 10;

/// Where downtown is thickest, and how far it reaches.
pub const DOWNTOWN: (i32, i32) = (40, -230);
const DOWNTOWN_REACH: i32 = 290;

/// The garage door faces east down a driveway to the avenue at x = 50.
pub const DRIVEWAY: (i32, i32, i32) = (13, 46, 5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum District {
    Home,
    Downtown,
    Midtown,
    Suburb,
    Park,
}

/// A block between road edges, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    pub district: District,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PieceSpec {
    pub pos: V3,
    pub half: V3,
    pub rot: Quat,
    pub rgb: Rgb,
    pub style: i32,
    pub sup: [u32; 4],
    pub nsup: u8,
    pub all: bool,
    /// on the ground floor of a building (counted for its collapse)
    pub base: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupSpec {
    pub start: u32,
    pub end: u32,
    /// a building, which comes down whole once enough of its ground floor goes
    pub building: bool,
}

/// A painted mark on a road: a centre-line dash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub pos: V3,
    pub along_x: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub towers: u32,
    pub midrises: u32,
    pub houses: u32,
    pub trees: u32,
    pub overpass_pieces: u32,
    pub tallest: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Blueprint {
    pub pieces: Vec<PieceSpec>,
    pub groups: Vec<GroupSpec>,
    /// the ground kind of each grid cell, rows of constant z
    pub kinds: Vec<u8>,
    pub blocks: Vec<Block>,
    pub marks: Vec<Mark>,
    pub counts: Counts,
}

fn interval(list: &[(i32, i32)], v: i32) -> Option<(i32, i32)> {
    list.windows(2)
        .map(|w| (w[0].0 + w[0].1, w[1].0 - w[1].1))
        .find(|&(lo, hi)| v >= lo && v < hi)
}

/// Whether a point (metres) is on a road.
pub fn on_road(x: i32, z: i32) -> bool {
    if x.abs() > CITY || z.abs() > CITY {
        return false;
    }
    let driveway = x >= DRIVEWAY.0 && x < DRIVEWAY.1 && z.abs() < DRIVEWAY.2;
    driveway
        || AVENUES.iter().any(|&(a, h)| (x - a).abs() < h)
        || STREETS.iter().any(|&(s, h)| (z - s).abs() < h)
}

pub fn downtown_score(x: i32, z: i32) -> i32 {
    let d = v3(int(x - DOWNTOWN.0), 0, int(z - DOWNTOWN.1)).len_xz();
    (ONE - fx::div(d, int(DOWNTOWN_REACH))).max(0)
}

fn district_of(x0: i32, z0: i32, x1: i32, z1: i32, t: &Terrain) -> District {
    let (cx, cz) = ((x0 + x1) / 2, (z0 + z1) / 2);
    if x0 <= 0 && x1 > 0 && z0 <= 0 && z1 > 0 {
        return District::Home;
    }
    let h = t.height(int(cx), int(cz));
    if h >= int(22) {
        return District::Park;
    }
    if downtown_score(cx, cz) >= ONE * 2 / 5 {
        return District::Downtown;
    }
    if h >= int(7) || cz >= 190 || cx <= -270 {
        return District::Suburb;
    }
    let mut r = Rng::new((cx * 7919 + cz * 104_729) as u64);
    if r.chance(9) {
        return District::Park;
    }
    District::Midtown
}

pub fn blocks(t: &Terrain) -> Vec<Block> {
    let mut out = Vec::new();
    for zw in STREETS.windows(2) {
        for xw in AVENUES.windows(2) {
            let (x0, x1) = (xw[0].0 + xw[0].1, xw[1].0 - xw[1].1);
            let (z0, z1) = (zw[0].0 + zw[0].1, zw[1].0 - zw[1].1);
            out.push(Block {
                x0,
                z0,
                x1,
                z1,
                district: district_of(x0, z0, x1, z1, t),
            });
        }
    }
    out
}

/// The ground kind of every grid cell.
pub fn kinds(blocks: &[Block]) -> Vec<u8> {
    let mut k = vec![KIND_GRASS; N * N];
    for j in 0..N {
        for i in 0..N {
            let x = -GRID + i as i32 * CELL + CELL / 2;
            let z = -GRID + j as i32 * CELL + CELL / 2;
            k[j * N + i] = if x.abs() > CITY || z.abs() > CITY {
                KIND_GRASS
            } else if on_road(x, z) {
                KIND_ROAD
            } else {
                let found = interval(&AVENUES, x)
                    .zip(interval(&STREETS, z))
                    .and_then(|((x0, _), (z0, _))| blocks.iter().find(|b| b.x0 == x0 && b.z0 == z0));
                match found {
                    Some(b) => {
                        let rim = x - b.x0 < CELL || b.x1 - x < CELL || z - b.z0 < CELL || b.z1 - z < CELL;
                        match b.district {
                            _ if rim => KIND_WALK,
                            District::Downtown | District::Midtown => KIND_WALK,
                            _ => KIND_GRASS,
                        }
                    }
                    None => KIND_GRASS,
                }
            };
        }
    }
    k
}

struct Builder<'a> {
    bp: Blueprint,
    t: &'a Terrain,
    pal: &'a Palette,
}

impl Builder<'_> {
    fn begin(&self) -> u32 {
        self.bp.pieces.len() as u32
    }

    fn end(&mut self, start: u32, building: bool) {
        let end = self.bp.pieces.len() as u32;
        if end > start {
            self.bp.groups.push(GroupSpec { start, end, building });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn piece(&mut self, pos: V3, half: V3, rot: Quat, rgb: Rgb, style: i32, sup: &[u32], all: bool) -> u32 {
        let mut s = [0u32; 4];
        for (k, &v) in sup.iter().take(4).enumerate() {
            s[k] = v;
        }
        self.bp.pieces.push(PieceSpec {
            pos,
            half,
            rot,
            rgb,
            style,
            sup: s,
            nsup: sup.len().min(4) as u8,
            all,
            base: false,
        });
        self.bp.pieces.len() as u32 - 1
    }

    /// A building of `nx × ny × nz` modules from the corner `(x0, z0)`. Big
    /// footprints are a shell with a full roof; the pieces inside the shell
    /// would never be seen and would cost every frame.
    #[allow(clippy::too_many_arguments)]
    fn building(
        &mut self,
        x0: i32,
        z0: i32,
        n: (usize, usize, usize),
        module: (i32, i32),
        rgb: Rgb,
        style: i32,
        roof: Option<Rgb>,
    ) -> Vec<u32> {
        let (nx, ny, nz) = n;
        let (mw, mh) = module;
        let start = self.begin();
        let x1 = x0 + mw * nx as i32;
        let z1 = z0 + mw * nz as i32;
        let base = self.t.lowest(x0, z0, x1, z1) - ONE / 2;
        let hollow = nx >= 3 && nz >= 3;
        let mut idx = vec![u32::MAX; nx * ny * nz];
        let at = |ix: usize, iy: usize, iz: usize| (iy * nz + iz) * nx + ix;
        let half = v3(mw / 2, mh / 2, mw / 2);
        for iy in 0..ny {
            for iz in 0..nz {
                for ix in 0..nx {
                    let rim = ix == 0 || iz == 0 || ix == nx - 1 || iz == nz - 1;
                    if hollow && !rim && iy != ny - 1 {
                        continue;
                    }
                    let pos = v3(
                        x0 + mw * ix as i32 + mw / 2,
                        base + mh * iy as i32 + mh / 2,
                        z0 + mw * iz as i32 + mw / 2,
                    );
                    let mut sup = Vec::new();
                    if iy > 0 {
                        let below = idx[at(ix, iy - 1, iz)];
                        if below != u32::MAX {
                            sup.push(below);
                        } else {
                            // a roof piece over the hollow: held by the
                            // shell below it in each of the four directions
                            let row = |x: usize, z: usize| idx[at(x, iy - 1, z)];
                            sup.push(row(0, iz));
                            sup.push(row(nx - 1, iz));
                            sup.push(row(ix, 0));
                            sup.push(row(ix, nz - 1));
                        }
                    }
                    let i = self.piece(pos, half, Quat::IDENTITY, rgb, style, &sup, false);
                    if iy == 0 {
                        self.bp.pieces[i as usize].base = true;
                    }
                    idx[at(ix, iy, iz)] = i;
                }
            }
        }
        let top = base + mh * ny as i32;
        let mut tops: Vec<u32> = (0..nz)
            .flat_map(|iz| (0..nx).map(move |ix| (ix, iz)))
            .map(|(ix, iz)| idx[at(ix, ny - 1, iz)])
            .collect();
        if let Some(roof_rgb) = roof {
            // a roof slab a metre thick on every top piece
            let mut slabs = Vec::new();
            for iz in 0..nz {
                for ix in 0..nx {
                    let pos = v3(
                        x0 + mw * ix as i32 + mw / 2,
                        top + ONE / 2,
                        z0 + mw * iz as i32 + mw / 2,
                    );
                    let below = idx[at(ix, ny - 1, iz)];
                    slabs.push(self.piece(
                        pos,
                        v3(mw / 2, ONE / 2, mw / 2),
                        Quat::IDENTITY,
                        roof_rgb,
                        STYLE_PLAIN,
                        &[below],
                        false,
                    ));
                }
            }
            tops = slabs;
        }
        self.end(start, true);
        let _ = top;
        tops
    }

    /// Rooftop clutter after Sam_Comm_final: air-conditioning boxes and tanks.
    fn clutter(&mut self, tops: &[u32], r: &mut Rng, count: i32) {
        if tops.is_empty() {
            return;
        }
        let start = self.begin();
        for _ in 0..count {
            let on = tops[r.range(0, tops.len() as i32 - 1) as usize];
            let p = self.bp.pieces[on as usize];
            let (sx, sy, sz) = (
                r.range(8, 22) * ONE / 10,
                r.range(8, 30) * ONE / 10,
                r.range(8, 22) * ONE / 10,
            );
            let pos = v3(
                p.pos.x + r.range(-(p.half.x - sx / 2) / 256, (p.half.x - sx / 2) / 256) * 256,
                p.pos.y + p.half.y + sy / 2,
                p.pos.z + r.range(-(p.half.z - sz / 2) / 256, (p.half.z - sz / 2) / 256) * 256,
            );
            let rgb = self.pal.pick("clutter", r.range(0, 7) as usize);
            self.piece(
                pos,
                v3(sx / 2, sy / 2, sz / 2),
                Quat::IDENTITY,
                rgb,
                STYLE_PLAIN,
                &[on],
                false,
            );
        }
        self.end(start, false);
    }

    fn tree(&mut self, x: i32, z: i32, r: &mut Rng) {
        let y = self.t.height(x, z);
        let trunk_h = r.range(25, 45) * ONE / 10;
        let trunk = self.piece(
            v3(x, y + trunk_h / 2 - ONE / 4, z),
            v3(ONE * 3 / 10, trunk_h / 2, ONE * 3 / 10),
            Quat::IDENTITY,
            self.pal.get("trunk"),
            STYLE_PLAIN,
            &[],
            false,
        );
        let c = r.range(16, 26) * ONE / 10;
        let crown = self.pal.pick("crown", r.range(0, 7) as usize);
        self.piece(
            v3(x, y + trunk_h - ONE / 4 + c * 8 / 10, z),
            v3(c, c * 8 / 10, c),
            Quat::yaw(r.range(0, fx::TURN / 4)),
            crown,
            STYLE_PLAIN,
            &[trunk],
            false,
        );
        self.bp.counts.trees += 1;
    }

    fn downtown(&mut self, b: Block, r: &mut Rng) {
        let (x0, z0, x1, z1) = (b.x0 + CELL, b.z0 + CELL, b.x1 - CELL, b.z1 - CELL);
        let s = downtown_score((x0 + x1) / 2, (z0 + z1) / 2);
        let kx = r.range(1, 2);
        let kz = r.range(1, 2);
        let (w, d) = ((x1 - x0) / kx, (z1 - z0) / kz);
        for a in 0..kx {
            for c in 0..kz {
                let module = 8;
                let nx = ((w - 6) / module).clamp(2, 6) as usize;
                let nz = ((d - 6) / module).clamp(2, 6) as usize;
                let lx = x0 + a * w + (w - nx as i32 * module) / 2;
                let lz = z0 + c * d + (d - nz as i32 * module) / 2;
                let height = 30 + fx::mul(int(170), s) / ONE * r.range(55, 100) / 100;
                let ny = (height / module).max(3) as usize;
                let rgb = self.pal.pick("tower", r.range(0, 11) as usize);
                let tops = self.building(
                    int(lx),
                    int(lz),
                    (nx, ny, nz),
                    (int(module), int(module)),
                    rgb,
                    STYLE_TOWER,
                    None,
                );
                let count = r.range(1, 4);
                self.clutter(&tops, r, count);
                self.bp.counts.towers += 1;
                self.bp.counts.tallest = self.bp.counts.tallest.max(ny as i32 * module);
            }
        }
    }

    fn midtown(&mut self, b: Block, r: &mut Rng) {
        let (x0, z0, x1, z1) = (b.x0 + CELL, b.z0 + CELL, b.x1 - CELL, b.z1 - CELL);
        let s = downtown_score((x0 + x1) / 2, (z0 + z1) / 2);
        let kx = r.range(2, 3);
        let kz = 2;
        let (w, d) = ((x1 - x0) / kx, (z1 - z0) / kz);
        for a in 0..kx {
            for c in 0..kz {
                if r.chance(12) {
                    // a gap: a lot with a tree or two
                    let n = r.range(1, 2);
                    let start = self.begin();
                    for _ in 0..n {
                        let tx = x0 + a * w + r.range(4, w - 4);
                        let tz = z0 + c * d + r.range(4, d - 4);
                        self.tree(int(tx), int(tz), r);
                    }
                    self.end(start, false);
                    continue;
                }
                let module = 6;
                let nx = ((w - 4) / module).clamp(2, 5) as usize;
                let nz = ((d - 4) / module).clamp(2, 4) as usize;
                let lx = x0 + a * w + (w - nx as i32 * module) / 2;
                let lz = z0 + c * d + (d - nz as i32 * module) / 2;
                let floors = r.range(1, 3) + fx::mul(int(4), s) / ONE + r.range(0, 1);
                let rgb = self.pal.pick("mid", r.range(0, 11) as usize);
                let tops = self.building(
                    int(lx),
                    int(lz),
                    (nx, floors as usize, nz),
                    (int(module), int(7)),
                    rgb,
                    STYLE_MID,
                    None,
                );
                let count = r.range(0, 2);
                self.clutter(&tops, r, count);
                self.bp.counts.midrises += 1;
            }
        }
    }

    fn suburb(&mut self, b: Block, r: &mut Rng) {
        let (x0, z0, x1, z1) = (b.x0 + CELL, b.z0 + CELL, b.x1 - CELL, b.z1 - CELL);
        let lots = ((x1 - x0) / 18).max(1);
        let w = (x1 - x0) / lots;
        let d = (z1 - z0) / 2;
        for a in 0..lots {
            for c in 0..2 {
                let module = 4;
                let nx = r.range(2, 3) as usize;
                let nz = 2;
                let storeys = r.range(1, 2) as usize;
                let lx = x0 + a * w + (w - nx as i32 * module) / 2;
                // set toward the street the lot faces
                let lz = if c == 0 {
                    z0 + 4
                } else {
                    z1 - 4 - nz as i32 * module
                };
                let rgb = self.pal.pick("house", r.range(0, 11) as usize);
                let roof = self.pal.pick("roof", r.range(0, 7) as usize);
                self.building(
                    int(lx),
                    int(lz),
                    (nx, storeys, nz),
                    (int(module), int(3)),
                    rgb,
                    STYLE_HOUSE,
                    Some(roof),
                );
                self.bp.counts.houses += 1;
                let start = self.begin();
                if r.chance(70) {
                    let tz = if c == 0 { z0 + d - 4 } else { z0 + d + 4 };
                    let tx = x0 + a * w + r.range(3, (w - 3).max(4));
                    self.tree(int(tx), int(tz), r);
                }
                self.end(start, false);
            }
        }
    }

    fn park(&mut self, b: Block, r: &mut Rng, keep_clear: Option<(i32, i32, i32, i32)>) {
        let start = self.begin();
        let mut z = b.z0 + 10;
        while z < b.z1 - 6 {
            let mut x = b.x0 + 10;
            while x < b.x1 - 6 {
                let (tx, tz) = (x + r.range(-4, 4), z + r.range(-4, 4));
                let clear = keep_clear
                    .map(|(a, c, e, f)| tx >= a && tx <= e && tz >= c && tz <= f)
                    .unwrap_or(false);
                if !clear && r.chance(75) {
                    self.tree(int(tx), int(tz), r);
                }
                x += 14;
            }
            z += 14;
        }
        self.end(start, false);
    }

    /// The overpass: two lanes of deck in 10 m spans, each span held by the
    /// pillars at both its ends (it needs both), with a barrier on the outer
    /// edge of each lane. Pillars stand clear of the avenues it crosses.
    fn overpass(&mut self) {
        let deck_rgb = self.pal.get("deck");
        let pillar_rgb = self.pal.get("pillar");
        let barrier_rgb = self.pal.get("barrier");
        let height = |x: i32| -> i32 {
            let ax = x.abs();
            if ax <= DECK_LEVEL {
                int(DECK_HEIGHT)
            } else {
                fx::mul(
                    int(DECK_HEIGHT),
                    fx::div(int(DECK_END - ax), int(DECK_END - DECK_LEVEL)),
                )
                .max(0)
            }
        };
        let z_mid = int(OVERPASS_Z);
        let lanes = [z_mid - int(4), z_mid + int(4)];
        let ground = |x: i32| self.t.height(int(x), z_mid);
        let top = |x: i32| ground(x) + height(x);
        let clear_of_avenues = |x: i32| AVENUES.iter().all(|&(a, h)| (x - a).abs() >= h + 4);
        // pillars: where each stands, and the top piece of each lane's pillar
        let mut pillars: Vec<(i32, [u32; 2])> = Vec::new();
        let start_all = self.begin();
        let mut x = -DECK_END;
        let mut start = self.begin();
        let mut spans = 0;
        while x <= DECK_END {
            if height(x) >= int(2) && clear_of_avenues(x) {
                let mut tops = [0u32; 2];
                for (k, &lz) in lanes.iter().enumerate() {
                    let g = ground(x) - ONE;
                    let bottom = top(x) - int(1) - ONE / 5;
                    let tall = bottom - g;
                    let n = (tall / int(4) + 1).max(1);
                    let piece_h = tall / n;
                    let mut below: Option<u32> = None;
                    for i in 0..n {
                        let y = g + piece_h * i + piece_h / 2;
                        let sup: Vec<u32> = below.into_iter().collect();
                        let p = self.piece(
                            v3(int(x), y, lz),
                            v3(ONE, piece_h / 2, ONE),
                            Quat::IDENTITY,
                            pillar_rgb,
                            STYLE_PLAIN,
                            &sup,
                            false,
                        );
                        below = Some(p);
                    }
                    tops[k] = below.unwrap_or(0);
                }
                pillars.push((x, tops));
            }
            x += SEGMENT;
        }
        // deck spans
        let mut x = -DECK_END;
        while x < DECK_END {
            let (xa, xb) = (x, x + SEGMENT);
            let (ya, yb) = (top(xa), top(xb));
            let dy = yb - ya;
            let len = v3(int(SEGMENT), dy, 0).len();
            let pitch = fx::atan2(dy, int(SEGMENT));
            let rot = Quat::axis_angle(v3(0, 0, ONE), pitch);
            let up = rot.rotate(V3::UP);
            for (k, &lz) in lanes.iter().enumerate() {
                let mid = v3(int(xa + SEGMENT / 2), (ya + yb) / 2, lz);
                let low = height(xa) < int(2) && height(xb) < int(2);
                let left = pillars.iter().rev().find(|p| p.0 <= xa).map(|p| p.1[k]);
                let right = pillars.iter().find(|p| p.0 >= xb).map(|p| p.1[k]);
                let sup: Vec<u32> = if low {
                    Vec::new()
                } else {
                    left.into_iter().chain(right).collect()
                };
                let deck = self.piece(
                    mid.sub(up.scale(ONE * 6 / 10)),
                    v3(len / 2, ONE * 6 / 10, int(4)),
                    rot,
                    deck_rgb,
                    STYLE_DECK,
                    &sup,
                    true,
                );
                let edge = if k == 0 {
                    lz - int(4) + ONE / 4
                } else {
                    lz + int(4) - ONE / 4
                };
                self.piece(
                    v3(mid.x, mid.y, edge).add(up.scale(ONE / 2)),
                    v3(len / 2, ONE / 2, ONE / 4),
                    rot,
                    barrier_rgb,
                    STYLE_PLAIN,
                    &[deck],
                    false,
                );
            }
            spans += 1;
            if spans % 6 == 0 {
                // pillars and spans for culling: close a group every 60 m
                self.end(start, false);
                start = self.begin();
            }
            x = xb;
        }
        self.end(start, false);
        self.bp.counts.overpass_pieces = self.begin() - start_all;
    }

    fn marks(&mut self) {
        let step = 12;
        for &(a, h) in AVENUES.iter() {
            if h < 5 || a.abs() > 480 {
                continue;
            }
            let mut z = -490;
            while z < 490 {
                let on_street = STREETS.iter().any(|&(s, sh)| (z - s).abs() < sh + 3);
                if !on_street {
                    self.bp.marks.push(Mark {
                        pos: v3(int(a), self.t.height(int(a), int(z)), int(z)),
                        along_x: false,
                    });
                }
                z += step;
            }
        }
        for &(s, h) in STREETS.iter() {
            if h < 5 || s.abs() > 480 {
                continue;
            }
            let mut x = -490;
            while x < 490 {
                let on_avenue = AVENUES.iter().any(|&(a, ah)| (x - a).abs() < ah + 3);
                if !on_avenue {
                    let z = if h > 5 { s + 7 } else { s };
                    self.bp.marks.push(Mark {
                        pos: v3(int(x), self.t.height(int(x), int(z)), int(z)),
                        along_x: true,
                    });
                }
                x += step;
            }
        }
    }
}

/// The whole city, the same every time.
pub fn plan(t: &Terrain, pal: &Palette) -> Blueprint {
    let blocks = blocks(t);
    let mut b = Builder {
        bp: Blueprint::default(),
        t,
        pal,
    };
    b.bp.kinds = kinds(&blocks);
    for (k, block) in blocks.iter().enumerate() {
        let mut r = Rng::new(9001 + k as u64 * 31);
        match block.district {
            District::Home => b.park(*block, &mut r, Some((-30, -22, 40, 22))),
            District::Downtown => b.downtown(*block, &mut r),
            District::Midtown => b.midtown(*block, &mut r),
            District::Suburb => b.suburb(*block, &mut r),
            District::Park => b.park(*block, &mut r, None),
        }
    }
    b.overpass();
    b.marks();
    b.bp.blocks = blocks;
    b.bp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette::parse(include_str!("../../../data/palette.json")).unwrap()
    }

    #[test]
    fn the_city_has_a_downtown_a_suburb_a_home_and_one_overpass() {
        let t = Terrain::new();
        let bp = plan(&t, &palette());
        let count = |d: District| bp.blocks.iter().filter(|b| b.district == d).count();
        assert_eq!(count(District::Home), 1);
        assert!(count(District::Downtown) >= 6, "{}", count(District::Downtown));
        assert!(count(District::Suburb) >= 20, "{}", count(District::Suburb));
        assert!(count(District::Midtown) >= 20, "{}", count(District::Midtown));
        assert!(bp.counts.overpass_pieces > 200, "{:?}", bp.counts);
        assert!(bp.counts.tallest >= 150, "{:?}", bp.counts);
        assert!(bp.counts.houses > 100 && bp.counts.towers > 10, "{:?}", bp.counts);
    }

    #[test]
    fn the_home_block_is_in_the_middle_and_the_driveway_reaches_the_avenue() {
        let t = Terrain::new();
        let blocks = blocks(&t);
        let home = blocks.iter().find(|b| b.district == District::Home).unwrap();
        assert!(home.x0 < 0 && home.x1 > 0 && home.z0 < 0 && home.z1 > 0);
        for x in DRIVEWAY.0..DRIVEWAY.1 + 6 {
            assert!(on_road(x, 0), "{x}");
        }
    }

    #[test]
    fn every_supporter_comes_before_what_it_holds_and_groups_tile_the_pieces() {
        let t = Terrain::new();
        let bp = plan(&t, &palette());
        let mut next = 0;
        for g in &bp.groups {
            assert_eq!(g.start, next);
            assert!(g.end > g.start);
            next = g.end;
        }
        assert_eq!(next as usize, bp.pieces.len());
        for (i, p) in bp.pieces.iter().enumerate() {
            for &s in &p.sup[..p.nsup as usize] {
                assert!((s as usize) < i, "piece {i} held by {s}");
            }
        }
    }

    #[test]
    fn no_piece_starts_inside_the_house_lot_or_on_a_road() {
        let t = Terrain::new();
        let bp = plan(&t, &palette());
        for p in &bp.pieces {
            let (x, z) = (p.pos.x / ONE, p.pos.z / ONE);
            assert!(!(x.abs() < 30 && z.abs() < 20), "piece at {x},{z}");
            let overpass = (z - OVERPASS_Z).abs() <= 10;
            if !overpass {
                assert!(!on_road(x, z), "piece on a road at {x},{z}");
            }
        }
    }
}

#[cfg(test)]
mod probe {
    use super::*;

    #[test]
    #[ignore]
    fn print_counts() {
        let t = Terrain::new();
        let bp = plan(
            &t,
            &Palette::parse(include_str!("../../../data/palette.json")).unwrap(),
        );
        let mut d = [0; 5];
        for b in &bp.blocks {
            d[b.district as usize] += 1;
        }
        let msg = format!(
            "pieces {} groups {} marks {} {:?} districts home/down/mid/sub/park {:?}",
            bp.pieces.len(),
            bp.groups.len(),
            bp.marks.len(),
            bp.counts,
            d
        );
        panic!("{msg}");
    }
}
