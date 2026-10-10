//! Unit quaternions in Q16.16, for the orientation of every loose piece and
//! of the car. A quaternion integrates an angular velocity without the drift
//! a rotation matrix collects, and is renormalised each step.

use crate::fx::{self, ONE};
use crate::geom::{v3, V3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quat {
    pub w: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[allow(clippy::should_implement_trait)]
impl Quat {
    pub const IDENTITY: Quat = Quat {
        w: ONE,
        x: 0,
        y: 0,
        z: 0,
    };

    /// A turn of `angle` (TURN units) about a unit axis.
    pub fn axis_angle(axis: V3, angle: i32) -> Quat {
        let (s, c) = (fx::sin(angle / 2), fx::cos(angle / 2));
        Quat {
            w: c,
            x: fx::mul(axis.x, s),
            y: fx::mul(axis.y, s),
            z: fx::mul(axis.z, s),
        }
    }

    /// A heading, as `geom::facing` reads one: zero faces −z, positive turns left.
    pub fn yaw(angle: i32) -> Quat {
        Quat::axis_angle(V3::UP, angle)
    }

    /// `self * o`: rotate by `o` first, then by `self`.
    pub fn mul(self, o: Quat) -> Quat {
        let m = fx::mul;
        Quat {
            w: m(self.w, o.w) - m(self.x, o.x) - m(self.y, o.y) - m(self.z, o.z),
            x: m(self.w, o.x) + m(self.x, o.w) + m(self.y, o.z) - m(self.z, o.y),
            y: m(self.w, o.y) - m(self.x, o.z) + m(self.y, o.w) + m(self.z, o.x),
            z: m(self.w, o.z) + m(self.x, o.y) - m(self.y, o.x) + m(self.z, o.w),
        }
    }

    pub fn norm(self) -> Quat {
        let sq = self.w as i64 * self.w as i64
            + self.x as i64 * self.x as i64
            + self.y as i64 * self.y as i64
            + self.z as i64 * self.z as i64;
        let l = fx::isqrt(sq as u64) as i32;
        if l == 0 {
            return Quat::IDENTITY;
        }
        Quat {
            w: fx::div(self.w, l),
            x: fx::div(self.x, l),
            y: fx::div(self.y, l),
            z: fx::div(self.z, l),
        }
    }

    /// One step of `dq/dt = ½ ω q` for a world-space angular velocity `w`
    /// (radians a second) over `dt` seconds.
    pub fn integrate(self, w: V3, dt: i32) -> Quat {
        let h = dt / 2;
        let o = Quat {
            w: 0,
            x: fx::mul(w.x, h),
            y: fx::mul(w.y, h),
            z: fx::mul(w.z, h),
        }
        .mul(self);
        Quat {
            w: self.w + o.w,
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z + o.z,
        }
        .norm()
    }

    /// The rotation's three columns: where local x, y and z point.
    pub fn basis(self) -> [V3; 3] {
        let m = fx::mul;
        let (w, x, y, z) = (self.w, self.x, self.y, self.z);
        let (xx, yy, zz) = (m(x, x), m(y, y), m(z, z));
        let (xy, xz, yz) = (m(x, y), m(x, z), m(y, z));
        let (wx, wy, wz) = (m(w, x), m(w, y), m(w, z));
        [
            v3(ONE - 2 * (yy + zz), 2 * (xy + wz), 2 * (xz - wy)),
            v3(2 * (xy - wz), ONE - 2 * (xx + zz), 2 * (yz + wx)),
            v3(2 * (xz + wy), 2 * (yz - wx), ONE - 2 * (xx + yy)),
        ]
    }

    pub fn rotate(self, v: V3) -> V3 {
        let b = self.basis();
        b[0].scale(v.x).add(b[1].scale(v.y)).add(b[2].scale(v.z))
    }

    /// The quaternion of an orthonormal basis (columns), Shepperd's method.
    pub fn from_basis(b: [V3; 3]) -> Quat {
        let (m00, m11, m22) = (b[0].x, b[1].y, b[2].z);
        let trace = m00 + m11 + m22;
        let half_sqrt = |v: i32| (fx::isqrt(((v.max(0)) as u64) << 16) as i32) / 2;
        let q = if trace > 0 {
            let s = half_sqrt(trace + ONE);
            let f = fx::div(ONE / 4, s);
            Quat {
                w: s,
                x: fx::mul(b[1].z - b[2].y, f),
                y: fx::mul(b[2].x - b[0].z, f),
                z: fx::mul(b[0].y - b[1].x, f),
            }
        } else if m00 >= m11 && m00 >= m22 {
            let s = half_sqrt(ONE + m00 - m11 - m22);
            let f = fx::div(ONE / 4, s);
            Quat {
                w: fx::mul(b[1].z - b[2].y, f),
                x: s,
                y: fx::mul(b[1].x + b[0].y, f),
                z: fx::mul(b[2].x + b[0].z, f),
            }
        } else if m11 >= m22 {
            let s = half_sqrt(ONE + m11 - m00 - m22);
            let f = fx::div(ONE / 4, s);
            Quat {
                w: fx::mul(b[2].x - b[0].z, f),
                x: fx::mul(b[1].x + b[0].y, f),
                y: s,
                z: fx::mul(b[2].y + b[1].z, f),
            }
        } else {
            let s = half_sqrt(ONE + m22 - m00 - m11);
            let f = fx::div(ONE / 4, s);
            Quat {
                w: fx::mul(b[0].y - b[1].x, f),
                x: fx::mul(b[2].x + b[0].z, f),
                y: fx::mul(b[2].y + b[1].z, f),
                z: s,
            }
        };
        q.norm()
    }
}

/// `v` expressed in a basis's local frame (the transpose applied).
pub fn to_local(b: &[V3; 3], v: V3) -> V3 {
    v3(b[0].dot(v), b[1].dot(v), b[2].dot(v))
}

pub fn from_local(b: &[V3; 3], v: V3) -> V3 {
    b[0].scale(v.x).add(b[1].scale(v.y)).add(b[2].scale(v.z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{deg, QUARTER};
    use crate::geom::facing;

    fn near(a: V3, b: V3, tol: i32) -> bool {
        (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol && (a.z - b.z).abs() <= tol
    }

    #[test]
    fn a_yaw_quaternion_turns_minus_z_the_way_facing_does() {
        for a in [deg(30), QUARTER, -deg(70)] {
            assert!(near(Quat::yaw(a).rotate(v3(0, 0, -ONE)), facing(a, 0), 8));
        }
    }

    #[test]
    fn integrating_a_spin_for_a_second_turns_that_many_radians() {
        // 90° a second about y is π/2 rad/s = 102 944 in Q16
        let mut q = Quat::IDENTITY;
        for _ in 0..60 {
            q = q.integrate(v3(0, 102_944, 0), fx::ratio(1, 60));
        }
        assert!(
            near(q.rotate(v3(0, 0, -ONE)), v3(-ONE, 0, 0), 600),
            "{:?}",
            q.rotate(v3(0, 0, -ONE))
        );
    }

    #[test]
    fn a_basis_round_trips_through_a_quaternion() {
        let q = Quat::axis_angle(v3(ONE, ONE, 0).norm(), deg(130)).mul(Quat::yaw(deg(40)));
        let back = Quat::from_basis(q.basis());
        let (a, b) = (q.basis(), back.basis());
        for k in 0..3 {
            assert!(near(a[k], b[k], 40), "{k}: {:?} {:?}", a[k], b[k]);
        }
    }

    #[test]
    fn local_and_world_are_inverse() {
        let b = Quat::axis_angle(v3(0, ONE, ONE).norm(), deg(77)).basis();
        let v = v3(ONE * 3, -ONE, ONE * 2);
        assert!(near(from_local(&b, to_local(&b, v)), v, 16));
    }
}
