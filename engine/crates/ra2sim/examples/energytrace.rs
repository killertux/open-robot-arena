//! Battery and air traces from our simulation, to compare with the original's
//! live power step (`tools/oracle/frida/ra2_power_step.js`, docs/30).
//!
//! ```text
//! energytrace drive <arena.py> <bot> "0-6:1,0;6-11:0,1" <seconds>
//! energytrace match <arena.py> <botA> <botB> <seconds>
//! energytrace fire <arena.py> <bot> <input name> <interval s> <count>
//! ```
//!
//! `fire` presses the AI input `<input name>` (`Flip`, `Fire`) `count` times
//! `interval` seconds apart with the robot parked, and prints what each press
//! cost (the original: 291-299 electric for the Scout's `Flip`, 22.8-24.4 air
//! for ALARM's `Fire`, in 0.26 s of 100/s).
//!
//! `drive` prints `t  electric-charge  air-charge  scale` every 0.5 s of one
//! scripted robot. `match` runs the shipped AI match and prints each robot's
//! battery/air fractions at the end (the HUD bars) and the charge used.

use ra2sim::energy::Channel;
use ra2sim::{
    BotSetup, DriveScript, Duel, GameType, Match, MatchConfig, MatchState, RobotEntry, DEFAULT_STEP,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("drive") if a.len() == 5 => {
            let secs: f32 = a[4].parse()?;
            let script = DriveScript::parse(&a[3]);
            let mut duel = Duel::new(
                &a[1],
                &[BotSetup {
                    path: a[2].clone(),
                    yaw_deg: None,
                    spawn: None,
                }],
            )?;
            let r0 = &duel.robots[0];
            println!(
                "elec total {} air total {}",
                r0.energy.elec_total, r0.energy.air_total
            );
            let steps = (secs / DEFAULT_STEP) as usize;
            let every = (0.5 / DEFAULT_STEP) as usize;
            for s in 0..steps {
                let t = s as f32 * DEFAULT_STEP;
                duel.step_direct(&[script.at(t)], DEFAULT_STEP);
                if s.is_multiple_of(every) {
                    let r = &duel.robots[0];
                    println!(
                        "{t:6.2}  elec {:9.2}  air {:8.2}  demand {:.3}  scale {:.3}",
                        r.energy.charge(Channel::Elec, &r.attached),
                        r.energy.charge(Channel::Air, &r.attached),
                        r.energy.last.elec.demand,
                        r.energy.last.elec.scale,
                    );
                }
            }
        }
        Some("match") if a.len() == 5 => {
            let secs: f32 = a[4].parse()?;
            let cfg = MatchConfig::new(
                a[1].clone(),
                GameType::Deathmatch,
                vec![
                    RobotEntry::ai(0, a[2].clone()),
                    RobotEntry::ai(1, a[3].clone()),
                ],
            )
            .with_duration(Some(secs));
            let mut m = Match::new(cfg)?;
            let mut step = 0usize;
            while m.state() != MatchState::Results && step < (secs * 90.0) as usize + 2000 {
                m.step();
                step += 1;
                if std::env::var_os("ENERGYTRACE_VERBOSE").is_some() && step.is_multiple_of(180) {
                    for (i, r) in m.duel.robots.iter().enumerate() {
                        println!(
                            "t {:.1} robot {i}: elec demand {:.3} supply {:.3} scale {:.3} drain {:.3}",
                            m.clock(),
                            r.energy.last.elec.demand,
                            r.energy.last.elec.supply,
                            r.energy.last.elec.scale,
                            r.energy.last.elec.drain
                        );
                    }
                }
            }
            for (i, r) in m.duel.robots.iter().enumerate() {
                println!(
                    "robot {i}: battery {:.1}% ({:.0} of {:.0})  air {:.1}% ({:.0} of {:.0})  dot {}  immobilized {} destroyed {}",
                    100.0 * r.energy.fraction(Channel::Elec, &r.attached),
                    r.energy.charge(Channel::Elec, &r.attached),
                    r.energy.elec_total,
                    100.0 * r.energy.fraction(Channel::Air, &r.attached),
                    r.energy.charge(Channel::Air, &r.attached),
                    r.energy.air_total,
                    r.criticals.min(2),
                    r.immobilized,
                    r.destroyed,
                );
            }
        }
        Some("fire") if a.len() == 6 => {
            let interval: f32 = a[4].parse()?;
            let count: usize = a[5].parse()?;
            let mut duel = Duel::new(
                &a[1],
                &[BotSetup {
                    path: a[2].clone(),
                    yaw_deg: None,
                    spawn: None,
                }],
            )?;
            // Let it settle on its wheels first.
            for _ in 0..(2.0 / DEFAULT_STEP) as usize {
                duel.step_direct(&[Default::default()], DEFAULT_STEP);
            }
            let spent = |d: &Duel| {
                let r = &d.robots[0];
                (
                    r.energy.elec_total - r.energy.charge(Channel::Elec, &r.attached),
                    r.energy.air_total - r.energy.charge(Channel::Air, &r.attached),
                )
            };
            for n in 0..count {
                let before = spent(&duel);
                duel.robots[0].apply_inputs(&[(a[3].clone(), 0, 1.0)]);
                let mut active = 0.0f32;
                for s in 0..(interval / DEFAULT_STEP) as usize {
                    duel.step_direct(&[Default::default()], DEFAULT_STEP);
                    let b = duel.robots[0].energy.last;
                    if b.elec.demand > 0.0 || b.air.demand > 0.0 {
                        active = (s + 1) as f32 * DEFAULT_STEP;
                    }
                }
                let after = spent(&duel);
                println!(
                    "press {n}: electric {:.1}  air {:.1}  (drawing for {active:.2} s)",
                    after.0 - before.0,
                    after.1 - before.1
                );
            }
        }
        _ => {
            eprintln!("usage: energytrace drive <arena> <bot> <script> <seconds>\n       energytrace match <arena> <botA> <botB> <seconds>");
            std::process::exit(2);
        }
    }
    Ok(())
}
