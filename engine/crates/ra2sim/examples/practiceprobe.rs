//! Diagnostic: the Bot Lab's practice arena without the lab (docs/29 section
//! 12ak). One robot drives forward (W) into the far wall; every priced contact
//! (`Duel::impacts`) is printed with its time.
//!
//! Usage: `RA2_GAME=<game> practiceprobe <bot> <seconds> [hold-seconds]`
use ra2sim::{DriveInput, GameType, Match, MatchConfig, RobotEntry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let secs: f32 = a[1].parse()?;
    let hold: f32 = a.get(2).map_or(secs, |s| s.parse().unwrap_or(secs));
    let game = std::env::var("RA2_GAME")?;
    std::env::set_var("RA2_HAZARDS", "0");
    let cfg = MatchConfig::new(
        format!("{game}/Scripts/Practice.py"),
        GameType::Deathmatch,
        vec![RobotEntry::human(0, a[0].clone())],
    )
    .with_practice();
    let mut m = Match::new(cfg)?;
    m.duel.record_impacts = true;
    m.duel.human_robot = Some(0);
    let dt = m.config.dt;
    let mut t = 0.0;
    let mut input = DriveInput::default();
    input.keys.set(17, true);
    while t < secs {
        if t >= hold {
            input.keys.set(17, false);
        }
        m.step_with(&[Some(input)]);
        t += dt;
        for imp in std::mem::take(&mut m.duel.impacts) {
            println!(
                "{t:.2}s impact damage {:.2} comp {} attacker {} speed {:.2} at {:?}",
                imp.damage,
                imp.component,
                imp.attacker == ra2sim::combat::NO_ATTACKER,
                imp.speed,
                imp.point
            );
        }
        if (t * 4.0).fract() < dt * 4.0 {
            let p = m.duel.robots[0].position(&*m.duel.physics);
            println!("{t:.2}s pos {:.2} {:.2} {:.2}", p.x, p.y, p.z);
        }
    }
    Ok(())
}
