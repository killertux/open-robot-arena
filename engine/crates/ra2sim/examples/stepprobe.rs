//! Diagnostic: a §12s placement trial (docs/29) stepped from the live state
//! at every substep. Starts as `twoprobe` (hold, the live placed state, the
//! live ticks and fires, the throttles), then before every substep `i` that
//! the live capture has a resolver entry for, every body is put where the
//! original had it at that entry (`B` row: centre, rotation, velocities),
//! our detection runs there (`detect_now`, so the resolver gets the contacts
//! of the live poses) and the world steps once. What our step makes of the
//! live state is printed before the next placement, after our driving: what
//! the original's next resolver entry saw is the same instant.
//!
//! Usage: `stepprobe <arena.py> <botA> <botB> <state.txt> <poses.txt> <steps>`
//!   state.txt: as `twoprobe`'s (`mkours.py`).
//!   poses.txt: `FRAME` lines (`spincmp.py frames`: each live body as built,
//!   to turn the original's body frames into ours), then per live substep
//!   `F <step>` and one `B <ptr> <mass> cx cy cz r0..r8 vx vy vz wx wy wz` per
//!   body (rotation by columns).
//!
//! Prints twoprobe's `S` and `J` lines, `MAP <ptr> <robot> <part>` once per live body and, per step `i`
//! after the first placement, `O <i> <ptr> cx cy cz vx vy vz wx wy wz` for
//! every mapped body (our state after the step and the next driving).
//! `STEPPROBE_EVERY=<k>`: place only at every `k`-th step.
//! `STEPPROBE_FREE=<n>`: stop placing after step `n` (the run goes on alone).
//! Each joint's servo target and lock bound keep their offsets from the
//! joint's position across a placement (the controllers' state is ours; the
//! original's is not in the capture).
use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};
use std::collections::HashMap;

const HOLD: usize = 30;

/// A body at rest: handle, centre of mass, rotation, origin offset from the
/// centre (body frame), mass.
type Rest = (u32, Vec3, Quat, Vec3, f32);

struct Live {
    ptr: String,
    mass: f32,
    com: Vec3,
    rot: Quat,
    vel: Vec3,
    ang: Vec3,
}

fn floats(w: &[&str]) -> Vec<f32> {
    w.iter().map(|x| x.parse().unwrap()).collect()
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
    let steps: usize = a[5].parse()?;
    let every: usize = std::env::var("STEPPROBE_EVERY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);
    let free: usize = std::env::var("STEPPROBE_FREE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    // twoprobe's state file.
    let mut placed: Vec<(usize, Live)> = Vec::new();
    let mut throttle = [0.0f32; 2];
    let mut fires: Vec<(usize, usize)> = Vec::new();
    let mut ticks: Vec<Option<f32>> = vec![None; steps];
    for l in std::fs::read_to_string(&a[3])?.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().copied() {
            None | Some("#") | Some("phase") => {}
            Some("throttle") => throttle[w[1].parse::<usize>()?] = w[2].parse()?,
            Some("fire") => fires.push((w[1].parse()?, w[2].parse()?)),
            Some("tick") => {
                let i: usize = w[1].parse()?;
                if i < steps {
                    ticks[i] = Some(ticks[i].unwrap_or(0.0) + w[2].parse::<f32>()?);
                }
            }
            Some(_) => {
                let v = floats(&w);
                placed.push((
                    v[0] as usize,
                    Live {
                        ptr: String::new(),
                        mass: v[1],
                        com: Vec3::new(v[2], v[3], v[4]),
                        rot: Quat::from_xyzw(v[6], v[7], v[8], v[5]).normalize(),
                        vel: Vec3::new(v[9], v[10], v[11]),
                        ang: Vec3::new(v[12], v[13], v[14]),
                    },
                ));
            }
        }
    }
    // The live entries.
    let mut frames: HashMap<usize, Vec<Live>> = HashMap::new();
    let mut build: Vec<(Vec3, Quat)> = Vec::new();
    let mut cur: Option<usize> = None;
    for l in std::fs::read_to_string(&a[4])?.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().copied() {
            Some("FRAME") => {
                let v = floats(&w[1..]);
                build.push((
                    Vec3::new(v[0], v[1], v[2]),
                    Quat::from_xyzw(v[4], v[5], v[6], v[3]).normalize(),
                ));
            }
            Some("F") => {
                let s: usize = w[1].parse()?;
                cur = (!frames.contains_key(&s)).then_some(s);
                if let Some(s) = cur {
                    frames.insert(s, Vec::new());
                }
            }
            Some("B") => {
                let Some(s) = cur else { continue };
                let v = floats(&w[2..]);
                let m = Mat3::from_cols_array(&[
                    v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12],
                ]);
                frames.get_mut(&s).unwrap().push(Live {
                    ptr: w[1].to_string(),
                    mass: v[0],
                    com: Vec3::new(v[1], v[2], v[3]),
                    rot: Quat::from_mat3(&m).normalize(),
                    vel: Vec3::new(v[13], v[14], v[15]),
                    ang: Vec3::new(v[16], v[17], v[18]),
                });
            }
            _ => {}
        }
    }
    let n = duel.robots.len();
    let idle = vec![DriveInput::default(); n];
    for _ in 0..60 {
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let command: Vec<DriveInput> = (0..n)
        .map(|r| DriveInput::new(throttle.get(r).copied().unwrap_or(0.0) / 100.0, 0.0))
        .collect();
    // Rest poses: handle, centre, rotation, origin offset in the body frame.
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
                    (
                        p.body,
                        c,
                        s.rotation,
                        s.rotation.inverse() * (s.position - c),
                        m,
                    )
                })
                .collect()
        })
        .collect();
    // The turn from each live body frame to ours, from the build (orderprobe).
    let turn: Vec<Vec<Quat>> = duel
        .robots
        .iter()
        .zip(&rest)
        .map(|(robot, rr)| {
            let built = |p: usize| {
                let (_, q, _) = robot.rig.components[robot.rig.parts[p].root]
                    .local
                    .to_scale_rotation_translation();
                q
            };
            let frame = rr[0].2 * built(0).inverse();
            (0..rr.len())
                .map(|p| {
                    if p == 0 {
                        return built(0);
                    }
                    let d = frame.inverse() * (rr[p].1 - rr[0].1);
                    build
                        .iter()
                        .min_by(|x, y| x.0.distance(d).total_cmp(&y.0.distance(d)))
                        .filter(|l| l.0.distance(d) < 0.15)
                        .map_or(Quat::IDENTITY, |l| l.1.inverse() * built(p))
                })
                .collect()
        })
        .collect();
    // Hold, then the live placed state (as twoprobe).
    for _ in 0..HOLD {
        for robot in &rest {
            for &(b, c, rot, off, _) in robot {
                let at = c + Vec3::Y;
                duel.physics.set_transform(b, at + rot * off, rot);
                duel.physics.set_velocity(b, Vec3::ZERO, Vec3::ZERO);
            }
        }
        duel.step_direct(&command, DEFAULT_STEP);
    }
    for (r, robot) in rest.iter().enumerate() {
        let lv: Vec<&Live> = placed.iter().filter(|l| l.0 == r).map(|l| &l.1).collect();
        let Some(lc) = lv.first() else { continue };
        let (_, c0, r0, _, _) = robot[0];
        let q = lc.rot * r0.inverse();
        for (i, &(b, c, rot, off, _)) in robot.iter().enumerate() {
            let carried = lc.com + q * (c - c0);
            let r2 = if i == 0 { lc.rot } else { q * rot };
            let at = if i == 0 { lc.com } else { carried };
            duel.physics.set_transform(b, at + r2 * off, r2);
            duel.physics
                .set_velocity(b, lc.vel + lc.ang.cross(at - lc.com), lc.ang);
        }
    }
    if ticks.iter().any(|t| t.is_some()) {
        for r in duel.robots.iter_mut() {
            r.set_tick_schedule(ticks.iter().copied());
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
    // Live pointer -> (robot, part), by mass and where the chassis carries it.
    let mut map: HashMap<String, (usize, usize)> = HashMap::new();
    let mut last_placed: Option<usize> = None;
    for i in 0..steps {
        ra2sim::havok17::dump_line(format_args!("[\"T\",0,{i}]"));
        for &(r, s) in &fires {
            if s == i {
                let ev: Vec<(String, i32, f32)> =
                    triggers[r].iter().map(|n| (n.clone(), 0, 1.0)).collect();
                duel.robots[r].apply_inputs(&ev);
            }
        }
        // twoprobe's `S` lines, for `tools/oracle/twobody/cmp.py`.
        for (r, robot) in duel.robots.iter().enumerate() {
            let ch = robot.parts[0].body;
            let s = duel.physics.state(ch).unwrap();
            let c = duel.physics.center_of_mass(ch).unwrap();
            let up = s.rotation * Vec3::Y;
            let (w, v) = (s.angular_velocity, s.linear_velocity);
            println!(
                "S {i} {r} c {:.4} {:.4} {:.4} up {:.4} {:.4} {:.4} w {:.4} {:.4} {:.4} v {:.4} {:.4} {:.4}",
                c.x, c.y, c.z, up.x, up.y, up.z, w.x, w.y, w.z, v.x, v.y, v.z
            );
            for (p, part) in robot.parts.iter().enumerate() {
                if let (Some(j), Some(info)) = (part.joint, robot.rig.parts[p].joint) {
                    if info.slider || info.limits[0] > -3.1 {
                        let x = if info.slider {
                            duel.physics.slider_offset(j)
                        } else {
                            duel.physics.hinge_angle(j)
                        };
                        println!("J {i} {r} {p} {x:.4}");
                    }
                }
            }
        }
        let live = if i <= free && i % every == 0 {
            frames.get(&i)
        } else {
            None
        };
        duel.step_direct_with(&command, DEFAULT_STEP, &mut |duel: &mut Duel| {
            if last_placed.is_some() {
                for (ptr, &(r, p)) in &map {
                    let h = rest[r][p].0;
                    let s = duel.physics.state(h).unwrap();
                    let c = duel.physics.center_of_mass(h).unwrap();
                    let (v, w) = (s.linear_velocity, s.angular_velocity);
                    println!(
                        "O {i} {ptr} {} {} {} {} {} {} {} {} {}",
                        c.x, c.y, c.z, v.x, v.y, v.z, w.x, w.y, w.z
                    );
                }
            }
            let Some(bodies) = live else { return };
            for l in bodies {
                if map.contains_key(&l.ptr) {
                    continue;
                }
                if let Some(r) = rest.iter().position(|r| (r[0].4 - l.mass).abs() < 0.05) {
                    map.insert(l.ptr.clone(), (r, 0));
                    println!("MAP {} {r} 0", l.ptr);
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
                    let q = lc.rot * turn[r][0] * robot[0].2.inverse();
                    for (p, b) in robot.iter().enumerate().skip(1) {
                        if (b.4 - l.mass).abs() > 0.05 || map.values().any(|&k| k == (r, p)) {
                            continue;
                        }
                        let d = (lc.com + q * (b.1 - robot[0].1)).distance(l.com);
                        if best.is_none_or(|x| d < x.0) {
                            best = Some((d, r, p));
                        }
                    }
                }
                if let Some((_, r, p)) = best.filter(|b| b.0 < 0.5) {
                    map.insert(l.ptr.clone(), (r, p));
                    println!("MAP {} {r} {p}", l.ptr);
                }
            }
            // The joints' controllers keep their errors about the bodies'
            // new places (`STEPPROBE_RAW=1`: they keep their targets).
            let errors = duel.physics.joint_errors();
            for l in bodies {
                let Some(&(r, p)) = map.get(&l.ptr) else {
                    continue;
                };
                let (h, _, _, off, _) = rest[r][p];
                let rot = l.rot * turn[r][p];
                duel.physics.set_transform(h, l.com + rot * off, rot);
                duel.physics.set_velocity(h, l.vel, l.ang);
            }
            if std::env::var_os("STEPPROBE_RAW").is_none() {
                duel.physics.set_joint_errors(&errors);
            }
            duel.physics.detect_now();
            last_placed = Some(i);
        });
    }
    Ok(())
}
