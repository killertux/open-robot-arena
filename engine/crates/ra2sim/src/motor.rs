//! How the original turns a motor's percentage, speed and torque into the
//! numbers its Havok constraint motor runs on (docs/29 section 12).
//!
//! Every robot hinge (a spin motor's, a wheel's, a burst motor's) and every
//! arena hinge is driven by a *hinge controller* (`HingeController`, vtable
//! `0x6a7e44`), and every piston and arena slider by a *prismatic controller*
//! (vtable `0x6a91b8`). Both run once per Havok substep (their update is a
//! world action, called from `0x531030`; measured live: 302 world steps and
//! `11 x 302` hinge updates in 5 s).
//!
//! **Hinge controller.** `SetPowerSettings(S, T)` (`0x4810e0`) stores the speed
//! `S` and the raw torque `T` (`+0xb0`/`+0xb4`) and the motor's speed and
//! maximum torque `S` and `40 T` (`+0xbc`/`+0xb8`). A robot then *caps* the last
//! pair by the load each hinge carries ([`HingeSettings::load_cap`], vtable slot
//! 10 = `0x480510`, called for every controller of the robot by `0x4dc390`
//! whenever a component is added while it is built, and again when one comes
//! off: `0x4dbe3f`, `0x417c1d`). The update (`0x481450`) runs the Havok motor
//! at velocity `trunc(-(p · 0.01 · speed))` (sign-corrected) with the maximum
//! torque, `p` the controller's integer percentage (`+0x7c`): an x87 product
//! truncated by `__ftol`, so `0.01f` (just under a hundredth) makes a full
//! command one rad/s *short* of an integer speed — live the Scout's drive hinge
//! (speed 18) runs at 17, the Octagon's hammer (95) at 94, the Scout's fork
//! firing (50) at 49. A zero percentage turns the motor off (or locks the
//! hinge, when it auto-locks).
//!
//! **Prismatic controller.** `SetPowerSettings(S, F)` (`0x4b2af0`) stores the
//! raw pair (`+0x74`/`+0x78`, no ×40), and the update (`0x4b5240`, skipped while
//! the slider is locked) is a *force*, not a motor: while the slider is short of
//! the stop it is driven toward, it pushes the rod with `-(dir · F)` along its
//! axis (`0x4b51b0`), `dir` the signed direction (`+0x80`, the scripts' `±100`
//! or `±1`) — unless the rod already moves the commanded way at `S` or more.

use glam::Vec3;

/// `SetPowerSettings`' torque unit (`0x4810e0`: `+0xb8 = 40 · T`).
pub const TORQUE_UNIT: f32 = 40.0;

/// The hinge update's percentage scale, `0.01f` (`DAT_006a69cc`).
const PERCENT: f32 = 0.01;

/// The load cap's torque multiple (`DAT_006f3b4c`).
const CAP_TORQUE: f32 = 75.0;

/// The load cap's newton-metres per kilogram (`DAT_006f3b50`).
const CAP_PER_KG: f32 = 25.0;

/// The motor velocity a hinge controller asks for at an integer percentage:
/// `trunc(percent · 0.01f · speed)`, in extended precision as the x87 code
/// computes it (`0x4814ca..0x4814dc`).
pub fn hinge_velocity(percent: i32, speed: f32) -> f32 {
    (f64::from(percent) * f64::from(PERCENT) * f64::from(speed)).trunc() as f32
}

/// A hinge controller's power settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HingeSettings {
    /// `SetPowerSettings`' speed (`+0xb0`).
    pub speed: f32,
    /// `SetPowerSettings`' raw torque (`+0xb4`).
    pub torque: f32,
    /// The motor's maximum torque in N·m (`+0xb8`).
    pub max_torque: f32,
    /// The speed the percentage scales (`+0xbc`).
    pub motor_speed: f32,
}

impl HingeSettings {
    /// `SetPowerSettings(speed, torque)`.
    pub fn new(speed: f32, torque: f32) -> Self {
        HingeSettings {
            speed,
            torque,
            max_torque: torque * TORQUE_UNIT,
            motor_speed: speed,
        }
    }

    /// `SetPowerSettings` again, as a burst motor does each time it fires and
    /// cocks: the load cap is gone until the robot caps its hinges again.
    pub fn set_power(&mut self, speed: f32, torque: f32) {
        *self = HingeSettings::new(speed, torque);
    }

    /// Cap the motor by the body it drives (`0x480510`): `m` is that body's
    /// mass (the hinge's body B, the child) and `d` its pivot's offset along the
    /// hinge's reference perpendicular ([`load_offset`]).
    ///
    /// * maximum torque `min(75 T, 25 m (1 - |d|))`, or `75 T` when
    ///   `m (1 - |d|) <= 0`;
    /// * speed `S · min(1, 2 T / (m (1 + |d|)))`.
    ///
    /// Live (`hingectl-scout-alarm-box__live__000`): the Scout's drive hinges
    /// (`18 14`, 27 kg wheel bodies, `d = 0`) read `675` and `18`; Tornado's
    /// spinner (`36 48`, 125 kg) `3124` and `27.6` (the motor at 27 rad/s);
    /// Ripblade's (`36 24`, 53 kg) `1324.5` and `32.6`.
    pub fn load_cap(&mut self, mass: f32, d: f32) {
        let d = d.abs();
        let t = self.torque;
        self.max_torque = CAP_TORQUE * t;
        let lever = mass * (1.0 - d);
        if lever > 0.0 && self.max_torque / lever > CAP_PER_KG {
            self.max_torque = CAP_PER_KG * lever;
        }
        let ratio = 2.0 * t / (mass * (1.0 + d));
        self.motor_speed = self.speed * if ratio.is_nan() { 1.0 } else { ratio.min(1.0) };
    }

    /// The motor velocity at an integer percentage ([`hinge_velocity`]).
    pub fn velocity(&self, percent: i32) -> f32 {
        hinge_velocity(percent, self.motor_speed)
    }
}

/// The hinge's reference perpendicular (`0x525830`, building the constraint):
/// `axis × X` when `|axis.x| < 0.57` (`DAT_006abfc8`, a double), else
/// `axis × Y`, normalised. Taken in the robot's own frame.
pub fn reference_perpendicular(axis: Vec3) -> Vec3 {
    let p = if axis.x.abs() < 0.57 {
        axis.cross(Vec3::X)
    } else {
        axis.cross(Vec3::Y)
    };
    p.normalize_or_zero()
}

/// The load cap's `d` (`0x4805d2..0x4805f1`): the hinge's pivot in body B's
/// frame, measured from B's centre of mass, along the reference perpendicular
/// in B's frame. Live: zero for a wheel (its pivot is on the axle), `0.288` for
/// the Scout's fork arm (pivot `(0.053, -0.294, 0.002)`, perpendicular
/// `(-0.087, -0.996, 0)`).
pub fn load_offset(pivot_b: Vec3, com_b: Vec3, perp_b: Vec3) -> f32 {
    (pivot_b - com_b).dot(perp_b)
}

/// A prismatic controller's power settings (`0x4b2af0`, raw: no ×40).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrismaticSettings {
    /// The rod's speed limit (m/s, `+0x74`).
    pub speed: f32,
    /// The force per unit of direction (`+0x78`).
    pub force: f32,
}

impl PrismaticSettings {
    /// The push this substep (`0x4b5240`), `dir · F` in the sense a positive
    /// direction drives the rod (the controller applies it as `-(dir · F)`
    /// along the slider's axis), or `None` when it pushes nothing: `dir` is the
    /// signed direction, `along` the rod's velocity relative to the housing in
    /// that same sense, `rel_speed` the length of the relative velocity. A rod
    /// already moving the commanded way at the speed limit is not pushed.
    pub fn force(&self, dir: f32, along: f32, rel_speed: f32) -> Option<f32> {
        if dir == 0.0 {
            return None;
        }
        // Moving the commanded way at the speed limit or faster: no push.
        let with = (dir > 0.0 && along > 0.0) || (dir < 0.0 && along < 0.0);
        if with && rel_speed >= self.speed {
            return None;
        }
        Some(dir * self.force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_command_runs_one_short_of_an_integer_speed() {
        // Live: the Scout's drive hinge (18) at 17, the Octagon's hammer (95) at
        // 94 and its retract (3) at -2, the fork firing (50) at 49.
        assert_eq!(hinge_velocity(100, 18.0), 17.0);
        assert_eq!(hinge_velocity(99, 18.0), 17.0);
        assert_eq!(hinge_velocity(100, 95.0), 94.0);
        assert_eq!(hinge_velocity(-100, 3.0), -2.0);
        assert_eq!(hinge_velocity(100, 50.0 * 0.999_922), 49.0);
        assert_eq!(hinge_velocity(0, 18.0), 0.0);
        assert_eq!(hinge_velocity(50, 18.0), 8.0);
    }

    #[test]
    fn the_load_cap_reads_as_the_live_controllers() {
        // Scout drive: 18 14 on a 27 kg wheel body.
        let mut s = HingeSettings::new(18.0, 14.0);
        assert_eq!(s.max_torque, 560.0);
        s.load_cap(27.0, 0.0);
        assert!((s.max_torque - 675.0).abs() < 1e-3);
        assert_eq!(s.motor_speed, 18.0);
        assert_eq!(s.velocity(100), 17.0);
        // Tornado's spinner: 36 48 on 125 kg.
        let mut s = HingeSettings::new(36.0, 48.0);
        s.load_cap(125.0, 0.0);
        assert!((s.max_torque - 3125.0).abs() < 1e-2);
        assert!((s.motor_speed - 27.648).abs() < 1e-3);
        assert_eq!(s.velocity(100), 27.0);
        // Ripblade's: 36 24 on 53 kg.
        let mut s = HingeSettings::new(36.0, 24.0);
        s.load_cap(53.0, 0.0);
        assert!((s.max_torque - 1325.0).abs() < 1e-2);
        assert_eq!(s.velocity(-100), -32.0);
        // The Scout's fork arm, 26 kg with the pivot 0.288 off: 462.4.
        let mut s = HingeSettings::new(5.0, 35.0);
        s.load_cap(26.0, 0.2885);
        assert!((s.max_torque - 462.4).abs() < 0.5, "{}", s.max_torque);
        // A light body keeps 75 T.
        let mut s = HingeSettings::new(5.0, 1.0);
        s.load_cap(26.0, 0.0);
        assert_eq!(s.max_torque, 75.0);
        // A body pivoted a metre or more off its centre is not capped.
        let mut s = HingeSettings::new(5.0, 35.0);
        s.load_cap(26.0, 1.2);
        assert_eq!(s.max_torque, 75.0 * 35.0);
        // SetPowerSettings drops the cap again.
        s.set_power(50.0, 90.0);
        assert_eq!((s.max_torque, s.motor_speed), (3600.0, 50.0));
    }

    #[test]
    fn the_reference_perpendicular_follows_the_axis() {
        // Live: a wheel axle along -X has (0, 0, -1).
        assert_eq!(reference_perpendicular(-Vec3::X), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(reference_perpendicular(Vec3::X * 0.8), Vec3::Z);
        // An axle along Z, or a vertical spinner, takes `axis x X`.
        assert_eq!(reference_perpendicular(Vec3::Z), Vec3::Y);
        assert_eq!(reference_perpendicular(Vec3::Y), -Vec3::Z);
    }

    #[test]
    fn a_prismatic_pushes_until_it_moves_at_its_speed() {
        let s = PrismaticSettings {
            speed: 60.0,
            force: 100.0,
        };
        assert_eq!(s.force(100.0, 0.0, 0.0), Some(10_000.0));
        assert_eq!(s.force(100.0, 59.0, 59.0), Some(10_000.0));
        assert_eq!(s.force(100.0, 61.0, 61.0), None);
        // Moving the other way is always pushed against.
        assert_eq!(s.force(100.0, -70.0, 70.0), Some(10_000.0));
        assert_eq!(s.force(-100.0, -70.0, 70.0), None);
        assert_eq!(s.force(0.0, 0.0, 0.0), None);
    }
}
