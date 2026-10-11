//! The game: its modes, the fixed 60 Hz step, the cameras, and everything
//! the page reads back each frame.
//!
//! Modes: the title; the house, walking and picking things up; the chase,
//! on foot and then in the car; caught. Holding a thing for `hold_seconds`
//! folds the house and raises that thing's pursuer. The score is the time
//! from the pursuer standing up to it catching you.

use crate::car::{self, Car, Controls};
use crate::city::{self, Blueprint, KIND_GRASS, KIND_ROAD, KIND_WALK};
use crate::content::{Palette, Rgb};
use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, v3, Affine, V3};
use crate::house::{self, House, Item, Shape};
use crate::mesh::{self, Mesh};
use crate::monster::{Monster, Tuning};
use crate::quat::{from_local, Quat};
use crate::render::{
    self, Batch, Camera, View, BUF_DYNAMIC, BUF_PIECES, BUF_SCENERY, DRAW_VALUES, INSTANCE_VALUES, LAYERS,
    LAYER_CAR, LAYER_COCKPIT, LAYER_WORLD, MAT_VALUES, PASS_INK, PASS_SOLID, PASS_SPRITE, RECT_VALUES,
};
use crate::rng::Rng;
use crate::rubble::{self, Rubble};
use crate::terrain::{self, Terrain};
use crate::walker::{Legs, Walker};
use serde::Deserialize;
use serde_json::json;

/// The driver's eye above the car's middle: high in the seat, so the road
/// ahead shows over the dash.
const SEAT_EYE: i32 = ONE * 82 / 100;
/// How long the view shakes after the pursuer jumps in front of you.
const SCARE_TICKS: i32 = 50;

pub const FORWARD: u32 = 1;
pub const BACK: u32 = 2;
pub const LEFT: u32 = 4;
pub const RIGHT: u32 = 8;
pub const JUMP: u32 = 16;
pub const FLY: u32 = 32;
pub const FLIP: u32 = 64;
pub const USE: u32 = 128;
pub const SHIFT: u32 = 256;
/// set by the page while the game has the pointer
pub const FOCUS: u32 = 512;
pub const BIG: u32 = 1024;
pub const DASH: u32 = 2048;
pub const SPIN: u32 = 4096;
pub const NITROUS: u32 = 8192;

const TICK_US: i64 = 1_000_000 / 60;
const MAX_TICKS_PER_FRAME: i32 = 4;
const DT: i32 = ONE / 60;
/// How near an item must be to pick it up, and the car to get in.
const REACH: i32 = ONE * 26 / 10;
const CAR_REACH: i32 = ONE * 5;
/// The rear-view mirror, in the car's frame, and how far it tilts from
/// level (a little down) toward the pursuer's face (up).
const REAR_EYE: V3 = v3(0, ONE * 88 / 100, ONE * 40 / 100);
const REAR_TILT_LEAST: i32 = -deg(3);
const REAR_TILT_MOST: i32 = deg(32);
/// Within this many metres of the record player its callout shows.
const PLAYER_CALLOUT: i32 = ONE * 9;
/// The most pixels a touch screen is drawn at per CSS pixel, in percent: a
/// phone's 3x screen costs more than it shows.
const TOUCH_DPR_MOST: i32 = 200;
/// How far a software renderer draws.
const LOW_FAR: i32 = 320 * ONE;
/// Mouse sensitivity: angle units per CSS pixel (scaled by 100 at the shim).
const LOOK: i32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Title,
    House,
    Chase,
    Caught,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Upgrade {
    pub id: String,
    pub name: String,
    pub text: String,
    /// the text for a touch screen, which names its button and not a key
    #[serde(default)]
    pub touch: Option<String>,
}

#[derive(Deserialize)]
struct UpgradesFile {
    start: Upgrade,
    at: Vec<i32>,
    pool: Vec<Upgrade>,
}

/// The car's abilities: the one it starts with, the times (seconds) a new
/// one arrives, and the pool each is drawn from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrades {
    pub start: Upgrade,
    pub at: Vec<i32>,
    pub pool: Vec<Upgrade>,
}

pub fn parse_upgrades(json: &str) -> Result<Upgrades, String> {
    let f: UpgradesFile = serde_json::from_str(json).map_err(|e| format!("upgrades.json: {e}"))?;
    let known = [
        "flip",
        "big_wheels",
        "turbo",
        "wings",
        "dash",
        "spin",
        "nitrous",
        "leap",
    ];
    for u in std::iter::once(&f.start).chain(&f.pool) {
        if !known.contains(&u.id.as_str()) {
            return Err(format!("upgrades.json: no rule called {}", u.id));
        }
    }
    Ok(Upgrades {
        start: f.start,
        at: f.at,
        pool: f.pool,
    })
}

/// The next ability, at random from those in `pool` not yet `drawn`; of
/// entries that share an id, only the first not yet drawn can come.
pub fn draw(pool: &[Upgrade], drawn: &[usize], rng: &mut Rng) -> Option<usize> {
    let open: Vec<usize> = (0..pool.len())
        .filter(|k| !drawn.contains(k))
        .filter(|&k| (0..k).all(|j| pool[j].id != pool[k].id || drawn.contains(&j)))
        .collect();
    if open.is_empty() {
        return None;
    }
    Some(open[rng.range(0, open.len() as i32 - 1) as usize])
}

/// A record on the record player: the chase music.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub name: String,
    /// whether the build holds its recording (scripts/build.sh)
    pub present: bool,
}

#[derive(Deserialize)]
struct MusicFile {
    tracks: Vec<TrackJson>,
}

#[derive(Deserialize)]
struct TrackJson {
    id: String,
    name: String,
    #[serde(default = "yes")]
    present: bool,
}

fn yes() -> bool {
    true
}

pub fn parse_tracks(json: &str) -> Result<Vec<Track>, String> {
    let f: MusicFile = serde_json::from_str(json).map_err(|e| format!("music.json: {e}"))?;
    let tracks: Vec<Track> = f
        .tracks
        .into_iter()
        .map(|t| Track {
            id: t.id,
            name: t.name,
            present: t.present,
        })
        .collect();
    if !tracks.iter().any(|t| t.present) {
        return Err("music.json: no track is present".into());
    }
    Ok(tracks)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dust {
    pos: V3,
    vel: V3,
    size: i32,
    life: i32,
}

/// The scenery: values, and the solid and outlined runs as (mesh, offset, count).
#[derive(Debug, Clone, Default)]
struct Scenery {
    values: Vec<i32>,
    solid: Vec<(u32, u32, u32)>,
    ink: Vec<(u32, u32, u32)>,
}

pub struct Game {
    mode: Mode,
    paused: bool,
    pal: Palette,
    items: Vec<Item>,
    hold_ticks: i32,
    upgrades: Upgrades,
    /// the pool's abilities drawn so far this chase, in order
    drawn: Vec<usize>,
    /// chance for the draws, seeded from the page's clock as the chase starts
    luck: Rng,
    tracks: Vec<Track>,
    /// the record on the player: the chase music
    song: usize,
    /// counts changes of record, so the page can pop its callout again
    song_seq: u32,
    /// looking at the record player
    aim_player: bool,
    tuning: Tuning,
    terrain: Terrain,
    blueprint: Blueprint,
    pristine: Rubble,
    city: Rubble,
    house: House,
    walker: Walker,
    car: Car,
    driving: bool,
    monster: Option<Monster>,
    /// ticks since the house began to fold; negative before
    fold: i32,
    aim: Option<usize>,
    held: Option<usize>,
    hold: i32,
    chosen: Option<usize>,
    /// ticks of the chase so far: the score
    ticks: u32,
    best: u32,
    new_best: bool,
    unlocked: usize,
    /// the ability to tell of (a pool index, or None for the car's own) and ticks left
    toast: Option<(Option<usize>, i32)>,
    dust: Vec<Dust>,
    rng: Rng,
    look_yaw: i32,
    look_pitch: i32,
    look_idle: i32,
    shake: i32,
    /// ticks left of the jolt from the pursuer jumping in front of you
    scare: i32,
    clock: Option<i64>,
    acc: i64,
    width: i32,
    height: i32,
    sounds: Vec<&'static str>,
    pieces_version: u32,
    scenery: Scenery,
    // the frame's output
    dynamic: Vec<i32>,
    draws: Vec<u32>,
    rects: Vec<i32>,
    mats: Vec<i32>,
    views: Vec<View>,
    dirty: Vec<u32>,
    held_bits: u32,
    /// keys down on a display frame that ran no tick, for the next tick: a
    /// tap released before then still counts
    pending_bits: u32,
    title_angle: i32,
    /// drawing on a software renderer: no outlines, a shorter view, one mirror
    low: bool,
    /// played on a touch screen: drawn at no more than `TOUCH_DPR_MOST`
    touch: bool,
}

fn bit(bits: u32, b: u32) -> bool {
    bits & b != 0
}

/// Seconds and tenths, as the HUD shows a time: `m:ss.t`.
pub fn clock_text(ticks: u32) -> String {
    let tenths = ticks / 6;
    format!("{}:{:02}.{}", tenths / 600, (tenths / 10) % 60, tenths % 10)
}

/// A Q16 percentage as CSS wants it, two decimals.
fn pct(v: i32) -> String {
    let hundredths = (v as i64 * 100) >> 16;
    format!("{}.{:02}%", hundredths / 100, (hundredths % 100).abs())
}

/// A Q16 fraction as a decimal string, three places.
fn frac(v: i32) -> String {
    let thousandths = ((v.clamp(0, ONE) as i64) * 1000) >> 16;
    format!("{}.{:03}", thousandths / 1000, thousandths % 1000)
}

impl Game {
    pub fn new(
        palette: &str,
        items: &str,
        upgrades: &str,
        pursuer: &str,
        music: &str,
        saved: &str,
    ) -> Result<Game, String> {
        let pal = Palette::parse(palette)?;
        let (items, hold_seconds) = house::parse_items(items)?;
        let upgrades = parse_upgrades(upgrades)?;
        let tuning = Tuning::parse(pursuer)?;
        let tracks = parse_tracks(music)?;
        let terrain = Terrain::new();
        let blueprint = city::plan(&terrain, &pal);
        let pristine = Rubble::new(&blueprint);
        let house = House::new(&pal);
        let record: Option<serde_json::Value> = serde_json::from_str(saved).ok();
        let best = record
            .as_ref()
            .and_then(|r| r.get("best").and_then(|b| b.as_u64()))
            .unwrap_or(0) as u32;
        let first = tracks.iter().position(|t| t.present).unwrap_or(0);
        let song = record
            .as_ref()
            .and_then(|r| r.get("song").and_then(|s| s.as_str()))
            .and_then(|id| tracks.iter().position(|t| t.id == id && t.present))
            .unwrap_or(first);
        let mut g = Game {
            mode: Mode::Title,
            paused: false,
            pal,
            items,
            hold_ticks: hold_seconds * 60,
            upgrades,
            drawn: Vec::new(),
            luck: Rng::new(0),
            tracks,
            song,
            song_seq: 0,
            aim_player: false,
            tuning,
            city: pristine.clone(),
            pristine,
            terrain,
            blueprint,
            house,
            walker: Walker::new(V3::ZERO, 0, 0),
            car: Car::new(house::CAR_AT, house::CAR_YAW),
            driving: false,
            monster: None,
            fold: -1,
            aim: None,
            held: None,
            hold: 0,
            chosen: None,
            ticks: 0,
            best,
            new_best: false,
            unlocked: 0,
            toast: None,
            dust: Vec::new(),
            rng: Rng::new(5),
            look_yaw: 0,
            look_pitch: 0,
            look_idle: 0,
            shake: 0,
            scare: 0,
            clock: None,
            acc: 0,
            width: 1280,
            height: 720,
            sounds: Vec::new(),
            pieces_version: 0,
            scenery: Scenery::default(),
            dynamic: Vec::new(),
            draws: Vec::new(),
            rects: Vec::new(),
            mats: Vec::new(),
            views: Vec::new(),
            dirty: Vec::new(),
            held_bits: 0,
            pending_bits: 0,
            title_angle: 0,
            low: false,
            touch: false,
        };
        g.scenery = g.build_scenery();
        g.reset();
        g.frame();
        Ok(g)
    }

    /// Everything back as it was before a chase: the city whole, the house
    /// standing, you inside facing the first thing on the list.
    fn reset(&mut self) {
        self.city = self.pristine.clone();
        self.city.full_upload = true;
        self.pieces_version += 1;
        self.car = Car::new(house::CAR_AT, house::CAR_YAW);
        let start = v3(-ONE * 28 / 10, house::FLOOR, ONE * 33 / 10);
        let item = self.items.first().map(|i| i.centre).unwrap_or(V3::ZERO);
        let eye = start.add(V3::UP.scale(crate::walker::EYE));
        let (yaw, pitch) = crate::geom::yaw_pitch_of(item.sub(eye));
        self.walker = Walker::new(start, yaw, pitch);
        self.driving = false;
        self.monster = None;
        self.fold = -1;
        self.aim = None;
        self.aim_player = false;
        self.held = None;
        self.hold = 0;
        self.chosen = None;
        self.ticks = 0;
        self.new_best = false;
        self.unlocked = 0;
        self.drawn.clear();
        self.toast = None;
        self.dust.clear();
        self.look_yaw = 0;
        self.look_pitch = 0;
        self.scare = 0;
        // let the car settle on its wheels
        for _ in 0..90 {
            self.car
                .step(Controls::default(), &self.terrain, &mut self.city, DT);
        }
    }

    // ---- what the page calls ----

    pub fn resize(&mut self, css_w: i32, css_h: i32, dpr_pct: i32) -> [i32; 2] {
        let dpr_pct = if self.touch {
            dpr_pct.min(TOUCH_DPR_MOST)
        } else {
            dpr_pct
        };
        self.width = (css_w * dpr_pct / 100).clamp(64, 4096);
        self.height = (css_h * dpr_pct / 100).clamp(64, 4096);
        self.frame();
        [self.width, self.height]
    }

    /// The page is played on a touch screen.
    pub fn set_touch(&mut self, touch: bool) {
        self.touch = touch;
    }

    /// The page's renderer draws in software: draw less.
    pub fn set_low_detail(&mut self, low: bool) {
        self.low = low;
        self.frame();
    }

    pub fn begin(&mut self) {
        if self.mode == Mode::Title || self.mode == Mode::Caught {
            if self.mode == Mode::Caught {
                self.reset();
            }
            self.mode = Mode::House;
            self.paused = false;
        }
    }

    pub fn again(&mut self) {
        self.reset();
        self.mode = Mode::House;
        self.paused = false;
    }

    pub fn pause(&mut self) {
        if self.mode == Mode::House || self.mode == Mode::Chase {
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.clock = None;
    }

    pub fn to_title(&mut self) {
        self.reset();
        self.mode = Mode::Title;
        self.paused = false;
    }

    /// Knocks every piece of the city loose at once: the worst case the
    /// rubble's budget is built for, reachable from the page as `#everything`.
    pub fn wreck_all(&mut self) {
        for i in 0..self.city.len() {
            let k = (i as i32 % 7) - 3;
            self.city.loose(i, v3(k * ONE, int(2), -k * ONE), V3::ZERO);
        }
    }

    /// For filming (tools/film/): sets up a scene, as a person could have
    /// played to it, from a JSON description. Every field is optional.
    ///
    /// - `walker`: [x, z, heading°, pitch°] in the house, before the chase
    /// - `item`: the chase is on, with this item's pursuer, the house folded
    /// - `seconds`: of the chase so far (the clock and the pursuer's pace)
    /// - `abilities`: the car's, by id, in the order drawn
    /// - `car`: [x, z, heading°, m/s]: you are driving there
    /// - `monster`: [x, z], or `behind`: metres behind the car
    /// - `jump_in`: seconds until the pursuer next jumps out in front
    /// - `ravage`: [[x0, z0, x1, z1], …]: paths the pursuer has torn down
    /// - `settle`: seconds the loosed city falls before the scene starts
    pub fn stage(&mut self, spec: &str) -> Result<(), String> {
        let s: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("stage: {e}"))?;
        let num = |v: &serde_json::Value| -> Result<i32, String> { crate::monster::decimal(v) };
        let list = |key: &str| -> Result<Vec<i32>, String> {
            s.get(key)
                .and_then(|v| v.as_array())
                .map(|a| a.iter().map(num).collect())
                .unwrap_or(Ok(Vec::new()))
        };
        self.reset();
        self.mode = Mode::House;
        self.paused = false;
        let w = list("walker")?;
        if w.len() == 4 {
            let at = v3(w[0], house::FLOOR, w[1]);
            self.walker = Walker::new(at, deg(w[2] / ONE), deg(w[3] / ONE));
        }
        if let Some(id) = s.get("item").and_then(|v| v.as_str()) {
            let k = self
                .items
                .iter()
                .position(|i| i.id == id)
                .ok_or(format!("stage: no item {id}"))?;
            self.held = Some(k);
            self.unfold();
            self.fold = house::FOLD_TICKS * 4;
            let secs = s.get("seconds").map(num).transpose()?.unwrap_or(0) / ONE;
            self.ticks = secs as u32 * 60;
            self.unlocked = self.upgrades.at.iter().filter(|&&a| a <= secs).count();
            if let Some(m) = self.monster.as_mut() {
                m.age += self.ticks as i32 + m.tune.rise_ticks + m.tune.wait_ticks;
            }
        }
        let c = list("car")?;
        if c.len() == 4 {
            let at = v3(c[0], self.terrain.height(c[0], c[1]) + ONE, c[1]);
            self.car = Car::new(at, deg(c[2] / ONE));
            self.car.body.vel = self.car.fwd().scale(c[3]);
            self.driving = true;
            self.walker.pos = at;
        }
        for id in s
            .get("abilities")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let id = id.as_str().unwrap_or("");
            let k = (0..self.upgrades.pool.len())
                .find(|k| self.upgrades.pool[*k].id == id && !self.drawn.contains(k))
                .ok_or(format!("stage: no ability {id} left to draw"))?;
            self.drawn.push(k);
            let kit = &mut self.car.kit;
            match id {
                "big_wheels" => kit.big_wheels = true,
                "wings" => kit.wings = true,
                "turbo" => kit.turbo += 1,
                "dash" => kit.dash = true,
                "spin" => kit.spin = true,
                "nitrous" => kit.nitrous = true,
                "leap" => kit.leap = true,
                _ => {}
            }
        }
        let player = self.player_pos();
        let fwd = self.car.fwd();
        if let Some(m) = self.monster.as_mut() {
            let mo = list("monster")?;
            if mo.len() == 2 {
                m.pos = v3(mo[0], 0, mo[1]);
            }
            if let Some(d) = s.get("behind").map(num).transpose()? {
                m.pos = player.sub(v3(fwd.x, 0, fwd.z).norm().scale(d));
            }
            m.pos.y = self.terrain.height(m.pos.x, m.pos.z);
            let to = player.sub(m.pos);
            m.yaw = fx::atan2(-to.x, -to.z);
            if let Some(j) = s.get("jump_in").map(num).transpose()? {
                m.jump_in = fx::mul(j, 60).max(1);
            }
        }
        for path in s.get("ravage").and_then(|v| v.as_array()).into_iter().flatten() {
            let p: Vec<i32> = path
                .as_array()
                .ok_or("stage: a ravage path is [x0, z0, x1, z1]")?
                .iter()
                .map(num)
                .collect::<Result<_, _>>()?;
            let (a, b) = (v3(p[0], 0, p[1]), v3(p[2], 0, p[3]));
            let along = b.sub(a);
            let fwd = along.norm();
            let steps = along.len() / (2 * ONE);
            for k in 0..=steps {
                let at = a.add(fwd.scale(2 * ONE * k));
                let at = v3(at.x, self.terrain.height(at.x, at.z), at.z);
                self.city.sweep(
                    at,
                    fwd,
                    self.tuning.width / 2,
                    self.tuning.height,
                    -ONE,
                    int(4),
                    int(25),
                );
            }
        }
        let settle = s.get("settle").map(num).transpose()?.unwrap_or(0);
        for _ in 0..fx::mul(settle, 60) {
            self.city.step(&self.terrain, DT, player);
        }
        self.city.bursts.clear();
        self.clock = None;
        if let Some(seed) = s.get("seed").and_then(|v| v.as_u64()) {
            self.luck = Rng::new(seed);
            self.rng = Rng::new(seed);
        }
        self.frame();
        Ok(())
    }

    /// For filming: where things are, for the film's driver to steer by:
    /// with `ahead`, how far is clear (m, up to 60) along rays from the car's
    /// nose, 40° left to 40° right.
    pub fn probe(&mut self) -> String {
        let mut scratch = Vec::new();
        let c = &self.car;
        let nose = c.body.pos.add(c.fwd().scale(int(3)));
        let flat = v3(c.fwd().x, 0, c.fwd().z).norm();
        let ahead: Vec<i32> = [-40, -25, -12, 0, 12, 25, 40]
            .iter()
            .map(|&a| {
                let d = Quat::yaw(-deg(a)).rotate(flat);
                self.city
                    .ray(nose, d, int(60), &mut scratch)
                    .map(|h| h.0 / ONE)
                    .unwrap_or(60)
            })
            .collect();
        // where the pursuer's face shows in the rear-view mirror, in
        // thousandths of the screen from its top left
        let mirror_face = self.monster.as_ref().zip(self.views.get(1)).and_then(|(m, v)| {
            let face = m.pos.add(V3::UP.scale(m.tune.height * 2 / 3));
            let aspect = fx::div(v.rect[2].max(1), v.rect[3].max(1));
            let (x, y) = v.cam.screen(aspect, face)?;
            // the mirror shows its picture flipped left to right
            let x = if v.mirror { int(100) - x } else { x };
            let (w, h) = (self.width.max(1) as i64, self.height.max(1) as i64);
            let sx = (v.rect[0] as i64 + x as i64 * v.rect[2] as i64 / int(100) as i64) * 1000 / w;
            let top = h - v.rect[1] as i64 - v.rect[3] as i64;
            let sy = (top + y as i64 * v.rect[3] as i64 / int(100) as i64) * 1000 / h;
            Some([sx, sy])
        });
        let c = &self.car;
        let f = c.fwd();
        let deg_of = |a: i32| a as i64 * 360 / fx::TURN as i64;
        let m = self.monster.as_ref();
        let target = self.player_pos();
        json!({
            "mode": format!("{:?}", self.mode).to_lowercase(),
            "driving": self.driving,
            "ticks": self.ticks,
            "car": [c.body.pos.x / ONE, c.body.pos.y / ONE, c.body.pos.z / ONE],
            "heading": deg_of(fx::atan2(-f.x, -f.z)),
            "speed": c.body.vel.len_xz() / ONE,
            "up": c.up().y * 100 / ONE,
            "grounded": c.grounded,
            "walker": [self.walker.pos.x / ONE, self.walker.pos.z / ONE, deg_of(self.walker.yaw)],
            "aim": self.aim.map(|k| self.items[k].id.clone()),
            "held": self.held.map(|k| self.items[k].id.clone()),
            "monster": m.map(|m| [m.pos.x / ONE, m.pos.z / ONE]),
            "monster_distance": m.map(|m| m.distance(target) / ONE),
            "monster_ahead": m.map(|m| m.pos.sub(target).dot(f) / ONE),
            "jump_in": m.map(|m| m.jump_in),
            "awake": self.city.awake_count(),
            "intact": self.city.intact_count(),
            "ahead": ahead,
            "mirror_face": mirror_face,
        })
        .to_string()
    }

    pub fn saved(&self) -> String {
        json!({ "best": self.best, "song": self.tracks[self.song].id }).to_string()
    }

    /// The page's clock (microseconds), the held action bits, and the mouse
    /// movement (CSS pixels × 100).
    pub fn advance(&mut self, now_us: i64, bits: u32, dx: i32, dy: i32) {
        self.sounds.clear();
        let elapsed = match self.clock {
            Some(c) => (now_us - c).clamp(0, TICK_US * MAX_TICKS_PER_FRAME as i64),
            None => TICK_US,
        };
        self.clock = Some(now_us);
        let live = !self.paused && matches!(self.mode, Mode::House | Mode::Chase) && bit(bits, FOCUS);
        if live {
            self.mouse(dx, dy);
        }
        if self.mode == Mode::Title {
            self.title_angle = (self.title_angle + 4) & (fx::TURN - 1);
        }
        self.acc += elapsed;
        let mut n = 0;
        while self.acc >= TICK_US && n < MAX_TICKS_PER_FRAME {
            self.acc -= TICK_US;
            n += 1;
            if live {
                self.tick(bits | self.pending_bits);
                self.pending_bits = 0;
            } else if !self.paused {
                // the rubble settles behind the title and the verdict
                let focus = self.player_pos();
                self.city.step(&self.terrain, DT, focus);
            }
        }
        if n == MAX_TICKS_PER_FRAME {
            self.acc = 0;
        }
        // the held keys are remembered by the ticks alone: a display frame
        // that runs no tick must not swallow a key's press
        if !live {
            self.held_bits = 0;
            self.pending_bits = 0;
        } else if n == 0 {
            self.pending_bits |= bits;
        }
        self.frame();
    }

    fn mouse(&mut self, dx: i32, dy: i32) {
        let (yaw, pitch) = (-dx * LOOK / 100, -dy * LOOK / 100);
        if self.driving {
            self.look_yaw = (self.look_yaw + yaw).clamp(-deg(150), deg(150));
            self.look_pitch = (self.look_pitch + pitch).clamp(-deg(60), deg(60));
            if dx != 0 || dy != 0 {
                self.look_idle = 0;
            }
        } else {
            self.walker.yaw = (self.walker.yaw + yaw) & (fx::TURN - 1);
            self.walker.pitch = (self.walker.pitch + pitch).clamp(-deg(85), deg(85));
        }
    }

    fn player_pos(&self) -> V3 {
        if self.driving {
            self.car.body.pos
        } else {
            self.walker.pos
        }
    }

    fn legs(bits: u32) -> Legs {
        let axis = |a: u32, b: u32| (bit(bits, a) as i32 - bit(bits, b) as i32) * ONE;
        Legs {
            forward: axis(FORWARD, BACK),
            left: axis(LEFT, RIGHT),
            run: bit(bits, SHIFT),
            jump: bit(bits, JUMP),
        }
    }

    fn tick(&mut self, bits: u32) {
        let pressed = |b: u32| bit(bits, b) && !bit(self.held_bits, b);
        let solids: Vec<house::Solid> = self.house.blocking(self.fold.max(0)).copied().collect();
        if !self.driving {
            let step = self.walker.step(
                Self::legs(bits),
                &solids,
                Some(&self.car.body),
                &mut self.city,
                &self.terrain,
                DT,
            );
            if step {
                self.sounds.push(if self.walker.left_foot {
                    "squeak_l"
                } else {
                    "squeak_r"
                });
            }
            self.car
                .step(Controls::default(), &self.terrain, &mut self.city, DT);
        }
        match self.mode {
            Mode::House => {
                self.tick_house(bits);
                let focus = self.player_pos();
                self.city.step(&self.terrain, DT, focus);
            }
            Mode::Chase => self.tick_chase(bits, pressed(USE)),
            _ => {}
        }
        self.held_bits = bits;
    }

    /// What the walker is looking at: the nearest item, or the record
    /// player, whose middle is within its radius of the line of sight and
    /// within reach. `Some(None)` is the record player.
    fn looked_at(&self) -> Option<Option<usize>> {
        let eye = self.walker.eye();
        let look = self.walker.look();
        let things = self
            .items
            .iter()
            .enumerate()
            .map(|(k, it)| (Some(k), it.centre, it.radius))
            .chain([(None, house::TURNTABLE, house::TURNTABLE_RADIUS)]);
        let mut best: Option<(Option<usize>, i32)> = None;
        for (k, centre, radius) in things {
            let rel = centre.sub(eye);
            let along = rel.dot(look);
            if along <= 0 || along > REACH {
                continue;
            }
            let off = rel.sub(look.scale(along)).len();
            if off <= radius + ONE / 10 && best.map(|b| along < b.1).unwrap_or(true) {
                best = Some((k, along));
            }
        }
        best.map(|b| b.0)
    }

    /// The next record the build holds.
    fn next_song(&mut self) {
        let n = self.tracks.len();
        for step in 1..=n {
            let k = (self.song + step) % n;
            if self.tracks[k].present {
                self.song = k;
                break;
            }
        }
        self.song_seq += 1;
    }

    fn tick_house(&mut self, bits: u32) {
        let looked = self.looked_at();
        self.aim = looked.flatten();
        self.aim_player = looked == Some(None);
        match (self.held, bit(bits, USE)) {
            (None, true) if !bit(self.held_bits, USE) => {
                if let Some(k) = self.aim {
                    self.held = Some(k);
                    self.hold = 0;
                    self.sounds.push("pickup");
                } else if self.aim_player {
                    self.next_song();
                    self.sounds.push("needle");
                }
            }
            (Some(_), true) => {
                self.hold += 1;
                if self.hold >= self.hold_ticks {
                    self.unfold();
                }
            }
            (Some(_), false) => {
                self.held = None;
                self.hold = 0;
                self.sounds.push("putdown");
            }
            _ => {}
        }
    }

    /// The house folds and the pursuer stands up.
    fn unfold(&mut self) {
        self.chosen = self.held;
        self.held = None;
        self.hold = 0;
        self.fold = 0;
        self.mode = Mode::Chase;
        // the draws differ from chase to chase: the page's clock is the seed
        self.luck = Rng::new(self.clock.unwrap_or(0) as u64);
        let at = v3(-self.tuning.spawn_distance, 0, 0);
        let mut m = Monster::new(at, 0, self.tuning);
        m.pos.y = self.terrain.height(at.x, at.z);
        self.monster = Some(m);
    }

    fn tick_chase(&mut self, bits: u32, use_pressed: bool) {
        self.fold += 1;
        self.ticks += 1;
        if self.driving {
            let axis = |a: u32, b: u32| (bit(bits, a) as i32 - bit(bits, b) as i32) * ONE;
            let c = Controls {
                throttle: axis(FORWARD, BACK),
                steer: axis(LEFT, RIGHT),
                handbrake: bit(bits, SHIFT),
                fly: bit(bits, FLY),
                flip: bit(bits, FLIP),
                big: bit(bits, BIG),
                dash: bit(bits, DASH),
                spin: bit(bits, SPIN),
                nitrous: bit(bits, NITROUS),
                leap: bit(bits, JUMP),
            };
            let quiet = |car: &Car| [car.dash, car.spin_round, car.leap, car.nitrous].map(|ch| ch.left == 0);
            let before = quiet(&self.car);
            self.car.step(c, &self.terrain, &mut self.city, DT);
            let now = quiet(&self.car);
            if (0..3).any(|k| before[k] && !now[k]) {
                self.sounds.push("whoosh");
            }
            if before[3] && !now[3] {
                self.sounds.push("nitrous");
            }
            self.look_idle += 1;
            if self.look_idle > 90 {
                self.look_yaw = fx::approach(self.look_yaw, 0, deg(3));
                self.look_pitch = fx::approach(self.look_pitch, 0, deg(2));
            }
        } else if use_pressed && self.walker.pos.dist(self.car.body.pos) < CAR_REACH {
            self.driving = true;
            self.look_yaw = 0;
            self.look_pitch = 0;
            self.toast = Some((None, 60 * 5));
        }
        let target = self.player_pos();
        let heading = if self.driving {
            let v = self.car.body.vel;
            if v.len_xz() > int(4) {
                v
            } else {
                self.car.fwd()
            }
        } else {
            facing(self.walker.yaw, 0)
        };
        if let Some(m) = self.monster.as_mut() {
            if m.step(target, heading, &self.terrain, &mut self.city, DT) {
                self.sounds.push("scare");
                self.scare = SCARE_TICKS;
            }
        }
        self.scare = (self.scare - 1).max(0);
        self.city.step(&self.terrain, DT, target);
        for b in self.city.bursts.drain(..) {
            for _ in 0..2 {
                let r = &mut self.rng;
                self.dust.push(Dust {
                    pos: b.at,
                    vel: v3(
                        r.range(-int(4), int(4)),
                        r.range(int(1), int(5)),
                        r.range(-int(4), int(4)),
                    ),
                    size: b.size.clamp(ONE, int(5)) + r.range(0, int(2)),
                    life: 90,
                });
            }
        }
        for d in &mut self.dust {
            d.pos = d.pos.add(d.vel.scale(DT));
            d.vel = d.vel.scale(ONE - ONE / 40);
            d.life -= 1;
        }
        self.dust.retain(|d| d.life > 0);
        if self.dust.len() > 160 {
            let extra = self.dust.len() - 160;
            self.dust.drain(..extra);
        }
        // abilities, drawn at random as time is survived
        while self.unlocked < self.upgrades.at.len()
            && self.ticks as i32 >= self.upgrades.at[self.unlocked] * 60
        {
            self.unlocked += 1;
            let Some(k) = draw(&self.upgrades.pool, &self.drawn, &mut self.luck) else {
                continue;
            };
            self.drawn.push(k);
            let kit = &mut self.car.kit;
            match self.upgrades.pool[k].id.as_str() {
                "big_wheels" => kit.big_wheels = true,
                "wings" => kit.wings = true,
                "turbo" => kit.turbo += 1,
                "dash" => kit.dash = true,
                "spin" => kit.spin = true,
                "nitrous" => kit.nitrous = true,
                "leap" => kit.leap = true,
                _ => {}
            }
            self.toast = Some((Some(k), 60 * 5));
        }
        if let Some((k, t)) = self.toast {
            self.toast = if t > 1 { Some((k, t - 1)) } else { None };
        }
        let caught = self.monster.as_ref().map(|m| m.caught(target)).unwrap_or(false);
        if caught {
            self.mode = Mode::Caught;
            if self.ticks > self.best {
                self.best = self.ticks;
                self.new_best = true;
            }
        }
        self.shake = self
            .monster
            .as_ref()
            .map(|m| ONE - fx::div(m.distance(target).min(int(90)), int(90)))
            .unwrap_or(0)
            .max(fx::div(self.scare, SCARE_TICKS));
    }

    // ---- the frame ----

    fn camera(&mut self) -> Camera {
        match self.mode {
            Mode::Title => {
                let a = self.title_angle;
                let eye = v3(
                    fx::mul(fx::sin(a), int(420)),
                    int(150),
                    fx::mul(fx::cos(a), int(420)),
                );
                Camera {
                    eye,
                    fwd: v3(0, int(20), 0).sub(eye).norm(),
                    up: V3::UP,
                    fov: deg(60),
                }
            }
            Mode::Caught => {
                let eye = self.player_pos().add(V3::UP.scale(int(2)));
                let at = self
                    .monster
                    .as_ref()
                    .map(|m| m.pos.add(V3::UP.scale(m.tune.height * 2 / 3)))
                    .unwrap_or(V3::ZERO);
                Camera {
                    eye,
                    fwd: at.sub(eye).norm(),
                    up: V3::UP,
                    fov: deg(80),
                }
            }
            _ if self.driving => {
                let b = &self.car.body;
                let eye = b.pos.add(from_local(
                    &b.basis,
                    v3(-ONE * 40 / 100, SEAT_EYE, ONE * 30 / 100),
                ));
                let fwd = from_local(&b.basis, facing(self.look_yaw, self.look_pitch - deg(7)));
                let up = b.basis[1].add(V3::UP).norm();
                let jolt = self.jitter(ONE / 30);
                Camera {
                    eye: eye.add(jolt),
                    fwd,
                    up,
                    fov: deg(78),
                }
            }
            _ => {
                let dread = if self.held.is_some() {
                    fx::div(self.hold, self.hold_ticks.max(1))
                } else {
                    0
                };
                let wobble = fx::mul(dread, fx::mul(dread, deg(2)));
                let (wy, wp) = (self.rng.range(-wobble, wobble), self.rng.range(-wobble, wobble));
                Camera {
                    eye: self.walker.eye().add(self.jitter(ONE / 12)),
                    fwd: facing(self.walker.yaw + wy, self.walker.pitch + wp),
                    up: V3::UP,
                    fov: deg(74),
                }
            }
        }
    }

    fn jitter(&mut self, most: i32) -> V3 {
        let s = fx::mul(self.shake, most);
        if s <= 0 || self.paused {
            return V3::ZERO;
        }
        v3(
            self.rng.range(-s, s),
            self.rng.range(-s, s),
            self.rng.range(-s, s),
        )
    }

    /// Where the rear-view mirror looks, in the car's frame: back and a
    /// little down, tilted up toward the pursuer's face when it is close
    /// enough to fill the mirror with its legs.
    fn rear_aim(&self) -> V3 {
        let level = v3(0, -ONE / 18, ONE);
        let Some(m) = self.monster.as_ref().filter(|_| self.driving) else {
            return level;
        };
        let b = &self.car.body;
        let eye = b.pos.add(from_local(&b.basis, REAR_EYE));
        let face = m.pos.add(V3::UP.scale(m.tune.height * 2 / 3));
        let to = crate::quat::to_local(&b.basis, face.sub(eye));
        if to.z <= 0 {
            return level;
        }
        let tilt = fx::atan2(to.y, to.z);
        let tilt = if tilt > fx::HALF { tilt - fx::TURN } else { tilt };
        // the face near the top of the mirror's picture, not in its middle
        let tilt = (tilt - deg(9)).clamp(REAR_TILT_LEAST, REAR_TILT_MOST);
        v3(0, fx::sin(tilt), fx::cos(tilt))
    }

    fn mirror_cam(&self, at: V3, back: V3, fov: i32) -> Camera {
        let b = &self.car.body;
        Camera {
            eye: b.pos.add(from_local(&b.basis, at)),
            fwd: from_local(&b.basis, back.norm()),
            up: b.basis[1],
            fov,
        }
    }

    /// The views on screen: the windscreen, and the three mirrors in the car.
    fn layout(&mut self) -> Vec<View> {
        let (w, h) = (self.width, self.height);
        let mut main_cam = self.camera();
        main_cam.fov = render::tall_fov(main_cam.fov, w, h);
        let in_car = self.driving && self.mode == Mode::Chase;
        let mut views = vec![View {
            rect: [0, 0, w, h],
            mirror: false,
            cam: main_cam,
            far: render::FAR,
            ink: true,
            layers: [true, !in_car, in_car || self.held.is_some()],
        }];
        if in_car {
            let world_car = [true, true, false];
            // held upright, a phone's mirrors are wider and shallower
            let tall = h > w;
            let rear = if tall {
                (w * 64 / 100, h * 11 / 100)
            } else {
                (w * 34 / 100, h * 12 / 100)
            };
            views.push(View {
                rect: [(w - rear.0) / 2, h - h * 25 / 1000 - rear.1, rear.0, rear.1],
                mirror: true,
                cam: self.mirror_cam(REAR_EYE, self.rear_aim(), deg(24)),
                far: int(900),
                ink: false,
                layers: [true, false, false],
            });
            let (side, top) = if tall {
                let side = (w * 27 / 100, h * 8 / 100);
                (side, h - h * 30 / 100 - side.1)
            } else {
                let side = (w * 17 / 100, h * 13 / 100);
                (side, h - h * 58 / 100 - side.1)
            };
            views.push(View {
                rect: [w * 2 / 100, top, side.0, side.1],
                mirror: true,
                cam: self.mirror_cam(
                    v3(-ONE * 105 / 100, ONE * 55 / 100, -ONE * 7 / 10),
                    v3(-ONE / 6, -ONE / 30, ONE),
                    deg(30),
                ),
                far: int(700),
                ink: false,
                layers: world_car,
            });
            views.push(View {
                rect: [w - w * 2 / 100 - side.0, top, side.0, side.1],
                mirror: true,
                cam: self.mirror_cam(
                    v3(ONE * 105 / 100, ONE * 55 / 100, -ONE * 7 / 10),
                    v3(ONE / 6, -ONE / 30, ONE),
                    deg(30),
                ),
                far: int(700),
                ink: false,
                layers: world_car,
            });
        }
        if self.low {
            views.truncate(2);
            for v in &mut views {
                v.ink = false;
                v.far = v.far.min(LOW_FAR);
            }
        }
        views
    }

    fn car_shapes(&self, b: &mut Batch) {
        let c = &self.car;
        let body = &c.body;
        let place = Affine {
            c0: body.basis[0],
            c1: body.basis[1],
            c2: body.basis[2],
            t: body.pos,
        };
        let paint = self.pal.get("car_body");
        let trim = self.pal.get("car_trim");
        let glass = self.pal.get("car_glass");
        let part = |layer: usize, mesh: Mesh, at: V3, size: V3, rgb: Rgb, b: &mut Batch| {
            b.push(layer, mesh, &place.then(&render::boxed(at, size)), rgb, 0);
        };
        let o = |x: i32, y: i32, z: i32| v3(x * ONE / 100, y * ONE / 100, z * ONE / 100);
        // outside
        part(LAYER_CAR, Mesh::Cube, o(0, -5, 0), o(190, 70, 440), paint, b);
        part(LAYER_CAR, Mesh::Cube, o(0, 66, 25), o(170, 72, 230), glass, b);
        part(LAYER_CAR, Mesh::Cube, o(0, 105, 25), o(176, 8, 210), paint, b);
        part(LAYER_CAR, Mesh::Cube, o(0, -28, -224), o(196, 26, 14), trim, b);
        part(LAYER_CAR, Mesh::Cube, o(0, -28, 224), o(196, 26, 14), trim, b);
        part(
            LAYER_CAR,
            Mesh::Cube,
            o(-60, 6, -221),
            o(40, 14, 4),
            self.pal.get("headlight"),
            b,
        );
        part(
            LAYER_CAR,
            Mesh::Cube,
            o(60, 6, -221),
            o(40, 14, 4),
            self.pal.get("headlight"),
            b,
        );
        part(LAYER_CAR, Mesh::Cube, o(0, 12, 0), o(194, 8, 300), trim, b);
        // wheels: a cylinder on its side, turned by the steering, spun by the road
        let tune = c.tune();
        for (k, m) in tune.mounts().iter().enumerate() {
            let centre = m.sub(v3(0, c.travel[k], 0));
            if c.travel[k] > ONE {
                // the big wheels' stilts
                let strut = Affine::translate(m.add(centre).scale(ONE / 2)).then(&Affine::scale(v3(
                    ONE / 5,
                    c.travel[k],
                    ONE / 5,
                )));
                b.push(LAYER_CAR, Mesh::Cyl, &place.then(&strut), trim, 0);
            }
            let steer = if k < 2 { c.steer } else { 0 };
            let d = tune.radius * 2;
            let width = tune.radius * 4 / 10 + ONE / 6;
            let wheel = Affine::translate(centre)
                .then(&Affine::rot_y(steer))
                .then(&Affine::rot_x(-c.spin))
                .then(&Affine::rot_z(fx::QUARTER));
            b.push(
                LAYER_CAR,
                Mesh::Cyl,
                &place.then(&wheel).then(&Affine::scale(v3(d, width, d))),
                self.pal.get("tire"),
                0,
            );
            b.push(
                LAYER_CAR,
                Mesh::Cyl,
                &place
                    .then(&wheel)
                    .then(&Affine::scale(v3(d / 2, width + ONE / 50, d / 2))),
                self.pal.get("hub"),
                0,
            );
        }
        if !self.driving {
            return;
        }
        // the cockpit, from the driver's seat
        let dash = self.pal.get("dash");
        part(
            LAYER_COCKPIT,
            Mesh::Cube,
            o(0, 26, -160),
            o(186, 14, 130),
            paint,
            b,
        );
        part(LAYER_COCKPIT, Mesh::Cube, o(0, 38, -78), o(172, 24, 38), dash, b);
        part(LAYER_COCKPIT, Mesh::Cube, o(0, 104, -36), o(176, 8, 14), paint, b);
        part(LAYER_COCKPIT, Mesh::Cube, o(-93, 44, 20), o(8, 10, 200), paint, b);
        part(LAYER_COCKPIT, Mesh::Cube, o(93, 44, 20), o(8, 10, 200), paint, b);

        for side in [-1, 1] {
            let pillar = Affine::translate(o(side * 85, 74, -62))
                .then(&Affine::rot_x(deg(40)))
                .then(&Affine::scale(o(7, 72, 7)));
            b.push(LAYER_COCKPIT, Mesh::Cube, &place.then(&pillar), paint, 0);
            let housing = Affine::translate(o(side * 106, 55, -72)).then(&Affine::scale(o(10, 16, 26)));
            b.push(LAYER_COCKPIT, Mesh::Cube, &place.then(&housing), dash, 0);
        }
        let wheel = Affine::translate(o(-40, 52, -50))
            .then(&Affine::rot_x(deg(62)))
            .then(&Affine::rot_y(-c.steer * 3))
            .then(&Affine::scale(o(36, 4, 36)));
        b.push(LAYER_COCKPIT, Mesh::Ring, &place.then(&wheel), dash, 0);
        let column = Affine::translate(o(-40, 46, -62))
            .then(&Affine::rot_x(deg(62)))
            .then(&Affine::scale(o(5, 20, 5)));
        b.push(LAYER_COCKPIT, Mesh::Cyl, &place.then(&column), dash, 0);
    }

    fn held_shapes(&self, cam: &Camera, b: &mut Batch) {
        let Some(k) = self.held else { return };
        let it = &self.items[k];
        let right = cam.fwd.cross(cam.up).norm();
        let up = right.cross(cam.fwd);
        let tilt = fx::div(self.hold, self.hold_ticks.max(1));
        let spin = fx::mul(tilt, deg(200)) + deg(160);
        let size = it.radius * 2;
        let scale = fx::div(ONE * 32 / 100, size.max(ONE / 4)).min(ONE);
        let at = cam
            .eye
            .add(cam.fwd.scale(ONE * 70 / 100))
            .add(right.scale(ONE * 18 / 100))
            .sub(up.scale(ONE * 26 / 100))
            .sub(it.centre.sub(it.at).scale(scale));
        let place = Affine {
            c0: right.scale(scale),
            c1: up.scale(scale),
            c2: cam.fwd.neg().scale(scale),
            t: at,
        }
        .then(&Affine::rot_y(spin));
        let mut shapes = Vec::new();
        it.shapes(&place, &mut shapes);
        for s in shapes {
            b.push(LAYER_COCKPIT, s.mesh, &s.at, s.rgb, 0);
        }
        if let Some(a) = it.cover_at(&place) {
            b.sprite(&a, self.cover_texture(k));
        }
    }

    fn monster_shapes(&self, b: &mut Batch) {
        let Some(m) = &self.monster else { return };
        let fwd = m.fwd();
        let right = fwd.neg().cross(V3::UP).norm();
        let lean = fx::mul(ONE - m.rise(), fx::QUARTER);
        let (s, c) = (fx::sin(lean), fx::cos(lean));
        let up = V3::UP.scale(c).sub(fwd.scale(s));
        let normal = fwd.scale(c).add(V3::UP.scale(s));
        let (wd, ht) = (m.tune.width, m.tune.height);
        let card = Affine {
            c0: right.scale(wd),
            c1: up.scale(ht),
            c2: normal,
            t: m.pos.add(up.scale(ht / 2)).add(V3::UP.scale(ONE / 2)),
        };
        b.sprite(&card, self.chosen.unwrap_or(0) as u32);
        // the cardboard strut behind, once it stands: its tall end a metre
        // behind the card, which in the card's own plane showed through it
        if m.rise() > ONE / 2 {
            let strut = Affine::translate(m.pos.sub(fwd.scale(int(9))).add(V3::UP.scale(ht / 4)))
                .then(&Affine {
                    c0: right,
                    c1: V3::UP,
                    c2: fwd,
                    t: V3::ZERO,
                })
                .then(&Affine::scale(v3(int(2), ht / 2, int(16))));
            b.push(LAYER_WORLD, Mesh::Wedge, &strut, self.pal.get("cardboard"), 0);
        }
    }

    fn build_dynamic(&mut self, main: &Camera) -> Batch {
        let mut b = Batch::new();
        let mut shapes: Vec<Shape> = Vec::new();
        self.house.shapes(self.fold.max(0), &mut shapes);
        for (k, it) in self.items.iter().enumerate() {
            if Some(k) == self.held || (Some(k) == self.chosen && self.fold >= 0) {
                continue;
            }
            it.shapes(&it.rest(), &mut shapes);
            if let Some(a) = it.cover_at(&it.rest()) {
                b.sprite(&a, self.cover_texture(k));
            }
        }
        for s in shapes {
            b.push(LAYER_WORLD, s.mesh, &s.at, s.rgb, 0);
        }
        // the item looked at glows a little
        if let (Some(k), None, Mode::House) = (self.aim, self.held, self.mode) {
            let it = &self.items[k];
            let ring = render::boxed(
                it.at.add(V3::UP.scale(ONE / 50)),
                v3(it.radius * 3, ONE / 50, it.radius * 3),
            );
            b.push(
                LAYER_WORLD,
                Mesh::Ring,
                &ring,
                self.pal.get("window_lit"),
                crate::city::STYLE_GLOW * ONE,
            );
        }
        if self.aim_player && self.held.is_none() && self.mode == Mode::House {
            let ring = render::boxed(
                house::TURNTABLE.add(V3::UP.scale(ONE / 50)),
                v3(ONE * 6 / 10, ONE / 50, ONE * 6 / 10),
            );
            b.push(
                LAYER_WORLD,
                Mesh::Ring,
                &ring,
                self.pal.get("window_lit"),
                crate::city::STYLE_GLOW * ONE,
            );
        }
        self.car_shapes(&mut b);
        self.held_shapes(main, &mut b);
        self.monster_shapes(&mut b);
        let dust = self.pal.get("dust");
        for d in &self.dust {
            let s = fx::mul(d.size, fx::div(d.life, 90)) + ONE / 4;
            b.push(
                LAYER_WORLD,
                Mesh::Blob,
                &render::boxed(d.pos, v3(s, s, s)),
                dust,
                0,
            );
        }
        b
    }

    fn frame(&mut self) {
        let views = self.layout();
        let batch = self.build_dynamic(&views[0].cam);
        let (values, runs, sprites) = batch.finish();
        self.dynamic = values;
        self.rects.clear();
        self.mats.clear();
        self.draws.clear();
        for (vi, v) in views.iter().enumerate() {
            let vi = vi as u32;
            self.rects
                .extend_from_slice(&[v.rect[0], v.rect[1], v.rect[2], v.rect[3], v.mirror as i32]);
            self.mats.extend_from_slice(&v.mat_values());
            let pieces = render::piece_runs(&self.city.groups, v);
            let mut push = |d: [u32; DRAW_VALUES]| self.draws.extend_from_slice(&d);
            for &(off, n) in &pieces {
                push([vi, BUF_PIECES, Mesh::Cube as u32, off, n, PASS_SOLID, 0]);
            }
            for &(mesh, off, n) in &self.scenery.solid {
                if !(self.low && (mesh == render::MESH_OUTSKIRTS || mesh == Mesh::Blob as u32)) {
                    push([vi, BUF_SCENERY, mesh, off, n, PASS_SOLID, 0]);
                }
            }
            for &(layer, mesh, off, n) in &runs {
                if v.layers[layer] {
                    push([vi, BUF_DYNAMIC, mesh, off, n, PASS_SOLID, 0]);
                }
            }
            for &(off, n, tex) in &sprites {
                push([vi, BUF_DYNAMIC, Mesh::Card as u32, off, n, PASS_SPRITE, tex]);
            }
            if v.ink {
                for &(off, n) in &pieces {
                    push([vi, BUF_PIECES, Mesh::Cube as u32, off, n, PASS_INK, 0]);
                }
                for &(mesh, off, n) in &self.scenery.ink {
                    push([vi, BUF_SCENERY, mesh, off, n, PASS_INK, 0]);
                }
                for &(layer, mesh, off, n) in &runs {
                    if v.layers[layer] && layer != LAYER_COCKPIT {
                        push([vi, BUF_DYNAMIC, mesh, off, n, PASS_INK, 0]);
                    }
                }
            }
        }
        self.views = views;
    }

    fn build_scenery(&self) -> Scenery {
        let mut lists: Vec<(u32, Vec<i32>, bool)> = Vec::new();
        let ident = Affine::IDENTITY;
        for (mesh, name) in [
            (render::MESH_GRASS, "grass"),
            (render::MESH_ROAD, "road"),
            (render::MESH_WALK, "walk"),
            (render::MESH_OUTSKIRTS, "outskirts"),
        ] {
            lists.push((
                mesh,
                render::instance(&ident, self.pal.get(name), 0).to_vec(),
                false,
            ));
        }
        // centre-line dashes, laid along the slope
        let mut dashes = Vec::new();
        let lane = self.pal.get("lane");
        for mk in &self.blueprint.marks {
            let (along, len) = if mk.along_x {
                (v3(ONE, 0, 0), int(3))
            } else {
                (v3(0, 0, ONE), int(3))
            };
            let a = mk.pos.sub(along.scale(len / 2));
            let b = mk.pos.add(along.scale(len / 2));
            let (ha, hb) = (self.terrain.height(a.x, a.z), self.terrain.height(b.x, b.z));
            let dir = v3(along.x * 3, fx::div(hb - ha, ONE), along.z * 3).norm();
            let side = dir.cross(V3::UP).norm();
            let up = side.cross(dir);
            let at = Affine {
                c0: side.scale(ONE * 25 / 100),
                c1: up.scale(ONE / 10),
                c2: dir.neg().scale(len),
                t: mk.pos.add(V3::UP.scale(ONE / 25)),
            };
            dashes.extend_from_slice(&render::instance(&at, lane, 0));
        }
        lists.push((Mesh::Cube as u32, dashes, false));
        // clouds, outlined as Sam_Comm_final's are
        let mut clouds = Vec::new();
        let mut r = Rng::new(31);
        let cloud = self.pal.get("window_lit");
        for k in 0..16 {
            let a = fx::TURN * k / 16 + r.range(0, fx::TURN / 32);
            let d = int(r.range(1100, 1500));
            let c = v3(
                fx::mul(fx::sin(a), d),
                int(r.range(240, 420)),
                fx::mul(fx::cos(a), d),
            );
            for _ in 0..4 {
                let s = int(r.range(70, 140));
                let off = v3(
                    int(r.range(-110, 110)),
                    int(r.range(-15, 25)),
                    int(r.range(-60, 60)),
                );
                let at = render::boxed(c.add(off), v3(s * 2, s * 3 / 4, s));
                clouds.extend_from_slice(&render::instance(&at, cloud, 0));
            }
        }
        lists.push((Mesh::Blob as u32, clouds, true));
        let mut s = Scenery::default();
        for (mesh, v, inked) in lists {
            if v.is_empty() {
                continue;
            }
            let off = (s.values.len() * render::BYTES_PER_VALUE) as u32;
            let n = (v.len() / INSTANCE_VALUES) as u32;
            s.solid.push((mesh, off, n));
            if inked {
                s.ink.push((mesh, off, n));
            }
            s.values.extend_from_slice(&v);
        }
        s
    }

    // ---- read by the page ----

    pub fn pieces_values(&self) -> &[i32] {
        self.city.values()
    }

    pub fn pieces_version(&self) -> u32 {
        self.pieces_version
    }

    /// The runs of piece slots to upload since the last call.
    pub fn take_dirty(&mut self) -> Vec<u32> {
        self.dirty = self.city.take_dirty();
        self.dirty.clone()
    }

    pub fn scenery(&self) -> Vec<i32> {
        self.scenery.values.clone()
    }

    pub fn dynamic(&self) -> Vec<i32> {
        self.dynamic.clone()
    }

    pub fn draws(&self) -> Vec<u32> {
        self.draws.clone()
    }

    pub fn rects(&self) -> Vec<i32> {
        self.rects.clone()
    }

    pub fn mats(&self) -> Vec<i32> {
        self.mats.clone()
    }

    pub fn mesh(&self, id: u32) -> Vec<i32> {
        let kinds = &self.blueprint.kinds;
        match id {
            render::MESH_GRASS => self.terrain.cells_mesh(kinds, KIND_GRASS, 0),
            render::MESH_ROAD => self.terrain.cells_mesh(kinds, KIND_ROAD, 0),
            render::MESH_WALK => self.terrain.cells_mesh(kinds, KIND_WALK, ONE / 25),
            render::MESH_OUTSKIRTS => self.terrain.outskirts_mesh(2400, 60),
            _ => Mesh::ALL
                .iter()
                .find(|m| **m as u32 == id)
                .map(|m| mesh::vertices(*m))
                .unwrap_or_default(),
        }
    }

    pub fn uniforms(&self) -> Vec<i32> {
        let p = &self.pal;
        let light = v3(ONE * 4 / 10, ONE * 8 / 10, ONE * 3 / 10).norm();
        let mut u = vec![light.x, light.y, light.z];
        for name in ["sky", "fog", "light", "window_dark", "window_lit", "ink"] {
            u.extend_from_slice(&p.get(name));
        }
        // fog from and to, metres; the ink line's least width, growth a metre, most
        u.extend_from_slice(&[int(260), int(1500), ONE * 3 / 100, ONE / 160, ONE * 22 / 100]);
        u
    }

    pub fn clear_color(&self) -> Vec<i32> {
        self.pal.get("sky").to_vec()
    }

    /// The texture of item `k`'s cover: the page loads every item's pursuer
    /// first, in order, and then each cover, in the items' order.
    fn cover_texture(&self, k: usize) -> u32 {
        (self.items.len() + self.items[..k].iter().filter(|i| i.cover.is_some()).count()) as u32
    }

    pub fn items_json(&self) -> String {
        let list: Vec<serde_json::Value> = self
            .items
            .iter()
            .map(|i| json!({ "id": i.id, "monster": i.monster, "name": i.name, "cover": i.cover }))
            .collect();
        serde_json::Value::Array(list).to_string()
    }

    pub fn hud_json(&self) -> String {
        let mode = match self.mode {
            Mode::Title => "title",
            Mode::House => "house",
            Mode::Chase => "chase",
            Mode::Caught => "caught",
        };
        let mode = if self.paused { "paused" } else { mode };
        let target = self.player_pos();
        let item = |k: Option<usize>| k.map(|k| self.items[k].name.clone());
        let near_car =
            !self.driving && self.mode == Mode::Chase && self.walker.pos.dist(self.car.body.pos) < CAR_REACH;
        let prompt = match self.mode {
            Mode::House if self.held.is_some() => "holding",
            Mode::House if self.aim.is_some() => "pick",
            Mode::House if self.aim_player => "record",
            Mode::Chase if near_car => "enter",
            Mode::Chase if !self.driving => "run",
            _ => "",
        };
        let dread = if self.held.is_some() {
            fx::div(self.hold, self.hold_ticks.max(1))
        } else {
            0
        };
        let c = &self.car;
        let ability = |id: &str, ch: car::Charge, total: i32, recharge: i32| {
            let (state, level) = if ch.left > 0 {
                ("active", fx::div(ch.left, total))
            } else if ch.wait > 0 {
                ("charging", ONE - fx::div(ch.wait, total + recharge))
            } else {
                ("ready", ONE)
            };
            json!({ "id": id, "state": state, "level": pct(level * 100) })
        };
        // the car's own, then the ones it has drawn, in the order they came
        let mut abilities = vec![
            json!({ "id": "flip", "state": if c.flipping { "active" } else { "ready" }, "level": "100.00%" }),
        ];
        for &k in &self.drawn {
            let id = self.upgrades.pool[k].id.as_str();
            let row = match id {
                "big_wheels" => ability(id, c.big, car::BIG_TICKS, car::BIG_RECHARGE),
                "wings" => ability(id, c.fly, car::FLY_TICKS, car::FLY_RECHARGE),
                "dash" => ability(id, c.dash, car::DASH_TICKS, car::DASH_RECHARGE),
                "spin" => ability(id, c.spin_round, car::SPIN_TICKS, car::SPIN_RECHARGE),
                "nitrous" => ability(id, c.nitrous, car::NITROUS_TICKS, car::NITROUS_RECHARGE),
                "leap" => ability(id, c.leap, car::LEAP_TICKS, car::LEAP_RECHARGE),
                _ => continue,
            };
            abilities.push(row);
        }
        let mirrors: Vec<serde_json::Value> = self
            .views
            .iter()
            .skip(1)
            .map(|v| {
                let (w, h) = (self.width.max(1), self.height.max(1));
                let top = h - v.rect[1] - v.rect[3];
                json!([
                    pct(fx::div(v.rect[0], w) * 100),
                    pct(fx::div(top, h) * 100),
                    pct(fx::div(v.rect[2], w) * 100),
                    pct(fx::div(v.rect[3], h) * 100)
                ])
            })
            .collect();
        let car_marker = if self.mode == Mode::Chase && !self.driving {
            let aspect = fx::div(self.width.max(1), self.height.max(1));
            self.views[0]
                .cam
                .screen(aspect, self.car.body.pos.add(V3::UP.scale(int(2))))
                .map(|(x, y)| json!([pct(x.clamp(int(3), int(97))), pct(y.clamp(int(5), int(92)))]))
        } else {
            None
        };
        // the record player's callout, over it while you are near it in the house
        let player_marker = if self.mode == Mode::House
            && self.held.is_none()
            && self.walker.eye().dist(house::TURNTABLE) < PLAYER_CALLOUT
        {
            let aspect = fx::div(self.width.max(1), self.height.max(1));
            self.views[0]
                .cam
                .screen(aspect, house::TURNTABLE.add(V3::UP.scale(ONE * 7 / 10)))
                .filter(|&(x, y)| x > 0 && x < int(100) && y > 0 && y < int(100))
                .map(|(x, y)| json!([pct(x), pct(y)]))
        } else {
            None
        };
        let toast = self.toast.map(|(k, _)| {
            let u = k.map(|k| &self.upgrades.pool[k]).unwrap_or(&self.upgrades.start);
            json!({ "name": u.name, "text": u.text, "touch": u.touch.as_ref().unwrap_or(&u.text) })
        });
        // the next is a surprise: only when it comes is shown
        let next = (self.drawn.len() < self.upgrades.pool.len())
            .then(|| self.upgrades.at.get(self.unlocked))
            .flatten()
            .map(|&at| json!({ "in": clock_text((at as u32 * 60).saturating_sub(self.ticks)) }));
        let chosen = self.chosen.map(|k| &self.items[k]);
        let monster_dist = self
            .monster
            .as_ref()
            .map(|m| m.distance(target) / ONE)
            .unwrap_or(0);
        let speed_kmh = c.body.vel.len() as i64 * 36 / 10 / ONE as i64;
        json!({
            "mode": mode,
            "driving": self.driving,
            "prompt": prompt,
            "aim": item(self.aim),
            "held": item(self.held),
            "dread": frac(dread),
            "time": clock_text(self.ticks),
            "best": clock_text(self.best),
            "new_best": self.new_best,
            "speed": speed_kmh.to_string(),
            "distance": monster_dist.to_string(),
            "danger": frac(self.shake),
            "monster": chosen.map(|i| i.monster_name.clone()),
            "monster_id": chosen.map(|i| i.monster.clone()),
            "line": chosen.map(|i| i.line.clone()),
            "abilities": abilities,
            "mirrors": mirrors,
            "car_marker": car_marker,
            "toast": toast,
            "next": next,
            "sounds": self.sounds,
            "music": self.mode == Mode::Chase && !self.paused,
            "song": self.tracks[self.song].name,
            "song_id": self.tracks[self.song].id,
            "song_seq": self.song_seq,
            "player_marker": player_marker,
            "flying": c.flying(),
            "nitrous": c.nitrous.active(),
            "upside_down": c.up().y < ONE / 3 && self.driving,
            "loosed": self.city.stats.loosed,
            "awake": self.city.awake_count(),
            "pieces": self.city.len(),
        })
        .to_string()
    }

    /// For tests and the probe: the parts of the state a check reads.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn driving(&self) -> bool {
        self.driving
    }

    pub fn city(&self) -> &Rubble {
        &self.city
    }

    pub fn car(&self) -> &Car {
        &self.car
    }

    pub fn monster(&self) -> Option<&Monster> {
        self.monster.as_ref()
    }

    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    /// Puts the walker at a place facing a way (tests and the probe).
    pub fn place_walker(&mut self, at: V3, yaw: i32, pitch: i32) {
        self.walker = Walker::new(at, yaw, pitch);
    }
}

/// The layout numbers the page needs, so it holds none of its own.
pub fn numbers() -> String {
    let i = |name: &str, size: usize, offset: usize| json!([name, size, offset * 4]);
    json!({
        "vertex": { "stride": mesh::VERTEX_VALUES * 4, "attributes": [i("aPos", 3, 0), i("aNormal", 3, 3)] },
        "instance": {
            "stride": INSTANCE_VALUES * 4,
            "attributes": [i("iA", 4, 0), i("iB", 4, 4), i("iC", 4, 8), i("iD", 4, 12)]
        },
        "buffers": { "pieces": BUF_PIECES, "scenery": BUF_SCENERY, "dynamic": BUF_DYNAMIC },
        "passes": { "solid": PASS_SOLID, "sprite": PASS_SPRITE, "ink": PASS_INK },
        "draw_values": DRAW_VALUES,
        "vertex_values": mesh::VERTEX_VALUES,
        "rect_values": RECT_VALUES,
        "mat_values": MAT_VALUES,
        "view_values": 16,
        "dirty_values": 3,
        "mesh_count": render::MESH_COUNT,
        "layers": LAYERS,
        "uniforms": [["uLightDir", 3], ["uSky", 3], ["uFog", 3], ["uLight", 3], ["uWindowDark", 3], ["uWindowLit", 3], ["uInk", 3], ["uFogRange", 2], ["uInkSize", 3]],
        "actions": {
            "forward": FORWARD, "back": BACK, "left": LEFT, "right": RIGHT, "jump": JUMP, "fly": FLY,
            "flip": FLIP, "use": USE, "shift": SHIFT, "focus": FOCUS, "big": BIG,
            "dash": DASH, "spin": SPIN, "nitrous": NITROUS
        },
        "terrain_cells": terrain::N,
        "rubble_full": rubble::FULL,
    })
    .to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn game() -> Game {
        Game::new(
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/items.json"),
            include_str!("../../../data/upgrades.json"),
            include_str!("../../../data/pursuer.json"),
            include_str!("../../../data/music.json"),
            "",
        )
        .unwrap()
    }

    fn run(g: &mut Game, bits: u32, frames: usize, t: &mut i64) {
        for _ in 0..frames {
            *t += TICK_US;
            g.advance(*t, bits | FOCUS, 0, 0);
        }
    }

    #[test]
    fn the_game_opens_on_the_title_and_begins_in_the_house_looking_at_an_item() {
        let mut g = game();
        assert_eq!(g.mode(), Mode::Title);
        g.begin();
        assert_eq!(g.mode(), Mode::House);
        let mut t = 0;
        run(&mut g, 0, 5, &mut t);
        assert_eq!(g.aim, Some(0), "the walker starts facing the teddy bear");
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        assert_eq!(hud["prompt"], "pick");
    }

    #[test]
    fn letting_go_in_time_puts_it_back_and_holding_on_brings_the_pursuer() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, 0, 2, &mut t);
        run(&mut g, USE, 60 * 2, &mut t);
        assert!(g.held.is_some());
        run(&mut g, 0, 2, &mut t);
        assert!(g.held.is_none(), "let go");
        assert_eq!(g.mode(), Mode::House);
        run(&mut g, USE, 60 * 5 + 10, &mut t);
        assert_eq!(g.mode(), Mode::Chase);
        assert!(g.monster().is_some());
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        assert_eq!(hud["monster"], "Mr. Buttons");
        assert_eq!(hud["music"], true);
    }

    #[test]
    fn standing_still_gets_you_caught_and_the_time_is_the_score() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, USE, 60 * 5 + 5, &mut t);
        run(&mut g, 0, 60 * 60, &mut t);
        assert_eq!(g.mode(), Mode::Caught);
        assert!(g.ticks() > 60 * 10, "{}", g.ticks());
        assert!(g.saved().contains(&g.ticks().to_string()));
        assert!(g.city().stats.loosed > 0, "it broke things on the way");
    }

    #[test]
    fn the_car_can_be_entered_and_driven_out_of_the_garage() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, USE, 60 * 5 + 5, &mut t);
        let near = house::CAR_AT.add(v3(0, 0, int(2)));
        g.place_walker(v3(near.x, house::FLOOR, near.z), 0, 0);
        run(&mut g, 0, 2, &mut t);
        run(&mut g, USE, 2, &mut t);
        assert!(g.driving());
        run(&mut g, FORWARD, 60 * 3, &mut t);
        assert!(
            g.car().body.pos.x > int(30),
            "out the garage door, east: {}",
            g.car().body.pos.x / ONE
        );
        assert_eq!(g.views.len(), 4, "the windscreen and three mirrors");
    }

    #[test]
    fn a_press_on_a_frame_with_no_tick_is_still_a_press() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, 0, 3, &mut t);
        // a display frame too soon after the last for a tick to run
        g.advance(t + TICK_US / 4, USE | FOCUS, 0, 0);
        run(&mut g, USE, 2, &mut t);
        assert!(g.held.is_some(), "the press reached the house");
    }

    #[test]
    fn a_tap_let_go_before_any_tick_ran_still_counts() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        // into the chase and into the car
        run(&mut g, USE, 60 * 6, &mut t);
        g.walker.pos = g.car.body.pos;
        run(&mut g, 0, 2, &mut t);
        run(&mut g, USE, 2, &mut t);
        assert!(g.driving());
        g.car.kit.big_wheels = true;
        // Q down and up within one display frame that ran no tick
        g.advance(t + TICK_US / 4, BIG | FOCUS, 0, 0);
        run(&mut g, 0, 2, &mut t);
        assert!(g.car().big.left > 0, "the big wheels came out");
    }

    #[test]
    fn a_dash_whooshes_and_nitrous_roars() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, USE, 60 * 6, &mut t);
        g.walker.pos = g.car.body.pos;
        run(&mut g, 0, 2, &mut t);
        run(&mut g, USE, 2, &mut t);
        g.car.kit.dash = true;
        g.car.kit.nitrous = true;
        let heard = |g: &Game| g.sounds.clone();
        g.advance(t + TICK_US, DASH | FOCUS, 0, 0);
        t += TICK_US;
        assert!(heard(&g).contains(&"whoosh"), "{:?}", heard(&g));
        g.advance(t + TICK_US, NITROUS | FOCUS, 0, 0);
        assert!(heard(&g).contains(&"nitrous"), "{:?}", heard(&g));
    }

    #[test]
    fn the_pursuer_jumping_in_front_of_you_stings_and_shakes_the_view() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, USE, 60 * 6, &mut t);
        let you = g.player_pos();
        let m = g.monster.as_mut().unwrap();
        m.age = 60 * 30;
        m.pos = you.add(v3(int(300), 0, 0));
        m.jump_in = 1;
        run(&mut g, 0, 1, &mut t);
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        assert!(
            hud["sounds"].as_array().unwrap().iter().any(|s| s == "scare"),
            "{}",
            hud["sounds"]
        );
        assert!(g.shake > ONE * 9 / 10, "the view shakes: {}", g.shake);
        let d = g.monster().unwrap().distance(g.player_pos());
        assert!((d - g.tuning.jump_distance).abs() < int(2), "{}", d / ONE);
    }

    /// Stands the walker in front of the record player, looking at it.
    fn face_the_record_player(g: &mut Game) {
        let at = v3(-ONE * 105 / 100, house::FLOOR, -ONE * 380 / 100);
        let eye = at.add(V3::UP.scale(crate::walker::EYE));
        let (yaw, pitch) = crate::geom::yaw_pitch_of(house::TURNTABLE.sub(eye));
        g.place_walker(at, yaw, pitch);
    }

    #[test]
    fn the_record_player_changes_the_chase_music_and_the_choice_is_kept() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        face_the_record_player(&mut g);
        run(&mut g, 0, 2, &mut t);
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        assert_eq!(hud["prompt"], "record");
        assert_eq!(
            hud["song_id"], "hanging_tree",
            "the first record plays by default"
        );
        assert!(hud["player_marker"].is_array(), "its callout floats over it");
        run(&mut g, USE, 2, &mut t);
        run(&mut g, 0, 2, &mut t);
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        assert_eq!(hud["song_id"], "chiptune");
        assert_eq!(hud["song"], "Pursuer (chiptune)");
        assert_eq!(hud["song_seq"], 1);
        assert!(g.held.is_none(), "a press at the player picks nothing up");
        // kept across a visit
        let saved = g.saved();
        let again = Game::new(
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/items.json"),
            include_str!("../../../data/upgrades.json"),
            include_str!("../../../data/pursuer.json"),
            include_str!("../../../data/music.json"),
            &saved,
        )
        .unwrap();
        assert_eq!(again.tracks[again.song].id, "chiptune");
        // and back round to the first
        run(&mut g, USE, 2, &mut t);
        assert_eq!(g.tracks[g.song].id, "hanging_tree");
    }

    #[test]
    fn the_records_cover_is_the_texture_after_every_pursuer() {
        let g = game();
        let k = g.items.iter().position(|i| i.id == "vinyl").unwrap();
        assert_eq!(g.cover_texture(k), g.items.len() as u32);
        let list: serde_json::Value = serde_json::from_str(&g.items_json()).unwrap();
        assert_eq!(list[k]["cover"], "vinyl");
        assert_eq!(list[k]["monster"], "kanye");
        let mut b = Batch::new();
        let place = g.items[k].rest();
        b.sprite(&g.items[k].cover_at(&place).unwrap(), g.cover_texture(k));
        let (_, _, sprites) = b.finish();
        assert_eq!(sprites[0].2, 7);
    }

    #[test]
    fn a_record_the_build_lacks_is_skipped() {
        let music = include_str!("../../../data/music.json")
            .replace("\"rate\": 1", "\"rate\": 1, \"present\": false");
        let g = Game::new(
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/items.json"),
            include_str!("../../../data/upgrades.json"),
            include_str!("../../../data/pursuer.json"),
            &music,
            r#"{"best": 0, "song": "hanging_tree"}"#,
        )
        .unwrap();
        assert_eq!(
            g.tracks[g.song].id, "chiptune",
            "the default falls to a record that is there"
        );
        let mut g = g;
        g.next_song();
        assert_eq!(g.tracks[g.song].id, "chiptune", "and the player stays on it");
    }

    #[test]
    fn a_touch_screen_is_drawn_at_no_more_than_twice_its_css_size() {
        let mut g = game();
        assert_eq!(
            g.resize(390, 844, 300),
            [1170, 2532],
            "a mouse and a 3x screen: all of it"
        );
        g.set_touch(true);
        assert_eq!(g.resize(390, 844, 300), [780, 1688]);
        assert_eq!(g.resize(390, 844, 100), [390, 844], "and a 1x screen as it is");
    }

    #[test]
    fn a_staged_scene_puts_the_car_the_pursuer_and_the_abilities_where_it_says() {
        let mut g = game();
        g.stage(
            r#"{"item": "vinyl", "seconds": 120, "abilities": ["wings", "dash"],
                "car": [50, -300, 0, 30], "behind": 40, "jump_in": 2}"#,
        )
        .unwrap();
        assert_eq!(g.mode(), Mode::Chase);
        assert!(g.driving());
        let p: serde_json::Value = serde_json::from_str(&g.probe()).unwrap();
        assert_eq!(p["car"][0], 50);
        assert_eq!(p["speed"], 30);
        assert!(g.car().kit.wings && g.car().kit.dash && !g.car().kit.nitrous);
        let m = g.monster.as_ref().unwrap();
        assert_eq!(
            (m.pos.x / ONE, m.pos.z / ONE),
            (50, -260),
            "40 m behind a car heading north"
        );
        assert_eq!(m.jump_in, 120);
        assert_eq!(g.ticks, 120 * 60);
        assert_eq!(g.unlocked, 4, "the draws at 25, 55, 85 and 120 s are past");
        assert!(g.stage(r#"{"item": "nothing"}"#).is_err());
    }

    #[test]
    fn a_ravaged_street_is_torn_down() {
        let mut g = game();
        let before = g.city.intact_count();
        g.stage(r#"{"item": "vinyl", "ravage": [[50, -100, 50, -400]], "settle": 1}"#)
            .unwrap();
        assert!(
            g.city.intact_count() < before - 200,
            "{} of {before}",
            g.city.intact_count()
        );
    }

    #[test]
    fn the_rear_view_mirror_tilts_up_to_the_pursuers_face_when_it_is_close() {
        let mut g = game();
        g.stage(r#"{"item": "vinyl", "seconds": 60, "car": [50, -300, 0, 0], "behind": 400}"#)
            .unwrap();
        let far = g.rear_aim();
        g.stage(r#"{"item": "vinyl", "seconds": 60, "car": [50, -300, 0, 0], "behind": 30}"#)
            .unwrap();
        let near = g.rear_aim();
        assert!(far.y < ONE / 10, "far off, it looks back level: {far:?}");
        assert!(near.y > ONE / 3, "close, it looks up at the face: {near:?}");
    }

    #[test]
    fn upgrades_arrive_with_time() {
        let mut g = game();
        g.begin();
        let mut t = 0;
        run(&mut g, USE, 60 * 5 + 5, &mut t);
        g.driving = true;
        // keep it out of reach: the car is moved each frame far from it
        for _ in 0..(60 * 60) {
            if let Some(m) = g.monster.as_mut() {
                m.pos = v3(-int(400), 0, -int(400));
            }
            run(&mut g, 0, 1, &mut t);
        }
        assert_eq!(g.drawn.len(), 2, "one at 25 s and one at 55 s");
        assert_ne!(g.drawn[0], g.drawn[1]);
        let kit = g.car().kit;
        let owned = [
            kit.big_wheels,
            kit.wings,
            kit.dash,
            kit.spin,
            kit.nitrous,
            kit.leap,
            kit.turbo > 0,
        ];
        assert_eq!(owned.iter().filter(|&&o| o).count(), 2, "{kit:?}");
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        assert_eq!(
            hud["abilities"].as_array().unwrap().len(),
            1 + 2 - kit.turbo as usize
        );
    }

    #[test]
    fn the_draw_never_repeats_and_takes_the_turbos_in_order() {
        let pool = parse_upgrades(include_str!("../../../data/upgrades.json"))
            .unwrap()
            .pool;
        let mut firsts = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let mut rng = Rng::new(seed);
            let mut drawn = Vec::new();
            while let Some(k) = draw(&pool, &drawn, &mut rng) {
                assert!(!drawn.contains(&k));
                let turbos: Vec<&str> = drawn
                    .iter()
                    .chain([&k])
                    .filter(|&&j| pool[j].id == "turbo")
                    .map(|&j| pool[j].name.as_str())
                    .collect();
                assert!(
                    ["Turbo", "Turbo II", "Turbo III"].starts_with(&turbos),
                    "{turbos:?}"
                );
                drawn.push(k);
            }
            assert_eq!(drawn.len(), pool.len(), "all of it, in the end");
            firsts.insert(drawn[0]);
        }
        assert!(firsts.len() > 3, "the first draw varies: {firsts:?}");
    }

    #[test]
    fn the_numbers_parse_and_clock_text_reads_minutes() {
        let n: serde_json::Value = serde_json::from_str(&numbers()).unwrap();
        assert_eq!(n["instance"]["stride"], 64);
        assert_eq!(clock_text(60 * 83 + 30), "1:23.5");
        assert_eq!(pct(ONE / 2), "0.50%");
    }

    #[test]
    fn every_mesh_has_whole_triangles() {
        let g = game();
        for id in 0..render::MESH_COUNT {
            let v = g.mesh(id);
            assert!(!v.is_empty(), "mesh {id}");
            assert_eq!(v.len() % (3 * mesh::VERTEX_VALUES), 0);
        }
    }
}

/// White noise for the page's sound, from the core's seeded generator, as
/// Q16 samples in −1..1.
pub fn noise(n: u32, seed: u32) -> Vec<i32> {
    let mut r = Rng::new(seed as u64);
    (0..n.min(1 << 20)).map(|_| r.range(-ONE, ONE)).collect()
}

#[cfg(test)]
mod handedness {
    use super::*;

    /// Every dynamic and scenery instance, in every mode of a chase, has
    /// right-handed axes (a mirrored shape is drawn inside out).
    #[test]
    fn every_drawn_shape_is_right_handed() {
        let mut g = tests::game();
        g.begin();
        let mut t = 0i64;
        let check = |g: &Game, when: &str| {
            for (k, v) in g
                .dynamic
                .chunks(INSTANCE_VALUES)
                .chain(g.scenery.values.chunks(INSTANCE_VALUES))
                .enumerate()
            {
                let a = Affine {
                    c0: v3(v[0], v[1], v[2]),
                    c1: v3(v[4], v[5], v[6]),
                    c2: v3(v[8], v[9], v[10]),
                    t: v3(v[3], v[7], v[11]),
                };
                assert!(
                    render::right_handed(&a),
                    "{when}: instance {k} is mirrored: {a:?}"
                );
            }
        };
        check(&g, "house");
        for _ in 0..(60 * 5 + 200) {
            t += TICK_US;
            g.advance(t, USE | FOCUS, 0, 0);
        }
        check(&g, "folded");
        g.driving = true;
        g.frame();
        check(&g, "driving");
    }
}
