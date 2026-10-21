//! Diagnostic: an AI-v-AI match, printing both chassis' height and up-vector
//! every half second (who tips over, when).
//!
//! Usage: `fightprobe <arena.py> <botA> <botB> <seconds>`
use ra2sim::{GameType, Match, MatchConfig, MatchState, RobotEntry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let secs: f32 = a[3].parse()?;
    let cfg = MatchConfig::new(
        a[0].clone(),
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, a[1].clone()),
            RobotEntry::ai(1, a[2].clone()),
        ],
    )
    .with_duration(Some(secs));
    let mut m = Match::new(cfg)?;
    // `FIDTRACE_WARMUP=<n>`: n idle physics steps first, as `fidtrace match`.
    let warmup: usize = std::env::var("FIDTRACE_WARMUP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for _ in 0..warmup {
        let n = m.duel.robots.len();
        m.duel.step_direct(
            &vec![ra2sim::sim::DriveInput::default(); n],
            ra2sim::DEFAULT_STEP,
        );
    }
    let window: Option<(f32, f32)> = a.get(4).and_then(|w| {
        let (x, y) = w.split_once('-')?;
        Some((x.parse().ok()?, y.parse().ok()?))
    });
    let mut next = 0.0f32;
    let mut steps = 0usize;
    while m.state() != MatchState::Results && m.clock() < secs && steps < (secs * 400.0) as usize {
        m.step();
        steps += 1;
        let t = m.clock();
        let detail = window.is_some_and(|(x, y)| t >= x && t <= y);
        if detail {
            let mut line = format!("{t:7.3}");
            for r in &m.duel.robots {
                let s = m.duel.physics.state(r.parts[0].body).unwrap();
                let up = s.rotation * glam::Vec3::Y;
                let fwd = s.rotation * r.rig.forward();
                line += &format!(
                    " | y {:5.2} up {:5.2} fwd {:5.2} v ({:5.1},{:5.1},{:5.1}) w {:5.1}",
                    s.position.y,
                    up.y,
                    fwd.y,
                    s.linear_velocity.x,
                    s.linear_velocity.y,
                    s.linear_velocity.z,
                    s.angular_velocity.length()
                );
                for (p, part) in r.parts.iter().enumerate() {
                    let Some(j) = part.joint else { continue };
                    let info = r.rig.parts[p].joint.unwrap();
                    if info.slider {
                        line += &format!(" s{p} {:.2}", m.duel.physics.slider_offset(j));
                    } else if info.use_kind == ra2sim::rig::JointUse::Burst {
                        line += &format!(" h{p} {:.2}", m.duel.physics.hinge_angle(j));
                    }
                }
            }
            println!("{line}");
        } else if t >= next {
            next += 0.5;
            let mut line = format!("{t:6.2}");
            for r in &m.duel.robots {
                let s = m.duel.physics.state(r.parts[0].body).unwrap();
                let up = s.rotation * glam::Vec3::Y;
                let fwd = s.rotation * r.rig.forward();
                line += &format!(
                    " | ({:6.2},{:6.2},{:6.2}) up.y {:5.2} fwd.y {:5.2}",
                    s.position.x, s.position.y, s.position.z, up.y, fwd.y
                );
            }
            println!("{line}");
        }
    }
    Ok(())
}
