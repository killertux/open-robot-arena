//! Havok 1.x's deactivation, as *Robot Arena 2* runs it (docs/29 section 12t).
//!
//! A body is **active** (`+0x1c` bit 1) or not. An inactive body is not
//! integrated (its centre, orientation and velocity stay where they are, its
//! force and torque accumulators keep what they hold), no collision pair is
//! detected between two bodies neither of which is active (a fixed body is
//! never active), and the constraint solver leaves a constraint alone while one
//! of its moving bodies is inactive. The contacts already detected are still
//! resolved, and their impulses still reach an inactive body.
//!
//! **Who deactivates.** Once a frame (after the frame's second substep):
//!
//! * the world's deactivator (`0x53cad0`, listener `0x53cc00`) checks every
//!   active entity that belongs to no system, with the default thresholds;
//!   the game sets its two frequencies to 0.5 Hz (`0x4ac2e0` through
//!   `0x4acab0`/`0x4acb00`), so both checks come every 2 s, whatever the
//!   arena's `.gmf` says;
//! * every robot is a *system* (`0x5236e0` builds one, `0x533c80`, with the
//!   thresholds scaled by 10, `0x533db0`); its step (`0x533fd0` → `0x534050`)
//!   runs a short check every 0.2 s and a long one every 10 s and freezes all
//!   its bodies at once when every one of them passes (`0x534171`).
//!
//! **The check** (`CheckFreeze`, `0x522cb0`) compares the body's centre and
//! orientation with what they were at the last check that failed: a short
//! check passes when the centre moved less than `t0` and the quaternion's
//! components less than `t1 / r` (r the half diagonal of the body's bounding
//! box when it was first checked), a long check when less than `t2` and
//! `t3 / r`. A failed check takes the current pose as its new reference; the
//! first check of a body only takes the references.
//!
//! **Who wakes.** The collision detector, when a pair it detects has an
//! inactive moving body (`0x548da0` → `0x548ef0`, without touching its
//! references); the game, whenever it pushes a body (its force, torque and
//! impulse wrappers `0x4b0860`, `0x4b0950`, `0x4b0a10` wake it first) or a
//! dashpot joins an active body to an inactive one (`0x52cc00`), and a
//! frozen system once any of its bodies is active again (`0x5341cb`, all of
//! them). Those wakes go through the rigid body's `setActive(1, 1)`
//! (`0x523230`), which also forgets both references, so the next checks
//! fail. Live (`deact-scout-alarm-box-rest__live__000`): a robot at rest is
//! frozen at a frame's end, its traction action wakes the chassis in the
//! next substep's actions, and the system wakes the wheels at the next
//! frame's end: the chassis misses one integration, the other bodies two,
//! every 12-14 frames.

use glam::{Quat, Vec3};

/// The default thresholds (`0x6abdf4`, copied to every system from
/// `0x7021b4`): short centre, short orientation, long centre, long
/// orientation.
pub const THRESHOLDS: [f32; 4] = [0.01, 0.005, 0.1, 0.2];

/// A robot's system scales the thresholds by this (`0x533db0(10.0)`).
pub const SYSTEM_SCALE: f32 = 10.0;

/// A system's periods (`0x533c80`: `+0x50 = 0.2`, `+0x5c = 10`).
pub const SYSTEM_SHORT_PERIOD: f32 = 0.2;
pub const SYSTEM_LONG_PERIOD: f32 = 10.0;

/// The world deactivator's periods: 1 / 0.5 Hz each (`0x4ac2e0`).
pub const WORLD_SHORT_PERIOD: f32 = 2.0;
pub const WORLD_LONG_PERIOD: f32 = 2.0;

/// Havok's time: whole seconds and the fraction, as the world keeps it
/// (`+0x60`/`+0x64`) and the deactivators compare it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clock {
    pub secs: i32,
    pub frac: f32,
}

impl Clock {
    pub fn advance(&mut self, dt: f32) {
        self.frac += dt;
        if self.frac >= 1.0 {
            let whole = self.frac.floor();
            self.secs += whole as i32;
            self.frac -= whole;
        }
    }

    /// Seconds since `earlier`, in the deactivators' arithmetic
    /// (`(frac - frac') + (float)(secs - secs')`).
    pub fn since(&self, earlier: &Clock) -> f32 {
        (self.frac - earlier.frac) + (self.secs - earlier.secs) as f32
    }
}

/// A body's references (`+0x1fc..+0x244`). `None` stands for the position
/// `FLT_MAX` a wake writes: no check against it passes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Freeze {
    pub short: Option<(Vec3, Quat)>,
    pub long: Option<(Vec3, Quat)>,
    /// The squared half diagonal of the body's bounding box, from its first
    /// check (`+0x244`, -1 until then).
    pub radius2: Option<f32>,
}

impl Freeze {
    /// What a wake through `setActive(1, 1)` does to the references.
    pub fn forget(&mut self) {
        self.short = None;
        self.long = None;
    }

    /// One check of a body at `com`, `rotation` (`0x522cb0`). `radius2` gives
    /// the bounding box's squared half diagonal when this is the body's first
    /// check. True: the body may be frozen.
    pub fn check(
        &mut self,
        com: Vec3,
        rotation: Quat,
        radius2: impl FnOnce() -> f32,
        short: bool,
        long: bool,
        t: &[f32; 4],
    ) -> bool {
        let Some(r2) = self.radius2 else {
            self.radius2 = Some(radius2());
            self.short = Some((com, rotation));
            self.long = Some((com, rotation));
            return false;
        };
        let still = |reference: Option<(Vec3, Quat)>, tc: f32, tq: f32| match reference {
            Some((c, q)) => {
                let dq = Vec3::new(q.w - rotation.w, q.x - rotation.x, q.y - rotation.y)
                    .length_squared()
                    + (q.z - rotation.z) * (q.z - rotation.z);
                (c - com).length_squared() < tc * tc && dq * r2 < tq * tq
            }
            None => false,
        };
        if short {
            if still(self.short, t[0], t[1]) {
                return true;
            }
            self.short = Some((com, rotation));
        }
        if long {
            if still(self.long, t[2], t[3]) {
                return true;
            }
            self.long = Some((com, rotation));
        }
        false
    }
}

/// A robot's system (`0x533c80`): its bodies freeze and wake together.
#[derive(Clone, Debug)]
pub struct System {
    /// The robot's collision group id.
    pub group: u32,
    pub bodies: Vec<usize>,
    pub thresholds: [f32; 4],
    pub last_short: Clock,
    pub last_long: Clock,
    /// `+0x68`: false once the system has frozen its bodies.
    pub active: bool,
}

impl System {
    pub fn new(group: u32) -> Self {
        Self {
            group,
            bodies: Vec::new(),
            thresholds: THRESHOLDS.map(|t| t * SYSTEM_SCALE),
            last_short: Clock::default(),
            last_long: Clock::default(),
            active: true,
        }
    }
}

/// The world deactivator's two timers (`+0x28..+0x34`).
#[derive(Clone, Copy, Debug, Default)]
pub struct WorldDeactivator {
    pub last_short: Clock,
    pub last_long: Clock,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_check_only_takes_the_references() {
        let mut f = Freeze::default();
        let t = THRESHOLDS;
        assert!(!f.check(Vec3::ZERO, Quat::IDENTITY, || 1.0, true, true, &t));
        assert!(f.check(Vec3::ZERO, Quat::IDENTITY, || 1.0, true, false, &t));
    }

    #[test]
    fn a_failed_check_moves_its_reference_and_a_wake_forgets_both() {
        let mut f = Freeze::default();
        let t = THRESHOLDS.map(|x| x * SYSTEM_SCALE);
        f.check(Vec3::ZERO, Quat::IDENTITY, || 1.0, true, false, &t);
        // 0.11 m in one short period: fails, and that pose is the reference.
        let p = Vec3::new(0.11, 0.0, 0.0);
        assert!(!f.check(p, Quat::IDENTITY, || 1.0, true, false, &t));
        assert!(f.check(p + Vec3::X * 0.09, Quat::IDENTITY, || 1.0, true, false, &t));
        f.forget();
        assert!(!f.check(p, Quat::IDENTITY, || 1.0, true, true, &t));
        assert!(f.check(p, Quat::IDENTITY, || 1.0, true, false, &t));
    }

    #[test]
    fn the_orientation_counts_through_the_bounding_radius() {
        let mut f = Freeze::default();
        let t = THRESHOLDS.map(|x| x * SYSTEM_SCALE);
        // r = 1.2 m: |dq| < 0.05 / 1.2, a turn of 0.083 rad.
        f.check(Vec3::ZERO, Quat::IDENTITY, || 1.44, true, false, &t);
        assert!(f.check(
            Vec3::ZERO,
            Quat::from_rotation_y(0.08),
            || 0.0,
            true,
            false,
            &t
        ));
        assert!(!f.check(
            Vec3::ZERO,
            Quat::from_rotation_y(0.09),
            || 0.0,
            true,
            false,
            &t
        ));
    }

    #[test]
    fn the_clock_counts_as_havok_does() {
        let mut c = Clock::default();
        let start = c;
        for _ in 0..7 {
            c.advance(1.0 / 30.0);
        }
        assert!(c.since(&start) > SYSTEM_SHORT_PERIOD);
        for _ in 0..30 {
            c.advance(1.0 / 30.0);
        }
        assert_eq!(c.secs, 1);
    }
}
