//! Diagnostic: every rigid body of each robot as built, in the form the live
//! `P`/`B` rows of `frida/ra2_fightcontacts.js` give them (docs/29 §12q): mass,
//! local inertia (diagonal, products, principal moments), and the centre of
//! mass and inertia in the chassis body's frame; plus each body's collision
//! points' extent in the chassis frame.
//!
//! Usage: `bodycmp <arena.py> <bot>...`; with `BODYCMP_SETTLE=<seconds>` the
//! robots first stand that long with no input, and each chassis' centre of
//! mass height and axes are printed every 0.25 s (the live rest pose).
use glam::{Mat3, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

fn r4(v: Vec3) -> String {
    format!("[{:.4}, {:.4}, {:.4}]", v.x, v.y, v.z)
}

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
    let settle: f32 = std::env::var("BODYCMP_SETTLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let steps = (settle / DEFAULT_STEP) as usize;
    let idle = vec![DriveInput::default(); duel.robots.len()];
    for i in 0..steps {
        duel.step_direct(&idle, DEFAULT_STEP);
        if (i + 1) % 15 == 0 {
            for r in &duel.robots {
                let b = r.parts[0].body;
                let s = duel.physics.state(b).unwrap();
                let c = duel.physics.center_of_mass(b).unwrap();
                let m = Mat3::from_quat(s.rotation);
                println!(
                    "t {:.2} {} y {:.4} up {} fwd.y {:.4} side.y {:.4}",
                    (i + 1) as f32 * DEFAULT_STEP,
                    r.rig.name,
                    c.y,
                    r4(m.y_axis),
                    m.z_axis.y,
                    m.x_axis.y
                );
            }
        }
    }
    for r in &duel.robots {
        let cb = r.parts[0].body;
        let cs = duel.physics.state(cb).unwrap();
        let ccom = duel.physics.center_of_mass(cb).unwrap();
        let rc = Mat3::from_quat(cs.rotation);
        let worlds = r.component_worlds(&*duel.physics);
        println!("== {}", r.rig.name);
        for (p, part) in r.rig.parts.iter().enumerate() {
            let b = r.parts[p].body;
            let Some((mass, _, inv)) = duel.physics.mass_properties(b) else {
                continue;
            };
            let s = duel.physics.state(b).unwrap();
            let com = duel.physics.center_of_mass(b).unwrap();
            let i = inv.inverse();
            let rb = Mat3::from_quat(s.rotation);
            let rr = rc.transpose() * rb;
            let ic = rr * i * rr.transpose();
            let names: Vec<&str> = part
                .members
                .iter()
                .map(|&m| r.rig.components[m].name.as_str())
                .collect();
            println!("part {p} m {mass:.3} {names:?}");
            println!(
                "   rot in chassis (rows) {} {} {}",
                r4(rr.row(0)),
                r4(rr.row(1)),
                r4(rr.row(2))
            );
            println!(
                "   Ibody diag {} off(xy,xz,yz) {} ",
                r4(Vec3::new(i.x_axis.x, i.y_axis.y, i.z_axis.z)),
                r4(Vec3::new(i.y_axis.x, i.z_axis.x, i.z_axis.y)),
            );
            println!(
                "   com in chassis {} I in chassis diag {} off {}",
                r4(rc.transpose() * (com - ccom)),
                r4(Vec3::new(ic.x_axis.x, ic.y_axis.y, ic.z_axis.z)),
                r4(Vec3::new(ic.y_axis.x, ic.z_axis.x, ic.z_axis.y)),
            );
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for &m in &part.members {
                let c = &r.rig.components[m];
                for piece in c
                    .havok
                    .collision
                    .iter()
                    .chain(c.havok.rotor_collision.iter())
                {
                    for q in piece {
                        let w = worlds[m].transform_point3(*q);
                        let l = rc.transpose() * (w - ccom);
                        lo = lo.min(l);
                        hi = hi.max(l);
                    }
                }
            }
            if lo.x <= hi.x {
                println!("   points in chassis {} .. {}", r4(lo), r4(hi));
            }
        }
        for (m, c) in r.rig.components.iter().enumerate() {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for piece in c
                .havok
                .collision
                .iter()
                .chain(c.havok.rotor_collision.iter())
            {
                for q in piece {
                    let w = worlds[m].transform_point3(*q);
                    let l = rc.transpose() * (w - ccom);
                    lo = lo.min(l);
                    hi = hi.max(l);
                }
            }
            let (_, cr, ct) = worlds[m].to_scale_rotation_translation();
            let crr = rc.transpose() * Mat3::from_quat(cr);
            println!(
                "   comp #{m} {:24} frame rows {} {} {} at {}",
                c.name,
                r4(crr.row(0)),
                r4(crr.row(1)),
                r4(crr.row(2)),
                r4(rc.transpose() * (ct - ccom))
            );
            if lo.x <= hi.x {
                println!(
                    "   comp #{m} {:24} part {} pts {} .. {}",
                    c.name,
                    r.rig.part_of[m],
                    r4(lo),
                    r4(hi)
                );
            }
        }
    }
    Ok(())
}
