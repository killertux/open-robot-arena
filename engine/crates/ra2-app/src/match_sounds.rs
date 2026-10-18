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
}

/// Motor angular speed (rad/s) above which its loop sounds.
pub const MOTOR_AUDIBLE_RATE: f32 = 1.0;
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
        }
        self.last_state = Some(state);

        // Impacts.
        let impacts = std::mem::take(&mut matched.duel.impacts);
        for imp in impacts {
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
                if on != was {
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

/// The audio voice id of a robot's motor loop, clear of the script handles.
pub fn motor_key(robot: usize, part: usize) -> u64 {
    (1 << 48) | ((robot as u64) << 16) | part as u64
}
