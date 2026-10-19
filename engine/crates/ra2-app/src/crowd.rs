//! The arena crowd: `RACrowd`, the mood machine the match object owns.
//!
//! Recovered from `Robot Arena 2.exe` (all addresses are file VAs):
//!
//! * `FUN_00415500` builds it (`FUN_004d4420` constructor, `FUN_004d46e0`
//!   loads the 22 sounds). Excitement starts at `0x37` (55), the mood at 2.
//! * `FUN_004d5260(dt)` is its per-update tick. Excitement falls by one point
//!   per second; the mood is a step function of it (`<35` bored, `<45`
//!   somewhat bored, `<70` normal, `<85` mildly excited, `<95` excited, else
//!   ecstatic/"loud"). A mood change goes through `FUN_004d5430`.
//! * `FUN_004d53e0(n, burst)` adds excitement (capped at 100). A gain of 10 or
//!   more also resets the idle clock. Callers: `FUN_004175b0` (a damage event:
//!   more than 50 gives 5, more than 100 gives 10, more than 300 gives 20),
//!   `FUN_00418040` (a robot is destroyed, state 1: 20 and a loud burst cheer)
//!   and `FUN_0045f4e0` (20).
//! * `FUN_004d5430(mood, stop_previous)` stops the previous mood loop, starts
//!   the new mood's loop, and plays a one-shot: rising to a higher mood plays
//!   the "burst" for it (`FUN_004d5710`), otherwise the `X_to_Y` transition into
//!   the new mood. The current shout is faded out over 1000 ms.
//! * `FUN_004d5560` is the shout: in the top mood (table `+0x24`: chant,
//!   stomp, waytogo, hitemagain), or once the crowd has been bored for ten
//!   seconds with excitement under 15 (table `+0x34`: youmakemesick, boo,
//!   comeon), a random entry is played every ten seconds of `+0xa0`. An entry
//!   with its flag byte set (chant, stomp) is looped (`FUN_004f0680`) and holds
//!   the next shout off for its 6.0 s; the others are played once
//!   (`FUN_004f03b0`).
//! * `FUN_00416440` (from the fight start, `FUN_00416360`) re-enters the
//!   current mood without stopping the previous one: normal loop plus the
//!   `mildly_to_normal` one-shot.
//!
//! Volumes are never set, so everything plays at unity (the `FUN_004f0bb0`
//! calls pass 0 dB). The loops are 2D (the crowd sounds are created without a
//! position).

/// Excitement at construction (`0x37`).
pub const START_EXCITEMENT: i32 = 55;
/// The mood at construction.
pub const START_MOOD: u8 = 2;
/// Seconds between shout attempts (`0x41200000`).
pub const SHOUT_COOLDOWN: f32 = 10.0;
/// How long a chant or stomp blocks the next shout (`6.0`, `0x40c00000`).
pub const CHANT_SECONDS: f32 = 6.0;
/// Seconds without a big event before the crowd counts as bored (`10.0`).
pub const BORED_AFTER: f32 = 10.0;
/// Fade of the previous shout when a new one starts (`500` ms).
pub const SHOUT_FADE_MS: f32 = 500.0;
/// Fade of the current shout on a mood change (`1000` ms).
pub const MOOD_FADE_MS: f32 = 1000.0;

/// The mood loops, by mood number.
pub const LOOPS: [&str; 6] = [
    "Sounds/crowd/bored_loop.wav",
    "Sounds/crowd/somewhat_bored_loop.wav",
    "Sounds/crowd/normal_loop.wav",
    "Sounds/crowd/mildly_excited_loop.wav",
    "Sounds/crowd/excited_loop.wav",
    "Sounds/crowd/loud_loop.wav",
];

/// The `X_to_Y` one-shots played when settling into mood 0 to 4
/// (`FUN_004d5430`'s slots `+0x50..+0x70`).
pub const DOWN: [&str; 5] = [
    "Sounds/crowd/somewhat_to_bored.wav",
    "Sounds/crowd/normal_to_somewhat.wav",
    "Sounds/crowd/mildly_to_normal.wav",
    "Sounds/crowd/excited_to_mildly.wav",
    "Sounds/crowd/ecstatic_to_excited.wav",
];

/// The one-shots played when rising into mood 1 to 5 (`FUN_004d5710`).
pub fn rise_sound(mood: u8) -> Option<&'static str> {
    match mood {
        5 => Some("Sounds/crowd/loud_burstcheer.wav"),
        3 | 4 => Some("Sounds/crowd/med_burstcheer.wav"),
        2 => Some("Sounds/crowd/somewhat_to_normal.wav"),
        1 => Some("Sounds/crowd/bored_to_somewhat.wav"),
        _ => None,
    }
}

/// The top-mood shouts (`+0x24`), in load order; `true` is the looped chants
/// (flag byte 1 in `FUN_004d46e0`).
pub const SHOUTS_LOUD: [(&str, bool); 4] = [
    ("Sounds/crowd/chant.wav", true),
    ("Sounds/crowd/stompstompclap.wav", true),
    ("Sounds/crowd/waytogo.wav", false),
    ("Sounds/crowd/hitemagain.wav", false),
];

/// The taunts (`+0x34`) a bored crowd uses.
pub const SHOUTS_BORED: [&str; 3] = [
    "Sounds/crowd/youmakemesick.wav",
    "Sounds/crowd/boo.wav",
    "Sounds/crowd/comeon.wav",
];

/// What the crowd wants heard.
#[derive(Clone, Debug, PartialEq)]
pub enum CrowdEvent {
    /// Start (`on`) or stop a mood loop.
    Loop { mood: u8, on: bool },
    /// A one-shot at unity.
    OneShot(&'static str),
    /// A looped shout (the chants); it keeps sounding until faded out.
    LoopShout(&'static str),
    /// Fade out the looped shout that sounds, if any, over `ms`.
    FadeShout { ms: f32 },
}

/// The crowd's state.
#[derive(Clone, Debug)]
pub struct Crowd {
    mood: u8,
    excitement: i32,
    /// `+0x9c`: seconds since the last excitement decay.
    decay_clock: f32,
    /// `+0xa0`: seconds until the next shout attempt.
    shout_cooldown: f32,
    /// `+0xa4`: seconds left on the current chant/stomp.
    shout_left: f32,
    /// `+0xa8`: seconds since the last big event.
    idle: f32,
    /// `+0xac`: the crowd has been idle for `BORED_AFTER`.
    bored: bool,
    /// `+0xad`: bored and low enough to taunt.
    taunting: bool,
    /// `+0xae`: a shout is sounding.
    shouting: bool,
    rng: u64,
}

impl Default for Crowd {
    fn default() -> Self {
        Crowd::new()
    }
}

impl Crowd {
    pub fn new() -> Crowd {
        Crowd {
            mood: START_MOOD,
            excitement: START_EXCITEMENT,
            decay_clock: 0.0,
            shout_cooldown: 0.0,
            shout_left: 0.0,
            idle: 0.0,
            bored: false,
            taunting: false,
            shouting: false,
            rng: 0x9e37_79b9_7f4a_7c15,
        }
    }

    /// The current mood, 0 (bored) to 5 (loud).
    pub fn mood(&self) -> u8 {
        self.mood
    }

    /// The current excitement, 0 to 100.
    pub fn excitement(&self) -> i32 {
        self.excitement
    }

    fn next(&mut self) -> u32 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32) as u32
    }

    /// The fight begins (`FUN_00416360` calling `FUN_00416440`): re-enter the
    /// current mood without stopping the previous loop.
    pub fn start(&mut self) -> Vec<CrowdEvent> {
        let mut out = Vec::new();
        self.enter(self.mood, false, &mut out);
        out
    }

    /// `FUN_004d53e0(amount, burst)`.
    pub fn excite(&mut self, amount: i32, burst: bool) -> Vec<CrowdEvent> {
        if amount > 9 {
            self.idle = 0.0;
            self.bored = false;
            self.taunting = false;
        }
        self.excitement = (self.excitement + amount).min(100);
        let mut out = Vec::new();
        if burst {
            if let Some(path) = rise_sound(5) {
                out.push(CrowdEvent::OneShot(path));
            }
        }
        out
    }

    /// A damage event of `damage` points (`FUN_004175b0`).
    pub fn damage(&mut self, damage: f32) -> Vec<CrowdEvent> {
        let amount = if damage > 300.0 {
            20
        } else if damage > 100.0 {
            10
        } else if damage > 50.0 {
            5
        } else {
            return Vec::new();
        };
        self.excite(amount, false)
    }

    /// `FUN_004d5430`.
    fn enter(&mut self, mood: u8, stop_previous: bool, out: &mut Vec<CrowdEvent>) {
        if stop_previous {
            out.push(CrowdEvent::Loop {
                mood: self.mood,
                on: false,
            });
        }
        out.push(CrowdEvent::Loop { mood, on: true });
        if self.mood < mood {
            if let Some(path) = rise_sound(mood) {
                out.push(CrowdEvent::OneShot(path));
            }
        } else if let Some(path) = DOWN.get(mood as usize) {
            out.push(CrowdEvent::OneShot(path));
        }
        out.push(CrowdEvent::FadeShout { ms: MOOD_FADE_MS });
        self.mood = mood;
    }

    /// `FUN_004d5560`.
    fn shout(&mut self, out: &mut Vec<CrowdEvent>) {
        self.shout_cooldown = SHOUT_COOLDOWN;
        if self.mood == 5 {
            let pick = (self.next() as usize) % SHOUTS_LOUD.len();
            let (path, looped) = SHOUTS_LOUD[pick];
            self.play_shout(path, looped, out);
        } else if self.taunting {
            let pick = (self.next() as usize) % SHOUTS_BORED.len();
            self.play_shout(SHOUTS_BORED[pick], false, out);
        }
    }

    fn play_shout(&mut self, path: &'static str, looped: bool, out: &mut Vec<CrowdEvent>) {
        if looped {
            out.push(CrowdEvent::LoopShout(path));
            self.shouting = true;
            self.shout_left = CHANT_SECONDS;
        } else {
            out.push(CrowdEvent::OneShot(path));
            self.shouting = false;
        }
    }

    /// `FUN_004d5260(dt)`.
    pub fn tick(&mut self, dt: f32) -> Vec<CrowdEvent> {
        let mut out = Vec::new();
        self.decay_clock += dt;
        self.shout_cooldown -= dt;
        self.shout_left -= dt;
        self.idle += dt;
        if self.decay_clock >= 1.0 {
            self.decay_clock = 0.0;
            if self.excitement > 0 {
                self.excitement -= 1;
            }
        }
        if self.idle > BORED_AFTER {
            self.bored = true;
        }
        let e = self.excitement;
        let target = if e < 35 {
            if e < 15 {
                if self.bored && !self.taunting {
                    self.taunting = true;
                    None
                } else {
                    Some(0)
                }
            } else {
                self.taunting = false;
                Some(0)
            }
        } else if e < 45 {
            Some(1)
        } else if e < 70 {
            Some(2)
        } else if e < 85 {
            Some(3)
        } else if e < 95 {
            Some(4)
        } else {
            Some(5)
        };
        if let Some(target) = target {
            if target != self.mood {
                self.enter(target, true, &mut out);
            }
        }
        if (self.mood == 5 || self.taunting) && self.shout_cooldown <= 0.0 && self.shout_left <= 0.0
        {
            if self.shouting {
                out.push(CrowdEvent::FadeShout { ms: SHOUT_FADE_MS });
            }
            self.shout(&mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(crowd: &mut Crowd, seconds: f32) -> Vec<CrowdEvent> {
        let mut out = Vec::new();
        let steps = (seconds / 0.05).round() as usize;
        for _ in 0..steps {
            out.extend(crowd.tick(0.05));
        }
        out
    }

    #[test]
    fn it_starts_normal_and_the_fight_start_plays_the_normal_loop() {
        let mut crowd = Crowd::new();
        assert_eq!(crowd.mood(), 2);
        let events = crowd.start();
        assert!(events.contains(&CrowdEvent::Loop { mood: 2, on: true }));
        // The previous loop is not stopped on a restart.
        assert!(!events.contains(&CrowdEvent::Loop { mood: 2, on: false }));
        assert!(events.contains(&CrowdEvent::OneShot("Sounds/crowd/mildly_to_normal.wav")));
    }

    #[test]
    fn excitement_falls_one_point_a_second_and_the_mood_follows() {
        let mut crowd = Crowd::new();
        // 55 -> 44 after eleven seconds: below 45 is "somewhat bored".
        let events = run(&mut crowd, 11.0);
        assert_eq!(crowd.excitement(), 44);
        assert_eq!(crowd.mood(), 1);
        assert!(events.contains(&CrowdEvent::Loop { mood: 2, on: false }));
        assert!(events.contains(&CrowdEvent::Loop { mood: 1, on: true }));
        assert!(events.contains(&CrowdEvent::OneShot("Sounds/crowd/normal_to_somewhat.wav")));
        // 34 after twenty-one: bored.
        run(&mut crowd, 10.0);
        assert_eq!(crowd.mood(), 0);
    }

    #[test]
    fn damage_thresholds_are_50_100_300() {
        let mut crowd = Crowd::new();
        crowd.damage(50.0);
        assert_eq!(crowd.excitement(), 55);
        crowd.damage(51.0);
        assert_eq!(crowd.excitement(), 60);
        crowd.damage(101.0);
        assert_eq!(crowd.excitement(), 70);
        crowd.damage(301.0);
        assert_eq!(crowd.excitement(), 90);
        crowd.excite(50, false);
        assert_eq!(crowd.excitement(), 100);
    }

    #[test]
    fn a_big_gain_rises_with_a_burst_cheer() {
        let mut crowd = Crowd::new();
        crowd.excite(20, true);
        let events = run(&mut crowd, 0.1);
        // 75 is "mildly excited": a rise from 2 to 3 plays the medium burst.
        assert_eq!(crowd.mood(), 3);
        assert!(events.contains(&CrowdEvent::OneShot("Sounds/crowd/med_burstcheer.wav")));
    }

    #[test]
    fn the_top_mood_shouts_every_ten_seconds_at_most() {
        let mut crowd = Crowd::new();
        crowd.excite(45, false);
        let mut shouts = 0;
        let mut looped = 0;
        let mut t = 0.0;
        while t < 60.0 {
            crowd.excite(1, false);
            for event in crowd.tick(0.05) {
                match event {
                    CrowdEvent::LoopShout(p) => {
                        assert!(p.contains("chant") || p.contains("stomp"), "{p}");
                        shouts += 1;
                        looped += 1;
                    }
                    CrowdEvent::OneShot(p)
                        if p.ends_with("waytogo.wav") || p.ends_with("hitemagain.wav") =>
                    {
                        shouts += 1;
                    }
                    _ => {}
                }
            }
            t += 0.05;
        }
        // One attempt at 0 s and one every ten seconds after: six or seven.
        assert!((5..=7).contains(&shouts), "{shouts}");
        assert!(looped <= shouts);
        assert_eq!(crowd.mood(), 5);
    }

    #[test]
    fn a_crowd_left_alone_goes_bored_and_taunts() {
        let mut crowd = Crowd::new();
        let events = run(&mut crowd, 70.0);
        assert_eq!(crowd.mood(), 0);
        let taunts = events
            .iter()
            .filter(|e| {
                matches!(e, CrowdEvent::OneShot(p)
                    if p.ends_with("boo.wav")
                        || p.ends_with("comeon.wav")
                        || p.ends_with("youmakemesick.wav"))
            })
            .count();
        // Excitement is under 15 from 41 s on; the first taunt lands as the
        // flag is raised and then every ten seconds.
        assert!((2..=4).contains(&taunts), "{taunts}");
    }
}
