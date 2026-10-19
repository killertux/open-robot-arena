//! The match's own sounds: impacts, the countdown, criticals and motor loops.
//!
//! These are the sounds the original makes from C++ rather than from the arena
//! script (those arrive as `plus.createSound` requests). The rules and where
//! they were recovered are in [`ra2_audio::rules`] and `docs/20-client-audio.md`:
//!
//! * **Impacts** (`FUN_0045f3a0`): every priced contact plays a random element
//!   of the *struck component's* `damagesounds` list at the contact point, at a
//!   volume that rises with the damage and is silent at 4 or less.
//! * **Countdown / critical hit**: the match object (`0x415500`) creates
//!   `match_countdown.wav`, `chat.wav`, `critical_hit.wav` and
//!   `immobile_countdown.wav` when it is built. Which event plays which is
//!   ours where the exe does not say: the countdown beeps follow the number on
//!   screen, the high beep marks the bell, and the critical hit plays on a
//!   chassis critical.
//! * **Motor loops**: a component's `motorsound` (or the middle file of its
//!   `runsequence`) loops while the motor's hinge turns. The original's pitch
//!   and volume scaling with speed was not recovered, so the loop plays at its
//!   recorded pitch.

use std::collections::HashMap;

use crate::crowd::{Crowd, CrowdEvent};
use ra2_audio::rules::impact_volume_hdb;
use ra2sim::r#match::{countdown_number, Match, MatchState};

/// One thing the match wants heard.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundCue {
    /// A contact: `path` is the definition's spelling (`sounds\x.wav`).
    Impact {
        path: String,
        position: [f32; 3],
        hundredths_db: i32,
    },
    /// A countdown beep; `go` is the bell that starts the fight.
    Beep { go: bool },
    /// A chassis critical hit.
    Critical { position: [f32; 3] },
    /// A chat line was sent.
    Chat,
    /// A robot's removal: one of the four `frc_generic` sounds.
    Force { path: &'static str },
    /// A weapon fired: its component's `firesound`.
    Fire { path: String, position: [f32; 3] },
    /// One second of an immobility warning: `immobile_countdown.wav`.
    ImmobileTick,
    /// The match is over and the crowd goes quiet: every loop fades out.
    CrowdStop,
    /// A crowd mood loop starts or stops (`crowd.rs`).
    CrowdLoop { mood: u8, on: bool },
    /// A crowd one-shot (a transition, a burst cheer, a chant).
    CrowdOneShot { path: &'static str },
    /// A looped crowd shout (a chant).
    CrowdLoopShout { path: &'static str },
    /// The crowd's looped shout fades out over `ms`.
    CrowdFadeShout { ms: f32 },
    /// A running motor's loop changes pitch (`FUN_004f5b50`).
    MotorPitch { key: u64, speed: f32 },
    /// A motor started or stopped turning.
    Motor {
        key: u64,
        on: bool,
        path: String,
        position: [f32; 3],
    },
}

/// Observes a match step by step and says what it should sound like.
#[derive(Debug)]
pub struct MatchSounds {
    rng: u64,
    last_number: Option<u32>,
    last_state: Option<MatchState>,
    criticals: Vec<u32>,
    angles: HashMap<(usize, usize), f32>,
    running: HashMap<(usize, usize), bool>,
    /// Seconds each running motor has been below the audible rate.
    quiet_for: HashMap<(usize, usize), f32>,
    crowd: Crowd,
    destroyed: Vec<bool>,
    /// The last playback rate sent for each running motor loop.
    pitch: HashMap<(usize, usize), f32>,
    /// A private copy of the immobility rule (`FUN_00412fc0`), run to know when
    /// a robot is under warning.
    immobility: ra2sim::immobility::Tracker,
    immobility_clock: f32,
    warned: Vec<bool>,
}

/// The playback rate of a motor loop at `rate` rad/s of a motor whose full
/// power rate is `full`: `FUN_004f5b50` returns `f/2 + (f - f/2) * ratio` for a
/// base frequency `f`, with `ratio = |omega| / motor.max_rate` (`FUN_004813e0`
/// over `FUN_004d33c0`), so the loop sits an octave low at rest and plays at
/// its own pitch at full rate.
pub fn motor_pitch(rate: f32, full: f32) -> f32 {
    if full <= f32::EPSILON {
        return 1.0;
    }
    0.5 + 0.5 * (rate.abs() / full)
}

/// Motor angular speed (rad/s) above which its loop sounds.
pub const MOTOR_AUDIBLE_RATE: f32 = 1.0;
/// The four sounds `FUN_00415500` builds at `match+0x110` and `FUN_00419160`
/// picks from at random (`frc_generic3.wav` is not among them).
pub const FRC_SOUNDS: [&str; 4] = [
    "Sounds/frc_generic1.wav",
    "Sounds/frc_generic2.wav",
    "Sounds/frc_generic4.wav",
    "Sounds/frc_generic5.wav",
];
/// Hinge rate (rad/s) above which a parked burst motor counts as released.
pub const BURST_FIRE_RATE: f32 = 3.0;
/// How long a motor must be still before its loop is released.
pub const MOTOR_RELEASE_SECONDS: f32 = 0.5;

impl MatchSounds {
    pub fn new() -> MatchSounds {
        MatchSounds {
            rng: 0x2545_f491_4f6c_dd1d,
            last_number: None,
            last_state: None,
            criticals: Vec::new(),
            angles: HashMap::new(),
            running: HashMap::new(),
            quiet_for: HashMap::new(),
            crowd: Crowd::new(),
            destroyed: Vec::new(),
            pitch: HashMap::new(),
            immobility: ra2sim::immobility::Tracker::default(),
            immobility_clock: 0.0,
            warned: Vec::new(),
        }
    }

    fn next(&mut self) -> u32 {
        // xorshift64*; the original picks with `rand`, so only the uniformity
        // matters.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32) as u32
    }

    /// Read what happened in the step that just ran. `matched.duel` must have
    /// `record_impacts` on; the impacts are consumed here.
    pub fn observe(&mut self, matched: &mut Match) -> Vec<SoundCue> {
        let mut cues = Vec::new();
        let state = matched.state();

        // Countdown number and the bell.
        if state == MatchState::Countdown {
            let number = countdown_number(matched.countdown_left());
            if number != self.last_number {
                if number.is_some() {
                    cues.push(SoundCue::Beep { go: false });
                }
                self.last_number = number;
            }
        }
        if state == MatchState::Fight && self.last_state == Some(MatchState::Countdown) {
            cues.push(SoundCue::Beep { go: true });
            // `FUN_00416360`: the fight begins and the crowd re-enters its mood.
            let events = self.crowd.start();
            cues.extend(events.into_iter().map(crowd_cue));
        }
        if state == MatchState::Results && self.last_state != Some(MatchState::Results) {
            // The crowd object goes with the match (`FUN_004164a0`).
            cues.push(SoundCue::CrowdStop);
        }
        self.last_state = Some(state);

        // The crowd's tick runs while the match is updating (`FUN_00412fc0`).
        if matches!(state, MatchState::Countdown | MatchState::Fight) {
            let events = self.crowd.tick(matched.config.dt);
            cues.extend(events.into_iter().map(crowd_cue));
        }

        // Impacts.
        let impacts = std::mem::take(&mut matched.duel.impacts);
        for imp in impacts {
            if state == MatchState::Fight {
                let events = self.crowd.damage(imp.damage);
                cues.extend(events.into_iter().map(crowd_cue));
            }
            let Some(hundredths_db) = impact_volume_hdb(imp.damage) else {
                continue;
            };
            let Some(victim) = matched.duel.robots.get(imp.target) else {
                continue;
            };
            let Some(component) = victim.rig.components.get(imp.component) else {
                continue;
            };
            let list = &component.sounds.damage;
            if list.is_empty() {
                continue;
            }
            let pick = (self.next() as usize) % list.len();
            cues.push(SoundCue::Impact {
                path: list[pick].clone(),
                position: [imp.point.x, imp.point.y, imp.point.z],
                hundredths_db,
            });
        }

        // A destroyed robot: `FUN_00418040` state 1 excites the crowd by 20
        // with a burst cheer.
        if self.destroyed.len() < matched.duel.robots.len() {
            self.destroyed.resize(matched.duel.robots.len(), false);
        }
        for (i, robot) in matched.duel.robots.iter().enumerate() {
            if robot.destroyed && !self.destroyed[i] && state == MatchState::Fight {
                // `FUN_00418040` (state 1: +20 and a burst cheer) and the
                // removal `FUN_0045f4e0` (+20 and a random one of the four
                // `frc_generic` sounds, `FUN_00419160`).
                let events = self.crowd.excite(20, true);
                cues.extend(events.into_iter().map(crowd_cue));
                let events = self.crowd.excite(20, false);
                cues.extend(events.into_iter().map(crowd_cue));
                let pick = (self.next() as usize) % FRC_SOUNDS.len();
                cues.push(SoundCue::Force {
                    path: FRC_SOUNDS[pick],
                });
            }
            self.destroyed[i] = robot.destroyed;
        }

        // The immobility warning's tick: once a second while a robot is warned
        // (10 s without leaving a sqrt(2) m sphere, until it is counted out at
        // 20 s). `immobile_countdown.wav` is built with the match object
        // (`0x415763`); where it is played was not found, so one tick per
        // sampled second of the warning is this client's reading of the
        // `immobility_countdown.gib` counter it accompanies.
        if state == MatchState::Fight {
            let robots = &matched.duel.robots;
            if self.warned.len() < robots.len() {
                self.warned.resize(robots.len(), false);
            }
            let positions: Vec<Option<glam::Vec3>> = robots
                .iter()
                .map(|r| {
                    r.is_alive()
                        .then(|| r.position(matched.duel.physics.as_ref()))
                })
                .collect();
            for event in self.immobility.update(matched.config.dt, &positions) {
                if let ra2sim::immobility::Event::Warning(id, on) = event {
                    if let Some(w) = self.warned.get_mut(id) {
                        *w = on;
                    }
                }
            }
            self.immobility_clock += matched.config.dt;
            if self.immobility_clock >= ra2sim::immobility::SAMPLE_PERIOD {
                self.immobility_clock -= ra2sim::immobility::SAMPLE_PERIOD;
                let alive_warned = robots
                    .iter()
                    .zip(&self.warned)
                    .any(|(r, w)| *w && r.is_alive());
                if alive_warned {
                    cues.push(SoundCue::ImmobileTick);
                }
            }
        }

        // Criticals.
        if self.criticals.len() < matched.duel.robots.len() {
            self.criticals.resize(matched.duel.robots.len(), 0);
        }
        for (i, robot) in matched.duel.robots.iter().enumerate() {
            if robot.criticals > self.criticals[i] {
                let p = robot.position(matched.duel.physics.as_ref());
                cues.push(SoundCue::Critical {
                    position: [p.x, p.y, p.z],
                });
            }
            self.criticals[i] = robot.criticals;
        }

        // Motor loops.
        let dt = matched.config.dt.max(1e-4);
        let mut motor_cues = Vec::new();
        for (ri, robot) in matched.duel.robots.iter().enumerate() {
            for (pi, part) in robot.rig.parts.iter().enumerate() {
                let (Some(joint), Some(state)) = (part.joint.as_ref(), robot.parts.get(pi)) else {
                    continue;
                };
                let Some(handle) = state.joint else { continue };
                if joint.slider {
                    continue;
                }
                let motor = &robot.rig.components[joint.motor];
                if joint.use_kind == ra2sim::rig::JointUse::Burst {
                    // A burst motor plays its `firesound` when it is released:
                    // the swing from its parked angle shows as a sudden rate.
                    let angle = matched.duel.physics.hinge_angle(handle);
                    let prev = self.angles.insert((ri, pi), angle).unwrap_or(angle);
                    let mut d = angle - prev;
                    if d > std::f32::consts::PI {
                        d -= std::f32::consts::TAU;
                    } else if d < -std::f32::consts::PI {
                        d += std::f32::consts::TAU;
                    }
                    let swinging = robot.attached[joint.motor] && (d / dt).abs() > BURST_FIRE_RATE;
                    let was = self.running.insert((ri, pi), swinging).unwrap_or(false);
                    if swinging && !was {
                        if let Some(path) = motor.sounds.fire.clone() {
                            let p = robot.position(matched.duel.physics.as_ref());
                            motor_cues.push(SoundCue::Fire {
                                path,
                                position: [p.x, p.y, p.z],
                            });
                        }
                    }
                    continue;
                }
                let sound = motor.sounds.motor.clone().or_else(|| {
                    // `runsequence = spin-up, loop, spin-down`: the loop.
                    motor.sounds.run_sequence.get(1).cloned()
                });
                let Some(path) = sound else { continue };
                let angle = matched.duel.physics.hinge_angle(handle);
                let prev = self.angles.insert((ri, pi), angle).unwrap_or(angle);
                let mut d = angle - prev;
                if d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                } else if d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                let turning = robot.attached[joint.motor]
                    && robot.is_alive()
                    && (d / dt).abs() > MOTOR_AUDIBLE_RATE;
                let was = self.running.get(&(ri, pi)).copied().unwrap_or(false);
                // A loop stops only after the motor has been quiet for a while,
                // so a turning wheel's frame-to-frame jitter is not a stutter.
                let quiet = self.quiet_for.entry((ri, pi)).or_insert(0.0);
                *quiet = if turning { 0.0 } else { *quiet + dt };
                let on = turning || (was && *quiet < MOTOR_RELEASE_SECONDS);
                self.running.insert((ri, pi), on);
                if on && was {
                    let speed = motor_pitch(d / dt, joint.rate);
                    let last = self.pitch.get(&(ri, pi)).copied().unwrap_or(1.0);
                    if (speed - last).abs() > 0.02 {
                        self.pitch.insert((ri, pi), speed);
                        motor_cues.push(SoundCue::MotorPitch {
                            key: motor_key(ri, pi),
                            speed,
                        });
                    }
                }
                if on != was {
                    self.pitch.remove(&(ri, pi));
                    let p = robot.position(matched.duel.physics.as_ref());
                    motor_cues.push(SoundCue::Motor {
                        key: motor_key(ri, pi),
                        on,
                        path,
                        position: [p.x, p.y, p.z],
                    });
                }
            }
        }
        cues.extend(motor_cues);
        cues
    }
}

impl Default for MatchSounds {
    fn default() -> Self {
        MatchSounds::new()
    }
}

fn crowd_cue(event: CrowdEvent) -> SoundCue {
    match event {
        CrowdEvent::Loop { mood, on } => SoundCue::CrowdLoop { mood, on },
        CrowdEvent::OneShot(path) => SoundCue::CrowdOneShot { path },
        CrowdEvent::LoopShout(path) => SoundCue::CrowdLoopShout { path },
        CrowdEvent::FadeShout { ms } => SoundCue::CrowdFadeShout { ms },
    }
}

/// The audio voice id of a crowd mood loop.
pub fn crowd_key(mood: u8) -> u64 {
    (2 << 48) | mood as u64
}

/// The audio voice id of the crowd's looped shout (`+0x48`).
pub fn crowd_shout_key() -> u64 {
    (2 << 48) | 0xff
}

/// The audio voice id of the `n`th crowd one-shot, from a ring of sixteen so a
/// transition, a burst cheer and a chant can overlap as they do in the original
/// (each is its own handle there).
pub fn crowd_shot_key(n: u64) -> u64 {
    (2 << 48) | (0x100 + n % 16)
}

/// The audio voice id of a robot's motor loop, clear of the script handles.
pub fn motor_key(robot: usize, part: usize) -> u64 {
    (1 << 48) | ((robot as u64) << 16) | part as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_motor_loop_sits_an_octave_low_at_rest_and_at_pitch_at_full_rate() {
        assert_eq!(motor_pitch(0.0, 10.0), 0.5);
        assert_eq!(motor_pitch(10.0, 10.0), 1.0);
        assert_eq!(motor_pitch(-5.0, 10.0), 0.75);
        // A motor with no rate keeps the file's own pitch.
        assert_eq!(motor_pitch(3.0, 0.0), 1.0);
    }

    #[test]
    fn crowd_voice_ids_stay_clear_of_the_script_and_motor_handles() {
        assert_ne!(crowd_key(0), motor_key(0, 0));
        assert_ne!(crowd_shot_key(0), crowd_key(5));
        assert_eq!(crowd_shot_key(16), crowd_shot_key(0));
    }
}
