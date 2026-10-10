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
//! Tyre forces act a set height (`Tune::roll`) below the car's middle, so a
//! hard turn leans the car on its outside springs, and too hard a turn at
//! speed rolls it over.
//!
//! Abilities come with survival time, drawn at random (`data/upgrades.json`):
//! big wheels (eight seconds on stilts, high enough to drive over a fallen
//! building's blocks), flight (five seconds as a plane), turbo, a dash in
//! the steered direction, an instant turn, nitrous (four seconds of a
//! rocket's push on loose tyres) and a leap. Each runs for a while and then
//! recharges (`Charge`). A held key rolls the car upright, as Vagrancy's
//! sticky feet stand a body up. Nothing hurts the car: there is no fall
//! damage.

use crate::fx::{self, int, ONE};
use crate::geom::{v3, V3};
use crate::quat::{from_local, to_local, Quat};
use crate::rubble::{self, Piece, Rubble};
use crate::terrain::{Terrain, BOUND};

/// The body's half size: width, height, length.
pub const HALF: V3 = v3(ONE * 95 / 100, ONE * 55 / 100, ONE * 22 / 10);
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
    /// half the distance between the wheels across, and along
    pub track: i32,
    pub base: i32,
    /// how far below the car's middle the tyres push sideways: higher rolls
    /// the car more in a turn
    pub roll: i32,
}

impl Tune {
    /// Where each wheel's spring is mounted, in the car's frame (−z is
    /// forward): front left, front right, back left, back right.
    pub fn mounts(&self) -> [V3; 4] {
        let y = -ONE / 4;
        [
            v3(-self.track, y, -self.base),
            v3(self.track, y, -self.base),
            v3(-self.track, y, self.base),
            v3(self.track, y, self.base),
        ]
    }
}

pub const BASE: Tune = Tune {
    radius: ONE * 38 / 100,
    rest: ONE * 45 / 100,
    spring: 48 * ONE,
    damper: 6 * ONE,
    grip: ONE * 13 / 10,
    drive: 17 * ONE,
    top: 40 * ONE,
    brake: 30 * ONE,
    reverse_top: 12 * ONE,
    steer: fx::deg(32),
    track: ONE * 85 / 100,
    base: ONE * 14 / 10,
    roll: ONE * 52 / 100,
};

/// What the car can do, from the upgrades earned so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kit {
    pub big_wheels: bool,
    pub wings: bool,
    pub turbo: u8,
    pub dash: bool,
    pub spin: bool,
    pub nitrous: bool,
    pub leap: bool,
}

impl Kit {
    /// The car's numbers, `lift` of the way (zero to ONE) out onto the big
    /// wheels.
    pub fn tune(&self, lift: i32) -> Tune {
        let mut t = BASE;
        if self.big_wheels && lift > 0 {
            let mix = |a: i32, b: i32| a + fx::mul(b - a, lift);
            // stilts: the body rides about 9 m up, over an 8 m block
            t.radius = mix(t.radius, ONE * 22 / 10);
            t.rest = mix(t.rest, ONE * 72 / 10);
            t.spring = mix(t.spring, 45 * ONE);
            t.damper = mix(t.damper, 9 * ONE);
            t.grip = mix(t.grip, ONE * 15 / 10);
            t.drive = mix(t.drive, 22 * ONE);
            t.track = mix(t.track, ONE * 19 / 10);
            t.base = mix(t.base, ONE * 24 / 10);
            t.roll = mix(t.roll, ONE * 16 / 10);
        }
        t.top += int(7) * self.turbo as i32;
        t.drive += int(3) * self.turbo as i32;
        t
    }
}

/// An ability's timer: ticks it has left to run, and ticks until it can be
/// used again (counting the run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Charge {
    pub left: i32,
    pub wait: i32,
}

impl Charge {
    /// Starts it, when `go` and it is ready: true when it starts.
    fn fire(&mut self, go: bool, ticks: i32, recharge: i32) -> bool {
        if go && self.left == 0 && self.wait == 0 {
            self.left = ticks;
            self.wait = ticks + recharge;
            true
        } else {
            false
        }
    }

    fn tick(&mut self) {
        self.left = (self.left - 1).max(0);
        self.wait = (self.wait - 1).max(0);
    }

    pub fn active(&self) -> bool {
        self.left > 0
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
    pub fly: bool,
    pub flip: bool,
    pub big: bool,
    pub dash: bool,
    pub spin: bool,
    pub nitrous: bool,
    pub leap: bool,
}

/// Each ability's run and recharge, in ticks. The instant ones (dash, turn,
/// leap) run only long enough for the HUD to light up.
pub const BIG_TICKS: i32 = 8 * 60;
pub const BIG_RECHARGE: i32 = 10 * 60;
pub const FLY_TICKS: i32 = 5 * 60;
pub const FLY_RECHARGE: i32 = 12 * 60;
pub const DASH_TICKS: i32 = 15;
pub const DASH_RECHARGE: i32 = 3 * 60;
pub const SPIN_TICKS: i32 = 15;
pub const SPIN_RECHARGE: i32 = 5 * 60;
pub const NITROUS_TICKS: i32 = 4 * 60;
pub const NITROUS_RECHARGE: i32 = 12 * 60;
pub const LEAP_TICKS: i32 = 30;
pub const LEAP_RECHARGE: i32 = 4 * 60;
/// The dash's kick, m/s, in the steered direction, and a little up.
const DASH_SPEED: i32 = 24 * ONE;
const DASH_HOP: i32 = 3 * ONE;
/// The leap's kick up, m/s: about six metres high.
const LEAP_SPEED: i32 = 16 * ONE;
/// Nitrous: a rocket's push along the nose, m/s², which fades to nothing
/// at `NITROUS_TOP` m/s, and the tyres' grip, in percent, while it burns.
const NITROUS_PUSH: i32 = 55 * ONE;
const NITROUS_TOP: i32 = 130 * ONE;
const NITROUS_GRIP: i32 = 55;
/// How hard the flip key turns the car's up toward the sky.
const RIGHTING: i32 = 7 * ONE;
/// How far the front wheels turn each tick toward where they are steered:
/// slow enough that a flick of the wheel takes a moment.
const STEER_RATE: i32 = fx::deg(2);
/// How hard a tyre pushes back against sliding sideways, per m/s of slide:
/// lower lets the back step out.
const SLIP_STIFFNESS: i32 = 5 * ONE;
/// The most a big wheel climbs onto at once, in metres: a fallen block, and
/// not a building's wall.
const LEDGE: i32 = 9 * ONE;
/// How fast, m/s, a big wheel climbs onto something higher.
const CLIMB: i32 = 8 * ONE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Car {
    pub body: Piece,
    pub kit: Kit,
    pub steer: i32,
    /// each wheel's spring length now, and its turn, for drawing
    pub travel: [i32; 4],
    pub spin: i32,
    pub grounded: u8,
    pub fly: Charge,
    pub big: Charge,
    pub dash: Charge,
    /// the instant turn's timer (`spin` is the wheels' turn)
    pub spin_round: Charge,
    pub nitrous: Charge,
    pub leap: Charge,
    /// how far out the big wheels are, zero to ONE: they take a second
    pub lift: i32,
    /// the height each big wheel stands at, which climbs slowly
    feet: [i32; 4],
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
            fly: Charge::default(),
            big: Charge::default(),
            dash: Charge::default(),
            spin_round: Charge::default(),
            nitrous: Charge::default(),
            leap: Charge::default(),
            lift: 0,
            feet: [pos.y; 4],
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

    /// The numbers it drives by now.
    pub fn tune(&self) -> Tune {
        self.kit.tune(self.lift)
    }

    pub fn flying(&self) -> bool {
        self.fly.active()
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
        let k = self.kit;
        self.big.fire(c.big && k.big_wheels, BIG_TICKS, BIG_RECHARGE);
        let out = if self.big.active() { ONE } else { 0 };
        self.lift = fx::approach(self.lift, out, ONE / 60);
        let mut tune = self.tune();
        if self.fly.fire(c.fly && k.wings, FLY_TICKS, FLY_RECHARGE) {
            // a hop into the air to start the flight
            self.body.vel = self.body.vel.add(V3::UP.scale(int(6)));
        }
        if self.dash.fire(c.dash && k.dash, DASH_TICKS, DASH_RECHARGE) {
            self.dash_off(c);
        }
        if self.spin_round.fire(c.spin && k.spin, SPIN_TICKS, SPIN_RECHARGE) {
            self.turn_round(c);
        }
        if self
            .leap
            .fire(c.leap && k.leap && self.grounded > 0, LEAP_TICKS, LEAP_RECHARGE)
        {
            let b = &mut self.body;
            b.vel.y = b.vel.y.max(0) + LEAP_SPEED;
        }
        self.nitrous
            .fire(c.nitrous && k.nitrous, NITROUS_TICKS, NITROUS_RECHARGE);
        let flying = self.fly.active();
        let burning = self.nitrous.active();
        for ch in [
            &mut self.big,
            &mut self.fly,
            &mut self.dash,
            &mut self.spin_round,
            &mut self.nitrous,
            &mut self.leap,
        ] {
            ch.tick();
        }
        if burning {
            // a rocket on loose tyres: it pushes whether or not they touch
            tune.grip = tune.grip * NITROUS_GRIP / 100;
            let b = &mut self.body;
            let fwd = b.basis[2].neg();
            let room = ONE - fx::div(b.vel.dot(fwd).clamp(0, NITROUS_TOP), NITROUS_TOP);
            b.vel = b.vel.add(fwd.scale(fx::mul(fx::mul(NITROUS_PUSH, room), dt)));
        }

        let want_steer = fx::mul(c.steer, tune.steer);
        // steer less at speed, as an arcade car does
        let speed = self.body.vel.len();
        let calm = ONE - fx::div(speed.min(int(45)), int(70));
        self.steer = fx::approach(self.steer, fx::mul(want_steer, calm), STEER_RATE);

        // gravity, lessened in flight
        let g = if flying {
            rubble::GRAVITY / 6
        } else {
            rubble::GRAVITY
        };
        self.body.vel.y -= fx::mul(g, dt);

        if flying {
            self.fly(c, tune, dt);
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
            ONE - ONE / 50
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
        let stilts = self.lift > ONE / 2 && down.y < -ONE / 2;
        // on the stilts, look further down, to know the ground below a ledge
        let reach = if stilts { max + int(12) } else { max };
        let mut grounded = 0;
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut hits: [Option<(i32, V3)>; 4] = [None; 4];
        // whether each wheel stands over a building still standing
        let mut standing = [false; 4];
        for (w, slot) in hits.iter_mut().enumerate() {
            let b = &self.body;
            let mount = b.pos.add(from_local(&b.basis, tune.mounts()[w]));
            // the ground: two steps of the ray against the height field
            let mut hit: Option<(i32, V3)> = None;
            if down.y < -ONE / 5 {
                let mut dist = fx::div(mount.y - t.height(mount.x, mount.z), -down.y);
                let p = mount.add(down.scale(dist));
                dist = fx::div(mount.y - t.height(p.x, p.z), -down.y);
                if dist >= -ONE / 2 && dist <= reach {
                    let p = mount.add(down.scale(dist));
                    hit = Some((dist.max(0), t.normal(p.x, p.z)));
                }
            }
            if let Some((d, n, j)) = city.ray(mount, down, reach, &mut scratch) {
                if hit.map(|h| d < h.0).unwrap_or(true) {
                    hit = Some((d, n));
                    standing[w] = city.pieces[j as usize].state == rubble::INTACT;
                }
            }
            *slot = hit;
        }
        // On the stilts each wheel stands on a floor of its own that rises
        // at most `CLIMB` m/s (and falls at most half as fast again), and no
        // wheel hangs more than a metre below the highest ground under the
        // four: a fallen block the high body clears is climbed level and
        // slowly, rather than bottoming the springs, launching the car or
        // tipping it off a corner. A building still standing, and anything
        // more than `LEDGE` above a wheel's floor, is a wall: that wheel
        // keeps its floor, and the body meets the wall or passes over it.
        let mounts = tune.mounts();
        let b = &self.body;
        let mount_y = |w: usize| b.pos.y + from_local(&b.basis, mounts[w]).y;
        let ground: [Option<i32>; 4] =
            std::array::from_fn(|w| hits[w].map(|h| mount_y(w) + fx::mul(down.y, h.0)));
        if stilts {
            let wall: [bool; 4] =
                std::array::from_fn(|w| ground[w].is_some_and(|g| standing[w] || g > self.feet[w] + LEDGE));
            let top = (0..4).filter(|&w| !wall[w]).filter_map(|w| ground[w]).max();
            for w in 0..4 {
                let want = if wall[w] {
                    Some(self.feet[w])
                } else {
                    top.map(|top| ground[w].unwrap_or(top - ONE).max(top - ONE))
                };
                let Some(want) = want else { continue };
                self.feet[w] = if want > self.feet[w] {
                    (self.feet[w] + fx::mul(CLIMB, dt)).min(want)
                } else {
                    (self.feet[w] - fx::mul(CLIMB * 3 / 2, dt)).max(want)
                };
                let d = fx::div(mount_y(w) - self.feet[w], -down.y);
                hits[w] = (d <= max).then(|| (d.max(0), hits[w].map(|h| h.1).unwrap_or(up)));
            }
        } else {
            for w in 0..4 {
                if let Some(g) = ground[w] {
                    self.feet[w] = g;
                }
            }
        }
        for (w, hit) in hits.into_iter().enumerate() {
            let b = &self.body;
            let mount = b.pos.add(from_local(&b.basis, tune.mounts()[w]));
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
            let lateral = (-fx::mul(vs, SLIP_STIFFNESS)).clamp(-cap, cap);
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
            // tyre forces act a little below the middle of the car
            let height = up.dot(b.pos.sub(contact));
            let level = contact.add(up.scale((height - tune.roll).max(0)));
            self.push(contact, n_up.scale(spring), dt);
            self.push(level, s.scale(lateral).add(f.scale(long)), dt);
        }
        self.scratch = scratch;
        self.grounded = grounded;
        if stilts {
            // the stilts hold the car level
            let b = &mut self.body;
            let level = b.basis[1].cross(V3::UP);
            b.ang = b.ang.add(level.scale(fx::mul(6 * ONE, dt)));
        } else if grounded == 0 {
            // a little control in the air
            let b = &mut self.body;
            let yaw = fx::mul(c.steer, ONE * 2);
            let pitch = fx::mul(c.throttle, -ONE * 3 / 2);
            let want = from_local(&b.basis, v3(pitch, yaw, 0));
            b.ang = b.ang.add(want.sub(b.ang).scale(ONE / 40));
        }
    }

    /// The dash: a kick along the ground toward where the keys point (the
    /// nose when none do), and a little up.
    fn dash_off(&mut self, c: Controls) {
        let b = &mut self.body;
        let fwd = b.basis[2].neg();
        let flat = v3(fwd.x, 0, fwd.z).norm();
        let left = V3::UP.cross(flat);
        let mut dir = flat
            .scale(c.throttle.signum() * ONE)
            .add(left.scale(c.steer.signum() * ONE));
        if dir.len() < ONE / 2 {
            dir = flat;
        }
        let kick = dir.norm().scale(DASH_SPEED).add(V3::UP.scale(DASH_HOP));
        b.vel = b.vel.add(kick);
    }

    /// The instant turn: the car and its speed swing round together, a
    /// quarter turn toward a held left or right, a half turn otherwise.
    fn turn_round(&mut self, c: Controls) {
        let angle = match c.steer.signum() {
            1 => fx::TURN / 4,
            -1 => -fx::TURN / 4,
            _ => fx::TURN / 2,
        };
        let q = Quat::yaw(angle);
        let b = &mut self.body;
        b.rot = q.mul(b.rot).norm();
        b.basis = b.rot.basis();
        b.vel = q.rotate(b.vel);
        b.ang = q.rotate(b.ang);
        self.steer = 0;
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
        b.ang = b.ang.add(axis.scale(fx::mul(RIGHTING, dt))).scale(ONE - ONE / 12);
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
    fn the_flip_key_has_a_car_on_its_roof_back_on_its_wheels_in_under_four_and_a_quarter_seconds() {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, int(2), 0), 0);
        car.body.rot = Quat::axis_angle(v3(0, 0, ONE), deg(180));
        car.body.basis = car.body.rot.basis();
        drive(&mut car, Controls::default(), &t, &mut r, 120);
        let flip = Controls {
            flip: true,
            ..Controls::default()
        };
        let mut k = 0;
        while k < 600 && !(car.up().y > ONE * 9 / 10 && car.grounded == 4) {
            let upright = car.up().y > ONE * 9 / 10;
            drive(
                &mut car,
                if upright { Controls::default() } else { flip },
                &t,
                &mut r,
                1,
            );
            k += 1;
        }
        assert!(k < 255, "{k} ticks");
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

    /// A car settled on flat ground and rolling north at `speed` m/s.
    fn rolling(speed: i32) -> (Terrain, Rubble, Car) {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, ONE, 0), 0);
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        car.body.vel = car.fwd().scale(int(speed));
        (t, r, car)
    }

    #[test]
    fn the_dash_kicks_the_car_sideways_toward_the_held_key() {
        let (t, mut r, mut car) = rolling(10);
        car.kit.dash = true;
        let dash = Controls {
            dash: true,
            steer: ONE,
            ..Controls::default()
        };
        drive(&mut car, dash, &t, &mut r, 1);
        assert!(
            car.body.vel.x < -int(15),
            "kicked left (−x): {}",
            car.body.vel.x / ONE
        );
        drive(&mut car, dash, &t, &mut r, 30);
        let x = car.body.vel.x;
        drive(&mut car, dash, &t, &mut r, 1);
        assert!(car.body.vel.x >= x - ONE, "once, then it recharges");
        assert!(car.dash.wait > 0);
    }

    #[test]
    fn the_instant_turn_swings_the_car_and_its_speed_round_together() {
        let (t, mut r, mut car) = rolling(20);
        car.kit.spin = true;
        let spin = Controls {
            spin: true,
            ..Controls::default()
        };
        drive(&mut car, spin, &t, &mut r, 1);
        assert!(car.fwd().z > ONE * 9 / 10, "it faces south: {:?}", car.fwd());
        assert!(car.forward_speed() > int(15), "and drives on that way");
        assert!(car.body.vel.z > int(15), "{}", car.body.vel.z / ONE);
        let (t, mut r, mut car) = rolling(20);
        car.kit.spin = true;
        let left = Controls {
            spin: true,
            steer: ONE,
            ..Controls::default()
        };
        drive(&mut car, left, &t, &mut r, 1);
        assert!(
            car.fwd().x < -ONE * 9 / 10,
            "a quarter turn left: {:?}",
            car.fwd()
        );
    }

    #[test]
    fn nitrous_sends_it_far_past_its_top_speed_and_then_burns_out() {
        let (t, mut r, mut car) = rolling(20);
        car.kit.nitrous = true;
        let go = Controls {
            nitrous: true,
            throttle: ONE,
            ..Controls::default()
        };
        drive(&mut car, go, &t, &mut r, NITROUS_TICKS as usize);
        assert!(
            car.forward_speed() > BASE.top * 3 / 2,
            "{} m/s",
            car.forward_speed() / ONE
        );
        let plain = Controls {
            throttle: ONE,
            ..Controls::default()
        };
        drive(&mut car, plain, &t, &mut r, 60 * 6);
        assert!(
            car.forward_speed() < BASE.top + int(2),
            "back to its own top speed"
        );
    }

    #[test]
    fn the_leap_jumps_the_car_off_the_ground_and_it_lands_on_its_wheels() {
        let (t, mut r, mut car) = rolling(0);
        car.kit.leap = true;
        let leap = Controls {
            leap: true,
            ..Controls::default()
        };
        let mut top = 0;
        for _ in 0..60 * 3 {
            car.step(leap, &t, &mut r, fx::ratio(1, 60));
            top = top.max(car.body.pos.y);
        }
        assert!(top > int(5), "up {} m", top / ONE);
        assert!(
            car.up().y > ONE * 9 / 10 && car.grounded == 4,
            "down again on its wheels"
        );
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

    /// Full lock held from `speed` m/s for `ticks`, then straight: the
    /// lowest the car's up pointed.
    fn hard_turn(speed: i32, ticks: usize) -> i32 {
        let t = Terrain::new();
        let mut r = empty();
        let mut car = Car::new(v3(0, ONE, 0), 0);
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        car.body.vel = car.fwd().scale(int(speed));
        let mut low = ONE;
        for k in 0..240 {
            let c = Controls {
                steer: if k < ticks { ONE } else { 0 },
                throttle: ONE / 2,
                ..Controls::default()
            };
            car.step(c, &t, &mut r, fx::ratio(1, 60));
            low = low.min(car.up().y);
        }
        low
    }

    #[test]
    fn too_hard_a_turn_at_speed_rolls_the_car_and_a_sane_one_does_not() {
        assert!(
            hard_turn(25, 150) > ONE * 9 / 10,
            "a long turn at 90 km/h stays up"
        );
        assert!(hard_turn(40, 30) > ONE * 9 / 10, "a flick at 144 km/h stays up");
        assert!(hard_turn(40, 150) < 0, "full lock held at 144 km/h rolls it");
    }

    /// One fallen downtown block, 8 m on a side, lying on the ground.
    fn a_fallen_block(z: i32) -> Rubble {
        let mut r = a_box(z, 4);
        r.pieces[0].state = rubble::ASLEEP;
        r
    }

    /// A building 8 m wide and long and `half` × 2 m tall, standing at `z`.
    fn a_box(z: i32, half: i32) -> Rubble {
        use crate::city::{GroupSpec, PieceSpec, STYLE_PLAIN};
        let t = Terrain::new();
        let bp = Blueprint {
            pieces: vec![PieceSpec {
                pos: v3(0, t.height(0, int(z)) + int(half), int(z)),
                half: v3(int(4), int(half), int(4)),
                rot: Quat::IDENTITY,
                rgb: [ONE; 3],
                style: STYLE_PLAIN,
                sup: [0; 4],
                nsup: 0,
                all: false,
                base: false,
            }],
            groups: vec![GroupSpec {
                start: 0,
                end: 1,
                building: false,
            }],
            ..Blueprint::default()
        };
        Rubble::new(&bp)
    }

    #[test]
    fn big_wheels_lift_the_car_over_a_fallen_block_for_eight_seconds() {
        let t = Terrain::new();
        let mut r = a_fallen_block(-40);
        let mut car = Car::new(v3(0, int(2), 0), 0);
        car.kit.big_wheels = true;
        drive(&mut car, Controls::default(), &t, &mut r, 60);
        let small = car.body.pos.y;
        let big = Controls {
            big: true,
            ..Controls::default()
        };
        drive(&mut car, big, &t, &mut r, 1);
        drive(&mut car, Controls::default(), &t, &mut r, 120);
        let ground = t.height(car.body.pos.x, car.body.pos.z);
        assert!(
            car.body.pos.y - HALF.y - ground > int(8),
            "the body clears 8 m: {} above {}",
            car.body.pos.y / ONE,
            small / ONE
        );
        let go = Controls {
            throttle: ONE,
            ..Controls::default()
        };
        drive(&mut car, go, &t, &mut r, 60 * 4);
        assert!(
            car.body.pos.z < -int(60),
            "over the block: {}",
            car.body.pos.z / ONE
        );
        assert!(car.up().y > ONE * 8 / 10, "upright: {:?}", car.up());
        drive(
            &mut car,
            Controls::default(),
            &t,
            &mut r,
            BIG_TICKS as usize + 120,
        );
        assert!(
            car.body.pos.y < small + int(2),
            "back down: {}",
            car.body.pos.y / ONE
        );
        assert!(car.big.wait > 0, "and recharging");
    }

    #[test]
    fn big_wheels_do_not_climb_a_building() {
        let t = Terrain::new();
        let mut r = a_box(-40, 3);
        let mut car = Car::new(v3(0, int(2), 0), 0);
        car.kit.big_wheels = true;
        let mut highest = 0;
        for k in 0..60 * 5 {
            let c = Controls {
                big: k == 0,
                throttle: ONE,
                ..Controls::default()
            };
            car.step(c, &t, &mut r, fx::ratio(1, 60));
            highest = highest.max(car.body.pos.y - t.height(car.body.pos.x, car.body.pos.z));
        }
        assert!(
            highest < int(13),
            "it rode over the house, not onto it: {}",
            highest / ONE
        );
        assert!(
            car.body.pos.z < -int(60),
            "and on past it: {}",
            car.body.pos.z / ONE
        );
    }

    #[test]
    fn on_small_wheels_the_block_stops_the_car() {
        let t = Terrain::new();
        let mut r = a_fallen_block(-40);
        let mut car = Car::new(v3(0, int(2), 0), 0);
        let go = Controls {
            throttle: ONE,
            ..Controls::default()
        };
        drive(&mut car, go, &t, &mut r, 60 * 5);
        assert!(car.body.pos.z > -int(36), "{}", car.body.pos.z / ONE);
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

#[cfg(test)]
mod probe_roll {
    use super::*;
    use crate::city::Blueprint;

    #[test]
    #[ignore]
    fn roll_table() {
        let t = Terrain::new();
        let mut log = String::new();
        for speed in [10, 15, 20, 25, 30, 35, 40] {
            for hold in [30, 60, 90, 150] {
                let steer = ONE;
                let mut r = Rubble::new(&Blueprint::default());
                let mut car = Car::new(v3(0, ONE, 0), 0);
                for _ in 0..60 {
                    car.step(Controls::default(), &t, &mut r, fx::ratio(1, 60));
                }
                car.body.vel = car.fwd().scale(int(speed));
                let mut low = ONE;
                for k in 0..240 {
                    let c = Controls {
                        steer: if k < hold { steer } else { 0 },
                        throttle: ONE / 2,
                        ..Controls::default()
                    };
                    car.step(c, &t, &mut r, fx::ratio(1, 60));
                    low = low.min(car.up().y);
                }
                log += &format!("{speed} m/s hold {hold}: lowest up.y {}\n", low * 100 / ONE);
            }
        }
        panic!("{log}");
    }
}
