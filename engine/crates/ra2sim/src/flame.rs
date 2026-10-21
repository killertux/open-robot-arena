//! The Flamethrower component (`Flamethrower`, RTTI `0x2ff5b0`, constructor
//! `FUN_00504080`, vtable `0x6ab664`; docs/37-effects.md section 6).
//!
//! It is a weapon with no body to strike with: it owns a flame particle emitter
//! (`Arenas\particles\flame.tga`, named "Fire burst") that follows its nozzle, and
//! the damage is dealt by the particles themselves.
//!
//! # Controls (`FUN_00504080` registers two handlers)
//!
//! | wire action | control | handler | scale |
//! |---|---|---|---|
//! | `Fire` | analog | `0x504a70(on, level)` | 1 |
//! | `Fire burst` | button | `0x504b60(on, level)` | 100 |
//!
//! `Fire` switches the jet: on while the control is non-zero, `level` the
//! control's value (the sum of its inputs, so `-100..=100`), off at zero.
//! `Fire burst` is the same call at level 100 with `burst` set and a
//! [`BURST_SECONDS`] timer (`+0x284 = 0.1`), after which the jet is switched off
//! again by [`Flamethrower::update`]; releasing the button does nothing.
//!
//! # State (`this + ...`)
//!
//! `+0x280` firing, `+0x281` burst, `+0x284` the burst timer, `+0x288`/`+0x28c`
//! `airmaxinoutrate`/`elecmaxinoutrate`, `+0x290` the jet direction `(0, 0, -5)`,
//! `+0x29c` the speed scale, `+0x2a0` the level, `+0x2a4` the emitter, `+0x2a8` the
//! sound handle.
//!
//! The speed scale is set **when the emitter starts**: `level * 0.01 * g`
//! (`g = *0x714a6c`, the power step's *air* scale, docs/30 section 3), and the emitter
//! starts only while both published scales (air `0x714a6c`, electric `0x714a70`)
//! are above zero: a robot with no air tank never lights it, and a draining tank
//! slows the jet (live: scale 1 on a full CO2 tank, 0.44 / 0.30 / 0.24 as it ran
//! down). A level change while the jet is already on only changes the sound.
//!
//! Every step (`0x504790`) the component, unless it is disabled (`+0xe0 > 0`),
//! counts the burst timer down, ends an elapsed burst, starts the emitter when
//! the jet is on and not yet emitting, and places the emitter (`0x5048e0`): at
//! `component * (0, 0.176, -0.9)`, flying along `component * (0, 0, -5) *
//! scale` plus the chassis' own velocity.
//!
//! The power draw (`0x504c50`, vtable `+0x5c`) is the definition's
//! `airmaxinoutrate` and `elecmaxinoutrate` times `dt`, whatever the level, while
//! the jet is on or the burst timer is running.

use glam::Vec3;

/// The burst timer (`0x3dcccccd`, set by `0x504b60`).
pub const BURST_SECONDS: f32 = 0.1;
/// The jet direction in the component's frame (`0x5040b0`, constructed
/// `(0, 0, -5)`); multiplied by [`Flamethrower::scale`].
pub const JET_LOCAL: Vec3 = Vec3::new(0.0, 0.0, -5.0);
/// The nozzle in the component's frame (`0x5048e0`: `0x3e343958`, `0xbf666666`).
pub const NOZZLE_LOCAL: Vec3 = Vec3::new(0.0, 0.176, -0.9);

/// What a call of the control handlers asks the sound to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    /// `Loop(handle)` of the `firesound` (the jet came on).
    Loop,
    /// `SetVolume(handle, hundredths of dB)`: `0x504c20` of the level.
    Volume(i32),
    /// `Stop(handle)`.
    Stop,
}

/// The sound level of a jet level (`0x504c20`): `0` at full, else `20 * level - 2000`
/// hundredths of a decibel.
pub fn volume(level: i32) -> i32 {
    if level.abs() == 100 {
        0
    } else {
        (level * 5 - 500) * 4
    }
}

/// One Flamethrower's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Flamethrower {
    /// `+0x280`: the jet is switched on.
    pub firing: bool,
    /// `+0x281`: the current jet is a burst.
    pub burst: bool,
    /// `+0x284`: seconds of burst left.
    pub timer: f32,
    /// `+0x2a0`: the level the jet was last switched on at.
    pub level: i32,
    /// `+0x29c`: the speed scale the emitter started with.
    pub scale: f32,
    /// The emitter's `+0x324`.
    pub emitting: bool,
}

impl Default for Flamethrower {
    fn default() -> Self {
        Flamethrower {
            firing: false,
            burst: false,
            timer: 0.0,
            level: 0,
            scale: 1.0,
            emitting: false,
        }
    }
}

impl Flamethrower {
    /// `0x504a70(on, level)`: the `Fire` handler; what the sound does is pushed to
    /// `sounds`. `powered` is whether both published power scales (`0x714a6c`,
    /// `0x714a70`) are above zero *now*: the `Loop` of the `firesound` is gated on
    /// it, and the scales are zero until the jet's own draw is asked for, so a jet
    /// switched on from rest never plays (live: `SetVolume` and `Stop` at every
    /// call, no `Loop`, `flame-open__live__005`).
    pub fn fire(
        &mut self,
        on: bool,
        level: i32,
        disabled: bool,
        powered: bool,
        sounds: &mut Vec<Sound>,
    ) {
        if on && !self.firing && !disabled && powered {
            sounds.push(Sound::Loop);
        }
        self.firing = on;
        if on {
            if !disabled {
                sounds.push(Sound::Volume(volume(level)));
            }
            self.level = level;
        } else {
            sounds.push(Sound::Stop);
            self.emitting = false;
        }
    }

    /// `0x504b60(on, level)`: the `Fire burst` handler. Only a press does anything.
    pub fn press_burst(
        &mut self,
        level: i32,
        disabled: bool,
        powered: bool,
        sounds: &mut Vec<Sound>,
    ) {
        self.burst = true;
        self.fire(true, level, disabled, powered, sounds);
        self.timer = BURST_SECONDS;
    }

    /// `0x504790`: one step. `air` and `elec` are the power budget's published
    /// scales of this step (`*0x714a6c`, `*0x714a70`, docs/30 section 3): the
    /// emitter starts only while both are above zero, and its speed scale is
    /// `level * 0.01 * air`.
    pub fn update(
        &mut self,
        dt: f32,
        disabled: bool,
        air: f32,
        elec: f32,
        sounds: &mut Vec<Sound>,
    ) {
        if disabled {
            if self.emitting {
                self.emitting = false;
                self.firing = false;
            }
            return;
        }
        if self.timer <= 0.0 && self.emitting && self.burst {
            self.emitting = false;
            self.burst = false;
            self.firing = false;
            sounds.push(Sound::Stop);
        }
        if self.timer > 0.0 {
            self.timer -= dt;
        }
        if self.firing && !self.emitting && air > 0.0 && elec > 0.0 {
            self.emitting = true;
            self.scale = self.level as f32 * 0.01 * air;
        }
    }

    /// Whether `0x504c50` draws power this step.
    pub fn drawing(&self) -> bool {
        self.timer > 0.0 || self.firing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(f: &mut Flamethrower, n: usize, dt: f32) -> Vec<Sound> {
        let mut s = Vec::new();
        for _ in 0..n {
            f.update(dt, false, 1.0, 1.0, &mut s);
        }
        s
    }

    #[test]
    fn a_held_jet_starts_the_emitter_at_the_level_it_was_switched_on_at() {
        let mut f = Flamethrower::default();
        let mut s = Vec::new();
        f.fire(true, 50, false, false, &mut s);
        // from rest the power scales are zero: no `Loop`, only the volume (live)
        assert_eq!(s, vec![Sound::Volume(-1000)]);
        step(&mut f, 1, 0.01);
        assert!(f.emitting);
        assert!((f.scale - 0.5).abs() < 1e-6);
        // a level change while on is a sound change only
        s.clear();
        f.fire(true, 100, false, true, &mut s);
        assert_eq!(s, vec![Sound::Volume(0)]);
        step(&mut f, 3, 0.01);
        assert!((f.scale - 0.5).abs() < 1e-6);
        s.clear();
        f.fire(false, 0, false, true, &mut s);
        assert_eq!(s, vec![Sound::Stop]);
        assert!(!f.emitting && !f.firing);
    }

    #[test]
    fn a_burst_burns_for_a_tenth_of_a_second_whatever_the_button_does() {
        let mut f = Flamethrower::default();
        let mut s = Vec::new();
        f.press_burst(100, false, false, &mut s);
        assert!(f.firing && f.burst && f.timer > 0.09);
        let mut on = 0;
        for _ in 0..40 {
            f.update(0.01, false, 1.0, 1.0, &mut s);
            if f.emitting {
                on += 1;
            }
        }
        // the timer runs out on the 11th step and the next one switches it off
        assert!((10..=12).contains(&on), "on for {on} steps");
        assert!(!f.firing && !f.burst);
        assert_eq!(s.last(), Some(&Sound::Stop));
    }

    #[test]
    fn a_disabled_flamethrower_goes_out() {
        let mut f = Flamethrower::default();
        let mut s = Vec::new();
        f.fire(true, 100, false, false, &mut s);
        f.update(0.01, false, 1.0, 1.0, &mut s);
        assert!(f.emitting);
        f.update(0.01, true, 1.0, 1.0, &mut s);
        assert!(!f.emitting && !f.firing);
    }

    #[test]
    fn it_draws_power_while_on_or_bursting() {
        let mut f = Flamethrower::default();
        assert!(!f.drawing());
        f.timer = 0.05;
        assert!(f.drawing());
        f.timer = 0.0;
        f.firing = true;
        assert!(f.drawing());
    }

    #[test]
    fn the_sound_level_follows_the_level() {
        assert_eq!(volume(100), 0);
        assert_eq!(volume(-100), 0);
        assert_eq!(volume(50), -1000);
        assert_eq!(volume(0), -2000);
    }
}

/// A robot for the tests: a stock Scout with a Flamethrower on its nose pointing
/// forward, its `Flame` (analog `Fire`) and `FlameBurst` (button `Fire burst`)
/// wires, and optionally the CO2 tank the jet needs. The bot is written into a
/// scratch directory that links to the game's files, so nothing under the game
/// directory is touched.
#[cfg(test)]
pub(crate) mod fixture {
    use std::path::{Path, PathBuf};

    use ra2fmt::bot::{self, Binding, Controller};

    /// The scratch game directory (symlinks to every entry of `game`).
    pub fn scratch_game(game: &Path, tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ra2-flame-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Bots")).expect("scratch dir");
        for entry in std::fs::read_dir(game).expect("game dir").flatten() {
            let target = dir.join(entry.file_name());
            if !target.exists() {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(entry.path(), &target);
            }
        }
        dir
    }

    /// Write the test robot and return its path.
    pub fn flame_bot(game: &Path, tag: &str, tank: bool) -> PathBuf {
        let scratch = scratch_game(game, tag);
        let data = std::fs::read(game.join("AI/Team0/Bot0.bot")).expect("Scout");
        let mut b = bot::parse(&data).expect("parse Scout");
        let mut c = ra2fmt::mount::chassis_child([0.0, 0.55], std::f32::consts::PI, 0.0);
        c.own_mount = 1;
        c.class = "Flamethrower".into();
        c.file = "Components\\flamethrower.txt".into();
        b.components.push(c);
        let idx = b.components.len() as i32 - 1;
        if tank {
            let mut t = ra2fmt::mount::chassis_child([0.3, 0.1], 0.0, 0.0);
            t.own_mount = 1;
            t.class = "AirTank".into();
            t.file = "Components\\airtank.txt".into();
            b.components.push(t);
        }
        b.controllers.push(Controller {
            name: "Flame".into(),
            position: [5, 5, 1],
            bindings: vec![[0, 0, 33, 0]],
        });
        b.controllers.push(Controller {
            name: "FlameBurst".into(),
            position: [6, 5, 4],
            bindings: vec![[0, 0, 34, 0]],
        });
        b.bindings.push(Binding {
            component: idx,
            action: "Fire".into(),
            control: [5, 5, 0],
        });
        b.bindings.push(Binding {
            component: idx,
            action: "Fire burst".into(),
            control: [6, 5, 0],
        });
        let path = scratch.join("Bots").join("flame.bot");
        b.write_to(&path).expect("write bot");
        path
    }
}
