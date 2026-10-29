//! Diagnostic: two robots placed at the bodies' states of a live contact
//! capture (a `fightcontacts` B-row snapshot, written by the scratch
//! `extract.py` as one line per body: mass, centre of mass, velocity, angular
//! velocity, rotation column by column), then stepped with fixed commands:
//! both drive straight at `throttle`, each fires its weapons at its own time.
//!
//! Usage: `meetprobe <arena.py> <botA> <botB> <state.txt> <seconds>
//!         <fireA s|-> <fireB s|-> [throttleA throttleB | commands.txt]`
//!
//! `commands.txt`: lines `t robot throttle steer` (seconds from the start),
//! each holding until the robot's next line (the live MagicAction thrusts
//! turned back into commands).
//!
//! Every body of a robot keeps its pose relative to the chassis as our
//! assembly has it (settled), moved rigidly onto the live chassis; the live
//! wheels and fork (matched by mass, nearest centre) give their own velocity
//! and angular velocity. `RA2_H17_DUMP` writes the same rows as the original's
//! capture, so one script reads both (docs/29 section 12n).
use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

struct LiveBody {
    mass: f32,
    com: Vec3,
    vel: Vec3,
    ang: Vec3,
    rot: Quat,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let secs: f32 = a[4].parse()?;
    let fire: Vec<Option<f32>> = [&a[5], &a[6]].iter().map(|s| s.parse().ok()).collect();
    let throttle: Vec<f32> = [a.get(7), a.get(8)]
        .iter()
        .map(|s| s.and_then(|s| s.parse().ok()).unwrap_or(1.0))
        .collect();
    let commands: Vec<(f32, usize, f32, f32)> = match a.get(7) {
        Some(p) if p.parse::<f32>().is_err() => std::fs::read_to_string(p)?
            .lines()
            .filter_map(|l| {
                let v: Vec<&str> = l.split_whitespace().collect();
                Some((
                    v.first()?.parse().ok()?,
                    v.get(1)?.parse().ok()?,
                    v.get(2)?.parse().ok()?,
                    v.get(3)?.parse().ok()?,
                ))
            })
            .collect(),
        _ => Vec::new(),
    };
    let live: Vec<LiveBody> = std::fs::read_to_string(&a[3])?
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            let m = Mat3::from_cols_array(&[
                v[10], v[11], v[12], v[13], v[14], v[15], v[16], v[17], v[18],
            ]);
            LiveBody {
                mass: v[0],
                com: Vec3::new(v[1], v[2], v[3]),
                vel: Vec3::new(v[4], v[5], v[6]),
                ang: Vec3::new(v[7], v[8], v[9]),
                rot: Quat::from_mat3(&m).normalize(),
            }
        })
        .collect();
    let mut duel = Duel::new(
        &a[0],
        &[
            BotSetup {
                path: a[1].clone(),
                yaw_deg: None,
                spawn: None,
            },
            BotSetup {
                path: a[2].clone(),
                yaw_deg: None,
                spawn: None,
            },
        ],
    )?;
    let n = duel.robots.len();
    for _ in 0..60 {
        duel.step_direct(&vec![DriveInput::default(); n], DEFAULT_STEP);
    }
    // The chassis of each robot: the heaviest live body nearest its mass.
    for r in 0..n {
        let chassis = duel.robots[r].parts[0].body;
        let (cm, _, _) = duel.physics.mass_properties(chassis).unwrap();
        let lc = live
            .iter()
            .min_by(|x, y| (x.mass - cm).abs().total_cmp(&(y.mass - cm).abs()))
            .unwrap();
        let s0 = duel.physics.state(chassis).unwrap();
        let c0 = duel.physics.center_of_mass(chassis).unwrap();
        let q = lc.rot * s0.rotation.inverse();
        let bodies: Vec<_> = duel.robots[r].parts.iter().map(|p| p.body).collect();
        for &b in &bodies {
            let s = duel.physics.state(b).unwrap();
            let com = duel.physics.center_of_mass(b).unwrap();
            let pos = lc.com + q * (s.position - c0);
            duel.physics.set_transform(b, pos, q * s.rotation);
            let new_com = lc.com + q * (com - c0);
            duel.physics
                .set_velocity(b, lc.vel + lc.ang.cross(new_com - lc.com), lc.ang);
        }
        // Wheels and the like: a live body of the same mass, not a chassis,
        // within 1.5 m of this chassis.
        for &b in &bodies[1..] {
            let (m, _, _) = duel.physics.mass_properties(b).unwrap();
            let com = duel.physics.center_of_mass(b).unwrap();
            let best = live
                .iter()
                .filter(|l| (l.mass - m).abs() < 0.01 && l.com.distance(lc.com) < 1.5)
                .min_by(|x, y| x.com.distance(com).total_cmp(&y.com.distance(com)));
            if let Some(l) = best {
                let s = duel.physics.state(b).unwrap();
                let (ax, an) = (s.rotation.inverse() * l.rot).to_axis_angle();
                println!(
                    "robot {r} body {b} {m:.1} kg: live centre {:.3} away, rotation off {:.3} rad about {ax:.2}",
                    l.com.distance(com),
                    an
                );
                duel.physics.set_velocity(b, l.vel, l.ang);
            }
        }
    }
    let triggers: Vec<Vec<String>> = duel
        .robots
        .iter()
        .map(|r| {
            r.rig
                .wiring
                .controls
                .iter()
                .filter(|c| !c.name.is_empty())
                .map(|c| c.name.clone())
                .filter(|n| n != "LeftRight" && n != "Forward")
                .collect()
        })
        .collect();
    println!("fire controls {triggers:?}");
    let steps = (secs / DEFAULT_STEP).round() as usize;
    let mut fired = [false; 2];
    for i in 0..steps {
        let t = i as f32 * DEFAULT_STEP;
        for r in 0..n.min(2) {
            if let Some(ft) = fire[r] {
                if !fired[r] && t >= ft {
                    fired[r] = true;
                    let ev: Vec<(String, i32, f32)> =
                        triggers[r].iter().map(|n| (n.clone(), 0, 1.0)).collect();
                    duel.robots[r].apply_inputs(&ev);
                }
            }
        }
        let inputs: Vec<DriveInput> = (0..n)
            .map(|r| {
                commands
                    .iter()
                    .rfind(|c| c.1 == r && c.0 <= t + 1e-4)
                    .map(|c| DriveInput::new(c.2, c.3))
                    .unwrap_or(DriveInput::new(throttle[r.min(1)], 0.0))
            })
            .collect();
        duel.step_direct(&inputs, DEFAULT_STEP);
        let mut line = format!("{:6.3}", t + DEFAULT_STEP);
        for r in &duel.robots {
            let s = duel.physics.state(r.parts[0].body).unwrap();
            let up = s.rotation * Vec3::Y;
            line += &format!(
                " | y {:6.3} vy {:5.2} up.y {:5.2} w {:5.2}",
                duel.physics.center_of_mass(r.parts[0].body).unwrap().y,
                s.linear_velocity.y,
                up.y,
                s.angular_velocity.length()
            );
            for (p, part) in r.parts.iter().enumerate() {
                let (Some(j), Some(info)) = (part.joint, r.rig.parts[p].joint) else {
                    continue;
                };
                if info.slider {
                    line += &format!(" s{p} {:.3}", duel.physics.slider_offset(j));
                } else if info.use_kind == ra2sim::rig::JointUse::Burst {
                    line += &format!(" h{p} {:.3}", duel.physics.hinge_angle(j));
                }
            }
        }
        println!("{line}");
    }
    Ok(())
}
