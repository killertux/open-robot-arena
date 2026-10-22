//! Diagnostic: the contacts our detection hands the resolver with every body
//! where the original had it (docs/29 section 12u). For every resolver entry of
//! a live capture (`frida/ra2_contacts.js` `R` rows with their `B` rows, or
//! `ra2_twobody.js`'s) every body is placed at the live pose and our detection
//! runs once (`PhysicsBackend::detect_now`), so the manifolds age with live's
//! poses; `RA2_H17_DUMP` writes our `R` rows, each after a `["T",0,<seq>]` row
//! carrying the live row's sequence number. `tools/oracle/twobody/ordercmp.py`
//! compares the two lists.
//!
//! Usage: `orderprobe <arena.py> <botA> <botB> <poses.txt>`
//!   poses.txt (from `ordercmp.py poses`): `F <seq>` at every live resolver
//!   entry, then one `B <ptr> <mass> cx cy cz r0..r8 vx vy vz wx wy wz` per
//!   live body (centre of mass, rotation by columns), and `REST cx cy cz qw qx
//!   qy qz` lines: every live body at rest (`twobody-...__live__000`'s `REST`
//!   row). The original's body frames are not ours (a wheel's, the fork's and
//!   the rod's differ by a fixed turn): each body's rotation is the live one
//!   times the turn from its live rest frame to ours (the live rest body
//!   nearest ours, after our 60 substeps of settling).
//!
//! Prints `MAP <ptr> <robot> <part> h<handle>` when a live body is first seen
//! (robot by its chassis' mass, part by mass and nearness to where our rest
//! pose carries it), with the angle between its live rotation and ours
//! relative to the chassis then. Bodies not seen yet are carried rigidly with
//! their chassis.
use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, Duel};
use std::collections::HashMap;

struct Rest {
    handle: u32,
    mass: f32,
    com: Vec3,
    rot: Quat,
    /// The origin's offset from the centre of mass, body frame.
    off: Vec3,
}

struct Live {
    ptr: String,
    mass: f32,
    com: Vec3,
    rot: Quat,
    vel: Vec3,
    ang: Vec3,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let setups: Vec<BotSetup> = a[1..3]
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let mut duel = Duel::new(&a[0], &setups)?;
    let idle = vec![ra2sim::DriveInput::default(); duel.robots.len()];
    for _ in 0..60 {
        duel.step_direct(&idle, ra2sim::DEFAULT_STEP);
    }
    let rest: Vec<Vec<Rest>> = duel
        .robots
        .iter()
        .map(|r| {
            r.parts
                .iter()
                .map(|p| {
                    let s = duel.physics.state(p.body).unwrap();
                    let c = duel.physics.center_of_mass(p.body).unwrap();
                    let (m, _, _) = duel.physics.mass_properties(p.body).unwrap();
                    Rest {
                        handle: p.body,
                        mass: m,
                        com: c,
                        rot: s.rotation,
                        off: s.rotation.inverse() * (s.position - c),
                    }
                })
                .collect()
        })
        .collect();
    // Live pointer → (robot, part).
    let mut map: HashMap<String, (usize, usize)> = HashMap::new();
    let text = std::fs::read_to_string(&a[3])?;
    let mut frames: Vec<(String, Vec<Live>)> = Vec::new();
    let mut live_rest: Vec<(Vec3, Quat)> = Vec::new();
    for l in text.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().copied() {
            Some("REST") => {
                let v: Vec<f32> = w[1..].iter().map(|x| x.parse().unwrap()).collect();
                live_rest.push((
                    Vec3::new(v[0], v[1], v[2]),
                    Quat::from_xyzw(v[4], v[5], v[6], v[3]).normalize(),
                ));
            }
            Some("F") => frames.push((w[1].to_string(), Vec::new())),
            Some("B") => {
                let v: Vec<f32> = w[2..].iter().map(|x| x.parse().unwrap()).collect();
                let m = Mat3::from_cols_array(&[
                    v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12],
                ]);
                if let Some(f) = frames.last_mut() {
                    f.1.push(Live {
                        ptr: w[1].to_string(),
                        mass: v[0],
                        com: Vec3::new(v[1], v[2], v[3]),
                        rot: Quat::from_mat3(&m).normalize(),
                        vel: Vec3::new(v[13], v[14], v[15]),
                        ang: Vec3::new(v[16], v[17], v[18]),
                    });
                }
            }
            _ => {}
        }
    }
    // Per body: the turn from the live frame to ours.
    let nearest = |c: Vec3| {
        live_rest
            .iter()
            .min_by(|x, y| x.0.distance(c).total_cmp(&y.0.distance(c)))
            .filter(|l| l.0.distance(c) < 0.1)
            .map(|l| l.1)
    };
    // Relative to the chassis (whose frames agree; the rest poses themselves
    // differ by a few centimetres and hundredths of a radian).
    let turn: Vec<Vec<Quat>> = rest
        .iter()
        .map(|robot| {
            let lc = nearest(robot[0].com);
            robot
                .iter()
                .enumerate()
                .map(|(p, b)| match (p, lc, nearest(b.com)) {
                    (0, ..) | (_, None, _) | (_, _, None) => Quat::IDENTITY,
                    (_, Some(lc), Some(lb)) => {
                        (lc.inverse() * lb).inverse() * (robot[0].rot.inverse() * b.rot)
                    }
                })
                .collect()
        })
        .collect();
    for (r, robot) in turn.iter().enumerate() {
        for (p, t) in robot.iter().enumerate() {
            println!("TURN {r} {p} {:.3} rad", t.angle_between(Quat::IDENTITY));
        }
    }
    for (seq, bodies) in &frames {
        // Learn the bodies first seen here.
        for l in bodies {
            if map.contains_key(&l.ptr) {
                continue;
            }
            if let Some(r) = rest.iter().position(|r| (r[0].mass - l.mass).abs() < 0.05) {
                map.insert(l.ptr.clone(), (r, 0));
                println!("MAP {} {r} 0 h{}", l.ptr, rest[r][0].handle);
            }
        }
        let chassis: Vec<Option<&Live>> = (0..rest.len())
            .map(|r| bodies.iter().find(|l| map.get(&l.ptr) == Some(&(r, 0))))
            .collect();
        for l in bodies {
            if map.contains_key(&l.ptr) {
                continue;
            }
            let mut best: Option<(f32, usize, usize)> = None;
            for (r, robot) in rest.iter().enumerate() {
                let Some(lc) = chassis[r] else { continue };
                let q = lc.rot * turn[r][0] * robot[0].rot.inverse();
                for (p, b) in robot.iter().enumerate().skip(1) {
                    if (b.mass - l.mass).abs() > 0.05 || map.values().any(|&k| k == (r, p)) {
                        continue;
                    }
                    let carried = lc.com + q * (b.com - robot[0].com);
                    let d = carried.distance(l.com);
                    if best.is_none_or(|x| d < x.0) {
                        best = Some((d, r, p));
                    }
                }
            }
            // A part is taken only near where its chassis carries it: both
            // robots may have parts of one mass (the Scout's and ALARM's
            // 27 kg wheels), and a robot's chassis may not be seen yet.
            if let Some((d, r, p)) = best.filter(|b| b.0 < 0.5) {
                map.insert(l.ptr.clone(), (r, p));
                let lc = chassis[r].unwrap();
                let ours = rest[r][0].rot.inverse() * rest[r][p].rot;
                let live = (lc.rot * turn[r][0]).inverse() * (l.rot * turn[r][p]);
                println!(
                    "MAP {} {r} {p} h{} at seq {seq}, {d:.3} m from carried, frames {:.3} rad apart",
                    l.ptr,
                    rest[r][p].handle,
                    ours.angle_between(live)
                );
            }
        }
        // Place every body: the live ones as live, the others carried.
        for (r, robot) in rest.iter().enumerate() {
            let lc = chassis[r];
            for (p, b) in robot.iter().enumerate() {
                let live = bodies.iter().find(|l| map.get(&l.ptr) == Some(&(r, p)));
                let (com, rot, vel, ang) = match (live, lc) {
                    (Some(l), _) => (l.com, l.rot * turn[r][p], l.vel, l.ang),
                    (None, Some(lc)) => {
                        let q = lc.rot * turn[r][0] * robot[0].rot.inverse();
                        let c = lc.com + q * (b.com - robot[0].com);
                        (c, q * b.rot, lc.vel + lc.ang.cross(c - lc.com), lc.ang)
                    }
                    (None, None) => (b.com, b.rot, Vec3::ZERO, Vec3::ZERO),
                };
                duel.physics.set_transform(b.handle, com + rot * b.off, rot);
                duel.physics.set_velocity(b.handle, vel, ang);
            }
        }
        ra2sim::havok17::dump_line(format_args!("[\"T\",0,{seq}]"));
        duel.physics.detect_now();
    }
    Ok(())
}
