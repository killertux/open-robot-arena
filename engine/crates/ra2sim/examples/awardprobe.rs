//! Diagnostic: a robot alone and undriven in the Event Results arena (what the
//! award scene shows, docs/36 section 9.23): prints the chassis pose over time.
//!
//! Usage: `[LIFT=metres] awardprobe <arena.py> <bot> [seconds]` (prints every 20 s; `LIFT` raises the
//! robot before the drop, as `prize_scene::SPAWN_LIFT` does)
use glam::Vec3;
use ra2sim::{BotSetup, DriveInput, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let secs: f32 = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(10.0);
    let mut duel = Duel::new(
        &a[0],
        &[BotSetup {
            path: a[1].clone(),
            yaw_deg: None,
            spawn: None,
        }],
    )?;
    let lift: f32 = std::env::var("LIFT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let bodies: Vec<_> = duel.robots[0].parts.iter().map(|p| p.body).collect();
    for b in bodies {
        let st = duel.physics.state(b).unwrap();
        duel.physics
            .set_transform(b, st.position + Vec3::new(0.0, lift, 0.0), st.rotation);
    }
    let dt = 1.0 / 60.0;
    let n = (secs / dt) as usize;
    for i in 0..=n {
        let s = duel.physics.state(duel.robots[0].body()).unwrap();
        if i % 1200 == 0 {
            let fwd = s.rotation * Vec3::Z;
            println!(
                "{:6.2} h={:.4} pos=({:.3} {:.3} {:.3}) v=({:.3} {:.3} {:.3})",
                i as f32 * dt,
                fwd.x.atan2(fwd.z),
                s.position.x,
                s.position.y,
                s.position.z,
                s.linear_velocity.x,
                s.linear_velocity.y,
                s.linear_velocity.z
            );
        }
        duel.step_direct(&[DriveInput::default()], dt);
    }
    Ok(())
}
