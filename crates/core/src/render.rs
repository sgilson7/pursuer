//! What the page draws, and in what order.
//!
//! Three instance buffers: every piece of the city in fixed slots (uploaded
//! whole once, then only the runs that moved), the scenery that never moves
//! (the ground, the road paint), and the few things rebuilt each frame (the
//! house, the car, the cockpit, the item in hand, dust, the pursuer). An
//! instance is a 3×4 transform and a colour with a style, as Q16 integers;
//! the shader reads them as integers and scales them.
//!
//! The screen holds up to four views: the windscreen, and while driving the
//! rear-view mirror and the two door mirrors. Each view gets its own list of
//! draws. Pieces are drawn in runs of whole structures that the view can
//! see, so a mirror looking down an empty street draws little of the city.
//! A mirror's picture is flipped left to right, as a mirror's is.

use crate::content::Rgb;
use crate::fx::{self, ONE};
use crate::geom::{v3, Affine, Mat4, V3};
use crate::mesh::Mesh;
use crate::rubble::Group;

pub const INSTANCE_VALUES: usize = 16;
pub const DRAW_VALUES: usize = 7;
/// x, y, width, height in pixels from the bottom left, and mirrored (0 or 1)
pub const RECT_VALUES: usize = 5;
/// the view-projection matrix and the eye
pub const MAT_VALUES: usize = 19;
pub const BYTES_PER_VALUE: usize = 4;

pub const BUF_PIECES: u32 = 0;
pub const BUF_SCENERY: u32 = 1;
pub const BUF_DYNAMIC: u32 = 2;

pub const PASS_SOLID: u32 = 0;
pub const PASS_SPRITE: u32 = 1;
pub const PASS_INK: u32 = 2;

/// What a dynamic shape belongs to, which decides the views it is drawn in.
pub const LAYER_WORLD: usize = 0;
/// the car seen from outside
pub const LAYER_CAR: usize = 1;
/// the cockpit and the thing in your hand: the windscreen view only
pub const LAYER_COCKPIT: usize = 2;
pub const LAYERS: usize = 3;

/// The meshes made from the city's ground, after the shared ones.
pub const MESH_GRASS: u32 = Mesh::ALL.len() as u32;
pub const MESH_ROAD: u32 = MESH_GRASS + 1;
pub const MESH_WALK: u32 = MESH_GRASS + 2;
pub const MESH_OUTSKIRTS: u32 = MESH_GRASS + 3;
pub const MESH_COUNT: u32 = MESH_GRASS + 4;

pub const NEAR: i32 = ONE / 10;
pub const FAR: i32 = fx::int(1800);
/// A run of pieces broken by fewer invisible ones than this stays one draw.
const RUN_GAP: u32 = 96;

/// A finished batch: its values; for each layer and mesh, (layer, mesh,
/// byte offset, count); and the sprite's (byte offset, count, texture).
pub type Finished = (Vec<i32>, Vec<(usize, u32, u32, u32)>, Option<(u32, u32, u32)>);

/// Shapes rebuilt each frame, sorted by layer and mesh.
#[derive(Debug, Clone)]
pub struct Batch {
    lists: Vec<Vec<i32>>,
    sprite: Vec<i32>,
    sprite_texture: u32,
}

fn slot(layer: usize, mesh: Mesh) -> usize {
    layer * Mesh::ALL.len() + mesh as usize
}

pub fn instance(a: &Affine, rgb: Rgb, style: i32) -> [i32; INSTANCE_VALUES] {
    [
        a.c0.x, a.c0.y, a.c0.z, a.t.x, a.c1.x, a.c1.y, a.c1.z, a.t.y, a.c2.x, a.c2.y, a.c2.z, a.t.z, rgb[0],
        rgb[1], rgb[2], style,
    ]
}

impl Default for Batch {
    fn default() -> Self {
        Self::new()
    }
}

impl Batch {
    pub fn new() -> Batch {
        Batch {
            lists: vec![Vec::new(); LAYERS * Mesh::ALL.len()],
            sprite: Vec::new(),
            sprite_texture: 0,
        }
    }

    pub fn push(&mut self, layer: usize, mesh: Mesh, a: &Affine, rgb: Rgb, style: i32) {
        self.lists[slot(layer, mesh)].extend_from_slice(&instance(a, rgb, style));
    }

    /// The pursuer's picture, on the card mesh, with its texture.
    pub fn sprite(&mut self, a: &Affine, texture: u32) {
        self.sprite.extend_from_slice(&instance(a, [ONE; 3], 0));
        self.sprite_texture = texture;
    }

    /// The values and, for each layer and mesh, where its instances start
    /// (bytes) and how many there are; and the sprite's.
    pub fn finish(self) -> Finished {
        let mut values = Vec::new();
        let mut runs = Vec::new();
        for layer in 0..LAYERS {
            for mesh in Mesh::ALL {
                let v = &self.lists[slot(layer, mesh)];
                if v.is_empty() {
                    continue;
                }
                let offset = (values.len() * BYTES_PER_VALUE) as u32;
                runs.push((layer, mesh as u32, offset, (v.len() / INSTANCE_VALUES) as u32));
                values.extend_from_slice(v);
            }
        }
        let sprite = if self.sprite.is_empty() {
            None
        } else {
            let offset = (values.len() * BYTES_PER_VALUE) as u32;
            let n = (self.sprite.len() / INSTANCE_VALUES) as u32;
            values.extend_from_slice(&self.sprite);
            Some((offset, n, self.sprite_texture))
        };
        (values, runs, sprite)
    }
}

/// A camera: where it is, where it looks, which way is up, its vertical
/// field of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    pub eye: V3,
    pub fwd: V3,
    pub up: V3,
    pub fov: i32,
}

impl Camera {
    pub fn matrix(&self, aspect: i32, far: i32, mirror: bool) -> Mat4 {
        let mut p = Mat4::perspective(self.fov, aspect, NEAR, far);
        if mirror {
            p.0[0] = -p.0[0];
        }
        p.mul(&Mat4::look(self.eye, self.fwd, self.up))
    }

    /// Where a world point lands, as percent (Q16) from the left and the top
    /// of the view; `None` behind the camera.
    pub fn screen(&self, aspect: i32, p: V3) -> Option<(i32, i32)> {
        let [x, y, _, w] = self.matrix(aspect, FAR, false).project(p);
        if w <= ONE / 100 {
            return None;
        }
        Some(((fx::div(x, w) + ONE) * 50, (ONE - fx::div(y, w)) * 50))
    }
}

/// One view on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// pixels from the bottom left
    pub rect: [i32; 4],
    pub mirror: bool,
    pub cam: Camera,
    pub far: i32,
    /// draw outlines (the windscreen does; the mirrors skip them)
    pub ink: bool,
    pub layers: [bool; LAYERS],
}

impl View {
    pub fn matrix(&self) -> Mat4 {
        let aspect = fx::div(self.rect[2].max(1), self.rect[3].max(1));
        self.cam.matrix(aspect, self.far, self.mirror)
    }

    pub fn mat_values(&self) -> [i32; MAT_VALUES] {
        let m = self.matrix().0;
        let mut out = [0; MAT_VALUES];
        out[..16].copy_from_slice(&m);
        out[16] = self.cam.eye.x;
        out[17] = self.cam.eye.y;
        out[18] = self.cam.eye.z;
        out
    }
}

/// The six planes of a view's frustum, each `(a, b, c, d)` with the inside
/// where `a·x + b·y + c·z + d ≥ 0` (Gribb and Hartmann).
pub fn planes(m: &Mat4) -> [[i64; 4]; 6] {
    let r = |i: usize| {
        [
            m.0[i] as i64,
            m.0[4 + i] as i64,
            m.0[8 + i] as i64,
            m.0[12 + i] as i64,
        ]
    };
    let (r0, r1, r2, r3) = (r(0), r(1), r(2), r(3));
    let add = |a: [i64; 4], b: [i64; 4], s: i64| {
        [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2], a[3] + s * b[3]]
    };
    [
        add(r3, r0, 1),
        add(r3, r0, -1),
        add(r3, r1, 1),
        add(r3, r1, -1),
        add(r3, r2, 1),
        add(r3, r2, -1),
    ]
}

/// Whether a box can be in view: no plane has it wholly outside.
pub fn visible(pl: &[[i64; 4]; 6], lo: V3, hi: V3) -> bool {
    pl.iter().all(|p| {
        let x = if p[0] >= 0 { hi.x } else { lo.x } as i64;
        let y = if p[1] >= 0 { hi.y } else { lo.y } as i64;
        let z = if p[2] >= 0 { hi.z } else { lo.z } as i64;
        p[0] * x + p[1] * y + p[2] * z + (p[3] << 16) >= 0
    })
}

/// The runs of piece slots a view must draw: whole groups in view, merged
/// across small gaps. Each run is (byte offset, count).
pub fn piece_runs(groups: &[Group], view: &View) -> Vec<(u32, u32)> {
    let pl = planes(&view.matrix());
    let eye = view.cam.eye;
    let far = view.far as i64;
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for g in groups {
        // nearest point of the box to the eye, for the distance cut
        let nx = eye.x.clamp(g.lo.x, g.hi.x);
        let nz = eye.z.clamp(g.lo.z, g.hi.z);
        let dx = (nx - eye.x) as i64;
        let dz = (nz - eye.z) as i64;
        if dx * dx + dz * dz > far * far || !visible(&pl, g.lo, g.hi) {
            continue;
        }
        match runs.last_mut() {
            Some(r) if g.start <= r.0 + r.1 + RUN_GAP => r.1 = g.end - r.0,
            _ => runs.push((g.start, g.end - g.start)),
        }
    }
    runs.into_iter()
        .map(|(s, n)| ((s as usize * INSTANCE_VALUES * BYTES_PER_VALUE) as u32, n))
        .collect()
}

/// A box standing on `at` (the middle of its foot) of a size.
pub fn standing(at: V3, size: V3) -> Affine {
    Affine::translate(at.add(v3(0, size.y / 2, 0))).then(&Affine::scale(size))
}

pub fn boxed(at: V3, size: V3) -> Affine {
    Affine::translate(at).then(&Affine::scale(size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{deg, int};
    use crate::geom::facing;

    fn view_north() -> View {
        View {
            rect: [0, 0, 1600, 900],
            mirror: false,
            cam: Camera {
                eye: v3(0, int(2), 0),
                fwd: facing(0, 0),
                up: V3::UP,
                fov: deg(70),
            },
            far: int(500),
            ink: true,
            layers: [true; LAYERS],
        }
    }

    fn group(start: u32, end: u32, at: V3) -> Group {
        Group {
            start,
            end,
            building: true,
            base_total: 1,
            base_lost: 0,
            collapsed: false,
            lo: at.sub(v3(int(5), 0, int(5))),
            hi: at.add(v3(int(5), int(20), int(5))),
        }
    }

    #[test]
    fn a_view_draws_what_is_ahead_and_skips_what_is_behind_or_too_far() {
        let v = view_north();
        let groups = [
            group(0, 200, v3(0, 0, -int(50))),
            group(200, 400, v3(0, 0, int(50))),
            group(400, 600, v3(0, 0, -int(900))),
        ];
        let runs = piece_runs(&groups, &v);
        assert_eq!(runs, vec![(0, 200)]);
    }

    #[test]
    fn neighbouring_groups_in_view_are_one_run() {
        let v = view_north();
        let groups = [
            group(0, 100, v3(0, 0, -int(50))),
            group(100, 250, v3(int(10), 0, -int(60))),
        ];
        assert_eq!(piece_runs(&groups, &v), vec![(0, 250)]);
    }

    #[test]
    fn a_mirror_flips_left_and_right() {
        let mut v = view_north();
        let left = v3(-int(10), int(2), -int(30));
        let [x, _, _, _] = v.matrix().project(left);
        assert!(x < 0);
        v.mirror = true;
        let [x, _, _, _] = v.matrix().project(left);
        assert!(x > 0);
    }

    #[test]
    fn the_batch_lays_out_layers_then_the_sprite() {
        let mut b = Batch::new();
        let a = boxed(V3::ZERO, v3(ONE, ONE, ONE));
        b.push(LAYER_COCKPIT, Mesh::Cube, &a, [ONE; 3], 0);
        b.push(LAYER_WORLD, Mesh::Cube, &a, [ONE; 3], 0);
        b.sprite(&a, 3);
        let (values, runs, sprite) = b.finish();
        assert_eq!(values.len(), 3 * INSTANCE_VALUES);
        assert_eq!(runs[0].0, LAYER_WORLD);
        assert_eq!(runs[1].0, LAYER_COCKPIT);
        assert_eq!(sprite, Some((2 * 64, 1, 3)));
    }
}

#[cfg(test)]
mod handed {
    use crate::geom::Affine;

    /// A transform whose axes are left-handed turns a mesh inside out: the
    /// shader then draws its back faces and culls its front ones.
    pub fn right_handed(a: &Affine) -> bool {
        a.c0.norm().cross(a.c1.norm()).dot(a.c2.norm()) > 0
    }
}

#[cfg(test)]
pub use handed::right_handed;
