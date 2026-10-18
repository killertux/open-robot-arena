//! The immobility rule: a robot that sits still is eventually counted out.
//!
//! `FUN_00412fc0` (match update, block `0x4133b8`) keeps one tracker per robot
//! (`FUN_00413900`): an anchor position and a time. Once per second of fight —
//! the gate `FUN_004138c0` accumulates `dt` and fires when it reaches `1.0` —
//! every robot still in state 0 is sampled:
//!
//! * if it is farther than sqrt(2) m from its anchor (`dist² > 2.0`) the time
//!   resets and the anchor moves to where it is; otherwise the time grows by 1;
//! * under 10 s nothing happens (a standing warning is withdrawn);
//! * at 10 s the warning starts (`ImmobilityWarning(id, 1)` on every AI);
//! * from the next sample on, at 20 s or more, the robot is IMMOBILIZED
//!   (state 2) by `FUN_00418040`.
//!
//! Only position counts: spinning wheels or being upside down is not checked.

use glam::Vec3;

/// Seconds between samples (`1.0` at `0x6a57a4`).
pub const SAMPLE_PERIOD: f32 = 1.0;
/// `dist²` beyond which the robot has moved (`2.0` at `0x6a55f8`).
pub const MOVED_DIST_SQ: f32 = 2.0;
/// Seconds without moving before the warning (`10.0` at `0x6a57a8`).
pub const WARN_AFTER: f32 = 10.0;
/// Seconds without moving before the robot is immobilized (`20.0` at `0x6a57f0`).
pub const IMMOBILIZE_AFTER: f32 = 20.0;

#[derive(Clone, Copy, Debug)]
struct Entry {
    anchor: Vec3,
    time: f32,
    warned: bool,
}

/// What a sample decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `ImmobilityWarning(robot, on)`.
    Warning(usize, bool),
    /// The robot enters state 2.
    Immobilized(usize),
}

/// The per-robot trackers and the one-second gate.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    acc: f32,
    entries: Vec<Option<Entry>>,
}

impl Tracker {
    /// Advance by `dt`; `positions[i]` is `None` for a robot not in state 0.
    pub fn update(&mut self, dt: f32, positions: &[Option<Vec3>]) -> Vec<Event> {
        self.acc += dt;
        if self.acc < SAMPLE_PERIOD {
            return Vec::new();
        }
        while self.acc >= SAMPLE_PERIOD {
            self.acc -= SAMPLE_PERIOD;
        }
        if self.entries.len() < positions.len() {
            self.entries.resize(positions.len(), None);
        }
        let mut out = Vec::new();
        for (id, pos) in positions.iter().enumerate() {
            let Some(pos) = *pos else {
                continue;
            };
            let entry = match &mut self.entries[id] {
                Some(e) => {
                    let d = pos - e.anchor;
                    if d.dot(d) > MOVED_DIST_SQ {
                        e.time = 0.0;
                        e.anchor = pos;
                    } else {
                        e.time += SAMPLE_PERIOD;
                    }
                    e
                }
                slot => {
                    // A new tracker starts at the robot's position, time 0.
                    *slot = Some(Entry {
                        anchor: pos,
                        time: 0.0,
                        warned: false,
                    });
                    slot.as_mut().expect("just set")
                }
            };
            if entry.time < WARN_AFTER {
                if entry.warned {
                    entry.warned = false;
                    out.push(Event::Warning(id, false));
                }
            } else if !entry.warned {
                entry.warned = true;
                out.push(Event::Warning(id, true));
            } else if entry.time >= IMMOBILIZE_AFTER {
                out.push(Event::Immobilized(id));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `seconds` one-second samples of a robot standing at `pos`.
    fn sit(t: &mut Tracker, seconds: usize, pos: Vec3) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..seconds {
            all.extend(t.update(1.0, &[Some(pos)]));
        }
        all
    }

    #[test]
    fn sampling_is_once_per_second() {
        let mut t = Tracker::default();
        // Ten 0.1 s steps make one sample; nine make none.
        for _ in 0..9 {
            assert!(t.update(0.1, &[Some(Vec3::ZERO)]).is_empty());
        }
        assert_eq!(t.acc.floor(), 0.0);
        t.update(0.11, &[Some(Vec3::ZERO)]);
        assert!(t.entries[0].is_some(), "the sample created the tracker");
    }

    #[test]
    fn a_robot_that_sits_is_warned_at_ten_and_immobilized_at_twenty() {
        let mut t = Tracker::default();
        // Sample 1 creates the tracker (time 0); sample 11 reaches time 10.
        let events = sit(&mut t, 10, Vec3::ZERO);
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(sit(&mut t, 1, Vec3::ZERO), vec![Event::Warning(0, true)]);
        // Times 11..=19: nothing more.
        assert!(sit(&mut t, 9, Vec3::ZERO).is_empty());
        // Time 20: counted out.
        assert_eq!(sit(&mut t, 1, Vec3::ZERO), vec![Event::Immobilized(0)]);
    }

    #[test]
    fn moving_more_than_root_two_resets_and_withdraws_the_warning() {
        let mut t = Tracker::default();
        sit(&mut t, 11, Vec3::ZERO);
        // 1.4 m is inside the sphere (1.96 < 2): still sitting.
        assert!(sit(&mut t, 1, Vec3::new(1.4, 0.0, 0.0)).is_empty());
        // 1.5 m is outside (2.25 > 2): reset, warning withdrawn.
        assert_eq!(
            sit(&mut t, 1, Vec3::new(1.5, 0.0, 0.0)),
            vec![Event::Warning(0, false)]
        );
        // The anchor moved with it, so it must sit another 10 s.
        assert!(sit(&mut t, 9, Vec3::new(1.5, 0.0, 0.0)).is_empty());
        assert_eq!(
            sit(&mut t, 1, Vec3::new(1.5, 0.0, 0.0)),
            vec![Event::Warning(0, true)]
        );
    }

    #[test]
    fn robots_out_of_state_zero_are_not_sampled() {
        let mut t = Tracker::default();
        for _ in 0..30 {
            assert!(t.update(1.0, &[None, Some(Vec3::ZERO)]).len() <= 1);
        }
        assert!(t.entries[0].is_none());
    }
}
