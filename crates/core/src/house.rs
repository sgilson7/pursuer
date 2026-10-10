//! The house in the middle of the city: its walls, furniture, the things to
//! pick up, and how it folds flat.
//!
//! The house is a paper diorama. Each outer wall is hinged at its foot on the
//! outside, and when the pursuer comes the walls fall outward one after
//! another and lie flat on the lawn, inside face up, as a Paper Mario house
//! opens; the roof lifts off and blows away. The garage is the east end, its
//! door facing a driveway to the avenue, with the car inside.

use crate::content::{hex, Palette, Rgb};
use crate::fx::{self, int, ONE};
use crate::geom::{v3, Affine, V3};
use crate::mesh::Mesh;
use crate::quat::Quat;
use serde::Deserialize;

/// The floor's top, metres.
pub const FLOOR: i32 = ONE * 15 / 100;
pub const WALL_H: i32 = 3 * ONE;
const THICK: i32 = ONE / 5;
/// The footprint: main rooms from x = −9 to 6, the garage from 6 to 14.
pub const X0: i32 = -9 * ONE;
pub const X1: i32 = 14 * ONE;
pub const XG: i32 = 6 * ONE;
pub const Z0: i32 = -6 * ONE;
pub const Z1: i32 = 6 * ONE;
/// Where the car waits, facing the garage door (east).
pub const CAR_AT: V3 = v3(11 * ONE, ONE, 0);
pub const CAR_YAW: i32 = -fx::QUARTER;
/// The walls take this long to fall, all told.
pub const FOLD_TICKS: i32 = 150;
/// The middle of the record player's platter, on the cabinet against the
/// north wall, and how near the line of sight must pass to aim at it.
pub const TURNTABLE: V3 = v3(-ONE * 105 / 100, ONE * 87 / 100, -ONE * 540 / 100);
pub const TURNTABLE_RADIUS: i32 = ONE * 30 / 100;

/// A box drawn in the house: mesh, transform, colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub mesh: Mesh,
    pub at: Affine,
    pub rgb: Rgb,
}

/// A box the walker cannot pass, axis-aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Solid {
    pub lo: V3,
    pub hi: V3,
    /// the wall it belongs to, which stops blocking once it has fallen
    pub wall: Option<usize>,
}

/// A rectangle of a wall, in the wall's own frame: along it, up it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Panel {
    u0: i32,
    u1: i32,
    v0: i32,
    v1: i32,
}

/// An outer wall: its foot runs from `a` along `dir` for `len`, and it falls
/// toward `out`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wall {
    a: V3,
    dir: V3,
    out: V3,
    height: i32,
    panels: Vec<Panel>,
    /// ticks into the fold before this wall starts to fall
    delay: i32,
    outside: Rgb,
    inside: Rgb,
}

/// The fraction of a wall's fall at `t` ticks after it starts: it tips
/// slowly, then fast, as a falling board does, and bounces once.
pub fn fall(t: i32) -> i32 {
    const DOWN: i32 = 40;
    if t <= 0 {
        return 0;
    }
    if t < DOWN {
        let s = fx::div(t, DOWN);
        return fx::mul(s, s);
    }
    let b = t - DOWN;
    if b < 14 {
        // a small bounce back up and down again
        let s = fx::div(b, 14);
        return ONE - fx::mul(fx::mul(s, ONE - s), ONE / 3);
    }
    ONE
}

impl Wall {
    /// The wall's frame at a fall fraction: along, up, and outward.
    fn frame(&self, f: i32) -> (V3, V3) {
        let angle = fx::mul(f, fx::QUARTER);
        let (s, c) = (fx::sin(angle), fx::cos(angle));
        let up = V3::UP.scale(c).add(self.out.scale(s));
        let out = self.out.scale(c).sub(V3::UP.scale(s));
        (up, out)
    }

    pub fn shapes(&self, fold: i32, out: &mut Vec<Shape>) {
        let f = fall(fold - self.delay);
        let (up, n) = self.frame(f);
        for p in &self.panels {
            let (u, v) = ((p.u0 + p.u1) / 2, (p.v0 + p.v1) / 2);
            let (su, sv) = (p.u1 - p.u0, p.v1 - p.v0);
            let base = self.a.add(self.dir.scale(u)).add(up.scale(v));
            // the outside skin, then the inside
            for (depth, w, rgb) in [
                (ONE / 25, 0, self.outside),
                (THICK - ONE / 25, ONE / 25, self.inside),
            ] {
                let centre = base.sub(n.scale(w + depth / 2));
                out.push(Shape {
                    mesh: Mesh::Cube,
                    at: Affine {
                        c0: self.dir.scale(su),
                        c1: up.scale(sv),
                        // right-handed, so the faces wind outward
                        c2: self.dir.cross(up).scale(depth),
                        t: centre,
                    },
                    rgb,
                });
            }
        }
    }

    pub fn standing(&self, fold: i32) -> bool {
        fall(fold - self.delay) < ONE / 4
    }

    fn solids(&self, k: usize, out: &mut Vec<Solid>) {
        for p in &self.panels {
            let a = self.a.add(self.dir.scale(p.u0)).add(V3::UP.scale(p.v0));
            let b = self
                .a
                .add(self.dir.scale(p.u1))
                .add(V3::UP.scale(p.v1))
                .sub(self.out.scale(THICK));
            out.push(Solid {
                lo: v3(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
                hi: v3(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
                wall: Some(k),
            });
        }
    }
}

/// Splits a wall `len` long into panels round its openings (each `u0, u1,
/// v0, v1` in metres along and up).
fn panels(len: i32, height: i32, holes: &[(i32, i32, i32, i32)]) -> Vec<Panel> {
    let mut cuts = vec![0, len];
    for h in holes {
        cuts.push(h.0);
        cuts.push(h.1);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        let (u0, u1) = (w[0], w[1]);
        let mid = (u0 + u1) / 2;
        match holes.iter().find(|h| mid > h.0 && mid < h.1) {
            Some(h) => {
                if h.2 > 0 {
                    out.push(Panel {
                        u0,
                        u1,
                        v0: 0,
                        v1: h.2,
                    });
                }
                if h.3 < height {
                    out.push(Panel {
                        u0,
                        u1,
                        v0: h.3,
                        v1: height,
                    });
                }
            }
            None => out.push(Panel {
                u0,
                u1,
                v0: 0,
                v1: height,
            }),
        }
    }
    out
}

fn m(v: i32) -> i32 {
    v * ONE / 100
}

/// One part of an item: mesh, centre and size in metres, colour, tilt, turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub mesh: Mesh,
    pub at: V3,
    pub size: V3,
    pub rgb: Rgb,
    pub tilt: i32,
    pub turn: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub monster: String,
    pub monster_name: String,
    pub line: String,
    pub at: V3,
    pub turn: i32,
    pub radius: i32,
    pub parts: Vec<Part>,
    /// the middle of the item, for looking at it
    pub centre: V3,
    /// a picture laid on the item's top (a record's sleeve): its name under
    /// web/art/covers/, and its side in metres
    pub cover: Option<String>,
    pub cover_size: i32,
    /// the height of the item's top above its foot
    pub top: i32,
}

#[derive(Deserialize)]
struct ItemsFile {
    hold_seconds: i32,
    items: Vec<ItemJson>,
}

#[derive(Deserialize)]
struct ItemJson {
    id: String,
    name: String,
    monster: String,
    monster_name: String,
    line: String,
    at: [i32; 3],
    turn: i32,
    radius: i32,
    parts: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    cover_size: i32,
}

/// The items and how many seconds of holding brings a pursuer.
pub fn parse_items(json: &str) -> Result<(Vec<Item>, i32), String> {
    let f: ItemsFile = serde_json::from_str(json).map_err(|e| format!("items.json: {e}"))?;
    let mut items = Vec::new();
    for it in f.items {
        let mut parts = Vec::new();
        let mut top = 0;
        for p in &it.parts {
            let num = |k: usize| p.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let mesh_name = p.first().and_then(|v| v.as_str()).unwrap_or("");
            let mesh = match mesh_name {
                "cube" => Mesh::Cube,
                "sphere" => Mesh::Sphere,
                "cyl" => Mesh::Cyl,
                "cone" => Mesh::Cone,
                "octa" => Mesh::Octa,
                "wedge" => Mesh::Wedge,
                other => return Err(format!("items.json {}: unknown mesh {other}", it.id)),
            };
            let colour = p
                .get(7)
                .and_then(|v| v.as_str())
                .ok_or(format!("items.json {}: a part lacks a colour", it.id))?;
            let part = Part {
                mesh,
                at: v3(m(num(1)), m(num(2)), m(num(3))),
                size: v3(m(num(4)), m(num(5)), m(num(6))),
                rgb: hex(colour)?,
                tilt: fx::deg(num(8)),
                turn: fx::deg(num(9)),
            };
            top = top.max(part.at.y + part.size.y / 2);
            parts.push(part);
        }
        let at = v3(m(it.at[0]), m(it.at[1]), m(it.at[2]));
        items.push(Item {
            id: it.id,
            name: it.name,
            monster: it.monster,
            monster_name: it.monster_name,
            line: it.line,
            at,
            turn: fx::deg(it.turn),
            radius: m(it.radius),
            parts,
            centre: at.add(V3::UP.scale(top / 2)),
            cover: it.cover,
            cover_size: m(it.cover_size),
            top,
        });
    }
    Ok((items, f.hold_seconds))
}

impl Item {
    /// The item's shapes, placed by `place` (its foot, turned).
    pub fn shapes(&self, place: &Affine, out: &mut Vec<Shape>) {
        for p in &self.parts {
            let local = Affine::translate(p.at)
                .then(&Affine::rot_y(p.turn))
                .then(&Affine::rot_x(p.tilt))
                .then(&Affine::scale(p.size));
            out.push(Shape {
                mesh: p.mesh,
                at: place.then(&local),
                rgb: p.rgb,
            });
        }
    }

    /// Where its cover lies, placed by `place`: flat on its top, the
    /// picture's top edge toward −z (away from someone facing north).
    pub fn cover_at(&self, place: &Affine) -> Option<Affine> {
        self.cover.as_ref()?;
        let s = self.cover_size;
        Some(place.then(&Affine {
            c0: v3(s, 0, 0),
            c1: v3(0, 0, -s),
            c2: V3::UP,
            t: v3(0, self.top + ONE / 500, 0),
        }))
    }

    /// Where it rests in the house.
    pub fn rest(&self) -> Affine {
        Affine::translate(self.at).then(&Affine::rot_y(self.turn))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct House {
    pub walls: Vec<Wall>,
    /// furniture, floor and the parts that do not fall
    pub fixed: Vec<Shape>,
    pub solids: Vec<Solid>,
    roof: Vec<Shape>,
}

impl House {
    pub fn new(pal: &Palette) -> House {
        let (outside, inside) = (pal.get("house_out"), pal.get("house_in"));
        let mut walls = Vec::new();
        let h = WALL_H;
        let wall = |a: V3, dir: V3, out: V3, len: i32, holes: &[(i32, i32, i32, i32)], delay: i32| Wall {
            a,
            dir,
            out,
            height: h,
            panels: panels(len, h, holes),
            delay,
            outside,
            inside,
        };
        let w = |x: i32| x * ONE / 10;
        let base = |x: i32, z: i32| v3(x, FLOOR, z);
        // north (z = −6), west to east: windows, and the garage's side window
        walls.push(wall(
            base(X0, Z0),
            v3(ONE, 0, 0),
            v3(0, 0, -ONE),
            X1 - X0,
            &[
                (w(25), w(45), w(10), w(22)),
                (w(80), w(100), w(10), w(22)),
                (w(185), w(205), w(12), w(22)),
            ],
            0,
        ));
        // south (z = 6): the front door and windows
        walls.push(wall(
            base(X0, Z1),
            v3(ONE, 0, 0),
            v3(0, 0, ONE),
            X1 - X0,
            &[
                (w(20), w(40), w(10), w(22)),
                (w(60), w(70), 0, w(22)),
                (w(95), w(115), w(10), w(22)),
            ],
            12,
        ));
        // west (x = −9)
        walls.push(wall(
            base(X0, Z0),
            v3(0, 0, ONE),
            v3(-ONE, 0, 0),
            Z1 - Z0,
            &[(w(40), w(80), w(10), w(22))],
            24,
        ));
        // east (x = 14): the garage door is its own panel, below
        walls.push(wall(
            base(X1, Z0),
            v3(0, 0, ONE),
            v3(ONE, 0, 0),
            Z1 - Z0,
            &[(w(25), w(95), 0, w(26))],
            6,
        ));
        // the garage door
        let mut door = wall(base(X1, Z0 + w(25)), v3(0, 0, ONE), v3(ONE, 0, 0), w(70), &[], 2);
        door.height = w(26);
        door.panels = panels(w(70), w(26), &[]);
        door.outside = pal.get("garage_door");
        door.inside = pal.get("garage_door");
        walls.push(door);
        // between the rooms and the garage: falls into the garage, short of the car
        let mut part = wall(
            base(XG + THICK, Z0),
            v3(0, 0, ONE),
            v3(ONE, 0, 0),
            Z1 - Z0,
            &[(w(50), w(70), 0, w(22))],
            30,
        );
        part.height = w(25);
        part.panels = panels(Z1 - Z0, w(25), &[(w(50), w(70), 0, w(22))]);
        walls.push(part);

        let mut fixed = Vec::new();
        let mut solids = Vec::new();
        let mut boxed = |lo: V3, hi: V3, rgb: Rgb, solid: bool| {
            let size = hi.sub(lo);
            fixed.push(Shape {
                mesh: Mesh::Cube,
                at: Affine::translate(lo.add(size.scale(ONE / 2))).then(&Affine::scale(size)),
                rgb,
            });
            if solid {
                solids.push(Solid { lo, hi, wall: None });
            }
        };
        let c = |x: i32, y: i32, z: i32| v3(m(x), m(y), m(z));
        // floors
        boxed(v3(X0, -ONE, Z0), v3(XG, FLOOR, Z1), pal.get("floor"), true);
        boxed(v3(XG, -ONE, Z0), v3(X1, FLOOR, Z1), pal.get("garage_floor"), true);
        boxed(c(-700, 15, 50), c(-300, 17, 350), pal.get("rug"), false);
        // the couch along the south wall
        boxed(c(-700, 15, 400), c(-300, 60, 540), pal.get("couch"), true);
        boxed(c(-700, 60, 500), c(-300, 105, 560), pal.get("couch"), true);
        boxed(c(-720, 15, 400), c(-700, 80, 560), pal.get("couch"), true);
        boxed(c(-300, 15, 400), c(-280, 80, 560), pal.get("couch"), true);
        // coffee table
        boxed(c(-600, 15, 180), c(-400, 55, 290), pal.get("wood"), true);
        // the television on its stand
        boxed(c(-650, 15, -560), c(-350, 75, -500), pal.get("wood_dark"), true);
        boxed(c(-620, 75, -545), c(-380, 150, -535), pal.get("screen"), false);
        // bookshelf on the west wall, three boards
        boxed(c(-880, 15, -300), c(-840, 215, -100), pal.get("wood"), true);
        for y in [70, 125, 175] {
            boxed(
                c(-880, y, -300),
                c(-800, y + 4, -100),
                pal.get("wood_dark"),
                false,
            );
        }
        // kitchen counter on the north wall, and the fridge
        boxed(c(100, 15, -580), c(550, 110, -490), pal.get("counter"), true);
        boxed(c(450, 15, -580), c(580, 215, -480), pal.get("appliance"), true);
        // dining table and two chairs
        boxed(c(150, 15, 250), c(400, 90, 400), pal.get("wood"), true);
        boxed(c(180, 15, 420), c(240, 60, 480), pal.get("wood_dark"), true);
        boxed(c(310, 15, 170), c(370, 60, 230), pal.get("wood_dark"), true);
        // the record player on its cabinet, between the television and the
        // kitchen; the record to pick up lies beside it (data/items.json)
        boxed(c(-170, 15, -585), c(40, 75, -495), pal.get("wood"), true);
        boxed(c(-160, 75, -575), c(-50, 85, -505), pal.get("turntable"), false);
        boxed(c(-75, 85, -520), c(-60, 89, -510), pal.get("chrome"), false);
        boxed(c(-72, 89, -575), c(-66, 92, -525), pal.get("chrome"), false);
        // a lamp
        boxed(c(-820, 15, 470), c(-790, 160, 500), pal.get("wood_dark"), false);
        boxed(c(-850, 140, 440), c(-760, 180, 530), pal.get("window_lit"), false);
        for (d, h, rgb) in [(40, 2, "record"), (12, 3, "record_label")] {
            fixed.push(Shape {
                mesh: Mesh::Cyl,
                at: Affine::translate(c(-105, 85 + h / 2, -540)).then(&Affine::scale(c(d, h, d))),
                rgb: pal.get(rgb),
            });
        }
        let mut solids_all = solids;
        for (k, wl) in walls.iter().enumerate() {
            wl.solids(k, &mut solids_all);
        }

        // the roof: a slab with a trim, lifted off as one
        let roof_rgb = pal.get("house_roof");
        let size = v3(X1 - X0 + ONE / 2, ONE * 3 / 10, Z1 - Z0 + ONE / 2);
        let roof = vec![
            Shape {
                mesh: Mesh::Cube,
                at: Affine::scale(size),
                rgb: roof_rgb,
            },
            Shape {
                mesh: Mesh::Cube,
                at: Affine::translate(v3(0, -ONE / 5, 0)).then(&Affine::scale(v3(
                    size.x + ONE / 5,
                    ONE / 8,
                    size.z + ONE / 5,
                ))),
                rgb: pal.get("house_trim"),
            },
        ];
        House {
            walls,
            fixed,
            solids: solids_all,
            roof,
        }
    }

    /// Every shape of the house `fold` ticks into its fall (zero: standing).
    pub fn shapes(&self, fold: i32, out: &mut Vec<Shape>) {
        out.extend_from_slice(&self.fixed);
        for w in &self.walls {
            w.shapes(fold, out);
        }
        // the roof rises, turns and is gone
        let lift = fx::div(fold.clamp(0, FOLD_TICKS), FOLD_TICKS);
        if lift >= ONE {
            return;
        }
        let centre = v3((X0 + X1) / 2, FLOOR + WALL_H + ONE * 3 / 20, 0);
        let rise = fx::mul(fx::mul(lift, lift), int(70));
        let drift = fx::mul(lift, int(25));
        let spin = Quat::axis_angle(v3(ONE, 0, ONE / 2).norm(), fx::mul(lift, fx::deg(200)));
        let shrink = ONE - fx::mul(lift, lift);
        let b = spin.basis();
        let place = Affine {
            c0: b[0].scale(shrink),
            c1: b[1].scale(shrink),
            c2: b[2].scale(shrink),
            t: centre.add(v3(-drift, rise, -drift / 2)),
        };
        for s in &self.roof {
            out.push(Shape {
                mesh: s.mesh,
                at: place.then(&s.at),
                rgb: s.rgb,
            });
        }
    }

    /// The boxes that block the walker at `fold`.
    pub fn blocking(&self, fold: i32) -> impl Iterator<Item = &Solid> {
        self.solids
            .iter()
            .filter(move |s| s.wall.map(|k| self.walls[k].standing(fold)).unwrap_or(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn house() -> House {
        House::new(&Palette::parse(include_str!("../../../data/palette.json")).unwrap())
    }

    #[test]
    fn panels_leave_the_openings_open() {
        let p = panels(10 * ONE, 3 * ONE, &[(2 * ONE, 4 * ONE, ONE, 2 * ONE)]);
        // left of the window, under it, over it, right of it
        assert_eq!(p.len(), 4);
        let area: i64 = p
            .iter()
            .map(|q| (q.u1 - q.u0) as i64 * (q.v1 - q.v0) as i64 / ONE as i64)
            .sum();
        assert_eq!(area, (30 - 2) as i64 * ONE as i64);
    }

    #[test]
    fn a_wall_falls_outward_and_lies_flat() {
        let h = house();
        let north = &h.walls[0];
        let mut before = Vec::new();
        north.shapes(0, &mut before);
        let mut after = Vec::new();
        north.shapes(200, &mut after);
        for s in &after {
            assert!(s.at.t.y < FLOOR + ONE / 4, "lies flat: {}", s.at.t.y);
        }
        let mean_z = |v: &[Shape]| v.iter().map(|s| s.at.t.z as i64).sum::<i64>() / v.len() as i64;
        assert!(
            mean_z(&after) < mean_z(&before) - ONE as i64,
            "fell north, outward"
        );
    }

    #[test]
    fn the_walls_stop_blocking_once_they_fall_and_the_furniture_does_not() {
        let h = house();
        let standing = h.blocking(0).count();
        let folded = h.blocking(FOLD_TICKS + 60).count();
        assert!(folded < standing);
        assert!(h.blocking(FOLD_TICKS + 60).all(|s| s.wall.is_none()));
        assert!(folded > 5);
    }

    #[test]
    fn the_fall_eases_in_and_ends_flat() {
        assert_eq!(fall(0), 0);
        assert!(fall(10) < fall(30));
        assert_eq!(fall(500), ONE);
    }

    #[test]
    fn every_item_parses_and_sits_inside_the_house() {
        let (items, hold) = parse_items(include_str!("../../../data/items.json")).unwrap();
        assert_eq!(items.len(), 7);
        assert_eq!(hold, 5);
        for it in &items {
            assert!(
                it.at.x > X0 && it.at.x < XG && it.at.z > Z0 && it.at.z < Z1,
                "{}",
                it.id
            );
            assert!(!it.parts.is_empty());
        }
    }
}
