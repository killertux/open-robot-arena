//! The floating damage numbers (`ArenaManager::DamageDisplay`, docs/37 section 7).
//!
//! Recovered from `Robot Arena 2.exe` and checked live (`tools/oracle/capture_numbers.py`):
//!
//! * `FUN_004175b0` (the hit handler) asks the display for a number through `FUN_004177e0` ->
//!   `FUN_004178a0` -> `FUN_00417940`, **only for a human player** (`player.vtable[0x18]`) and
//!   only outside King of the Hill: a hit with an attacker puts a **green** number on the
//!   attacker (the damage that scores: victim alive, not a team-mate in Team Match), a hit
//!   without one (a hazard, `plus.damage`, a flame, a wall) a **red** number on the robot that
//!   took it. The amount is the *effective* damage the struck component returned.
//! * Requests of the same player and colour within [`MERGE_SECONDS`] of the first one and
//!   closer than `sqrt(`[`MERGE_DISTANCE_SQUARED`]`)` metres are added into one entry
//!   (`FUN_00417940`); an entry older than [`MERGE_SECONDS`] is turned into a number
//!   (`FUN_00413b20` -> `FUN_00413bd0`), unless `GraphicMessages` is off.
//! * The number is the amount truncated (`__ftol`), in `fg_d20`, drawn 1:1 in pixels at the
//!   projection of a world point (its size does not follow the distance). It starts at
//!   `point -/+ 0.1 * normal` (red / green), stays there until 0.25 s, then moves along
//!   `normalize(up * 6 -/+ normal)` at 2 m/s (the mover `0x5a3780` from 0.25 s to 0.75 s over
//!   one metre, which keeps extrapolating), is opaque until 0.75 s, fades to nothing at 1.0 s
//!   (the fader `0x5a3c00`) and is gone then (`FUN_004141a0(1.0)`). Colour `0.8` of red or
//!   green.

use glam::Vec3;

/// `DAT_006ef568`: how long a request waits for more to join it.
pub const MERGE_SECONDS: f32 = 0.15;
/// `DAT_006ef56c`: the squared distance under which two requests are one.
pub const MERGE_DISTANCE_SQUARED: f32 = 2.0;
/// `FUN_004141a0(1.0)`: the number's life.
pub const LIFE: f32 = 1.0;
/// The mover's window (`FUN_005a3780(.., 0.25, 0.75)`).
pub const RISE_START: f32 = 0.25;
pub const RISE_END: f32 = 0.75;
/// The fader's window (`FUN_005a3c00(.., 0.75, 1.0)`).
pub const FADE_START: f32 = 0.75;
pub const FADE_END: f32 = 1.0;
/// The face the text is drawn in (`s_fg_d20_006ef624`).
pub const FACE: &str = "fg_d20";
/// `0x3dcccccd`: the offset along the contact normal.
const NORMAL_OFFSET: f32 = 0.1;
/// `0x40c00000`: the up vector the direction is built from.
const UP: f32 = 6.0;
/// The colour's strong channel (`0x3f4ccccd`).
const STRONG: f32 = 0.8;

/// One damage event the duel asks a number for, before the match's rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    /// The robot that struck, `None` for a hazard, a flame or a wall.
    pub attacker: Option<usize>,
    /// The robot that was struck.
    pub victim: usize,
    /// Whether the victim was still fighting before the blow (`robot + 0x34 == 0`).
    pub victim_alive: bool,
    /// Effective damage (`FUN_00489620`): what the component took.
    pub amount: f32,
    /// The contact point (`param_6`).
    pub point: Vec3,
    /// The hit record's normal, pointing *into* the struck component (`param_7`); zero for a
    /// hazard.
    pub normal: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    player: usize,
    green: bool,
    point: Vec3,
    normal: Vec3,
    amount: f32,
    born: f32,
}

/// A number on its way up.
#[derive(Clone, Debug, PartialEq)]
pub struct Number {
    pub player: usize,
    pub green: bool,
    pub text: String,
    start: Vec3,
    dir: Vec3,
    pub age: f32,
}

impl Number {
    /// Where the text is anchored in the world.
    pub fn position(&self) -> Vec3 {
        let t = (self.age - RISE_START).max(0.0);
        self.start + self.dir * (t / (RISE_END - RISE_START))
    }

    /// The text colour: `0.8` of green or red, opaque until the fade starts.
    pub fn colour(&self) -> [f32; 4] {
        let alpha = if self.age <= FADE_START {
            1.0
        } else {
            (1.0 - (self.age - FADE_START) / (FADE_END - FADE_START)).clamp(0.0, 1.0)
        };
        if self.green {
            [0.0, STRONG, 0.0, alpha]
        } else {
            [STRONG, 0.0, 0.0, alpha]
        }
    }
}

/// The number list: the entries still collecting hits and the numbers in flight.
#[derive(Clone, Debug, Default)]
pub struct DamageDisplay {
    time: f32,
    entries: Vec<Entry>,
    numbers: Vec<Number>,
}

impl DamageDisplay {
    pub fn new() -> DamageDisplay {
        DamageDisplay::default()
    }

    /// `FUN_00417940`: add a request for `player`, joining an entry of the same colour that is
    /// younger than [`MERGE_SECONDS`] and closer than [`MERGE_DISTANCE_SQUARED`].
    pub fn request(&mut self, player: usize, green: bool, amount: f32, point: Vec3, normal: Vec3) {
        for entry in &mut self.entries {
            if entry.player == player
                && entry.green == green
                && self.time - entry.born < MERGE_SECONDS
                && (point - entry.point).length_squared() < MERGE_DISTANCE_SQUARED
            {
                entry.amount += amount;
                return;
            }
        }
        self.entries.push(Entry {
            player,
            green,
            point,
            normal,
            amount,
            born: self.time,
        });
    }

    /// `FUN_00413b20(dt)`: advance the clock, turn the entries older than [`MERGE_SECONDS`]
    /// into numbers (none when `shown` is false: `GraphicMessages`), age the numbers and drop
    /// the ones past [`LIFE`].
    pub fn step(&mut self, dt: f32, shown: bool) {
        self.time += dt;
        for number in &mut self.numbers {
            number.age += dt;
        }
        self.numbers.retain(|n| n.age < LIFE);
        let time = self.time;
        let (due, rest): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|e| time - e.born > MERGE_SECONDS);
        self.entries = rest;
        if !shown {
            return;
        }
        for entry in due {
            self.numbers.push(spawn(&entry));
        }
    }

    /// The numbers to draw.
    pub fn numbers(&self) -> &[Number] {
        &self.numbers
    }

    /// Entries still waiting for more hits.
    pub fn pending(&self) -> usize {
        self.entries.len()
    }
}

/// `FUN_00413bd0`: the number an entry becomes.
fn spawn(entry: &Entry) -> Number {
    let up = Vec3::new(0.0, UP, 0.0);
    let (start, toward) = if entry.green {
        (
            entry.point + entry.normal * NORMAL_OFFSET,
            entry.normal + up,
        )
    } else {
        (
            entry.point - entry.normal * NORMAL_OFFSET,
            up - entry.normal,
        )
    };
    Number {
        player: entry.player,
        green: entry.green,
        // `__ftol`: truncated toward zero.
        text: format!("{}", entry.amount as i32),
        start,
        dir: toward.normalize_or_zero(),
        age: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hazard_number_rises_from_the_point_and_fades_over_the_last_quarter() {
        // Live (numbers__live): `plus.damage(0, 0, 25, point)` -> a red `25`, mover from the
        // point to point + (0, 1, 0), fader (0.8, 0, 0, 1) -> (0.8, 0, 0, 0) from 0.75 to 1.0.
        let mut d = DamageDisplay::new();
        let p = Vec3::new(-9.887, -1.307, 10.124);
        d.request(0, false, 25.0, p, Vec3::ZERO);
        assert_eq!(d.pending(), 1);
        d.step(0.1, true);
        assert!(d.numbers().is_empty(), "a request waits 0.15 s for company");
        d.step(0.06, true);
        assert_eq!(d.pending(), 0);
        let n = &d.numbers()[0];
        assert_eq!(n.text, "25");
        assert!(!n.green);
        assert_eq!(n.position(), p);
        assert_eq!(n.colour(), [0.8, 0.0, 0.0, 1.0]);
        let mut n = n.clone();
        n.age = 0.25;
        assert_eq!(n.position(), p);
        n.age = 0.75;
        assert!((n.position() - (p + Vec3::Y)).length() < 1e-5);
        assert_eq!(n.colour()[3], 1.0);
        n.age = 0.875;
        assert!((n.colour()[3] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_wall_hit_leans_the_number_away_from_the_normal() {
        // Live: a wall hit at (8.365, -1.965, -12.389) with normal (0, 0, 1) -> start
        // (8.365, -1.965, -12.489), after the rise (8.365, -0.979, -12.653).
        let mut d = DamageDisplay::new();
        d.request(
            0,
            false,
            34.297,
            Vec3::new(8.365, -1.965, -12.389),
            Vec3::Z,
        );
        d.step(0.2, true);
        let mut n = d.numbers()[0].clone();
        assert_eq!(n.text, "34");
        assert!((n.position() - Vec3::new(8.365, -1.965, -12.489)).length() < 1e-4);
        n.age = RISE_END;
        assert!((n.position() - Vec3::new(8.365, -0.979, -12.653)).length() < 2e-3);
    }

    #[test]
    fn an_attacker_number_is_green_and_leans_with_the_normal() {
        let mut d = DamageDisplay::new();
        d.request(0, true, 12.9, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        d.step(0.2, true);
        let n = &d.numbers()[0];
        assert_eq!(n.text, "12");
        assert_eq!(n.colour(), [0.0, 0.8, 0.0, 1.0]);
        assert!((n.position() - Vec3::new(1.0, 0.0, 0.1)).length() < 1e-6);
    }

    #[test]
    fn hits_of_one_colour_close_in_time_and_space_add_into_one_number() {
        let mut d = DamageDisplay::new();
        let p = Vec3::new(1.0, 1.0, 1.0);
        d.request(0, false, 2.0, p, Vec3::ZERO);
        d.step(0.05, true);
        d.request(0, false, 2.0, p + Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO);
        // The other colour, another player and a far point are their own numbers.
        d.request(0, true, 2.0, p, Vec3::ZERO);
        d.request(1, false, 2.0, p, Vec3::ZERO);
        d.request(0, false, 2.0, p + Vec3::new(1.5, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(d.pending(), 4);
        d.step(0.2, true);
        let texts: Vec<_> = d.numbers().iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["4", "2", "2", "2"]);
    }

    #[test]
    fn a_number_is_gone_after_a_second_and_never_made_with_messages_off() {
        let mut d = DamageDisplay::new();
        d.request(0, false, 5.0, Vec3::ZERO, Vec3::ZERO);
        d.step(0.2, false);
        assert!(d.numbers().is_empty());
        d.request(0, false, 5.0, Vec3::ZERO, Vec3::ZERO);
        d.step(0.2, true);
        assert_eq!(d.numbers().len(), 1);
        d.step(0.95, true);
        assert_eq!(d.numbers().len(), 1);
        d.step(0.06, true);
        assert!(d.numbers().is_empty());
    }
}
