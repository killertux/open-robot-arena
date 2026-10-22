//! One rigid body as Havok 1.x holds it: centre-of-mass position, orientation,
//! linear and angular velocity, and the force/torque accumulators the next
//! integration consumes.
//!
//! Field-for-field the state the integrator packs (`FUN_00520fb0`): position
//! (`+0x30`), orientation (`+0x40`), linear momentum = velocity (`+0x54`) times
//! mass (`+0x190`), angular velocity (`+0x64`); the accumulators are `+0x74`
//! (force) and `+0x84` (torque), cleared after every integration
//! (`FUN_00521030`).

use glam::{Mat3, Quat, Vec3};

use crate::backend::{BodyState, CollisionGroupDesc, MotionType};

#[derive(Clone, Debug)]
pub struct Body {
    pub alive: bool,
    pub motion: MotionType,
    pub sensor: bool,
    pub group: Option<CollisionGroupDesc>,
    /// Centre of mass, world space.
    pub com: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    /// The centre of mass in the body's own frame (from its origin).
    pub com_local: Vec3,
    pub mass: f32,
    pub inv_mass: f32,
    /// Inertia tensor about the centre of mass, body frame.
    pub inv_inertia_local: Mat3,
    /// The same, in world space (`+0x10c`), refreshed whenever the
    /// orientation changes.
    pub inv_inertia_world: Mat3,
    pub force: Vec3,
    pub torque: Vec3,
    pub friction: f32,
    pub restitution: f32,
    /// `+0x1c` bit 1: whether the body takes part in the simulation now
    /// ([`super::deactivation`]). A fixed body is never moved either way.
    pub active: bool,
    /// The deactivation check's references.
    pub freeze: super::deactivation::Freeze,
    /// The robot system the body belongs to (`+0x20`, the count of systems).
    pub system: Option<usize>,
}

impl Body {
    /// Whether the resolver and the constraints may move it. Havok's test is
    /// `((flags & ~3) | flags << 2) & 7 == 0` on the body flags (`0x53eea0`):
    /// fixed and keyframed bodies are never pushed.
    pub fn is_dynamic(&self) -> bool {
        self.alive && self.motion == MotionType::Dynamic
    }

    /// Whether the body moves at all (a keyframed body has a velocity).
    pub fn moves(&self) -> bool {
        self.alive && self.motion != MotionType::Static
    }

    /// Whether the body counts as active for the detector and the solver: a
    /// dynamic body while it is not deactivated, a keyframed body always, a
    /// fixed body never.
    pub fn awake(&self) -> bool {
        self.alive
            && match self.motion {
                MotionType::Static => false,
                MotionType::Dynamic => self.active,
                _ => true,
            }
    }

    /// A dynamic body the deactivators have frozen.
    pub fn asleep(&self) -> bool {
        self.alive && self.motion == MotionType::Dynamic && !self.active
    }

    pub fn origin(&self) -> Vec3 {
        self.com - self.rotation * self.com_local
    }

    pub fn set_origin(&mut self, origin: Vec3, rotation: Quat) {
        self.rotation = rotation.normalize();
        self.com = origin + self.rotation * self.com_local;
        self.refresh_inertia();
    }

    pub fn refresh_inertia(&mut self) {
        let r = Mat3::from_quat(self.rotation);
        self.inv_inertia_world = r * self.inv_inertia_local * r.transpose();
    }

    /// Velocity of the body's material point at world position `p`
    /// (`FUN_005207c0`); zero for a fixed body.
    pub fn point_velocity(&self, p: Vec3) -> Vec3 {
        if self.motion == MotionType::Static || !self.alive {
            return Vec3::ZERO;
        }
        self.velocity + self.angular_velocity.cross(p - self.com)
    }

    /// The impulse response at `p` (`FUN_00520930`): `K = m⁻¹ I - [r]× I⁻¹ [r]×`,
    /// the velocity change at `p` per unit impulse at `p`. Zero for a body the
    /// resolver may not move.
    pub fn collision_matrix(&self, p: Vec3) -> Mat3 {
        if !self.is_dynamic() {
            return Mat3::ZERO;
        }
        let r = skew(p - self.com);
        Mat3::IDENTITY * self.inv_mass - r * self.inv_inertia_world * r
    }

    /// Apply impulse `j` at world point `p` (`FUN_0053eea0`).
    pub fn apply_impulse_at(&mut self, j: Vec3, p: Vec3) {
        if !self.is_dynamic() {
            return;
        }
        self.velocity += j * self.inv_mass;
        self.angular_velocity += self.inv_inertia_world * (p - self.com).cross(j);
    }

    pub fn apply_angular_impulse(&mut self, l: Vec3) {
        if !self.is_dynamic() {
            return;
        }
        self.angular_velocity += self.inv_inertia_world * l;
    }

    pub fn state(&self) -> BodyState {
        BodyState {
            position: self.origin(),
            rotation: self.rotation,
            linear_velocity: self.velocity,
            angular_velocity: self.angular_velocity,
        }
    }
}

/// `[v]×`, the matrix of `x -> v × x`.
pub fn skew(v: Vec3) -> Mat3 {
    Mat3::from_cols(
        Vec3::new(0.0, v.z, -v.y),
        Vec3::new(-v.z, 0.0, v.x),
        Vec3::new(v.y, -v.x, 0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skew_is_the_cross_product() {
        let (a, b) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(-0.5, 0.25, 2.0));
        assert!((skew(a) * b - a.cross(b)).length() < 1e-6);
    }
}
