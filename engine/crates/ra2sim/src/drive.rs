//! The game's own drive model: how a robot's grounded wheels push it.
//!
//! Robot Arena 2 does **not** drive a wheel on the ground with its hinge
//! motor. `SpinMotor::SetPercent` (`FUN_004f5800`) switches the hinge motor off
//! while the motor's wheel touches something (`Wheel::IsTouching`,
//! `FUN_00507ff0`) and only runs it — a 17 rad/s servo for the Scout — while the
//! wheel is in the air. Live, the Scout's drive hinges have their motor flag
//! off for the whole of a full-throttle run while its hubs roll at 30 rad/s
//! (`havok-step__live__018/020`).
//!
//! The push comes from a Havok action the game installs on every robot,
//! `MagicAction` (RTTI `.?AVMagicAction@@`, vtable `0x6a80f4`, apply
//! `FUN_004856f0`), run with the other actions after every Havok step. It keeps
//! one record per wheel (`AddWheel`, `FUN_00485490`, called from
//! `FUN_004dc180` for every `SpinMotor`, `AxleMount` and `Steering` wheel) and,
//! for every wheel that touches, works on the **chassis** body in its own
//! frame:
//!
//! * thrust `d · thrust · grip` along the ground, `d = -(axle × n)`, where
//!   `thrust = 2 · trunc(0.7 · torque · percent)` (`FUN_004f5780`, constant
//!   `0x6aa990`, doubled by `FUN_00485580`): the motor's raw torque and its
//!   integer command percentage;
//! * a drag `-0.5 · m (d · v_p) d / N` on the wheel point's forward speed
//!   (`DAT_006f3de8`), `N` the number of wheel records;
//! * an *impulse* at the wheel point that removes its sideways slip:
//!   `-(|v_lat,body| / |v_lat,point|) v_lat,point · resistance · m / N` —
//!   a wheel definition's `resistance` is its sideways grip;
//!
//! then, once per robot: the summed force through the centre of mass; the
//! wheels' yaw torque when it exceeds 145 N·m (`DAT_006f3df0`), otherwise an
//! angular impulse that damps the spin about the robot's up axis
//! (`0.05 · m · touching / N`); a damping force `-m v` (`-0.1 m v` with no
//! wheel down, `DAT_006f3dec`); a down-force `2 |v| |g|`; and above 10 rad/s
//! (`DAT_006a57a8`) an angular impulse that bleeds the excess spin at
//! `0.1 · m`. `m` is the chassis body's mass (`body + 0x190`).
//!
//! Decoded from the x87 code of `0x4856f0` (its vector helpers: `0x404990`
//! scale, `0x45a730` cross, `0x535940` dot, `0x401ae0` length,
//! `0x4b0a10`/`0x4812e0` impulse at a point, `0x4b0950` force accumulator
//! `+0x74`, `0x4b0860` torque accumulator `+0x84`, `0x485ec0` `ω += I⁻¹ L`),
//! and checked live: the records hold `thrust = 1960`, `grip = 1`,
//! `resistance = 0.1` for the Scout at full throttle (`magic-scout__live__003`).

use glam::Vec3;

use crate::backend::{BodyHandle, PhysicsBackend};

/// `0.7` in `thrust = 2 · trunc(0.7 · torque · percent)` (`0x6aa990`).
pub const THRUST_SCALE: f32 = 0.7;
/// The wheel drag's `0.5` (`0x6f3de8`).
pub const WHEEL_DRAG: f32 = 0.5;
/// Yaw torque that is applied as such; below it the yaw is damped instead.
pub const YAW_TORQUE_MIN: f32 = 145.0;
/// The yaw damping's `0.05`.
pub const YAW_DAMPING: f32 = 0.05;
/// The robot's linear damping with a wheel down (`0x6f3dec`) and in the air.
pub const DAMPING_GROUNDED: f32 = 1.0;
pub const DAMPING_AIRBORNE: f32 = 0.1;
/// Down-force `2 |v| |g|`.
pub const DOWNFORCE: f32 = 2.0;
/// Spin above this many rad/s is bled (`0x6a57a8`) at `0.1 · m` per step.
pub const SPIN_LIMIT: f32 = 10.0;
pub const SPIN_BLEED: f32 = 0.1;
/// A wheel stays touching this long after its phantom last touched (`0x6a6080`).
pub const TOUCH_MEMORY: f32 = 0.2;

/// One wheel record (`0x50` bytes in the original).
#[derive(Clone, Debug)]
pub struct WheelRecord {
    /// The wheel's own body (its hub) — what touches the ground.
    pub part: usize,
    /// The `SpinMotor` component driving it (`None` for an axle mount).
    pub motor: Option<usize>,
    /// Hub centre relative to the chassis centre, chassis frame (`+0x2c`).
    pub point: Vec3,
    /// The axle, chassis frame (`+0x3c`).
    pub axle: Vec3,
    pub grip: f32,
    pub resistance: f32,
    /// `+0x4`: `2 · trunc(0.7 · torque · percent)`.
    pub thrust: f32,
    /// `+0x8` and the wheel's own timer (`Wheel + 0x294`).
    pub touching: bool,
    pub air: f32,
    /// `+0xc`: the surface normal under the wheel, world space.
    pub normal: Vec3,
}

/// The robot's `MagicAction`.
#[derive(Clone, Debug, Default)]
pub struct MagicAction {
    pub wheels: Vec<WheelRecord>,
}

impl MagicAction {
    /// The thrust a motor's command gives: `torque` is the motor's raw torque
    /// (the definition's, before the engine's ×40), `percent` its integer
    /// command.
    pub fn thrust(torque: f32, percent: i32) -> f32 {
        2.0 * (torque * percent as f32 * THRUST_SCALE).trunc()
    }

    /// Refresh a wheel's touching flag from what its phantom saw this step
    /// (`FUN_00507f90` counts, `FUN_00508080` lets the flag go 0.2 s after the
    /// last touch).
    pub fn touch(record: &mut WheelRecord, touch: Option<Vec3>, dt: f32) {
        match touch {
            Some(n) => {
                record.touching = true;
                record.air = 0.0;
                record.normal = n;
            }
            None => {
                record.air += dt;
                if record.air > TOUCH_MEMORY {
                    record.touching = false;
                }
            }
        }
    }

    /// One application on the chassis body (`FUN_004856f0`).
    pub fn apply(&self, physics: &mut dyn PhysicsBackend, chassis: BodyHandle) {
        let (Some(s), Some(com), Some(m)) = (
            physics.state(chassis),
            physics.center_of_mass(chassis),
            physics.mass(chassis),
        ) else {
            return;
        };
        if self.wheels.is_empty() {
            return;
        }
        let rot = s.rotation;
        let inv = rot.inverse();
        let v_local = inv * s.linear_velocity;
        let w_local = inv * s.angular_velocity;
        let n = self.wheels.len() as f32;
        let mut force = Vec3::ZERO;
        let mut yaw = 0.0f32;
        let mut down = 0usize;
        for w in self.wheels.iter().filter(|w| w.touching) {
            down += 1;
            let at = com + rot * w.point;
            let vp = inv * (s.linear_velocity + s.angular_velocity.cross(at - com));
            let normal = inv * w.normal;
            let d = (-w.axle.cross(normal)).normalize_or_zero();
            let drive = d * (w.thrust * w.grip);
            yaw += w.point.cross(drive).y;
            let drag = -(d * d.dot(vp)) * (WHEEL_DRAG * m / n);
            let slip = w.axle * w.axle.dot(vp);
            let body_slip = w.axle * w.axle.dot(v_local);
            let ratio = if slip.length() == 0.0 {
                1.0
            } else {
                body_slip.length() / slip.length()
            };
            let impulse = -(slip * ratio) * (w.resistance * m / n);
            physics.apply_impulse_at(chassis, rot * impulse, at);
            force += drive + drag;
        }
        physics.add_force(chassis, rot * force);
        let up = rot * Vec3::Y;
        if yaw.abs() > YAW_TORQUE_MIN {
            physics.add_torque(chassis, Vec3::new(0.0, yaw, 0.0));
        } else if down > 0 {
            if let Some(now) = physics.state(chassis) {
                let spin = now.angular_velocity.dot(up);
                physics.apply_angular_impulse(
                    chassis,
                    -up * (spin * YAW_DAMPING * m * down as f32 / n),
                );
            }
        }
        let damping = if down == 0 {
            DAMPING_AIRBORNE
        } else {
            DAMPING_GROUNDED
        };
        physics.add_force(chassis, rot * (-v_local * (damping * m)));
        let g = physics.gravity().length();
        physics.add_force(
            chassis,
            Vec3::new(0.0, -DOWNFORCE * v_local.length() * g, 0.0),
        );
        if w_local.length() > SPIN_LIMIT && down > 0 {
            if let Some(now) = physics.state(chassis) {
                let w = now.angular_velocity;
                let excess = w - w.normalize_or_zero() * SPIN_LIMIT;
                physics.apply_angular_impulse(chassis, -excess * (m * SPIN_BLEED));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scouts_thrust_is_the_live_record() {
        // Raw torque 14, full throttle: the live record reads 1960
        // (magic-scout__live__003); 99 % reads 1940 (magic-scout__live__000).
        assert_eq!(MagicAction::thrust(14.0, 100), 1960.0);
        assert_eq!(MagicAction::thrust(14.0, 99), 1940.0);
        assert_eq!(MagicAction::thrust(14.0, -99), -1940.0);
    }

    #[test]
    fn a_wheel_stays_down_for_a_fifth_of_a_second() {
        let mut w = WheelRecord {
            part: 1,
            motor: None,
            point: Vec3::ZERO,
            axle: Vec3::X,
            grip: 1.0,
            resistance: 0.1,
            thrust: 0.0,
            touching: false,
            air: 1.0,
            normal: Vec3::Y,
        };
        MagicAction::touch(&mut w, Some(Vec3::Y), 1.0 / 60.0);
        assert!(w.touching);
        for _ in 0..11 {
            MagicAction::touch(&mut w, None, 1.0 / 60.0);
        }
        assert!(w.touching, "0.18 s");
        MagicAction::touch(&mut w, None, 1.0 / 60.0);
        MagicAction::touch(&mut w, None, 1.0 / 60.0);
        assert!(!w.touching, "0.22 s");
    }
}
