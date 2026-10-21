//! Diagnostic: place a robot's chassis at a live pose (its body's centre of
//! mass and `+0xcc` rotation rows, as `frida/ra2_zone.js` writes them) and
//! print how far below the centre each component's collision geometry
//! reaches: what it can be resting on (docs/29 section 12j).
//!
//! Usage: `poseprobe <arena.py> <bot> <cx cy cz r0..r8>`
use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let v: Vec<f32> = a[2]
        .split_whitespace()
        .map(|x| x.parse().unwrap())
        .collect();
    let mut duel = Duel::new(
        &a[0],
        &[BotSetup {
            path: a[1].clone(),
            yaw_deg: None,
            spawn: None,
        }],
    )?;
    let r = &duel.robots[0];
    let body = r.body();
    let s = duel.physics.state(body).unwrap();
    let com0 = duel.physics.center_of_mass(body).unwrap();
    let off = s.rotation.inverse() * (com0 - s.position);
    let m = Mat3::from_cols(
        Vec3::new(v[3], v[4], v[5]),
        Vec3::new(v[6], v[7], v[8]),
        Vec3::new(v[9], v[10], v[11]),
    );
    let rot = Quat::from_mat3(&m).normalize();
    let com = Vec3::new(v[0], v[1], v[2]);
    // Every body of the robot keeps its pose relative to the chassis.
    let bodies: Vec<_> = r.parts.iter().map(|p| p.body).collect();
    let q = rot * s.rotation.inverse();
    let origin = com - rot * off;
    for b in bodies {
        let bs = duel.physics.state(b).unwrap();
        let rel = bs.position - s.position;
        duel.physics
            .set_transform(b, origin + q * rel, q * bs.rotation);
    }
    let r = &duel.robots[0];
    let worlds = r.component_worlds(&*duel.physics);
    println!("centre {com:?}");
    for (i, c) in r.rig.components.iter().enumerate() {
        let mut lo = f32::MAX;
        for piece in c
            .havok
            .collision
            .iter()
            .chain(c.havok.rotor_collision.iter())
        {
            for p in piece {
                lo = lo.min(worlds[i].transform_point3(*p).y);
            }
        }
        let hull = c
            .hull_points
            .iter()
            .map(|p| worlds[i].transform_point3(*p).y)
            .fold(f32::MAX, f32::min);
        println!(
            "#{i} {:24} collision lowest {:8.3} (centre - {:.3}) hull lowest {:8.3}",
            c.name,
            lo,
            com.y - lo,
            hull
        );
    }
    Ok(())
}
