//! Diagnostic: where each robot's SmartZone boxes sit in the chassis body's frame (origin = centre of
//! mass, axes = body axes), to compare with a live phantom collidable's vertices (docs/29 12av).
//!
//! Usage: `szgeom <arena.py> <botA> <botB>`
use glam::Vec3;
use ra2sim::{BotSetup, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bots: Vec<BotSetup> = a[1..3]
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let duel = Duel::new(&a[0], &bots)?;
    for (r, robot) in duel.robots.iter().enumerate() {
        let s = duel.physics.state(robot.body()).unwrap();
        let com = duel.physics.center_of_mass(robot.body()).unwrap();
        let worlds = robot.component_worlds(&*duel.physics);
        for zone in &robot.rig.smart_zones {
            let frame = worlds[zone.component] * zone.offset;
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for q in 0..8 {
                let c = Vec3::new(
                    if q & 1 == 0 { zone.min.x } else { zone.max.x },
                    if q & 2 == 0 { zone.min.y } else { zone.max.y },
                    if q & 4 == 0 { zone.min.z } else { zone.max.z },
                );
                let p = s.rotation.inverse() * (frame.transform_point3(c) - com);
                lo = lo.min(p);
                hi = hi.max(p);
            }
            println!(
                "robot {r} zone {:?} local box {:?}..{:?} in body frame {lo:.3?}..{hi:.3?}",
                zone.name, zone.min, zone.max
            );
        }
    }
    Ok(())
}
