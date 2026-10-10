//! The car: a box on four ray-cast springs, after the arcade cars of Crazy
//! Taxi rather than a simulation.
//!
//! Each wheel casts a ray down from its mount. Where the ray meets the
//! ground or a piece of the city, the spring pushes back along the surface's
//! normal, and the tyre pushes sideways against sliding and forward with the
//! engine, each limited by the spring's load, so a wheel in the air does
//! nothing. Tyre forces act at the height of the car's middle, which keeps it
//! planted through corners. The body's corners and edges collide like a piece
//! of rubble (`rubble::resolve_fixed`): it bounces off buildings, shoves light
//! rubble aside, and rides heavy rubble.
//!
//! Abilities come with survival time (`data/upgrades.json`): big wheels
//! (taller springs, more grip and drive), hover (three seconds held above
//! whatever is below), flight (five seconds as a plane) and turbo. A held
//! key rolls the car upright, slowly, as Vagrancy's sticky feet stand a body
//! up. Nothing hurts the car: there is no fall damage.

use crate::fx::{self, int, ONE};
use crate::geom::{v3, V3};
use crate::quat::{from_local, to_local, Quat};
use crate::rubble::{self, Piece, Rubble};
use crate::terrain::{Terrain, BOUND};

/// The body's half size: width, height, length.
pub const HALF: V3 = v3(ONE * 95 / 100, ONE * 55 / 100, ONE * 22 / 10);
/// Where each wheel's spring is mounted, in the car's frame (−z is forward).
pub const MOUNTS: [V3; 4] = [
    v3(-ONE * 85 / 100, -ONE / 4, -ONE * 14 / 10),
    v3(ONE * 85 / 100, -ONE / 4, -ONE * 14 / 10),
    v3(-ONE * 85 / 100, -ONE / 4, ONE * 14 / 10),
    v3(ONE * 85 / 100, -ONE / 4, ONE * 14 / 10),
];
/// The car pushes rubble lighter than this many cubic metres of it.
pub const PUSH_MASS: i32 = 60;
/// The car's mass, in the rubble's units.
const MASS: i32 = 30;

/// Every number of the car's feel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tune {
    pub radius: i32,
    pub rest: i32,
    pub spring: i32,
    pub damper: i32,
    pub grip: i32,
    pub drive: i32,
    pub top: i32,
    pub brake: i32,
    pub reverse_top: i32,
    pub steer: i32,
}

pub const BASE: Tune = Tune {
    radius: ONE * 38 / 100,
    rest: ONE * 45 / 100,
    spring: 60 * ONE,
    damper: 7 * ONE,
    grip: ONE * 13 / 10,
    drive: 17 * ONE,
    top: 40 * ONE,
    brake: 30 * ONE,
    reverse_top: 12 * ONE,
    steer: fx::deg(32),
};

/// What the car can do, from the upgrades earned so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kit {
    pub big_wheels: bool,
    pub hover: bool,
    pub wings: bool,
    pub turbo: u8,
}

impl Kit {
    pub fn tune(&self) -> Tune {
        let mut t = BASE;
        if self.big_wheels {
            t.radius = ONE * 72 / 100;
            t.rest = ONE * 75 / 100;
            t.spring = 70 * ONE;
            t.grip = ONE * 16 / 10;
            t.drive = 23 * ONE;
        }
        t.top += int(7) * self.turbo as i32;
        t.drive += int(3) * self.turbo as i32;
        t
    }
}

/// The controls the car reads, each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Controls {
    /// −ONE..ONE: back to forward
    pub throttle: i32,
    /// −ONE..ONE: right to left
    pub steer: i32,
    pub handbrake: bool,
    pub hover: bool,
    pub fly: bool,
    pub flip: bool,
}

pub const HOVER_TICKS: i32 = 3 * 60;
pub const HOVER_RECHARGE: i32 = 7 * 60;
pub const FLY_TICKS: i32 = 5 * 60;
pub const FLY_RECHARGE: i32 = 12 * 60;
const HOVER_HEIGHT: i32 = ONE * 22 / 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Car {
    pub body: Piece,
    pub kit: Kit,
    pub steer: i32,
    /// each wheel's spring length now, and its turn, for drawing
    pub travel: [i32; 4],
    pub spin: i32,
    pub grounded: u8,
    /// ticks of hover and flight left, and ticks until each is ready again
    pub hover_left: i32,
    pub hover_wait: i32,
    pub fly_left: i32,
    pub fly_wait: i32,
    pub flipping: bool,
    prev_comp: [i32; 4],
    scratch: Vec<u32>,
}

impl Car {
    pub fn new(pos: V3, yaw: i32) -> Car {
        let rot = Quat::yaw(yaw);
        let k = (fx::mul(HALF.x, HALF.x) + fx::mul(HALF.y, HALF.y) + fx::mul(HALF.z, HALF.z)) / 3;
        let mut body = rubble::blank_piece();
        body.pos = pos;
        body.rot = rot;
        body.basis = rot.basis();
        body.half = HALF;
        body.radius = HALF.len();
        body.mass = MASS;
        body.k = k * 3 / 2;
        body.lo = pos.sub(v3(int(3), int(3), int(3)));
        body.hi = pos.add(v3(int(3), int(3), int(3)));
        Car {
            body,
            kit: Kit::default(),
            steer: 0,
            travel: [BASE.rest; 4],
            spin: 0,
            grounded: 0,
            hover_left: 0,
            hover_wait: 0,
            fly_left: 0,
            fly_wait: 0,
            flipping: false,
            prev_comp: [0; 4],
            scratch: Vec::new(),
        }
    }

    pub fn fwd(&self) -> V3 {
        self.body.basis[2].neg()
    }

    pub fn up(&self) -> V3 {
        self.body.basis[1]
    }

    pub fn right(&self) -> V3 {
        self.body.basis[0]
    }

    /// Speed along the car's nose, m/s in Q16.
    pub fn forward_speed(&self) -> i32 {
        self.body.vel.dot(self.fwd())
    }

    pub fn hovering(&self) -> bool {
        self.hover_left > 0
    }

    pub fn flying(&self) -> bool {
        self.fly_left > 0
    }

    fn push(&mut self, at: V3, f: V3, dt: i32) {
        // forces are accelerations: the car's mass is one
        let b = &mut self.body;
        b.vel = b.vel.add(f.scale(dt));
        let r = at.sub(b.pos);
        let torque = r.cross(f);
        b.ang = b.ang.add(v3(
            fx::div(fx::mul(torque.x, dt), b.k),
            fx::div(fx::mul(torque.y, dt), b.k),
            fx::div(fx::mul(torque.z, dt), b.k),
        ));
    }

    pub fn step(&mut self, c: Controls, t: &Terrain, city: &mut Rubble, dt: i32) {
        let tune = self.kit.tune();
        self.hover_wait = (self.hover_wait - 1).max(0);
        self.fly_wait = (self.fly_wait - 1).max(0);
        if c.hover && self.kit.hover && self.hover_left == 0 && self.hover_wait == 0 && self.fly_left == 0 {
            self.hover_left = HOVER_TICKS;
            self.hover_wait = HOVER_TICKS + HOVER_RECHARGE;
        }
        if c.fly && self.kit.wings && self.fly_left == 0 && self.fly_wait == 0 {
            self.fly_left = FLY_TICKS;
            self.fly_wait = FLY_TICKS + FLY_RECHARGE;
            self.hover_left = 0;
            // a hop into the air to start the flight
            self.body.vel = self.body.vel.add(V3::UP.scale(int(6)));
        }
        let flying = self.fly_left > 0;
        let hovering = self.hover_left > 0 && !flying;
        self.hover_left = (self.hover_left - 1).max(0);
        self.fly_left = (self.fly_left - 1).max(0);

        let want_steer = fx::mul(c.steer, tune.steer);
        // steer less at speed, as an arcade car does
        let speed = self.body.vel.len();
        let calm = ONE - fx::div(speed.min(int(45)), int(70));
        self.steer = fx::approach(self.steer, fx::mul(want_steer, calm), fx::deg(4));

        // gravity, lessened in flight
        let g = if flying {
            rubble::GRAVITY / 6
        } else {
            rubble::GRAVITY
        };
        self.body.vel.y -= fx::mul(g, dt);

        if flying {
            self.fly(c, tune, dt);
        } else if hovering {
            self.hover(c, tune, t, city, dt);
        } else {
            self.wheels(c, tune, t, city, dt);
        }
        if c.flip {
            self.right_itself(t, dt);
        }
        self.flipping = c.flip;

        // air drag and spin damping
        let b = &mut self.body;
        let v = b.vel.len();
        b.vel = b.vel.sub(b.vel.scale(fx::mul(fx::mul(v, ONE / 600), dt)));
        b.ang = b.ang.scale(if self.grounded > 0 {
            ONE - ONE / 30
        } else {
            ONE - ONE / 120
        });
        // integrate
        b.pos = b.pos.add(b.vel.scale(dt));
        b.rot = b.rot.integrate(b.ang, dt);
        b.basis = b.rot.basis();
        self.collide(t, city);
        let ahead = self.forward_speed();
        // radians turned this step, in TURN units (2π is 411 775 in Q16)
        let rad = fx::div(fx::mul(ahead, dt), tune.radius) as i64;
        self.spin = (self.spin + (rad * fx::TURN as i64 / 411_775) as i32) & (fx::TURN - 1);
    }

    fn wheels(&mut self, c: Controls, tune: Tune, t: &Terrain, city: &mut Rubble, dt: i32) {
        let up = self.up();
        let down = up.neg();
        let max = tune.rest + tune.radius;
        let mut grounded = 0;
        let mut scratch = std::mem::take(&mut self.scratch);
        for w in 0..4 {
            let b = &self.body;
            let mount = b.pos.add(from_local(&b.basis, MOUNTS[w]));
            // the ground: two steps of the ray against the height field
            let mut hit: Option<(i32, V3)> = None;
            if down.y < -ONE / 5 {
                let mut dist = fx::div(mount.y - t.height(mount.x, mount.z), -down.y);
                let p = mount.add(down.scale(dist));
                dist = fx::div(mount.y - t.height(p.x, p.z), -down.y);
                if dist >= -ONE / 2 && dist <= max {
                    let p = mount.add(down.scale(dist));
                    hit = Some((dist.max(0), t.normal(p.x, p.z)));
                }
            }
            if let Some((d, n, _)) = city.ray(mount, down, max, &mut scratch) {
                if hit.map(|h| d < h.0).unwrap_or(true) {
                    hit = Some((d, n));
                }
            }
            let Some((dist, n)) = hit else {
                self.travel[w] = tune.rest;
                self.prev_comp[w] = 0;
                continue;
            };
            grounded += 1;
            self.travel[w] = (dist - tune.radius).clamp(0, tune.rest);
            let comp = max - dist;
            let comp_v = fx::div(comp - self.prev_comp[w], dt);
            self.prev_comp[w] = comp;
            let n_up = if n.dot(up) > 0 { n } else { up };
            let spring =
                (fx::mul(comp, tune.spring) + fx::mul(comp_v, tune.damper)).clamp(0, rubble::GRAVITY * 3);
            let contact = mount.add(down.scale(dist));
            // tyre frame on the surface
            let front = w < 2;
            let steer = if front { self.steer } else { 0 };
            let f0 = Quat::axis_angle(up, steer).rotate(self.fwd());
            let f = f0.sub(n_up.scale(f0.dot(n_up))).norm();
            let s = f.cross(n_up);
            let b = &self.body;
            let r = contact.sub(b.pos);
            let vc = b.vel.add(b.ang.cross(r));
            let (vf, vs) = (vc.dot(f), vc.dot(s));
            let mut grip = tune.grip;
            if c.handbrake && !front {
                grip = grip * 3 / 10;
            }
            let cap = fx::mul(spring, grip);
            let lateral = (-fx::mul(vs, 7 * ONE)).clamp(-cap, cap);
            let mut long;
            if c.throttle > 0 {
                let room = ONE - fx::div(vf.clamp(0, tune.top), tune.top);
                long = fx::mul(fx::mul(tune.drive, room), c.throttle) / 4;
            } else if c.throttle < 0 {
                if vf > ONE {
                    long = -tune.brake / 4;
                } else {
                    let room = ONE - fx::div((-vf).clamp(0, tune.reverse_top), tune.reverse_top);
                    long = fx::mul(fx::mul(tune.drive / 2, room), c.throttle) / 4;
                }
            } else {
                long = -fx::mul(vf, ONE / 3) / 4;
            }
            if c.handbrake {
                long -= fx::mul(vf, ONE * 2) / 4;
            }
            let long = long.clamp(-cap, cap);
            // tyre forces act level with the middle of the car
            let level = contact.add(up.scale(up.dot(b.pos.sub(contact))));
            self.push(contact, n_up.scale(spring), dt);
            self.push(level, s.scale(lateral).add(f.scale(long)), dt);
        }
        self.scratch = scratch;
        self.grounded = grounded;
        if grounded == 0 {
            // a little control in the air
            let b = &mut self.body;
            let yaw = fx::mul(c.steer, ONE * 2);
            let pitch = fx::mul(c.throttle, -ONE * 3 / 2);
            let want = from_local(&b.basis, v3(pitch, yaw, 0));
            b.ang = b.ang.add(want.sub(b.ang).scale(ONE / 40));
        }
    }

    fn hover(&mut self, c: Controls, tune: Tune, t: &Terrain, city: &mut Rubble, dt: i32) {
        let b = &self.body;
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut floor = t.height(b.pos.x, b.pos.z);
        let probe = b.pos.add(V3::UP.scale(int(2)));
        if let Some((d, _, _)) = city.ray(probe, V3::UP.neg(), int(12), &mut scratch) {
            floor = floor.max(probe.y - d);
        }
        self.scratch = scratch;
        let b = &mut self.body;
        let gap = b.pos.y - floor - HOVER_HEIGHT;
        let lift = rubble::GRAVITY - fx::mul(gap, 30 * ONE) - fx::mul(b.vel.y, 8 * ONE);
        b.vel.y += fx::mul(lift.clamp(0, rubble::GRAVITY * 3), dt);
        // level the car and steer it like a boat
        let level = b.basis[1].cross(V3::UP);
        b.ang = b.ang.add(level.scale(fx::mul(8 * ONE, dt)));
        b.ang.y = fx::mul(c.steer, ONE * 2);
        let fwd = b.basis[2].neg();
        let flat = v3(fwd.x, 0, fwd.z).norm();
        let along = b.vel.dot(flat);
        let want = fx::mul(c.throttle, tune.top);
        let side = v3(b.vel.x, 0, b.vel.z).sub(flat.scale(along));
        b.vel = b.vel.sub(side.scale(ONE / 12));
        b.vel = b.vel.add(flat.scale(fx::mul(want - along, ONE / 50)));
        self.grounded = 0;
    }

    fn fly(&mut self, c: Controls, tune: Tune, dt: i32) {
        let b = &mut self.body;
        // pitch with forward and back, turn and bank with left and right
        let pitch = fx::mul(c.throttle, ONE * 3 / 2);
        let yaw = fx::mul(c.steer, ONE);
        let bank = fx::mul(c.steer, ONE * 3 / 2);
        let roll_now = b.basis[0].y;
        let want = from_local(&b.basis, v3(-pitch, yaw, bank - fx::mul(roll_now, ONE * 2)));
        b.ang = b.ang.add(want.sub(b.ang).scale(ONE / 8));
        let fwd = b.basis[2].neg();
        let speed = (tune.top * 5 / 4).max(b.vel.len());
        let target = fwd.scale(speed);
        b.vel = b.vel.add(target.sub(b.vel).scale(ONE / 14));
        let _ = dt;
        self.grounded = 0;
    }

    /// The flip key: a slow turn of the car's up toward the sky, and a lift
    /// off the ground to turn in.
    fn right_itself(&mut self, t: &Terrain, dt: i32) {
        let b = &mut self.body;
        let above = b.pos.y - t.height(b.pos.x, b.pos.z);
        let up = b.basis[1];
        let mut axis = up.cross(V3::UP);
        if axis.len() < ONE / 4 && up.y < 0 {
            // flat on its roof: roll over the nose's axis
            axis = b.basis[2].neg();
        }
        b.ang = b.ang.add(axis.scale(fx::mul(6 * ONE, dt))).scale(ONE - ONE / 12);
        // lift it just clear of the ground to turn, never higher
        if up.y < ONE * 7 / 10 && b.vel.y < int(2) && above < int(3) {
            b.vel.y += fx::mul(rubble::GRAVITY + int(3), dt);
        }
    }

    /// The body's corners and edges against the ground and the city.
    fn collide(&mut self, t: &Terrain, city: &mut Rubble) {
        let mut pts = [V3::ZERO; 14];
        {
            let b = &self.body;
            let mut k = 0;
            for sx in [-1, 1] {
                for sy in [-1, 1] {
                    for sz in [-1, 1] {
                        pts[k] = b
                            .pos
                            .add(from_local(&b.basis, v3(sx * HALF.x, sy * HALF.y, sz * HALF.z)));
                        k += 1;
                    }
                }
            }
            for (sx, sz) in [(0, -1), (0, 1), (-1, 0), (1, 0), (0, 0), (0, 0)] {
                let sy = if k == 13 { 1 } else { -1 };
                pts[k] = b
                    .pos
                    .add(from_local(&b.basis, v3(sx * HALF.x, sy * HALF.y, sz * HALF.z)));
                k += 1;
            }
        }
        for &p in &pts {
            let h = t.height(p.x, p.z);
            if p.y < h {
                let n = t.normal(p.x, p.z);
                rubble::resolve_fixed(&mut self.body, p, n, fx::mul(h - p.y, n.y));
            }
        }
        let reach = v3(int(4), int(4), int(4));
        let (lo, hi) = (self.body.pos.sub(reach), self.body.pos.add(reach));
        let mut near = std::mem::take(&mut self.scratch);
        city.resting_near(lo, hi, &mut near);
        let mut moving = Vec::new();
        city.moving_near(lo, hi, &mut moving);
        for &j in near.iter().chain(moving.iter()) {
            let piece = city.pieces[j as usize];
            let light = piece.mass <= PUSH_MASS && piece.state != rubble::INTACT;
            for &p in &pts {
                if let Some((n, depth)) = piece.contains(p) {
                    if light {
                        // shove it: it takes the car's speed into it
                        let into = self.body.vel.dot(n.neg()).max(0);
                        city.shove(j as usize, n.neg().scale(into + int(2)), PUSH_MASS);
                        let b = &mut self.body;
                        b.vel = b.vel.sub(
                            n.neg()
                                .scale(fx::mul(into, fx::div(piece.mass, piece.mass + MASS * 2))),
                        );
                    } else {
                        rubble::resolve_fixed(&mut self.body, p, n, depth);
                    }
                }
            }
            if !light {
                for cnr in piece.corners() {
                    if let Some((n, depth)) = self.body.contains(cnr) {
                        rubble::resolve_fixed(&mut self.body, cnr, n.neg(), depth);
                    }
                }
            }
        }
        self.scratch = near;
        // the edge of the world
        let b = &mut self.body;
        let lim = int(BOUND);
        if b.pos.x.abs() > lim {
            b.pos.x = b.pos.x.clamp(-lim, lim);
            b.vel.x = -b.vel.x / 2;
        }
        if b.pos.z.abs() > lim {
            b.pos.z = b.pos.z.clamp(-lim, lim);
            b.vel.z = -b.vel.z / 2;
        }
        let floor = t.height(b.pos.x, b.pos.z);
        if b.pos.y < floor - ONE {
            b.pos.y = floor + int(2);
            b.vel.y = b.vel.y.max(0);
        }
        b.lo = b.pos.sub(reach);
        b.hi = b.pos.add(reach);
        let _ = to_local;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Blueprint;
    use crate::fx::deg;

    fn empty() -> Rubble {
        Rubble::new(&Blueprint::default())
    }

    fn drive(car: &mut Car, c: Controls, t: &Terrain, r: &mut Rubble, ticks: usize) {
        for _ in 0..ticks {
            car.step(c, t, r, fx::ratio(1, 60));
        }
    }

    #[test]
    fn a_car_dropped_on_flat_ground_settles_on_its_wheels() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, int(2), 0), 0);
        drive(&mut car, Controls::default(), &t, &mut r, 180);
        assert_eq!(car.grounded, 4);
        assert!(car.up().y > ONE * 99 / 100, "{:?}", car.up());
        assert!(car.body.vel.len() < ONE / 10, "{:?}", car.body.vel);
        let clearance = car.body.pos.y - HALF.y;
        assert!(clearance > ONE / 4 && clearance < ONE, "{clearance}");
    }

    #[test]
    fn the_throttle_drives_it_forward_to_about_its_top_speed() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, ONE, 0), 0);
        let go = Controls {
            throttle: ONE,
            ..Controls::default()
        };
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        drive(&mut car, go, &t, &mut r, 60 * 8);
        let v = car.forward_speed();
        assert!(v > BASE.top * 3 / 4 && v <= BASE.top + ONE, "{}", v / ONE);
        assert!(
            car.body.pos.z < -int(150),
            "it went north: {}",
            car.body.pos.z / ONE
        );
        assert!(car.up().y > ONE * 95 / 100);
    }

    #[test]
    fn steering_left_turns_it_left() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, ONE, 0), 0);
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        let c = Controls {
            throttle: ONE,
            steer: ONE,
            ..Controls::default()
        };
        drive(&mut car, c, &t, &mut r, 90);
        assert!(car.body.pos.x < -int(3), "{}", car.body.pos.x / ONE);
        assert!(car.up().y > ONE * 9 / 10, "it stayed upright: {:?}", car.up());
    }

    #[test]
    fn the_flip_key_rolls_an_upside_down_car_back_onto_its_wheels() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, int(2), 0), 0);
        car.body.rot = Quat::axis_angle(v3(0, 0, ONE), deg(180));
        car.body.basis = car.body.rot.basis();
        drive(&mut car, Controls::default(), &t, &mut r, 120);
        assert!(car.up().y < 0, "it lies on its roof");
        let flip = Controls {
            flip: true,
            ..Controls::default()
        };
        drive(&mut car, flip, &t, &mut r, 60 * 4);
        drive(&mut car, Controls::default(), &t, &mut r, 240);
        assert!(car.up().y > ONE * 9 / 10, "{:?}", car.up());
        assert_eq!(car.grounded, 4);
    }

    #[test]
    fn hover_holds_it_up_for_three_seconds_and_then_it_lands() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, ONE, 0), 0);
        car.kit.hover = true;
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        let h = Controls {
            hover: true,
            ..Controls::default()
        };
        drive(&mut car, h, &t, &mut r, 90);
        assert!(car.body.pos.y > int(2), "up: {}", car.body.pos.y);
        drive(&mut car, Controls::default(), &t, &mut r, 90 + 120);
        assert!(car.body.pos.y < ONE * 2, "down again: {}", car.body.pos.y);
        assert!(car.hover_wait > 0, "and recharging");
    }

    #[test]
    fn flight_climbs_with_the_nose_up_and_lasts_five_seconds() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, ONE, 0), 0);
        car.kit.wings = true;
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        let up = Controls {
            fly: true,
            throttle: -ONE,
            ..Controls::default()
        };
        drive(&mut car, up, &t, &mut r, 120);
        assert!(car.body.pos.y > int(15), "it climbed: {}", car.body.pos.y / ONE);
        assert!(car.flying());
        drive(&mut car, Controls::default(), &t, &mut r, FLY_TICKS as usize);
        assert!(!car.flying());
    }

    #[test]
    fn big_wheels_stand_the_car_taller() {
        let t = Terrain::new();
        let mut r = empty();
        let mut small = Car::new(v3(0, int(2), 0), 0);
        let mut big = Car::new(v3(0, int(2), int(10)), 0);
        big.kit.big_wheels = true;
        drive(&mut small, Controls::default(), &t, &mut r, 180);
        drive(&mut big, Controls::default(), &t, &mut r, 180);
        assert!(big.body.pos.y > small.body.pos.y + ONE / 4);
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use crate::city::Blueprint;

    #[test]
    #[ignore]
    fn trace_flip() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let mut car = Car::new(v3(0, int(2), 0), 0);
        car.body.rot = Quat::axis_angle(v3(0, 0, ONE), fx::deg(180));
        car.body.basis = car.body.rot.basis();
        let mut log = String::new();
        for s in 0..600 {
            let c = Controls {
                flip: (120..360).contains(&s),
                ..Controls::default()
            };
            car.step(c, &t, &mut r, fx::ratio(1, 60));
            if s % 15 == 0 {
                log += &format!(
                    "{s}: up {:?} y {} ang {:?} g {}\n",
                    car.up(),
                    car.body.pos.y * 100 / ONE,
                    car.body.ang,
                    car.grounded
                );
            }
        }
        panic!("{log}");
    }
}
