//! Havok 1.x's angular dashpot (`GM_HavokAngularDashpot`), an action.
//!
//! Recovered from the action's apply (`0x52cc00`, the class's vtable at
//! `0x6ac0cc`; the `.gmf` reader is `0x5ca940` over the base dashpot's
//! `0x5ca1b0`) and verified live on FlexTop's table
//! (`flextable-scout-alarm-flextop__live__000`, docs/29 section 12j). Each call:
//!
//! * `f = 200 · h`, `h` the time since the last call (the step piece: live
//!   `1/60` per step, less for a backstepped piece);
//! * the error rotation `q = q_A · rest · q_B⁻¹` as a rotation vector
//!   `θ n` (`2 acos |w|`, the shortest way round);
//! * the angular impulse `J = f (strength · θ n + damping · (ω_A - ω_B))`,
//!   `-J` to A and `+J` to B (`ω += I_world⁻¹ J`), each only when it is a
//!   movable body.
//!
//! Live, a free table tilted 0.135 rad gains 0.056 rad/s per step from its two
//! dashpots (strength 1000, damping 0.08, inertia 15551 kg·m²): this formula
//! predicts it to 3 % (the rest is gravity on a centre 0.88 m above the pivot).

use glam::{Quat, Vec3};

use super::body::Body;

/// The apply's time factor (`DAT_006ac0f0`).
pub const TIME_FACTOR: f32 = 200.0;

#[derive(Clone, Debug)]
pub struct AngularDashpot {
    pub a: usize,
    pub b: usize,
    /// B's orientation relative to A that the dashpot holds.
    pub rest: Quat,
    pub strength: f32,
    pub damping: f32,
}

/// The rotation vector of `q`, the short way round.
fn rotation_vector(q: Quat) -> Vec3 {
    let q = if q.w < 0.0 { -q } else { q };
    let v = Vec3::new(q.x, q.y, q.z);
    let s = v.length();
    // `0x4f80b0`: 2 acos |w|; below `DAT_006ac0e8` (0.001) no rotation.
    let angle = 2.0 * q.w.clamp(-1.0, 1.0).acos();
    if angle.abs() <= 1e-3 || s <= 1e-9 {
        return Vec3::ZERO;
    }
    v / s * angle
}

impl AngularDashpot {
    pub fn apply(&self, bodies: &mut [Body], h: f32) {
        let (Some(a), Some(b)) = (bodies.get(self.a), bodies.get(self.b)) else {
            return;
        };
        if !a.alive || !b.alive || (!a.is_dynamic() && !b.is_dynamic()) {
            return;
        }
        let f = TIME_FACTOR * h;
        let target = a.rotation * self.rest;
        let error = rotation_vector(target * b.rotation.inverse());
        let rel = a.angular_velocity - b.angular_velocity;
        let j = f * (self.strength * error + self.damping * rel);
        if bodies[self.a].is_dynamic() {
            let ba = &mut bodies[self.a];
            ba.angular_velocity -= ba.inv_inertia_world * j;
        }
        if bodies[self.b].is_dynamic() {
            let bb = &mut bodies[self.b];
            bb.angular_velocity += bb.inv_inertia_world * j;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MotionType;
    use glam::Mat3;

    fn body(motion: MotionType, inertia: f32) -> Body {
        let mut b = Body {
            alive: true,
            motion,
            sensor: false,
            group: None,
            com: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            com_local: Vec3::ZERO,
            mass: 400.0,
            inv_mass: 1.0 / 400.0,
            inv_inertia_local: Mat3::from_diagonal(Vec3::splat(1.0 / inertia)),
            inv_inertia_world: Mat3::from_diagonal(Vec3::splat(1.0 / inertia)),
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            friction: 0.4,
            restitution: 0.3,
        };
        if motion == MotionType::Static {
            b.inv_inertia_world = Mat3::ZERO;
        }
        b
    }

    /// FlexTop's table live (`flextable-scout-alarm-flextop__live__000`, step
    /// 1517): tilted by the rotation vector (0.0342, -0.0184, 0.1276) its two
    /// dashpots turn it back by (-0.0147, 0.0039, -0.0547) rad/s in one 1/60 s
    /// step (measured -0.0138, 0.0040, -0.0532 with gravity and the pivot).
    #[test]
    fn a_tilted_table_is_pulled_level_as_live() {
        let tilt = Vec3::new(0.0342, -0.0184, 0.1276);
        let mut bodies = vec![
            body(MotionType::Static, 1.0),
            body(MotionType::Dynamic, 1.0),
        ];
        bodies[1].rotation = Quat::from_scaled_axis(tilt);
        let inv = Mat3::from_diagonal(Vec3::new(1.0 / 15550.73, 1.0 / 31099.06, 1.0 / 15550.73));
        bodies[1].inv_inertia_world = inv;
        let d = AngularDashpot {
            a: 0,
            b: 1,
            rest: Quat::IDENTITY,
            strength: 1000.0,
            damping: 0.08,
        };
        d.apply(&mut bodies, 1.0 / 60.0);
        d.apply(&mut bodies, 1.0 / 60.0);
        let w = bodies[1].angular_velocity;
        let want = Vec3::new(-0.0147, 0.0039, -0.0547);
        assert!((w - want).length() < 1e-3, "{w:?} against {want:?}");
    }
}
