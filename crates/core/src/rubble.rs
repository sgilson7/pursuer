//! Every breakable piece of the city, and the physics of the ones knocked
//! loose.
//!
//! A piece is a box. It is *intact* while it stands where the city put it,
//! *awake* while it moves, and *asleep* once it has come to rest. Only awake
//! pieces cost anything: intact and asleep ones sit in a grid that the awake
//! ones, the car and the walker collide with, and are not touched otherwise.
//!
//! An awake piece falls, tumbles and lands. Its eight corners are tested
//! against the ground and against the boxes near it, and each contact gives
//! an impulse at that corner, which is what makes a falling slab turn over.
//! Awake pieces push each other with the same impulse, shared by mass. A
//! piece that stays slow for a third of a second sleeps.
//!
//! The cost is bounded. At most `FULL` pieces, the nearest the player, get
//! all of that; the rest fall with gravity and stop at the ground. So the
//! whole city can be knocked down and the frame still holds.
//!
//! Each piece has a fixed slot of `render::INSTANCE_VALUES` integers in one
//! buffer the page uploads. A piece that moved marks its slot, and the page
//! uploads only the marked runs.

use crate::city::{Blueprint, GroupSpec};
use crate::content::Rgb;
use crate::fx::{self, int, ONE};
use crate::geom::{v3, V3};
use crate::quat::{to_local, Quat};
use crate::rng::Rng;
use crate::terrain::Terrain;

pub const INTACT: u8 = 0;
pub const AWAKE: u8 = 1;
pub const ASLEEP: u8 = 2;

/// Values per piece in the instance buffer (render::INSTANCE_VALUES).
pub const SLOT: usize = 16;

/// The static grid's cells are this many metres across, from `ORIGIN`.
const CELL: i32 = 8;
const ORIGIN: i32 = -576;
const CELLS: i32 = 144;
/// The awake pieces' grid, rebuilt each step: coarser, so two big pieces
/// that touch are never more than a cell apart.
const DCELL: i32 = 16;
const DCELLS: i32 = 72;

/// Pieces given every contact each step; the rest only fall.
pub const FULL: usize = 2600;
/// Gravity, m/s².
pub const GRAVITY: i32 = 20 * ONE;
const RESTITUTION: i32 = ONE / 10;
const FRICTION: i32 = ONE * 6 / 10;
/// A contact's positional correction, as a share of its depth, and the most
/// it moves a piece in one step.
const PUSH: i32 = ONE * 6 / 10;
const MAX_PUSH: i32 = ONE / 2;
/// Asleep after this many slow steps.
const CALM_STEPS: u16 = 20;
const CALM_SPEED: i32 = ONE * 6 / 10;
const CALM_SPIN: i32 = ONE * 8 / 10;
/// Put to sleep after this many steps awake however it moves.
const MAX_AGE: u16 = 60 * 14;
/// A building comes down whole once this share of its ground floor is gone.
const COLLAPSE_PCT: u32 = 40;
/// A loose piece moving this fast into a resting one wakes it; a heavy one
/// moving faster still knocks an intact piece loose.
const WAKE_SPEED: i32 = 4 * ONE;
const CRUSH_SPEED: i32 = 12 * ONE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub pos: V3,
    pub vel: V3,
    pub rot: Quat,
    pub ang: V3,
    pub basis: [V3; 3],
    pub half: V3,
    pub radius: i32,
    /// volume in whole cubic metres, at least one: only ratios matter
    pub mass: i32,
    /// moment of inertia over mass, Q16 m²
    pub k: i32,
    pub state: u8,
    pub calm: u16,
    pub age: u16,
    pub group: u32,
    pub rgb: Rgb,
    pub style: i32,
    pub sup: [u32; 4],
    pub nsup: u8,
    pub all: bool,
    pub base: bool,
    /// its bounding box, kept current
    pub lo: V3,
    pub hi: V3,
    in_grid: bool,
    cells: [i16; 4],
}

impl Piece {
    pub fn corners(&self) -> [V3; 8] {
        let [a, b, c] = self.basis;
        let (ax, by, cz) = (a.scale(self.half.x), b.scale(self.half.y), c.scale(self.half.z));
        let mut out = [V3::ZERO; 8];
        for (k, o) in out.iter_mut().enumerate() {
            let sx = if k & 1 == 0 { ax } else { ax.neg() };
            let sy = if k & 2 == 0 { by } else { by.neg() };
            let sz = if k & 4 == 0 { cz } else { cz.neg() };
            *o = self.pos.add(sx).add(sy).add(sz);
        }
        out
    }

    fn extent(&self) -> V3 {
        let [a, b, c] = self.basis;
        let e = |f: fn(V3) -> i32| {
            fx::mul(f(a).abs(), self.half.x)
                + fx::mul(f(b).abs(), self.half.y)
                + fx::mul(f(c).abs(), self.half.z)
        };
        v3(e(|v| v.x), e(|v| v.y), e(|v| v.z))
    }

    fn refresh_bounds(&mut self) {
        let e = self.extent();
        self.lo = self.pos.sub(e);
        self.hi = self.pos.add(e);
    }

    /// How far a point is inside this box, and the face it is nearest: the
    /// outward normal and the depth. `None` when it is outside.
    pub fn contains(&self, p: V3) -> Option<(V3, i32)> {
        let l = to_local(&self.basis, p.sub(self.pos));
        let pen = [
            self.half.x - l.x.abs(),
            self.half.y - l.y.abs(),
            self.half.z - l.z.abs(),
        ];
        if pen.iter().any(|&d| d <= 0) {
            return None;
        }
        let (axis, depth) = pen
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|&(_, d)| d)
            .unwrap_or((1, 0));
        let sign = [l.x, l.y, l.z][axis];
        let n = if sign >= 0 {
            self.basis[axis]
        } else {
            self.basis[axis].neg()
        };
        Some((n, depth))
    }

    /// Where a ray from `o` along unit `d` enters this box, within `max`:
    /// the distance and the face's outward normal.
    pub fn ray(&self, o: V3, d: V3, max: i32) -> Option<(i32, V3)> {
        let lo = to_local(&self.basis, o.sub(self.pos));
        let ld = to_local(&self.basis, d);
        let (lo, ld, h) = (
            [lo.x, lo.y, lo.z],
            [ld.x, ld.y, ld.z],
            [self.half.x, self.half.y, self.half.z],
        );
        let (mut t0, mut t1) = (i32::MIN, i32::MAX);
        let mut face = 1;
        for a in 0..3 {
            if ld[a].abs() < 64 {
                if lo[a].abs() > h[a] {
                    return None;
                }
                continue;
            }
            let ta = fx::div(-h[a] - lo[a], ld[a]);
            let tb = fx::div(h[a] - lo[a], ld[a]);
            let (near, far) = if ta < tb { (ta, tb) } else { (tb, ta) };
            if near > t0 {
                t0 = near;
                face = a;
            }
            t1 = t1.min(far);
        }
        if t0 > t1 || t0 < -ONE / 2 || t0 > max {
            return None;
        }
        let n = if ld[face] > 0 {
            self.basis[face].neg()
        } else {
            self.basis[face]
        };
        Some((t0.max(0), n))
    }
}

/// A piece at the origin with nothing set, for bodies built from fields.
pub fn blank_piece() -> Piece {
    Piece {
        pos: V3::ZERO,
        vel: V3::ZERO,
        rot: Quat::IDENTITY,
        ang: V3::ZERO,
        basis: Quat::IDENTITY.basis(),
        half: v3(ONE, ONE, ONE),
        radius: ONE,
        mass: 1,
        k: ONE,
        state: AWAKE,
        calm: 0,
        age: 0,
        group: 0,
        rgb: [0; 3],
        style: 0,
        sup: [0; 4],
        nsup: 0,
        all: false,
        base: false,
        lo: V3::ZERO,
        hi: V3::ZERO,
        in_grid: false,
        cells: [0; 4],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub start: u32,
    pub end: u32,
    pub building: bool,
    pub base_total: u32,
    pub base_lost: u32,
    pub collapsed: bool,
    /// a box round every piece of the group, grown as pieces fly
    pub lo: V3,
    pub hi: V3,
}

/// A place a piece was knocked loose, for a puff of dust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Burst {
    pub at: V3,
    pub size: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub loosed: u32,
    pub awake_peak: u32,
    pub collapses: u32,
}

#[derive(Debug, Clone)]
pub struct Rubble {
    pub pieces: Vec<Piece>,
    pub groups: Vec<Group>,
    deps_start: Vec<u32>,
    deps: Vec<u32>,
    grid: Vec<Vec<u32>>,
    awake: Vec<u32>,
    inst: Vec<i32>,
    dirty: Vec<u32>,
    dirty_flag: Vec<bool>,
    mark: Vec<u32>,
    stamp: u32,
    dyn_start: Vec<u32>,
    dyn_items: Vec<u32>,
    rng: Rng,
    pub bursts: Vec<Burst>,
    pub stats: Stats,
    /// set when every slot must be uploaded again
    pub full_upload: bool,
}

fn cell_of(v: i32) -> i32 {
    ((v / ONE - ORIGIN).div_euclid(CELL)).clamp(0, CELLS - 1)
}

fn dcell_of(v: i32) -> i32 {
    ((v / ONE - ORIGIN).div_euclid(DCELL)).clamp(0, DCELLS - 1)
}

fn overlap(alo: V3, ahi: V3, blo: V3, bhi: V3) -> bool {
    alo.x <= bhi.x && ahi.x >= blo.x && alo.y <= bhi.y && ahi.y >= blo.y && alo.z <= bhi.z && ahi.z >= blo.z
}

/// The impulse that stops a point of a piece moving into a fixed surface,
/// with friction; applied to the piece. Returns the closing speed it removed.
pub fn resolve_fixed(a: &mut Piece, p: V3, n: V3, depth: i32) -> i32 {
    a.pos = a.pos.add(n.scale(fx::mul(depth, PUSH).min(MAX_PUSH)));
    let r = p.sub(a.pos);
    let v = a.vel.add(a.ang.cross(r));
    let vn = v.dot(n);
    if vn >= 0 {
        return 0;
    }
    let rn = r.cross(n);
    let denom = ONE + fx::div(rn.dot(rn), a.k.max(ONE / 16));
    let j = fx::div(fx::mul(-vn, ONE + RESTITUTION), denom);
    a.vel = a.vel.add(n.scale(j));
    a.ang = a.ang.add(fx_div_v(rn.scale(j), a.k));
    // friction, at most FRICTION times the normal impulse
    let vt = v.sub(n.scale(vn));
    let tl = vt.len();
    if tl > ONE / 64 {
        let t = v3(fx::div(vt.x, tl), fx::div(vt.y, tl), fx::div(vt.z, tl));
        let rt = r.cross(t);
        let dt = ONE + fx::div(rt.dot(rt), a.k.max(ONE / 16));
        let jt = fx::div(tl, dt).min(fx::mul(FRICTION, j));
        a.vel = a.vel.sub(t.scale(jt));
        a.ang = a.ang.sub(fx_div_v(rt.scale(jt), a.k));
    }
    -vn
}

fn fx_div_v(v: V3, d: i32) -> V3 {
    let d = d.max(ONE / 16);
    v3(fx::div(v.x, d), fx::div(v.y, d), fx::div(v.z, d))
}

/// The impulse between two moving pieces at a point, `n` pointing from `b`
/// to `a`, shared by mass. Returns the closing speed.
fn resolve_pair(a: &mut Piece, b: &mut Piece, p: V3, n: V3, depth: i32) -> i32 {
    let total = a.mass + b.mass;
    let share_a = fx::div(b.mass, total);
    let push = fx::mul(depth, PUSH).min(MAX_PUSH);
    a.pos = a.pos.add(n.scale(fx::mul(push, share_a)));
    b.pos = b.pos.sub(n.scale(fx::mul(push, ONE - share_a)));
    let (ra, rb) = (p.sub(a.pos), p.sub(b.pos));
    let va = a.vel.add(a.ang.cross(ra));
    let vb = b.vel.add(b.ang.cross(rb));
    let vn = va.sub(vb).dot(n);
    if vn >= 0 {
        return 0;
    }
    // inverse masses relative to the lighter piece, so the sums stay in range
    let m0 = a.mass.min(b.mass);
    let ia = fx::div(m0, a.mass);
    let ib = fx::div(m0, b.mass);
    let (rna, rnb) = (ra.cross(n), rb.cross(n));
    let denom = ia
        + ib
        + fx::mul(fx::div(rna.dot(rna), a.k.max(ONE / 16)), ia)
        + fx::mul(fx::div(rnb.dot(rnb), b.k.max(ONE / 16)), ib);
    let j = fx::div(fx::mul(-vn, ONE + RESTITUTION), denom.max(ONE / 64));
    a.vel = a.vel.add(n.scale(fx::mul(j, ia)));
    b.vel = b.vel.sub(n.scale(fx::mul(j, ib)));
    a.ang = a.ang.add(fx_div_v(rna.scale(fx::mul(j, ia)), a.k));
    b.ang = b.ang.sub(fx_div_v(rnb.scale(fx::mul(j, ib)), b.k));
    -vn
}

impl Rubble {
    pub fn new(bp: &Blueprint) -> Rubble {
        let n = bp.pieces.len();
        let mut pieces = Vec::with_capacity(n);
        for (i, s) in bp.pieces.iter().enumerate() {
            let vol = fx::mul(fx::mul(s.half.x * 2, s.half.y * 2), s.half.z * 2) / ONE;
            let hh = s.half;
            let k = (fx::mul(hh.x, hh.x) + fx::mul(hh.y, hh.y) + fx::mul(hh.z, hh.z)) / 3;
            let mut p = Piece {
                pos: s.pos,
                vel: V3::ZERO,
                rot: s.rot,
                ang: V3::ZERO,
                basis: s.rot.basis(),
                half: s.half,
                radius: s.half.len(),
                mass: vol.max(1),
                k: k.max(ONE / 8),
                state: INTACT,
                calm: 0,
                age: 0,
                group: 0,
                rgb: s.rgb,
                style: s.style * ONE + ((i as u32).wrapping_mul(2_654_435_761) >> 16) as i32 % ONE,
                sup: s.sup,
                nsup: s.nsup,
                all: s.all,
                base: s.base,
                lo: V3::ZERO,
                hi: V3::ZERO,
                in_grid: false,
                cells: [0; 4],
            };
            p.refresh_bounds();
            pieces.push(p);
        }
        let mut groups = Vec::with_capacity(bp.groups.len());
        for (g, &GroupSpec { start, end, building }) in bp.groups.iter().enumerate() {
            let (mut lo, mut hi) = (v3(i32::MAX, i32::MAX, i32::MAX), v3(i32::MIN, i32::MIN, i32::MIN));
            let mut base_total = 0;
            for p in &mut pieces[start as usize..end as usize] {
                p.group = g as u32;
                lo = v3(lo.x.min(p.lo.x), lo.y.min(p.lo.y), lo.z.min(p.lo.z));
                hi = v3(hi.x.max(p.hi.x), hi.y.max(p.hi.y), hi.z.max(p.hi.z));
                base_total += p.base as u32;
            }
            groups.push(Group {
                start,
                end,
                building,
                base_total,
                base_lost: 0,
                collapsed: false,
                lo,
                hi,
            });
        }
        // who each piece holds up
        let mut count = vec![0u32; n + 1];
        for p in &pieces {
            for &s in &p.sup[..p.nsup as usize] {
                count[s as usize + 1] += 1;
            }
        }
        for i in 0..n {
            count[i + 1] += count[i];
        }
        let mut deps = vec![0u32; count[n] as usize];
        let mut fill = count.clone();
        for (i, p) in pieces.iter().enumerate() {
            for &s in &p.sup[..p.nsup as usize] {
                deps[fill[s as usize] as usize] = i as u32;
                fill[s as usize] += 1;
            }
        }
        let mut r = Rubble {
            pieces,
            groups,
            deps_start: count,
            deps,
            grid: vec![Vec::new(); (CELLS * CELLS) as usize],
            awake: Vec::new(),
            inst: vec![0; n * SLOT],
            dirty: Vec::new(),
            dirty_flag: vec![false; n],
            mark: vec![0; n],
            stamp: 0,
            dyn_start: vec![0; (DCELLS * DCELLS + 1) as usize],
            dyn_items: Vec::new(),
            rng: Rng::new(77),
            bursts: Vec::new(),
            stats: Stats::default(),
            full_upload: true,
        };
        for i in 0..n {
            r.grid_insert(i);
            r.write(i);
        }
        r.dirty.clear();
        r.dirty_flag.iter_mut().for_each(|f| *f = false);
        r
    }

    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    pub fn awake_count(&self) -> usize {
        self.awake.len()
    }

    pub fn intact_count(&self) -> usize {
        self.pieces.iter().filter(|p| p.state == INTACT).count()
    }

    /// The instance values of every piece, in slot order.
    pub fn values(&self) -> &[i32] {
        &self.inst
    }

    fn grid_insert(&mut self, i: usize) {
        let p = &mut self.pieces[i];
        if p.in_grid {
            return;
        }
        let c = [cell_of(p.lo.x), cell_of(p.lo.z), cell_of(p.hi.x), cell_of(p.hi.z)];
        p.cells = c.map(|v| v as i16);
        p.in_grid = true;
        for z in c[1]..=c[3] {
            for x in c[0]..=c[2] {
                self.grid[(z * CELLS + x) as usize].push(i as u32);
            }
        }
    }

    fn grid_remove(&mut self, i: usize) {
        let p = &mut self.pieces[i];
        if !p.in_grid {
            return;
        }
        p.in_grid = false;
        let c = p.cells.map(|v| v as i32);
        for z in c[1]..=c[3] {
            for x in c[0]..=c[2] {
                let cell = &mut self.grid[(z * CELLS + x) as usize];
                if let Some(k) = cell.iter().position(|&v| v == i as u32) {
                    cell.swap_remove(k);
                }
            }
        }
    }

    /// Writes a piece's slot: its three scaled axes and position, its
    /// colour and its style.
    fn write(&mut self, i: usize) {
        let p = &self.pieces[i];
        let [a, b, c] = p.basis;
        let (a, b, c) = (
            a.scale(p.half.x * 2),
            b.scale(p.half.y * 2),
            c.scale(p.half.z * 2),
        );
        let s = &mut self.inst[i * SLOT..(i + 1) * SLOT];
        s.copy_from_slice(&[
            a.x, a.y, a.z, p.pos.x, b.x, b.y, b.z, p.pos.y, c.x, c.y, c.z, p.pos.z, p.rgb[0], p.rgb[1],
            p.rgb[2], p.style,
        ]);
        if !self.dirty_flag[i] {
            self.dirty_flag[i] = true;
            self.dirty.push(i as u32);
        }
    }

    fn supported(&self, i: usize) -> bool {
        let p = &self.pieces[i];
        let sup = &p.sup[..p.nsup as usize];
        if sup.is_empty() {
            return true;
        }
        let standing = |s: &u32| self.pieces[*s as usize].state == INTACT;
        if p.all {
            sup.iter().all(standing)
        } else {
            sup.iter().any(standing)
        }
    }

    /// Knocks a piece loose (or wakes a resting one) and gives it a velocity
    /// and spin. Whatever it held up and can no longer stand falls too, and a
    /// building that loses enough of its ground floor comes down whole.
    pub fn loose(&mut self, i: usize, vel: V3, ang: V3) {
        let mut stack = vec![(i, vel, ang)];
        while let Some((i, vel, ang)) = stack.pop() {
            let state = self.pieces[i].state;
            if state == AWAKE {
                let p = &mut self.pieces[i];
                p.vel = p.vel.add(vel);
                continue;
            }
            self.grid_remove(i);
            {
                let p = &mut self.pieces[i];
                p.state = AWAKE;
                p.vel = vel;
                p.ang = ang;
                p.calm = 0;
                p.age = 0;
            }
            self.awake.push(i as u32);
            if state != INTACT {
                continue;
            }
            self.stats.loosed += 1;
            let g = self.pieces[i].group as usize;
            if self.pieces[i].base {
                self.groups[g].base_lost += 1;
            }
            for k in self.deps_start[i]..self.deps_start[i + 1] {
                let d = self.deps[k as usize] as usize;
                if self.pieces[d].state == INTACT && !self.supported(d) {
                    let wobble = v3(self.rng.range(-ONE, ONE), 0, self.rng.range(-ONE, ONE));
                    let spin = v3(
                        self.rng.range(-ONE / 2, ONE / 2),
                        0,
                        self.rng.range(-ONE / 2, ONE / 2),
                    );
                    stack.push((d, wobble, spin));
                }
            }
            let grp = self.groups[g];
            if grp.building && !grp.collapsed && grp.base_lost * 100 >= grp.base_total * COLLAPSE_PCT {
                self.groups[g].collapsed = true;
                self.stats.collapses += 1;
                for j in grp.start..grp.end {
                    let j = j as usize;
                    if self.pieces[j].state == INTACT {
                        let lean = v3(
                            self.rng.range(-ONE * 2, ONE * 2),
                            0,
                            self.rng.range(-ONE * 2, ONE * 2),
                        );
                        let spin = v3(self.rng.range(-ONE, ONE), 0, self.rng.range(-ONE, ONE));
                        stack.push((j, lean, spin));
                    }
                }
            }
        }
    }

    fn sleep(&mut self, i: usize) {
        let p = &mut self.pieces[i];
        p.state = ASLEEP;
        p.vel = V3::ZERO;
        p.ang = V3::ZERO;
        p.refresh_bounds();
        self.grid_insert(i);
        self.write(i);
    }

    fn next_stamp(&mut self) -> u32 {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.mark.iter_mut().for_each(|m| *m = 0);
            self.stamp = 1;
        }
        self.stamp
    }

    /// Every resting or standing piece whose box meets `lo..hi`, once each.
    pub fn resting_near(&mut self, lo: V3, hi: V3, out: &mut Vec<u32>) {
        out.clear();
        let stamp = self.next_stamp();
        let (x0, z0, x1, z1) = (cell_of(lo.x), cell_of(lo.z), cell_of(hi.x), cell_of(hi.z));
        for z in z0..=z1 {
            for x in x0..=x1 {
                for &j in &self.grid[(z * CELLS + x) as usize] {
                    if self.mark[j as usize] == stamp {
                        continue;
                    }
                    self.mark[j as usize] = stamp;
                    let q = &self.pieces[j as usize];
                    if overlap(lo, hi, q.lo, q.hi) {
                        out.push(j);
                    }
                }
            }
        }
    }

    /// Every moving piece whose box meets `lo..hi`.
    pub fn moving_near(&self, lo: V3, hi: V3, out: &mut Vec<u32>) {
        out.clear();
        let (x0, z0, x1, z1) = (
            dcell_of(lo.x - int(8)),
            dcell_of(lo.z - int(8)),
            dcell_of(hi.x + int(8)),
            dcell_of(hi.z + int(8)),
        );
        for z in z0..=z1 {
            for x in x0..=x1 {
                let c = (z * DCELLS + x) as usize;
                for k in self.dyn_start[c]..self.dyn_start[c + 1] {
                    let j = self.dyn_items[k as usize];
                    let q = &self.pieces[j as usize];
                    if q.state == AWAKE && overlap(lo, hi, q.lo, q.hi) {
                        out.push(j);
                    }
                }
            }
        }
    }

    /// The nearest box a ray from `o` along unit `d` meets within `max`.
    pub fn ray(&mut self, o: V3, d: V3, max: i32, scratch: &mut Vec<u32>) -> Option<(i32, V3, u32)> {
        let end = o.add(d.scale(max));
        let lo = v3(o.x.min(end.x), o.y.min(end.y), o.z.min(end.z));
        let hi = v3(o.x.max(end.x), o.y.max(end.y), o.z.max(end.z));
        let mut best: Option<(i32, V3, u32)> = None;
        self.resting_near(lo, hi, scratch);
        let mut moving = Vec::new();
        self.moving_near(lo, hi, &mut moving);
        for &j in scratch.iter().chain(moving.iter()) {
            if let Some((t, n)) = self.pieces[j as usize].ray(o, d, max) {
                if best.map(|b| t < b.0).unwrap_or(true) {
                    best = Some((t, n, j));
                }
            }
        }
        best
    }

    /// A push on a piece from outside (the car): wakes a resting piece that
    /// is light enough, and changes a moving one's velocity.
    pub fn shove(&mut self, j: usize, dv: V3, max_mass: i32) {
        let p = self.pieces[j];
        if p.mass > max_mass || p.state == INTACT {
            return;
        }
        let spin = v3(
            self.rng.range(-ONE, ONE),
            self.rng.range(-ONE, ONE),
            self.rng.range(-ONE, ONE),
        );
        if p.state == ASLEEP {
            self.loose(j, dv, spin);
        } else {
            self.pieces[j].vel = p.vel.add(dv);
        }
    }

    /// The pursuer's pass: every piece in the slab in front of it, `width`
    /// to each side and `height` tall, from `near` to `far` ahead of `at`,
    /// is thrown forward and outward. Returns how many it hit.
    #[allow(clippy::too_many_arguments)]
    pub fn sweep(
        &mut self,
        at: V3,
        fwd: V3,
        width: i32,
        height: i32,
        near: i32,
        far: i32,
        speed: i32,
    ) -> u32 {
        let right = v3(-fwd.z, 0, fwd.x);
        let mut lo = v3(i32::MAX, at.y - int(2), i32::MAX);
        let mut hi = v3(i32::MIN, at.y + height, i32::MIN);
        for (f, s) in [(near, -width), (near, width), (far, -width), (far, width)] {
            let c = at.add(fwd.scale(f)).add(right.scale(s));
            lo = v3(lo.x.min(c.x), lo.y, lo.z.min(c.z));
            hi = v3(hi.x.max(c.x), hi.y, hi.z.max(c.z));
        }
        let mut found = Vec::new();
        self.resting_near(lo, hi, &mut found);
        let mut moving = Vec::new();
        self.moving_near(lo, hi, &mut moving);
        let mut hits = 0;
        for j in found.into_iter().chain(moving) {
            let p = self.pieces[j as usize];
            let rel = p.pos.sub(at);
            let (f, s) = (rel.dot(fwd), rel.dot(right));
            if f < near - p.radius / 2
                || f > far + p.radius / 2
                || s.abs() > width + p.radius / 2
                || rel.y > height
            {
                continue;
            }
            let r = &mut self.rng;
            let out = if s >= 0 { right } else { right.neg() };
            let vel = fwd
                .scale(fx::mul(speed, r.range(ONE * 9 / 10, ONE * 17 / 10)))
                .add(out.scale(r.range(2 * ONE, 10 * ONE)))
                .add(V3::UP.scale(r.range(3 * ONE, 13 * ONE)));
            let ang = v3(
                r.range(-4 * ONE, 4 * ONE),
                r.range(-2 * ONE, 2 * ONE),
                r.range(-4 * ONE, 4 * ONE),
            );
            if p.state != AWAKE && hits < 6 && self.bursts.len() < 64 {
                self.bursts.push(Burst {
                    at: p.pos,
                    size: p.radius,
                });
            }
            self.loose(j as usize, vel, ang);
            hits += 1;
        }
        hits
    }

    /// One step of `dt` seconds. `focus` is where the player is: the pieces
    /// nearest it get every contact when there are more than `FULL` awake.
    pub fn step(&mut self, t: &Terrain, dt: i32, focus: V3) {
        let n = self.awake.len();
        self.stats.awake_peak = self.stats.awake_peak.max(n as u32);
        if n == 0 {
            return;
        }
        // the nearest FULL pieces get contacts; order the list so they lead
        if n > FULL {
            let pieces = &self.pieces;
            let key = |i: &u32| {
                let d = pieces[*i as usize].pos.sub(focus);
                (d.x as i64 / 256).pow(2) + (d.z as i64 / 256).pow(2)
            };
            self.awake.select_nth_unstable_by_key(FULL, key);
        }
        // integrate
        let g = fx::mul(GRAVITY, dt);
        for k in 0..n {
            let i = self.awake[k] as usize;
            let p = &mut self.pieces[i];
            p.vel.y -= g;
            p.vel = p.vel.scale(ONE - ONE / 512);
            p.ang = p.ang.scale(ONE - ONE / 64);
            p.pos = p.pos.add(p.vel.scale(dt));
            p.rot = p.rot.integrate(p.ang, dt);
            p.basis = p.rot.basis();
            p.refresh_bounds();
        }
        // the awake grid, by centre, rebuilt by counting
        self.dyn_start.iter_mut().for_each(|c| *c = 0);
        let cell = |p: &Piece| (dcell_of(p.pos.z) * DCELLS + dcell_of(p.pos.x)) as usize;
        for &i in &self.awake {
            self.dyn_start[cell(&self.pieces[i as usize]) + 1] += 1;
        }
        for c in 0..(DCELLS * DCELLS) as usize {
            self.dyn_start[c + 1] += self.dyn_start[c];
        }
        self.dyn_items.resize(n, 0);
        let mut fill = self.dyn_start.clone();
        for &i in &self.awake {
            let c = cell(&self.pieces[i as usize]);
            self.dyn_items[fill[c] as usize] = i;
            fill[c] += 1;
        }
        let full = n.min(FULL);
        let mut near = Vec::new();
        let mut woken: Vec<(usize, V3)> = Vec::new();
        for k in 0..n {
            let i = self.awake[k] as usize;
            if k >= full {
                // far away: fall, and stop at the ground
                let p = &mut self.pieces[i];
                let ground = t.height(p.pos.x, p.pos.z);
                if p.lo.y < ground {
                    p.pos.y += ground - p.lo.y;
                    p.calm = CALM_STEPS;
                    p.vel = V3::ZERO;
                }
                continue;
            }
            // the ground
            let mut a = self.pieces[i];
            let mut touched = false;
            for c in a.corners() {
                let h = t.height(c.x, c.z);
                if c.y < h {
                    let nrm = t.normal(c.x, c.z);
                    resolve_fixed(&mut a, c, nrm, fx::mul(h - c.y, nrm.y));
                    touched = true;
                }
            }
            if a.pos.y < t.height(a.pos.x, a.pos.z) - a.radius {
                a.pos.y = t.height(a.pos.x, a.pos.z) + a.half.y;
            }
            // resting and standing pieces
            a.refresh_bounds();
            self.resting_near(a.lo, a.hi, &mut near);
            for &j in &near {
                let b = self.pieces[j as usize];
                let mut hit = 0;
                for c in a.corners() {
                    if let Some((nrm, depth)) = b.contains(c) {
                        hit = hit.max(resolve_fixed(&mut a, c, nrm, depth));
                        touched = true;
                    }
                }
                for c in b.corners() {
                    if let Some((nrm, depth)) = a.contains(c) {
                        hit = hit.max(resolve_fixed(&mut a, c, nrm.neg(), depth));
                        touched = true;
                    }
                }
                if hit > WAKE_SPEED && b.state == ASLEEP && a.mass * 2 >= b.mass {
                    woken.push((j as usize, a.vel.scale(ONE / 3)));
                }
                if hit > CRUSH_SPEED && b.state == INTACT && a.mass >= b.mass / 2 {
                    woken.push((j as usize, a.vel.scale(ONE / 4)));
                }
            }
            self.pieces[i] = a;
            // other moving pieces
            let (cx, cz) = (dcell_of(a.pos.x), dcell_of(a.pos.z));
            for z in (cz - 1).max(0)..=(cz + 1).min(DCELLS - 1) {
                for x in (cx - 1).max(0)..=(cx + 1).min(DCELLS - 1) {
                    let c = (z * DCELLS + x) as usize;
                    for q in self.dyn_start[c]..self.dyn_start[c + 1] {
                        let j = self.dyn_items[q as usize] as usize;
                        if j <= i {
                            continue;
                        }
                        let (pa, pb) = (self.pieces[i], self.pieces[j]);
                        let reach = pa.radius + pb.radius;
                        let d = pa.pos.sub(pb.pos);
                        if d.x.abs() > reach
                            || d.y.abs() > reach
                            || d.z.abs() > reach
                            || !overlap(pa.lo, pa.hi, pb.lo, pb.hi)
                        {
                            continue;
                        }
                        let (mut a2, mut b2) = (pa, pb);
                        for cnr in pa.corners() {
                            if let Some((nrm, depth)) = b2.contains(cnr) {
                                resolve_pair(&mut a2, &mut b2, cnr, nrm, depth);
                                touched = true;
                            }
                        }
                        for cnr in pb.corners() {
                            if let Some((nrm, depth)) = a2.contains(cnr) {
                                resolve_pair(&mut a2, &mut b2, cnr, nrm.neg(), depth);
                                touched = true;
                            }
                        }
                        self.pieces[i] = a2;
                        self.pieces[j] = b2;
                    }
                }
            }
            let p = &mut self.pieces[i];
            if touched {
                // scraping along the ground or a pile bleeds speed
                p.vel = p.vel.scale(ONE - ONE / 24);
                p.ang = p.ang.scale(ONE - ONE / 10);
            }
            let slow = p.vel.len() < CALM_SPEED && p.ang.len() < CALM_SPIN;
            p.calm = if slow { p.calm + 1 } else { 0 };
        }
        for (j, v) in woken {
            let spin = v3(self.rng.range(-ONE, ONE), 0, self.rng.range(-ONE, ONE));
            self.loose(j, v, spin);
        }
        // sleep, age, and write what moved
        let mut k = 0;
        while k < self.awake.len() {
            let i = self.awake[k] as usize;
            let p = &mut self.pieces[i];
            p.age = p.age.saturating_add(1);
            if p.calm >= CALM_STEPS || p.age >= MAX_AGE {
                self.awake.swap_remove(k);
                self.sleep(i);
                continue;
            }
            p.refresh_bounds();
            let (lo, hi, g) = (p.lo, p.hi, p.group as usize);
            let grp = &mut self.groups[g];
            grp.lo = v3(grp.lo.x.min(lo.x), grp.lo.y.min(lo.y), grp.lo.z.min(lo.z));
            grp.hi = v3(grp.hi.x.max(hi.x), grp.hi.y.max(hi.y), grp.hi.z.max(hi.z));
            self.write(i);
            k += 1;
        }
    }

    /// The runs of slots that changed since the last call, as
    /// (byte offset in the buffer, value offset in `values()`, value count),
    /// and forgets them. Runs closer than a few slots are merged.
    pub fn take_dirty(&mut self) -> Vec<u32> {
        let mut out = Vec::new();
        if self.full_upload {
            self.full_upload = false;
            for &i in &self.dirty {
                self.dirty_flag[i as usize] = false;
            }
            self.dirty.clear();
            return out;
        }
        self.dirty.sort_unstable();
        let mut run: Option<(u32, u32)> = None;
        let flush = |out: &mut Vec<u32>, (a, b): (u32, u32)| {
            let values = SLOT as u32;
            out.extend_from_slice(&[a * values * 4, a * values, (b - a + 1) * values]);
        };
        for &i in &self.dirty {
            self.dirty_flag[i as usize] = false;
            run = match run {
                Some((a, b)) if i <= b + 8 => Some((a, i)),
                Some(r) => {
                    flush(&mut out, r);
                    Some((i, i))
                }
                None => Some((i, i)),
            };
        }
        if let Some(r) = run {
            flush(&mut out, r);
        }
        self.dirty.clear();
        out
    }

    pub fn clear_bursts(&mut self) {
        self.bursts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::{PieceSpec, STYLE_PLAIN};

    fn spec(pos: V3, half: V3, sup: &[u32], all: bool) -> PieceSpec {
        let mut s = [0; 4];
        s[..sup.len()].copy_from_slice(sup);
        PieceSpec {
            pos,
            half,
            rot: Quat::IDENTITY,
            rgb: [ONE; 3],
            style: STYLE_PLAIN,
            sup: s,
            nsup: sup.len() as u8,
            all,
            base: sup.is_empty(),
        }
    }

    fn world(pieces: Vec<PieceSpec>, building: bool) -> Rubble {
        let n = pieces.len() as u32;
        let bp = Blueprint {
            pieces,
            groups: vec![GroupSpec {
                start: 0,
                end: n,
                building,
            }],
            ..Blueprint::default()
        };
        Rubble::new(&bp)
    }

    fn run(r: &mut Rubble, t: &Terrain, steps: usize) {
        for _ in 0..steps {
            r.step(t, fx::ratio(1, 60), V3::ZERO);
        }
    }

    /// a column of three boxes standing on the flat middle of the city
    fn column() -> Vec<PieceSpec> {
        let h = v3(ONE * 2, ONE * 2, ONE * 2);
        vec![
            spec(v3(0, int(2), 0), h, &[], false),
            spec(v3(0, int(6), 0), h, &[0], false),
            spec(v3(0, int(10), 0), h, &[1], false),
        ]
    }

    #[test]
    fn a_standing_structure_costs_nothing_and_moves_not_at_all() {
        let t = Terrain::new();
        let mut r = world(column(), false);
        let before: Vec<i32> = r.values().to_vec();
        run(&mut r, &t, 120);
        assert_eq!(r.awake_count(), 0);
        assert_eq!(r.values(), &before[..]);
    }

    #[test]
    fn knocking_out_the_bottom_brings_down_what_it_held() {
        let t = Terrain::new();
        let mut r = world(column(), false);
        r.loose(0, v3(int(10), int(2), 0), V3::ZERO);
        assert_eq!(r.awake_count(), 3, "the two above lost their support");
        run(&mut r, &t, 600);
        assert_eq!(r.awake_count(), 0, "all three came to rest");
        for p in &r.pieces {
            let ground = t.height(p.pos.x, p.pos.z);
            assert!(
                p.lo.y > ground - ONE / 2,
                "a piece sank: {} under {}",
                p.lo.y,
                ground
            );
            assert!(p.pos.y < int(10), "a piece stayed up at {}", p.pos.y);
        }
    }

    #[test]
    fn a_span_that_needs_both_pillars_falls_when_either_goes() {
        let h = v3(ONE, int(3), ONE);
        let pieces = vec![
            spec(v3(-int(5), int(3), 0), h, &[], false),
            spec(v3(int(5), int(3), 0), h, &[], false),
            spec(v3(0, int(7), 0), v3(int(6), ONE / 2, int(2)), &[0, 1], true),
        ];
        let mut r = world(pieces, false);
        r.loose(1, v3(int(5), 0, 0), V3::ZERO);
        assert_eq!(r.pieces[2].state, AWAKE);
        assert_eq!(r.pieces[0].state, INTACT, "the other pillar stands");
    }

    #[test]
    fn a_falling_box_lands_on_the_ground_and_sleeps_there() {
        let t = Terrain::new();
        let mut r = world(
            vec![spec(v3(int(3), int(30), int(4)), v3(ONE, ONE, ONE), &[], false)],
            false,
        );
        r.loose(0, V3::ZERO, v3(ONE, 0, ONE / 2));
        run(&mut r, &t, 400);
        let p = r.pieces[0];
        assert_eq!(p.state, ASLEEP);
        let ground = t.height(p.pos.x, p.pos.z);
        assert!(
            (p.lo.y - ground).abs() < ONE / 3,
            "rests on the ground: {} vs {}",
            p.lo.y,
            ground
        );
    }

    #[test]
    fn a_box_falling_on_a_standing_one_comes_to_rest_on_top() {
        let t = Terrain::new();
        let h = v3(ONE * 2, ONE * 2, ONE * 2);
        let pieces = vec![
            spec(v3(0, int(2), 0), h, &[], false),
            spec(v3(ONE / 2, int(12), 0), v3(ONE, ONE, ONE), &[], false),
        ];
        let mut r = world(pieces, false);
        r.loose(1, V3::ZERO, V3::ZERO);
        run(&mut r, &t, 300);
        let top = r.pieces[1];
        assert_eq!(top.state, ASLEEP);
        assert!(top.lo.y > int(4) - ONE / 3, "it fell through: {}", top.lo.y);
        assert_eq!(
            r.pieces[0].state, INTACT,
            "a light box does not knock the column loose"
        );
    }

    #[test]
    fn a_building_comes_down_whole_when_its_ground_floor_goes() {
        let h = v3(ONE * 2, ONE * 2, ONE * 2);
        let mut pieces = Vec::new();
        for x in 0..3 {
            pieces.push(spec(v3(int(4 * x), int(2), 0), h, &[], false));
        }
        for x in 0..3 {
            let below = x as u32;
            pieces.push(spec(v3(int(4 * x), int(6), 0), h, &[below], false));
        }
        let mut r = world(pieces, true);
        r.loose(0, v3(int(5), 0, 0), V3::ZERO);
        assert_eq!(r.awake_count(), 2, "one of three is not enough");
        r.loose(1, v3(int(5), 0, 0), V3::ZERO);
        assert_eq!(r.awake_count(), 6, "two of three brings it all down");
        assert_eq!(r.stats.collapses, 1);
    }

    #[test]
    fn a_sweep_throws_what_is_in_front_and_leaves_what_is_beside() {
        let h = v3(ONE, ONE, ONE);
        let pieces = vec![
            spec(v3(0, ONE, int(-5)), h, &[], false),
            spec(v3(int(30), ONE, int(-5)), h, &[], false),
        ];
        let mut r = world(pieces, false);
        let hits = r.sweep(V3::ZERO, v3(0, 0, -ONE), int(10), int(40), 0, int(8), int(20));
        assert_eq!(hits, 1);
        assert_eq!(r.pieces[0].state, AWAKE);
        assert!(
            r.pieces[0].vel.z < -int(10),
            "thrown forward: {:?}",
            r.pieces[0].vel
        );
        assert_eq!(r.pieces[1].state, INTACT);
    }

    #[test]
    fn only_moved_slots_are_uploaded_again() {
        let t = Terrain::new();
        let mut r = world(column(), false);
        assert!(r.take_dirty().is_empty(), "the first upload is whole");
        r.loose(2, v3(ONE, 0, 0), V3::ZERO);
        run(&mut r, &t, 1);
        let runs = r.take_dirty();
        assert_eq!(runs, vec![2 * 64, 2 * 16, 16]);
        assert!(r.take_dirty().is_empty());
    }

    #[test]
    fn a_ray_down_finds_the_top_of_a_box() {
        let mut r = world(column(), false);
        let mut s = Vec::new();
        let (dist, n, j) = r.ray(v3(0, int(20), 0), v3(0, -ONE, 0), int(30), &mut s).unwrap();
        assert_eq!(j, 2);
        assert!((dist - int(8)).abs() < ONE / 50, "{dist}");
        assert_eq!(n, V3::UP);
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use crate::city::{GroupSpec, PieceSpec};

    #[test]
    #[ignore]
    fn trace_a_fall() {
        let t = Terrain::new();
        let bp = Blueprint {
            pieces: vec![PieceSpec {
                pos: v3(int(3), int(30), int(4)),
                half: v3(ONE, ONE, ONE),
                rot: Quat::IDENTITY,
                rgb: [ONE; 3],
                style: 0,
                sup: [0; 4],
                nsup: 0,
                all: false,
                base: true,
            }],
            groups: vec![GroupSpec {
                start: 0,
                end: 1,
                building: false,
            }],
            ..Blueprint::default()
        };
        let mut r = Rubble::new(&bp);
        r.loose(0, V3::ZERO, v3(ONE, 0, ONE / 2));
        let mut log = String::new();
        for s in 0..400 {
            r.step(&t, fx::ratio(1, 60), V3::ZERO);
            let p = r.pieces[0];
            if s % 10 == 0 {
                log += &format!(
                    "{s}: y {:.2} v {:?} w {:?} calm {} st {}\n",
                    p.pos.y / 655,
                    p.vel,
                    p.ang,
                    p.calm,
                    p.state
                );
            }
        }
        panic!("{log}");
    }
}
