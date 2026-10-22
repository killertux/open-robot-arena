//! Diagnostic: where the start-point lowering (docs/29 section 12v) leaves
//! each robot, and how far its lowest point then is from the floor below it.
//!
//! Usage: `spawnprobe <arena.py> <bot>...`; prints per robot the chassis'
//! centre of mass and the lowest point of each component's hull (the
//! colliding pieces and the wheels' traction rings alike) above the floor.
use glam::Vec3;
use ra2sim::{BotSetup, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let setups: Vec<BotSetup> = a[1..]
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let mut duel = Duel::new(&a[0], &setups)?;
    // `SPAWN_AT=<y>`: put robot 0's chassis centre at that height (every body
    // moved with it) and run one collision detection (`RA2_H17_DUMP` writes
    // its contacts).
    let at: Vec<f32> = std::env::var("SPAWN_AT")
        .map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_default();
    for (ri, &y) in at.iter().enumerate() {
        let r = &duel.robots[ri];
        let dy = y - duel.physics.center_of_mass(r.body()).unwrap().y;
        let bodies: Vec<_> = r.parts.iter().map(|p| p.body).collect();
        for b in bodies {
            let s = duel.physics.state(b).unwrap();
            duel.physics
                .set_transform(b, s.position + Vec3::new(0.0, dy, 0.0), s.rotation);
        }
        duel.physics.detect_now();
    }
    for (i, r) in duel.robots.iter().enumerate() {
        let c = duel.physics.center_of_mass(r.body()).unwrap();
        let floor = std::env::var("FLOOR")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| {
                duel.physics
                    .ray_cast(Vec3::new(c.x, c.y - 0.3, c.z), Vec3::NEG_Y, 50.0)
                    .map_or(f32::NAN, |h| h.point.y)
            });
        let worlds = r.component_worlds(&*duel.physics);
        let mut lows: Vec<(f32, String)> = r
            .rig
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.hull_points.is_empty())
            .map(|(k, comp)| {
                let lo = comp
                    .hull_points
                    .iter()
                    .map(|p| worlds[k].transform_point3(*p).y)
                    .fold(f32::MAX, f32::min);
                (lo - floor, comp.name.clone())
            })
            .collect();
        lows.sort_by(|x, y| x.0.total_cmp(&y.0));
        // The lowest point of the colliding pieces and of the phantoms.
        let (mut coll, mut ghost) = (f32::MAX, f32::MAX);
        for (k, comp) in r.rig.components.iter().enumerate() {
            for piece in &comp.havok.collision {
                for p in piece {
                    coll = coll.min(worlds[k].transform_point3(*p).y - floor);
                }
            }
            for p in &comp.havok.phantom {
                ghost = ghost.min(worlds[k].transform_point3(*p).y - floor);
            }
        }
        println!("robot {i} collision lowest {coll:.4} phantom lowest {ghost:.4}");
        // Each colliding piece's lowest vertices (how it faces the floor).
        for (k, comp) in r.rig.components.iter().enumerate() {
            for piece in &comp.havok.collision {
                let mut ys: Vec<f32> = piece
                    .iter()
                    .map(|p| worlds[k].transform_point3(*p).y)
                    .collect();
                ys.sort_by(f32::total_cmp);
                if piece.len() >= 12 {
                    println!(
                        "  piece of {} ({} points): lowest {:?}",
                        comp.name,
                        piece.len(),
                        &ys[..5.min(ys.len())]
                    );
                }
            }
        }
        for p in &r.parts {
            let (m, _, _) = duel.physics.mass_properties(p.body).unwrap_or_default();
            let pc = duel.physics.center_of_mass(p.body).unwrap_or_default();
            println!(
                "  part {:.1} kg centre {:.4} {:.4} {:.4}",
                m, pc.x, pc.y, pc.z
            );
        }
        println!(
            "robot {i} {:?}: chassis centre y {:.4}, floor {:.4}, lowest {:?}",
            r.rig.name,
            c.y,
            floor,
            &lows[..lows.len().min(4)]
        );
    }
    Ok(())
}
