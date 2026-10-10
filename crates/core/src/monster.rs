//! The pursuer: a giant flat picture that stands up out of the ground and
//! walks at you, straight through whatever is in the way.
//!
//! It always faces you, as a cardboard standee faces the room, and moves
//! along the way it faces, turning at a limited rate, so a sharp turn at
//! speed can slip past it. Every step it sweeps the slab in front of it
//! through the city (`Rubble::sweep`), and what it meets is thrown forward
//! and outward. It gains speed the longer the chase lasts.
//!
//! Every so often it jumps: it vanishes and pops up a long way straight
//! ahead of you, stands there a moment, and comes on again from there.

use crate::fx::{self, int, ONE};
use crate::geom::{facing, v3, V3};
use crate::rng::Rng;
use crate::rubble::Rubble;
use crate::terrain::{Terrain, BOUND};
use serde::Deserialize;

/// Seconds, metres and m/s as written in `data/pursuer.json`, read into Q16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    pub spawn_distance: i32,
    pub rise_ticks: i32,
    pub wait_ticks: i32,
    pub start_speed: i32,
    /// m/s gained each tick
    pub accel: i32,
    pub max_speed: i32,
    /// angle units a tick
    pub turn: i32,
    pub catchup_distance: i32,
    pub catchup_percent: i32,
    pub width: i32,
    pub height: i32,
    /// the share of its half-width that catches you
    pub reach: i32,
    pub jump_first_ticks: i32,
    pub jump_min_ticks: i32,
    pub jump_max_ticks: i32,
    pub jump_distance: i32,
    pub jump_rise_ticks: i32,
    pub jump_hold_ticks: i32,
}

#[derive(Deserialize)]
struct TuningJson {
    spawn_distance: serde_json::Value,
    rise_seconds: serde_json::Value,
    wait_seconds: serde_json::Value,
    start_speed: serde_json::Value,
    accel: serde_json::Value,
    max_speed: serde_json::Value,
    turn_degrees: serde_json::Value,
    catchup_distance: serde_json::Value,
    catchup_percent: i32,
    width: serde_json::Value,
    height: serde_json::Value,
    reach_percent: i32,
    jump_first_seconds: serde_json::Value,
    jump_every_min_seconds: serde_json::Value,
    jump_every_max_seconds: serde_json::Value,
    jump_distance: serde_json::Value,
    jump_rise_seconds: serde_json::Value,
    jump_hold_seconds: serde_json::Value,
}

/// A decimal number from JSON as Q16, read from its text so the core never
/// holds a float.
pub fn decimal(v: &serde_json::Value) -> Result<i32, String> {
    let s = v.to_string();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, s),
    };
    let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
    let w: i64 = whole.parse().map_err(|_| format!("not a number: {v}"))?;
    let mut q = w << 16;
    let mut scale = 1i64;
    let mut f = 0i64;
    for ch in frac.chars().take(6) {
        let d = ch.to_digit(10).ok_or(format!("not a number: {v}"))? as i64;
        f = f * 10 + d;
        scale *= 10;
    }
    q += (f << 16) / scale;
    Ok(if neg { -(q as i32) } else { q as i32 })
}

impl Tuning {
    pub fn parse(json: &str) -> Result<Tuning, String> {
        let j: TuningJson = serde_json::from_str(json).map_err(|e| format!("pursuer.json: {e}"))?;
        let ticks = |v: &serde_json::Value| decimal(v).map(|s| fx::mul(s, int(60)) / ONE);
        Ok(Tuning {
            spawn_distance: decimal(&j.spawn_distance)?,
            rise_ticks: ticks(&j.rise_seconds)?,
            wait_ticks: ticks(&j.wait_seconds)?,
            start_speed: decimal(&j.start_speed)?,
            accel: decimal(&j.accel)? / 60,
            max_speed: decimal(&j.max_speed)?,
            turn: fx::deg(decimal(&j.turn_degrees)? / ONE) / 60,
            catchup_distance: decimal(&j.catchup_distance)?,
            catchup_percent: j.catchup_percent,
            width: decimal(&j.width)?,
            height: decimal(&j.height)?,
            reach: j.reach_percent * ONE / 100,
            jump_first_ticks: ticks(&j.jump_first_seconds)?,
            jump_min_ticks: ticks(&j.jump_every_min_seconds)?,
            jump_max_ticks: ticks(&j.jump_every_max_seconds)?,
            jump_distance: decimal(&j.jump_distance)?,
            jump_rise_ticks: ticks(&j.jump_rise_seconds)?.max(1),
            jump_hold_ticks: ticks(&j.jump_hold_seconds)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monster {
    /// the middle of its foot, on the ground
    pub pos: V3,
    pub yaw: i32,
    pub speed: i32,
    /// ticks since it began to rise
    pub age: i32,
    pub tune: Tuning,
    pub hits: u32,
    /// ticks since it last jumped ahead of you
    pub since_jump: i32,
    /// ticks of walking left before the next jump
    pub jump_in: i32,
    rng: Rng,
}

impl Monster {
    pub fn new(at: V3, yaw: i32, tune: Tuning) -> Monster {
        Monster {
            pos: at,
            yaw,
            speed: 0,
            age: 0,
            tune,
            hits: 0,
            since_jump: i32::MAX / 2,
            jump_in: tune.jump_first_ticks,
            rng: Rng::new(17),
        }
    }

    pub fn fwd(&self) -> V3 {
        facing(self.yaw, 0)
    }

    /// How far it has stood up: zero lying flat, ONE upright.
    pub fn rise(&self) -> i32 {
        if self.since_jump < self.tune.jump_rise_ticks {
            return Self::pop(fx::div(self.since_jump, self.tune.jump_rise_ticks));
        }
        Self::pop(fx::div(
            self.age.min(self.tune.rise_ticks),
            self.tune.rise_ticks.max(1),
        ))
    }

    fn pop(t: i32) -> i32 {
        // overshoot a little and settle, as a pop-up page does
        let s = ONE - fx::mul(ONE - t, ONE - t);
        if t >= ONE {
            return ONE;
        }
        s + fx::mul(fx::mul(t, ONE - t), ONE / 2)
    }

    pub fn moving(&self) -> bool {
        self.age >= self.tune.rise_ticks + self.tune.wait_ticks
    }

    /// Standing still where it has just popped up.
    pub fn holding(&self) -> bool {
        self.since_jump < self.tune.jump_hold_ticks
    }

    /// Where a jump would put it: `jump_distance` along the way you are
    /// heading, inside the city. None when it is already nearer than that,
    /// or the city's edge leaves no room ahead of you.
    fn jump_spot(&self, target: V3, heading: V3) -> Option<V3> {
        let d = self.tune.jump_distance;
        let flat = v3(heading.x, 0, heading.z);
        if self.distance(target) <= d + int(20) || flat.len() < ONE / 10 {
            return None;
        }
        let lim = int(BOUND - 10);
        let at = target.add(flat.norm().scale(d));
        let at = v3(at.x.clamp(-lim, lim), 0, at.z.clamp(-lim, lim));
        (at.sub(target).len_xz() > d * 3 / 4).then_some(at)
    }

    /// One tick. `heading` is the way you are going. Returns true on the
    /// tick it jumps.
    pub fn step(&mut self, target: V3, heading: V3, t: &Terrain, city: &mut Rubble, dt: i32) -> bool {
        self.age += 1;
        self.since_jump = self.since_jump.saturating_add(1);
        let to = target.sub(self.pos);
        let want = fx::atan2(-to.x, -to.z);
        let dist = to.len_xz();
        if self.moving() && !self.holding() {
            self.jump_in -= 1;
            if self.jump_in <= 0 {
                match self.jump_spot(target, heading) {
                    Some(at) => {
                        self.pos = v3(at.x, t.height(at.x, at.z), at.z);
                        let back = target.sub(self.pos);
                        self.yaw = fx::atan2(-back.x, -back.z);
                        self.since_jump = 0;
                        self.jump_in = self.rng.range(self.tune.jump_min_ticks, self.tune.jump_max_ticks);
                        return true;
                    }
                    // try again in a second
                    None => self.jump_in = 60,
                }
            }
        }
        if !self.moving() || self.holding() {
            self.yaw = want;
            self.pos.y = t.height(self.pos.x, self.pos.z);
            return false;
        }
        let turn = fx::wrap(want - self.yaw).clamp(-self.tune.turn, self.tune.turn);
        self.yaw = (self.yaw + turn) & (fx::TURN - 1);
        let base = (self.tune.start_speed
            + self.tune.accel * (self.age - self.tune.rise_ticks - self.tune.wait_ticks))
            .min(self.tune.max_speed);
        self.speed = if dist > self.tune.catchup_distance {
            base + base * self.tune.catchup_percent / 100
        } else {
            base
        };
        let step = fx::mul(self.speed, dt);
        let fwd = self.fwd();
        self.hits += city.sweep(
            self.pos,
            fwd,
            self.tune.width / 2,
            self.tune.height,
            -ONE,
            step + int(2),
            self.speed,
        );
        self.pos = self.pos.add(fwd.scale(step));
        let lim = int(BOUND + 40);
        self.pos.x = self.pos.x.clamp(-lim, lim);
        self.pos.z = self.pos.z.clamp(-lim, lim);
        self.pos.y = t.height(self.pos.x, self.pos.z);
        false
    }

    /// Whether its picture has reached a point: the point is within reach
    /// of its middle sideways, below its top, and level with or behind its
    /// face.
    pub fn caught(&self, p: V3) -> bool {
        if !self.moving() || self.since_jump < self.tune.jump_rise_ticks {
            return false;
        }
        let fwd = self.fwd();
        let right = v3(-fwd.z, 0, fwd.x);
        let rel = p.sub(self.pos);
        let ahead = rel.dot(fwd);
        let side = rel.dot(right).abs();
        ahead < ONE * 3 / 2
            && ahead > -int(6)
            && side < fx::mul(self.tune.width / 2, self.tune.reach)
            && rel.y < self.tune.height
    }

    /// The distance to a point, along the ground.
    pub fn distance(&self, p: V3) -> i32 {
        p.sub(self.pos).len_xz()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Blueprint;

    fn tune() -> Tuning {
        Tuning::parse(include_str!("../../../data/pursuer.json")).unwrap()
    }

    #[test]
    fn decimals_are_read_without_floats() {
        let read = |text: &str| decimal(&serde_json::from_str(text).unwrap()).unwrap();
        assert_eq!(read("2.5"), ONE * 5 / 2);
        assert_eq!(read("-3"), -3 * ONE);
        assert_eq!(read("0.16"), 10_485);
    }

    #[test]
    fn it_stands_still_while_it_rises_then_walks_at_you_and_catches_you() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let tn = tune();
        let mut m = Monster::new(v3(-int(100), 0, 0), 0, tn);
        let you = v3(int(20), 0, 0);
        for _ in 0..tn.rise_ticks {
            m.step(you, V3::ZERO, &t, &mut r, fx::ratio(1, 60));
        }
        assert_eq!(m.pos.x, -int(100));
        assert!(m.fwd().x > ONE * 99 / 100, "it faces you: {:?}", m.fwd());
        let mut caught = false;
        for _ in 0..60 * 30 {
            m.step(you, V3::ZERO, &t, &mut r, fx::ratio(1, 60));
            if m.caught(you) {
                caught = true;
                break;
            }
        }
        assert!(caught, "at {:?}", m.pos);
    }

    #[test]
    fn it_gains_speed_up_to_its_limit() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let tn = tune();
        let mut m = Monster::new(V3::ZERO, 0, tn);
        let far_ahead = v3(0, 0, -int(100));
        for _ in 0..(tn.rise_ticks + tn.wait_ticks + 60) {
            m.step(far_ahead, V3::ZERO, &t, &mut r, fx::ratio(1, 60));
        }
        let early = m.speed;
        m.age += 60 * 600;
        m.step(far_ahead, V3::ZERO, &t, &mut r, fx::ratio(1, 60));
        assert!(early < tn.start_speed + ONE);
        assert_eq!(m.speed, tn.max_speed);
    }

    #[test]
    fn a_sharp_turn_beside_it_is_not_a_catch() {
        let tn = tune();
        let mut m = Monster::new(V3::ZERO, 0, tn);
        m.age = 10_000;
        // right beside its edge, out of reach
        assert!(!m.caught(v3(tn.width / 2 + int(2), 0, 0)));
        assert!(m.caught(v3(int(3), 0, 0)));
        assert!(!m.caught(v3(0, 0, -int(20))), "well ahead of it");
    }

    #[test]
    fn now_and_then_it_jumps_to_just_ahead_of_you_and_stands_there() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let tn = tune();
        let mut m = Monster::new(v3(-int(300), 0, 0), 0, tn);
        let you = v3(int(100), 0, 0);
        let east = v3(ONE, 0, 0);
        m.age = tn.rise_ticks + tn.wait_ticks;
        assert_eq!(m.jump_in, tn.jump_first_ticks);
        m.jump_in = 30;
        let mut when = None;
        for k in 0..32 {
            if m.step(you, east, &t, &mut r, fx::ratio(1, 60)) {
                when = Some(k);
                break;
            }
        }
        assert_eq!(when, Some(29), "it jumps when its count runs out");
        let ahead = m.pos.sub(you);
        assert!(
            (ahead.x - tn.jump_distance).abs() < ONE && ahead.z.abs() < ONE,
            "it is {:?} from you",
            ahead
        );
        assert!(m.fwd().x < -ONE * 99 / 100, "it faces back at you: {:?}", m.fwd());
        assert!(m.rise() < ONE / 4, "it pops up from flat");
        assert!(!m.caught(m.pos), "no catch while it pops up");
        let at = m.pos;
        for _ in 0..tn.jump_hold_ticks - 1 {
            m.step(you, east, &t, &mut r, fx::ratio(1, 60));
        }
        assert_eq!((m.pos.x, m.pos.z), (at.x, at.z), "it stands still a moment");
        m.step(you, east, &t, &mut r, fx::ratio(1, 60));
        m.step(you, east, &t, &mut r, fx::ratio(1, 60));
        assert!(m.pos.x < at.x, "then it comes on");
        assert!(m.jump_in >= tn.jump_min_ticks - 2 && m.jump_in <= tn.jump_max_ticks);
    }

    #[test]
    fn it_does_not_jump_when_it_is_already_close() {
        let t = Terrain::new();
        let mut r = Rubble::new(&Blueprint::default());
        let tn = tune();
        let you = v3(int(100), 0, 0);
        let mut m = Monster::new(you.sub(v3(tn.jump_distance, 0, 0)), 0, tn);
        m.age = tn.rise_ticks + tn.wait_ticks;
        m.jump_in = 1;
        let start = m.pos;
        assert!(!m.step(you, v3(ONE, 0, 0), &t, &mut r, fx::ratio(1, 60)));
        assert!(m.pos.x > start.x, "it walks on");
    }
}
