//! On foot: walking the house in first person, and running for the car.
//!
//! The walker is an upright cylinder. It walks relative to where it looks,
//! steps up anything lower than a stair, and is pushed out of furniture,
//! standing walls, the car and the city's pieces. Every so many metres on the
//! floor its shoes squeak.

use crate::fx::{self, int, ONE};
use crate::geom::{facing, right_of, v3, V3};
use crate::house::Solid;
use crate::rubble::{self, Piece, Rubble};
use crate::terrain::{Terrain, BOUND};

pub const EYE: i32 = ONE * 165 / 100;
const RADIUS: i32 = ONE * 3 / 10;
const HEIGHT: i32 = ONE * 18 / 10;
const STEP: i32 = ONE * 35 / 100;
const WALK: i32 = ONE * 42 / 10;
const RUN: i32 = ONE * 7;
const JUMP: i32 = ONE * 6;
/// Metres between squeaks.
const STRIDE: i32 = ONE * 7 / 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walker {
    /// the feet
    pub pos: V3,
    pub vel: V3,
    pub yaw: i32,
    pub pitch: i32,
    pub on_ground: bool,
    stride: i32,
    pub left_foot: bool,
}

/// What the walker's legs are told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Legs {
    /// −ONE..ONE back to forward, and right to left
    pub forward: i32,
    pub left: i32,
    pub run: bool,
    pub jump: bool,
}

impl Walker {
    pub fn new(pos: V3, yaw: i32, pitch: i32) -> Walker {
        Walker {
            pos,
            vel: V3::ZERO,
            yaw,
            pitch,
            on_ground: true,
            stride: 0,
            left_foot: false,
        }
    }

    pub fn eye(&self) -> V3 {
        self.pos.add(V3::UP.scale(EYE))
    }

    pub fn look(&self) -> V3 {
        facing(self.yaw, self.pitch)
    }

    /// One step. Returns true on a footfall (a squeak).
    pub fn step(
        &mut self,
        legs: Legs,
        solids: &[Solid],
        car: Option<&Piece>,
        city: &mut Rubble,
        t: &Terrain,
        dt: i32,
    ) -> bool {
        let speed = if legs.run { RUN } else { WALK };
        let wish = facing(self.yaw, 0)
            .scale(legs.forward)
            .add(right_of(self.yaw).scale(-legs.left))
            .clamp_len(ONE)
            .scale(speed);
        let k = if self.on_ground { ONE / 4 } else { ONE / 20 };
        self.vel.x += fx::mul(wish.x - self.vel.x, k);
        self.vel.z += fx::mul(wish.z - self.vel.z, k);
        self.vel.y -= fx::mul(rubble::GRAVITY, dt);
        if legs.jump && self.on_ground {
            self.vel.y = JUMP;
        }
        let before = self.pos;
        self.pos.x += fx::mul(self.vel.x, dt);
        self.pos.z += fx::mul(self.vel.z, dt);
        // furniture and walls
        for s in solids {
            self.push_out(s.lo, s.hi);
        }
        // the car and the city, sampled at the knees and the chest
        let mut near = Vec::new();
        let reach = v3(int(3), int(3), int(3));
        city.resting_near(self.pos.sub(reach), self.pos.add(reach), &mut near);
        let mut moving = Vec::new();
        city.moving_near(self.pos.sub(reach), self.pos.add(reach), &mut moving);
        let mut bodies: Vec<Piece> = near
            .iter()
            .chain(moving.iter())
            .map(|&j| city.pieces[j as usize])
            .collect();
        if let Some(c) = car {
            bodies.push(*c);
        }
        let mut floor = t.height(self.pos.x, self.pos.z);
        for b in &bodies {
            for h in [STEP + ONE / 10, HEIGHT * 2 / 3] {
                for (dx, dz) in [(0, 0), (RADIUS, 0), (-RADIUS, 0), (0, RADIUS), (0, -RADIUS)] {
                    let p = self.pos.add(v3(dx, h, dz));
                    if let Some((n, depth)) = b.contains(p) {
                        let flat = v3(n.x, 0, n.z).norm();
                        self.pos = self.pos.add(flat.scale(depth.min(ONE / 3)));
                    }
                }
            }
            if let Some((d, _)) = b.ray(self.pos.add(V3::UP.scale(STEP)), V3::UP.neg(), STEP + ONE) {
                floor = floor.max(self.pos.y + STEP - d);
            }
        }
        // stand on furniture and floors lower than a stair
        for s in solids {
            let inside = self.pos.x > s.lo.x - RADIUS / 2
                && self.pos.x < s.hi.x + RADIUS / 2
                && self.pos.z > s.lo.z - RADIUS / 2
                && self.pos.z < s.hi.z + RADIUS / 2;
            if inside && s.hi.y <= self.pos.y + STEP {
                floor = floor.max(s.hi.y);
            }
        }
        self.pos.y += fx::mul(self.vel.y, dt);
        self.on_ground = false;
        if self.pos.y <= floor {
            self.pos.y = floor;
            self.vel.y = self.vel.y.max(0);
            self.on_ground = true;
        }
        let lim = int(BOUND);
        self.pos.x = self.pos.x.clamp(-lim, lim);
        self.pos.z = self.pos.z.clamp(-lim, lim);
        if !self.on_ground {
            return false;
        }
        let moved = v3(self.pos.x - before.x, 0, self.pos.z - before.z).len();
        self.stride += moved;
        if self.stride >= STRIDE {
            self.stride -= STRIDE;
            self.left_foot = !self.left_foot;
            return true;
        }
        false
    }

    /// Pushes the cylinder out of a box it overlaps, along the shorter way.
    fn push_out(&mut self, lo: V3, hi: V3) {
        let p = self.pos;
        if p.y + HEIGHT <= lo.y || p.y + STEP >= hi.y {
            return;
        }
        let (x0, x1, z0, z1) = (lo.x - RADIUS, hi.x + RADIUS, lo.z - RADIUS, hi.z + RADIUS);
        if p.x <= x0 || p.x >= x1 || p.z <= z0 || p.z >= z1 {
            return;
        }
        let pushes = [(x0 - p.x, 0), (x1 - p.x, 0), (0, z0 - p.z), (0, z1 - p.z)];
        let (dx, dz) = pushes
            .into_iter()
            .min_by_key(|(dx, dz)| dx.abs() + dz.abs())
            .unwrap_or((0, 0));
        self.pos.x += dx;
        self.pos.z += dz;
        if dx != 0 {
            self.vel.x = 0;
        }
        if dz != 0 {
            self.vel.z = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Blueprint;

    #[test]
    fn walking_forward_moves_where_it_looks_and_squeaks() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let mut w = Walker::new(v3(0, 0, 0), fx::QUARTER, 0);
        let mut squeaks = 0;
        let legs = Legs {
            forward: ONE,
            ..Legs::default()
        };
        for _ in 0..120 {
            squeaks += w.step(legs, &[], None, &mut r, &t, fx::ratio(1, 60)) as i32;
        }
        assert!(w.pos.x < -int(6), "a quarter turn left faces −x: {}", w.pos.x);
        assert!(squeaks >= 8, "{squeaks}");
    }

    #[test]
    fn a_wall_stops_it_and_a_step_does_not() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let wall = Solid {
            lo: v3(-int(4), 0, -int(10)),
            hi: v3(-int(3), int(3), int(10)),
            wall: None,
        };
        let step = Solid {
            lo: v3(int(2), 0, -int(10)),
            hi: v3(int(10), ONE * 3 / 10, int(10)),
            wall: None,
        };
        let legs = Legs {
            forward: ONE,
            ..Legs::default()
        };
        let mut w = Walker::new(V3::ZERO, fx::QUARTER, 0);
        for _ in 0..120 {
            w.step(legs, &[wall, step], None, &mut r, &t, fx::ratio(1, 60));
        }
        assert!(w.pos.x > -int(3), "{}", w.pos.x);
        let mut w = Walker::new(V3::ZERO, -fx::QUARTER, 0);
        for _ in 0..120 {
            w.step(legs, &[wall, step], None, &mut r, &t, fx::ratio(1, 60));
        }
        assert!(
            w.pos.x > int(4) && w.pos.y == ONE * 3 / 10,
            "{} {}",
            w.pos.x,
            w.pos.y
        );
    }
}
