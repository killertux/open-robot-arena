//! Diagnostic: one robot in an arena, driven by a script, printing what the
//! physics backend and the game's drive model see.
//!
//! Usage: `RA2_PHYSICS=havok17 h17probe <arena.py> <bot> <script> <seconds> [every]`
//! (the script is `fidtrace`'s: `start-end:throttle,turn;...`).
use ra2sim::{BotSetup, DriveScript, Duel, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let script = DriveScript::parse(&a[2]);
    let secs: f32 = a[3].parse()?;
    let every: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(15);
    let mut duel = Duel::new(
        &a[0],
        &[BotSetup {
            path: a[1].clone(),
            yaw_deg: None,
            spawn: None,
        }],
    )?;
    let r = &duel.robots[0];
    for (p, part) in r.parts.iter().enumerate() {
        println!(
            "part {p}: body {} mass {:?} com {:?} origin {:?}",
            part.body,
            duel.physics.mass(part.body),
            duel.physics.center_of_mass(part.body),
            duel.physics.state(part.body).map(|s| s.position)
        );
    }
    if let Some(m) = &r.magic {
        for w in &m.wheels {
            println!(
                "wheel part {} motor {:?} point {:?} axle {:?} grip {} resistance {}",
                w.part, w.motor, w.point, w.axle, w.grip, w.resistance
            );
        }
    }
    let steps = (secs / DEFAULT_STEP) as usize;
    for i in 0..steps {
        let t = i as f32 * DEFAULT_STEP;
        let input = script.at(t);
        duel.step_direct(&[input], DEFAULT_STEP);
        if i % every == 0 {
            let r = &duel.robots[0];
            let s = duel.physics.state(r.parts[0].body).unwrap();
            let wheels = r
                .magic
                .as_ref()
                .map(|m| {
                    m.wheels
                        .iter()
                        .map(|w| {
                            let hub = duel.physics.state(r.parts[w.part].body).unwrap();
                            // Spin about the axle against the rolling spin
                            // (n × v) / r for the hub's own velocity (r 0.185).
                            let axle = (s.rotation * w.axle).normalize_or_zero();
                            let roll = glam::Vec3::Y.cross(hub.linear_velocity) / 0.185;
                            format!(
                                "[th {:.0} t {} w {:.1} spin {:.1} roll {:.1}]",
                                w.thrust,
                                w.touching as u8,
                                hub.angular_velocity.length(),
                                hub.angular_velocity.dot(axle),
                                roll.dot(axle)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            println!(
                "t {t:5.2} y {:.3} v ({:.2},{:.2},{:.2}) |w| {:.2} {wheels}",
                s.position.y,
                s.linear_velocity.x,
                s.linear_velocity.y,
                s.linear_velocity.z,
                s.angular_velocity.length()
            );
        }
    }
    let r = &duel.robots[0];
    let worlds = r.component_worlds(&*duel.physics);
    for (i, c) in r.rig.components.iter().enumerate() {
        let lowest = |pieces: &[Vec<glam::Vec3>]| {
            pieces
                .iter()
                .flatten()
                .map(|p| worlds[i].transform_point3(*p).y)
                .fold(f32::MAX, f32::min)
        };
        println!(
            "  component {i:2} {:24} lowest collision {:.3} rotor {:.3} phantom {:.3} hull {:.3}",
            c.name,
            lowest(&c.havok.collision),
            lowest(&c.havok.rotor_collision),
            lowest(std::slice::from_ref(&c.havok.phantom)),
            lowest(std::slice::from_ref(&c.hull_points)),
        );
    }
    Ok(())
}
