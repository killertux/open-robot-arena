//! The robot's power budget: batteries, air tanks and what draws on them.
//!
//! Recovered from `Robot Arena 2.exe` and measured on the live original
//! (`docs/30-energy-and-watch-camera.md`, Frida agents `ra2_energy.js` and
//! `ra2_power_step.js`). The HUD's battery and air bars are not component
//! hitpoints: they are `Robot+0xa8` and `Robot+0xa4`, written by the per-robot
//! power step at `0x4dc3f0`, and this module is that step.
//!
//! ## One step (`0x4dc3f0`, once per physics step)
//!
//! Every component is asked for its power request (vtable slot `+0x5c`, three
//! arguments: `dt`, an air output, an electric output). A request is negative
//! for a draw and positive for a supply. Per channel the step sums the draws
//! `D` (`Robot+0x94`/`0x9c`) and the supplies `S` (`+0x98`/`0xa0`):
//!
//! * a [`Battery`](Channel::Elec) or `AirTank` supplies `dt * avail * k`, where
//!   `avail = current * rate / total` (`PowerSource+0x290`: the component's
//!   `elecmaxinoutrate` / `airmaxinoutrate` fraction of what is left) and
//!   `k = 0.5 + 0.5 * hp / max_hp` is `FUN_0045fa90`'s damage factor (`0.5` once
//!   the hitpoints are negative);
//! * a `SpinMotor` draws `|command| * dt` (its integer `+0x298`, the wired
//!   `-100..=100` command); a `BurstMotor` draws its `elecmaxinoutrate` (`-500`
//!   for the stock one) while triggered; a `BurstPiston` draws its
//!   `airmaxinoutrate` (`-100`) for its `0.25 s` stroke.
//!
//! With `q = S / D` the step publishes four globals (`0x714a6c`..`0x714a78`):
//!
//! ```text
//! D <= 0 or S <= 0 :  scale = 0, drain = 0
//! otherwise        :  scale = clamp(q, 0.25, 1)
//!                     drain = D / S   when q >= 1   (the sources pay exactly D)
//!                             scale   otherwise
//! ```
//!
//! `scale` multiplies every motor's speed and torque (electric: `0x714a70`, read
//! at `0x4466fb`, `0x4468bc`, `0x4f59fb`; air: `0x714a6c`, read by the burst
//! piston at `0x4487bb`). Each source then runs its own tick (`0x4b40c0` /
//! `0x4b4470`): `current -= dt * drain * avail`, floored at zero. Nothing ever
//! adds to `current` during a match (the only other writer, `0x4b3d90`, is the
//! reset to `total`), so a battery and a tank only run down. The idle drain
//! constants the tick consults (`0x714a74`/`0x714a78` before the step
//! overwrites them) are the same globals: with no draw there is no drain.
//!
//! The HUD fractions are `Σ current / Σ total` (`+0x90 / +0x88`, `+0x8c / +0x84`,
//! `0` when the total is `0`); the totals are fixed when the robot is built, so
//! a battery torn off the robot takes its charge with it.

use ra2fmt::component::ComponentDef;

/// The two channels a robot's power budget has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Batteries; motors and burst motors draw on it.
    Elec,
    /// Air tanks; pistons draw on it.
    Air,
}

/// `electotal` / `elecmaxinoutrate` / `airtotal` / `airmaxinoutrate` of a
/// component definition. A draw is a negative rate, as in the files.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PowerDef {
    pub elec_total: f32,
    pub elec_rate: f32,
    pub air_total: f32,
    pub air_rate: f32,
}

impl PowerDef {
    /// Read the four keys (absent keys are `0`).
    pub fn from_def(def: Option<&ComponentDef>) -> PowerDef {
        let num = |key: &str| def.and_then(|d| d.number(key)).unwrap_or(0.0);
        PowerDef {
            elec_total: num("electotal"),
            elec_rate: num("elecmaxinoutrate"),
            air_total: num("airtotal"),
            air_rate: num("airmaxinoutrate"),
        }
    }
}

/// One battery or air tank (`PowerSource`, fields `+0x280..+0x290`).
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    /// The component index in the robot's rig.
    pub component: usize,
    pub channel: Channel,
    /// `+0x280`.
    pub total: f32,
    /// `+0x284`.
    pub current: f32,
    /// `+0x28c`: `maxinoutrate / total`.
    pub rate_fraction: f32,
    /// The component's maximum hitpoints (for the damage factor).
    pub max_hp: f32,
}

/// What the step decided for one channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Budget {
    /// `D`: what the components asked for this step.
    pub demand: f32,
    /// `S`: what the sources offered this step.
    pub supply: f32,
    /// The factor motors multiply their speed and torque by.
    pub scale: f32,
    /// The factor the sources multiply their available rate by to get their
    /// own drain.
    pub drain: f32,
}

/// Both channels' budgets of one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Budgets {
    pub elec: Budget,
    pub air: Budget,
}

/// The power state of one robot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Energy {
    pub sources: Vec<Source>,
    /// `+0x88`: the electric total, fixed at build time.
    pub elec_total: f32,
    /// `+0x84`: the air total, fixed at build time.
    pub air_total: f32,
    /// The last step's budgets.
    pub last: Budgets,
}

/// The damage factor `FUN_0045fa90` applies to a component's power: `x * (0.5 +
/// 0.5 * hp / max)`, and `0.5 * x` once the hitpoints are below zero.
pub fn health_factor(hp: f32, max_hp: f32) -> f32 {
    if hp < 0.0 {
        0.5
    } else {
        0.5 + 0.5 * (hp / max_hp.max(1.0)).min(1.0)
    }
}

/// `scale` and `drain` for a channel: the branch table in the module docs.
pub fn budget(demand: f32, supply: f32) -> Budget {
    if demand <= 0.0 || supply <= 0.0 {
        return Budget {
            demand,
            supply,
            scale: 0.0,
            drain: 0.0,
        };
    }
    let q = supply / demand;
    let scale = q.clamp(0.25, 1.0);
    let drain = if q >= 1.0 { demand / supply } else { scale };
    Budget {
        demand,
        supply,
        scale,
        drain,
    }
}

impl Energy {
    /// A robot with no batteries and no tanks.
    pub fn none() -> Energy {
        Energy::default()
    }

    /// Add a battery or tank, full (`0x4b3d90`: `current = total`).
    pub fn add_source(&mut self, component: usize, channel: Channel, def: PowerDef, max_hp: f32) {
        let (total, rate) = match channel {
            Channel::Elec => (def.elec_total, def.elec_rate),
            Channel::Air => (def.air_total, def.air_rate),
        };
        if total <= 0.0 {
            return;
        }
        self.sources.push(Source {
            component,
            channel,
            total,
            current: total,
            rate_fraction: rate / total,
            max_hp,
        });
        match channel {
            Channel::Elec => self.elec_total += total,
            Channel::Air => self.air_total += total,
        }
    }

    /// Whether the robot carries any source of `channel` (attached or not).
    pub fn has(&self, channel: Channel) -> bool {
        self.sources.iter().any(|s| s.channel == channel)
    }

    /// `Σ current / Σ total` of the attached sources of `channel` (`0` when the
    /// robot never had one): the HUD bar's fraction.
    pub fn fraction(&self, channel: Channel, attached: &[bool]) -> f32 {
        let total = match channel {
            Channel::Elec => self.elec_total,
            Channel::Air => self.air_total,
        };
        if total <= 0.0 {
            return 0.0;
        }
        let now: f32 = self
            .sources
            .iter()
            .filter(|s| s.channel == channel && attached.get(s.component).copied().unwrap_or(false))
            .map(|s| s.current.max(0.0))
            .sum();
        (now / total).clamp(0.0, 1.0)
    }

    /// The charge left in `channel`, in the component files' units.
    pub fn charge(&self, channel: Channel, attached: &[bool]) -> f32 {
        self.sources
            .iter()
            .filter(|s| s.channel == channel && attached.get(s.component).copied().unwrap_or(false))
            .map(|s| s.current.max(0.0))
            .sum()
    }

    /// The power step: `0x4dc3f0` for one robot and one physics step of `dt`,
    /// given the electric and air draws (already multiplied by `dt`).
    pub fn step(
        &mut self,
        dt: f32,
        elec_demand: f32,
        air_demand: f32,
        health: &[f32],
        attached: &[bool],
    ) -> Budgets {
        let mut out = Budgets::default();
        for channel in [Channel::Elec, Channel::Air] {
            let demand = match channel {
                Channel::Elec => elec_demand,
                Channel::Air => air_demand,
            };
            let supply: f32 = self
                .sources
                .iter()
                .filter(|s| s.channel == channel && live(s, attached))
                .map(|s| {
                    let hp = health.get(s.component).copied().unwrap_or(0.0);
                    dt * avail(s) * health_factor(hp, s.max_hp)
                })
                .sum();
            let b = budget(demand, supply);
            for s in self
                .sources
                .iter_mut()
                .filter(|s| s.channel == channel && live(s, attached))
            {
                // `0x4b40c0`: `current -= dt * drain * avail`, floored at zero.
                let a = avail(s);
                s.current -= dt * b.drain * a;
                if s.current < 0.0 {
                    s.current = 0.0;
                }
            }
            match channel {
                Channel::Elec => out.elec = b,
                Channel::Air => out.air = b,
            }
        }
        self.last = out;
        out
    }
}

/// `PowerSource+0x290`: the rate the source can give right now.
fn avail(s: &Source) -> f32 {
    if s.current > 0.0 {
        s.current * s.rate_fraction
    } else {
        0.0
    }
}

fn live(s: &Source, attached: &[bool]) -> bool {
    attached.get(s.component).copied().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scout() -> Energy {
        let mut e = Energy::none();
        e.add_source(
            6,
            Channel::Elec,
            PowerDef {
                elec_total: 52000.0,
                elec_rate: 800.0,
                ..PowerDef::default()
            },
            400.0,
        );
        e
    }

    const DT: f32 = 1.0 / 30.0;

    #[test]
    fn a_full_supervolt_offers_its_rate() {
        let mut e = scout();
        let b = e.step(
            DT,
            0.0,
            0.0,
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0],
            &[true; 7],
        );
        // Measured: Esup = 26.667 at dt 1/30 for 52000/800.
        assert!((b.elec.supply - 800.0 * DT).abs() < 1e-3, "{b:?}");
        assert_eq!(b.elec.scale, 0.0, "no demand: the step publishes zero");
    }

    #[test]
    fn two_motors_at_full_throttle_cost_two_hundred_a_second() {
        let mut e = scout();
        let health = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0];
        let b = e.step(DT, 200.0 * DT, 0.0, &health, &[true; 7]);
        // Measured: Edem 6.667, G70 1.0, G78 0.25.
        assert_eq!(b.elec.scale, 1.0);
        assert!((b.elec.drain - 0.25).abs() < 1e-3, "{b:?}");
        // One step costs exactly the demand.
        let used = 52000.0 - e.charge(Channel::Elec, &[true; 7]);
        assert!((used - 200.0 * DT).abs() < 0.01, "used {used}");
    }

    #[test]
    fn a_tiny_charge_throttles_to_a_quarter_and_decays() {
        let mut e = scout();
        e.sources[0].current = 150.0;
        let health = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0];
        let b = e.step(DT, 200.0 * DT, 0.0, &health, &[true; 7]);
        // Measured: Esup 0.06, G70 = G78 = 0.25.
        assert_eq!(b.elec.scale, 0.25);
        assert_eq!(b.elec.drain, 0.25);
        let left = e.charge(Channel::Elec, &[true; 7]);
        assert!(left < 150.0 && left > 149.9, "decays, never jumps: {left}");
    }

    #[test]
    fn an_empty_battery_stops_the_motors() {
        let mut e = scout();
        e.sources[0].current = 0.0;
        let health = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0];
        let b = e.step(DT, 200.0 * DT, 0.0, &health, &[true; 7]);
        assert_eq!(b.elec.scale, 0.0);
        assert_eq!(e.fraction(Channel::Elec, &[true; 7]), 0.0);
    }

    #[test]
    fn a_hurt_battery_supplies_less_and_drains_faster() {
        let mut full = scout();
        let mut hurt = scout();
        let healthy = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0];
        let weak = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        full.step(DT, 200.0 * DT, 0.0, &healthy, &[true; 7]);
        hurt.step(DT, 200.0 * DT, 0.0, &weak, &[true; 7]);
        assert!(hurt.sources[0].current < full.sources[0].current);
    }

    #[test]
    fn a_torn_off_battery_takes_its_charge_with_it() {
        let e = scout();
        let mut attached = [true; 7];
        assert_eq!(e.fraction(Channel::Elec, &attached), 1.0);
        attached[6] = false;
        assert_eq!(e.fraction(Channel::Elec, &attached), 0.0);
    }

    #[test]
    fn a_robot_without_a_tank_has_an_empty_air_bar() {
        let e = scout();
        assert_eq!(e.fraction(Channel::Air, &[true; 7]), 0.0);
        assert!(!e.has(Channel::Air));
    }

    #[test]
    fn a_piston_stroke_costs_what_the_original_measured() {
        // ALARM: two 800/120 tanks; a burst piston draws 100/s for ~0.26 s and
        // the tanks lose 22.8-24.4 of it (the last step's draw is paid after).
        let mut e = Energy::none();
        for c in [1, 2] {
            e.add_source(
                c,
                Channel::Air,
                PowerDef {
                    air_total: 800.0,
                    air_rate: 120.0,
                    ..PowerDef::default()
                },
                400.0,
            );
        }
        let health = [0.0, 400.0, 400.0];
        let before = e.charge(Channel::Air, &[true; 3]);
        for _ in 0..8 {
            e.step(DT, 0.0, 100.0 * DT, &health, &[true; 3]);
        }
        let after = e.charge(Channel::Air, &[true; 3]);
        assert!((before - after - 100.0 * DT * 8.0).abs() < 0.01);
    }
}
