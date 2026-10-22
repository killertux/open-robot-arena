//! Diagnostic: two robots of a box match (the Scout, robot 0, and ALARM,
//! robot 1) placed exactly where `frida/ra2_twobody.js` placed them in the
//! original and let go (docs/29 section 12s). Both are held still `HOLD`
//! substeps 1 m above their rest poses, then both are placed as the live
//! placed state (the `X` 'place' row) has them: each chassis at its live pose,
//! the other bodies carried rigidly with it, every body at the velocities of
//! the live body of its robot with its mass nearest it.
//!
//! Usage: `twoprobe <arena.py> <botA> <botB> <state.txt> <steps>`
//!   state.txt: one line per live body, `robot mass cx cy cz qw qx qy qz vx vy vz wx wy wz`,
//!   and optional lines `throttle <robot> <value -100..100>` (from the start of
//!   the hold, as the capture sets the AIs before it), `fire <robot> <step>`
//!   (a trigger press taken by the component tick of that substep after the
//!   release), `phase <tick parity>` (whether the components tick on even
//!   (0) or odd (1) substeps after the release) or `tick <step> <dt>` (the
//!   live ticks: the components tick at those substeps only, with that time).
//!
//! Prints `MAP <robot> <live index> h<handle>` for every body and, every
//! substep, `S <step> <robot> c x y z up x y z w x y z v x y z` for both
//! chassis and `J <step> <robot> <part> <angle or offset>` for every driven
//! joint. `RA2_H17_DUMP` writes the capture rows (`R`, `B`, `V`, `A`, `M`)
//! with a `["T",0,<step>]` row at the head of every substep after the release.
use glam::{Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

const HOLD: usize = 30;

struct LiveBody {
    robot: usize,
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
    let steps: usize = a[4].parse()?;
    let mut live = Vec::new();
    let mut throttle = [0.0f32; 2];
    let mut fires: Vec<(usize, usize)> = Vec::new();
    let mut parity: Option<usize> = None;
    let mut ticks: Vec<Option<f32>> = vec![None; steps];
    for l in std::fs::read_to_string(&a[3])?.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().copied() {
            None | Some("#") => {}
            Some("throttle") => throttle[w[1].parse::<usize>()?] = w[2].parse()?,
            Some("fire") => fires.push((w[1].parse()?, w[2].parse()?)),
            Some("phase") => parity = Some(w[1].parse()?),
            Some("tick") => {
                let i: usize = w[1].parse()?;
                if i < steps {
                    ticks[i] = Some(ticks[i].unwrap_or(0.0) + w[2].parse::<f32>()?);
                }
            }
            Some(_) => {
                let v: Vec<f32> = w.iter().map(|x| x.parse().unwrap()).collect();
                live.push(LiveBody {
                    robot: v[0] as usize,
                    mass: v[1],
                    com: Vec3::new(v[2], v[3], v[4]),
                    rot: Quat::from_xyzw(v[6], v[7], v[8], v[5]).normalize(),
                    vel: Vec3::new(v[9], v[10], v[11]),
                    ang: Vec3::new(v[12], v[13], v[14]),
                });
            }
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
    // Every body's rest pose (handle, centre of mass, rotation, origin offset
    // from the centre in the body frame).
    let rest: Vec<Vec<(u32, Vec3, Quat, Vec3)>> = duel
        .robots
        .iter()
        .map(|r| {
            r.parts
                .iter()
                .map(|p| {
                    let s = duel.physics.state(p.body).unwrap();
                    let c = duel.physics.center_of_mass(p.body).unwrap();
                    (
                        p.body,
                        c,
                        s.rotation,
                        s.rotation.inverse() * (s.position - c),
                    )
                })
                .collect()
        })
        .collect();
    if std::env::var_os("TWOPROBE_REST").is_some() {
        // Our rest poses, as the capture's `REST` row (centre, quaternion w x y z).
        for (r, robot) in rest.iter().enumerate() {
            for &(b, c, q, _) in robot {
                let (m, _, _) = duel.physics.mass_properties(b).unwrap();
                println!(
                    "REST {r} {m} {} {} {} {} {} {} {}",
                    c.x, c.y, c.z, q.w, q.x, q.y, q.z
                );
            }
        }
        return Ok(());
    }
    let mut hold = HOLD;
    let mut k = 0;
    loop {
        for robot in &rest {
            for &(b, c, rot, off) in robot {
                let at = c + Vec3::Y;
                duel.physics.set_transform(b, at + rot * off, rot);
                duel.physics.set_velocity(b, Vec3::ZERO, Vec3::ZERO);
            }
        }
        duel.step_direct(&command, DEFAULT_STEP);
        k += 1;
        if k == hold {
            // The components tick when the frame clock is due; match the
            // live parity of the ticks after the release.
            if let Some(p) = parity {
                let due = duel.robots[0].tick_due();
                if due != (p == 0) {
                    hold += 1;
                }
            }
        }
        if k >= hold {
            break;
        }
    }
    // Place every body at the live state.
    for (r, robot) in rest.iter().enumerate() {
        let lv: Vec<&LiveBody> = live.iter().filter(|l| l.robot == r).collect();
        let Some(lc) = lv.first() else { continue };
        let (_, c0, r0, _) = robot[0];
        let q = lc.rot * r0.inverse();
        let mut used = vec![false; lv.len()];
        for (i, &(b, c, rot, off)) in robot.iter().enumerate() {
            let (m, _, _) = duel.physics.mass_properties(b).unwrap();
            let carried = lc.com + q * (c - c0);
            let best = if i == 0 {
                Some(0)
            } else {
                lv.iter()
                    .enumerate()
                    .filter(|(j, l)| !used[*j] && *j > 0 && (l.mass - m).abs() < 0.05)
                    .min_by(|x, y| {
                        x.1.com
                            .distance(carried)
                            .total_cmp(&y.1.com.distance(carried))
                    })
                    .map(|(j, _)| j)
            };
            match best {
                Some(j) => {
                    used[j] = true;
                    let l = lv[j];
                    println!(
                        "MAP {r} {j} h{b} {m:.3} kg, {:.4} m from carried",
                        l.com.distance(carried)
                    );
                    // Carried rigidly with the chassis (the bodies' own frames
                    // need not be the original's), at the live velocities.
                    let r2 = if i == 0 { l.rot } else { q * rot };
                    let at = if i == 0 { l.com } else { carried };
                    duel.physics.set_transform(b, at + r2 * off, r2);
                    duel.physics.set_velocity(b, l.vel, l.ang);
                }
                None => {
                    println!("MAP {r} - h{b} {m:.3} kg: no live body, carried");
                    let r2 = q * rot;
                    duel.physics.set_transform(b, carried + r2 * off, r2);
                    duel.physics
                        .set_velocity(b, lc.vel + lc.ang.cross(carried - lc.com), lc.ang);
                }
            }
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
    for i in 0..steps {
        ra2sim::havok17::dump_line(format_args!("[\"T\",0,{i}]"));
        for &(r, s) in &fires {
            if s == i {
                let ev: Vec<(String, i32, f32)> =
                    triggers[r].iter().map(|n| (n.clone(), 0, 1.0)).collect();
                duel.robots[r].apply_inputs(&ev);
            }
        }
        for (r, robot) in duel.robots.iter().enumerate() {
            let ch = robot.parts[0].body;
            let s = duel.physics.state(ch).unwrap();
            let c = duel.physics.center_of_mass(ch).unwrap();
            let up = s.rotation * Vec3::Y;
            let w = s.angular_velocity;
            let v = s.linear_velocity;
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
        duel.step_direct(&command, DEFAULT_STEP);
    }
    Ok(())
}
