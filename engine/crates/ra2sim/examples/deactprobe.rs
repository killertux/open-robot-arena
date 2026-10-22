//! Diagnostic: robots left alone in an arena (docs/29 section 12t), as
//! `capture_deact.py`'s first phase has them live: every robot's freezes
//! (the substeps its chassis is inactive) and its chassis' yaw and centre.
//!
//! Usage: `deactprobe <arena.py> <bot>... [-- seconds]`
//! Prints `F <step> <robot> <inactive bodies>/<bodies>` whenever a robot has
//! an inactive body, then per robot the freezes (chassis inactive), the mean
//! interval between them and the yaw and centre drift.
use glam::Vec3;
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

fn yaw(q: glam::Quat) -> f32 {
    let f = q * Vec3::Z;
    f.x.atan2(f.z)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let split = a.iter().position(|s| s == "--");
    let bots = &a[1..split.unwrap_or(a.len())];
    let secs: f32 = split.map_or(15.0, |i| a[i + 1].parse().unwrap());
    let setups: Vec<BotSetup> = bots
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let mut duel = Duel::new(&a[0], &setups)?;
    let n = duel.robots.len();
    let quiet = std::env::var_os("QUIET").is_some();
    let start: Vec<_> = (0..n)
        .map(|r| duel.physics.state(duel.robots[r].body()).unwrap())
        .collect();
    let mut freezes: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut was = vec![true; n];
    let steps = (secs / DEFAULT_STEP) as usize;
    let inputs = vec![DriveInput::default(); n];
    for i in 0..steps {
        duel.step_direct(&inputs, DEFAULT_STEP);
        for r in 0..n {
            let parts: Vec<_> = duel.robots[r]
                .parts
                .iter()
                .filter(|p| p.alive)
                .map(|p| p.body)
                .collect();
            let off = parts
                .iter()
                .filter(|&&b| !duel.physics.is_active(b))
                .count();
            if off > 0 && !quiet {
                println!("F {i} {r} {off}/{}", parts.len());
            }
            let ch = duel.physics.is_active(duel.robots[r].body());
            if !ch && was[r] {
                freezes[r].push(i);
            }
            was[r] = ch;
        }
    }
    for r in 0..n {
        let s = duel.physics.state(duel.robots[r].body()).unwrap();
        let f = &freezes[r];
        let gaps: Vec<usize> = f.windows(2).map(|w| w[1] - w[0]).collect();
        let mean = gaps.iter().sum::<usize>() as f32 / gaps.len().max(1) as f32;
        let d = s.position - start[r].position;
        println!(
            "robot {r}: {} freezes, mean gap {:.1} substeps, yaw {:+.4} rad, centre ({:+.3} {:+.3} {:+.3})",
            f.len(),
            mean,
            yaw(s.rotation) - yaw(start[r].rotation),
            d.x,
            d.y,
            d.z
        );
    }
    Ok(())
}
