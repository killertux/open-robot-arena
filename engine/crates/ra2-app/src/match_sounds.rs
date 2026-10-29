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
//!   `runsequence`) loops, created at -700, while the motor's output command is
//!   non-zero (`docs/35-audio-events.md` section 7); the pitch follows the hinge
//!   rate (`FUN_004f5b50`).
//! * **Weapon triggers**: the `firesound` plays on the trigger press, a piston on
//!   every press and an electric burst motor only when cocked (`-500`).
//! * **The countdown window**: a `Begin_*` line half a second after the bell.
//! * **Arena sub-material sounds**: a robot part touching a prop the arena script
//!   gave a sound plays it at the rule in `ra2_audio::rules::material_volume_hdb`.

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
    /// An arena sub-material sound (`SetSubMaterialSound`): a robot struck a
    /// named prop whose script registered `path`, at `hundredths_db`.
    Material {
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
    /// A weapon trigger: its component's `firesound` at `hundredths_db`
    /// (`-500` for an electric burst motor, `0` for a piston).
    Fire {
        path: String,
        position: [f32; 3],
        hundredths_db: i32,
    },
    /// The countdown window's announcer: one of the six `Begin_*` lines, played
    /// half a second after the bell.
    Announce { path: &'static str },
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
    /// A Flamethrower's `firesound` handle (`0x504a70`/`0x504790`): `Loop` when
    /// the jet comes on while the power scales are up, `SetVolume` with each
    /// call, `Stop` when it goes out. From rest the original never loops it
    /// (docs/37 section 6), so these cues are rare.
    Flame {
        key: u64,
        call: ra2sim::flame::Sound,
        path: String,
        position: [f32; 3],
    },
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
    crowd: Crowd,
    destroyed: Vec<bool>,
    /// Each robot's `attached` flags at the last step, to see a part come off.
    attached: Vec<Vec<bool>>,
    /// The last playback rate sent for each running motor loop.
    pitch: HashMap<(usize, usize), f32>,
    /// A private copy of the immobility rule (`FUN_00412fc0`), run to know when
    /// a robot is under warning.
    immobility: ra2sim::immobility::Tracker,
    immobility_clock: f32,
    warned: Vec<bool>,
    /// Seconds until the `Begin_*` line, while one is pending.
    begin_in: Option<f32>,
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

/// The four sounds `FUN_00415500` builds at `match+0x110` and `FUN_00419160`
/// picks from at random (`frc_generic3.wav` is not among them).
pub const FRC_SOUNDS: [&str; 4] = [
    "Sounds/frc_generic1.wav",
    "Sounds/frc_generic2.wav",
    "Sounds/frc_generic4.wav",
    "Sounds/frc_generic5.wav",
];
/// The six `Begin_*` announcer lines the countdown window holds (`0x474ef7`).
pub const BEGIN_LINES: [&str; 6] = [
    "Sounds/announcers/Begin_AndWereOff.wav",
    "Sounds/announcers/Begin_BuzzerSignalsStart.wav",
    "Sounds/announcers/Begin_HereWeGo.wav",
    "Sounds/announcers/Begin_KickMatchInGear.wav",
    "Sounds/announcers/Begin_LetsBegin.wav",
    "Sounds/announcers/Begin_MayStrongestWin.wav",
];
/// Seconds from the bell to the `Begin_*` line: 0.51 s in every live capture
/// (`audio-match-*__live`: bell created at 30.52, line played at 31.04).
pub const BEGIN_DELAY: f32 = 0.5;
/// The volume a motor loop is created at (`-700` hundredths of a dB, every
/// `motorloop*` handle of the live captures).
pub const MOTOR_LOOP_HDB: i32 = -700;
/// The volume the electric burst motor's `firesound` is created at (`-500`;
/// the piston's is `0`).
pub const BURST_FIRE_HDB: i32 = -500;

impl MatchSounds {
    pub fn new() -> MatchSounds {
        MatchSounds {
            rng: 0x2545_f491_4f6c_dd1d,
            last_number: None,
            last_state: None,
            criticals: Vec::new(),
            angles: HashMap::new(),
            running: HashMap::new(),
            crowd: Crowd::new(),
            destroyed: Vec::new(),
            attached: Vec::new(),
            pitch: HashMap::new(),
            immobility: ra2sim::immobility::Tracker::default(),
            immobility_clock: 0.0,
            warned: Vec::new(),
            begin_in: None,
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
            self.begin_in = Some(BEGIN_DELAY);
            // `FUN_00416360`: the fight begins and the crowd re-enters its mood.
            let events = self.crowd.start();
            cues.extend(events.into_iter().map(crowd_cue));
        }
        if state == MatchState::Results && self.last_state != Some(MatchState::Results) {
            // The crowd object goes with the match (`FUN_004164a0`).
            cues.push(SoundCue::CrowdStop);
        }
        self.last_state = Some(state);

        // The announcer's line half a second after the bell.
        if let Some(left) = self.begin_in {
            let left = left - matched.config.dt;
            if left <= 0.0 {
                self.begin_in = None;
                let pick = (self.next() as usize) % BEGIN_LINES.len();
                cues.push(SoundCue::Announce {
                    path: BEGIN_LINES[pick],
                });
            } else {
                self.begin_in = Some(left);
            }
        }

        // The crowd's tick runs while the match is updating (`FUN_00412fc0`),
        // which includes the arena intro: the object is built with the match
        // (live: `crowd/*.wav` created at 4.95 s, `normal_to_somewhat` 11.1 s
        // later, `somewhat_to_bored` 10 s after that, so the crowd is bored at
        // the bell and re-enters with `somewhat_to_bored`).
        if matches!(
            state,
            MatchState::Intro | MatchState::Countdown | MatchState::Fight
        ) {
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

        // Arena sub-material sounds: a robot part touched a named prop the
        // arena script gave a sound (`0x405090`).
        let contacts = std::mem::take(&mut matched.duel.material_contacts);
        if !contacts.is_empty() && state == MatchState::Fight {
            let registered = arena_material_sounds();
            for contact in contacts {
                let Some((_, factor, path)) = registered
                    .iter()
                    .find(|(name, _, _)| name.eq_ignore_ascii_case(&contact.prop))
                else {
                    continue;
                };
                let Some(hundredths_db) =
                    ra2_audio::rules::material_volume_hdb(*factor, contact.magnitude)
                else {
                    continue;
                };
                cues.push(SoundCue::Material {
                    path: path.clone(),
                    position: [contact.point.x, contact.point.y, contact.point.z],
                    hundredths_db,
                });
            }
        }

        // A component coming off (`FUN_00417d70` -> `FUN_0045f4e0`): one of the
        // four `frc_generic` sounds (`FUN_00419160`) and the crowd +20 with a
        // burst cheer. Live: `frc_generic5` + `loud_burstcheer` at the same
        // instant, 4 to 13 s into three of five fights, and none at the end.
        if self.attached.len() < matched.duel.robots.len() {
            self.attached.resize(matched.duel.robots.len(), Vec::new());
        }
        for (i, robot) in matched.duel.robots.iter().enumerate() {
            let before = std::mem::replace(&mut self.attached[i], robot.attached.clone());
            let lost = state == MatchState::Fight
                && before
                    .iter()
                    .zip(&robot.attached)
                    .skip(1)
                    .any(|(was, now)| *was && !*now);
            if lost {
                let events = self.crowd.excite(20, true);
                cues.extend(events.into_iter().map(crowd_cue));
                let pick = (self.next() as usize) % FRC_SOUNDS.len();
                cues.push(SoundCue::Force {
                    path: FRC_SOUNDS[pick],
                });
            }
        }

        // A robot destroyed (`FUN_00418040` state 1): the crowd +20 with a
        // burst cheer; the `frc_generic` sound is the component's, not this.
        if self.destroyed.len() < matched.duel.robots.len() {
            self.destroyed.resize(matched.duel.robots.len(), false);
        }
        for (i, robot) in matched.duel.robots.iter().enumerate() {
            if robot.destroyed && !self.destroyed[i] && state == MatchState::Fight {
                let events = self.crowd.excite(20, true);
                cues.extend(events.into_iter().map(crowd_cue));
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

        // Weapon triggers: the firesound plays on the trigger itself, not on
        // the stroke (`0x446810` burst motor, `0x448750` piston).
        for robot in matched.duel.robots.iter_mut() {
            let events = robot.take_fire_events();
            let p = robot.position(matched.duel.physics.as_ref());
            for motor in events {
                let Some(path) = robot.rig.components[motor].sounds.fire.clone() else {
                    continue;
                };
                let slider = robot.rig.parts.iter().any(|part| {
                    part.joint
                        .as_ref()
                        .is_some_and(|j| j.motor == motor && j.slider)
                });
                cues.push(SoundCue::Fire {
                    path,
                    position: [p.x, p.y, p.z],
                    hundredths_db: if slider { 0 } else { BURST_FIRE_HDB },
                });
            }
        }

        // Flamethrowers: the calls the component made on its `firesound` handle.
        for (ri, robot) in matched.duel.robots.iter_mut().enumerate() {
            let calls = robot.take_flame_sounds();
            let p = robot.position(matched.duel.physics.as_ref());
            for (component, call) in calls {
                let Some(path) = robot.rig.components[component].sounds.fire.clone() else {
                    continue;
                };
                cues.push(SoundCue::Flame {
                    key: flame_key(ri, component),
                    call,
                    path,
                    position: [p.x, p.y, p.z],
                });
            }
        }

        // Motor loops: one handle per motor component, looping while the
        // motor's output command is non-zero (`FUN_004f59f0`) and silent when
        // it returns to zero or the match is over.
        let dt = matched.config.dt.max(1e-4);
        let over = state == MatchState::Results || state == MatchState::Over;
        let mut motor_cues = Vec::new();
        for (ri, robot) in matched.duel.robots.iter().enumerate() {
            for (pi, part) in robot.rig.parts.iter().enumerate() {
                let (Some(joint), Some(state)) = (part.joint.as_ref(), robot.parts.get(pi)) else {
                    continue;
                };
                let Some(handle) = state.joint else { continue };
                if joint.slider || joint.use_kind == ra2sim::rig::JointUse::Burst {
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
                let command = robot
                    .control
                    .command(&robot.rig.wiring, joint.motor)
                    .unwrap_or(0.0);
                let on = !over && robot.attached[joint.motor] && robot.is_alive() && command != 0.0;
                let was = self.running.insert((ri, pi), on).unwrap_or(false);
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

/// The live arena's `SetSubMaterialSound` registrations that name a sound:
/// `(sub-material name, volume factor, sound path)`.
fn arena_material_sounds() -> Vec<(String, f32, String)> {
    let Some(arena) = ra2script::state().arena.clone() else {
        return Vec::new();
    };
    let data = arena.lock().unwrap_or_else(|e| e.into_inner());
    data.sub_materials
        .iter()
        .filter_map(|(name, _material, factor, sound)| {
            sound
                .as_ref()
                .map(|sound| (name.clone(), *factor, sound.clone()))
        })
        .collect()
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
/// The script-sound id of a Flamethrower's `firesound` handle.
pub fn flame_key(robot: usize, component: usize) -> u64 {
    (1 << 49) | ((robot as u64) << 16) | component as u64
}

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
