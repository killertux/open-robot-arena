//! Diagnostic: print a robot's component tree, roles, masses and stats.
//!
//! Usage: cargo run --release -p ra2sim --example rigdump -- <bot>

use ra2sim::rig::{self, Role};

fn main() {
    let path = std::env::args().nth(1).expect("usage: rigdump <bot>");
    let r = match rig::load(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "robot {:?}: {} components, mass {:.1} kg, bounds {:?}..{:?}",
        r.name,
        r.components.len(),
        r.mass,
        r.min,
        r.max
    );
    for c in &r.components {
        let parent = c.parent.map(|p| p as i32).unwrap_or(-1);
        let extra = match c.role {
            Role::Weapon => format!(
                "weapon reach={:.2} conc={:.2} pierce={:.2}",
                c.weapon.unwrap().reach,
                c.weapon.unwrap().concussion,
                c.weapon.unwrap().piercing
            ),
            Role::Motor => {
                let m = c.motor.unwrap();
                format!(
                    "motor speed={:.0} rad/s torque={:.0} N.m",
                    m.speed, m.torque
                )
            }
            Role::Wheel => {
                let w = c.wheel.clone().unwrap_or_default();
                format!(
                    "wheel grip={:.2} resistance={:.2} contact={}",
                    w.grip,
                    w.resistance,
                    w.contact.unwrap_or_else(|| "-".into())
                )
            }
            _ => String::new(),
        };
        let pos = c.local.w_axis.truncate();
        let mut bmin = glam::Vec3::splat(f32::MAX);
        let mut bmax = glam::Vec3::splat(f32::MIN);
        for q in &c.hull_points {
            let w = c.local.transform_point3(*q);
            bmin = bmin.min(w);
            bmax = bmax.max(w);
        }
        println!(
            "  {i:2} p={parent:2} {role:9} {class:12} {name:18} m={mass:5.1} hp={hp:4.0}{damageable} pos=({px:5.2},{py:5.2},{pz:5.2}) ext=({ex:5.2},{ey:5.2},{ez:5.2}) {extra}",
            i = c.index,
            role = format!("{:?}", c.role),
            class = c.class,
            name = c.name,
            mass = c.mass,
            hp = c.hitpoints,
            damageable = if c.damageable { "" } else { " (indestructible)" },
            px = pos.x,
            py = pos.y,
            pz = pos.z,
            ex = bmax.x - bmin.x,
            ey = bmax.y - bmin.y,
            ez = bmax.z - bmin.z,
        );
    }
    let thetas = vec![0.0; r.components.len()];
    let worlds = r.worlds(&thetas);
    println!(
        "worlds: {} (all finite: {})",
        worlds.len(),
        worlds.iter().all(|m| m.is_finite())
    );

    // Where the robot actually touches the ground: bot-space lowest hull point
    // per component (the chassis' visual shell versus the wheels matters).
    let mut lowest: Vec<(f32, usize, String)> = Vec::new();
    for c in &r.components {
        let mut lo = f32::MAX;
        for q in &c.hull_points {
            lo = lo.min(c.local.transform_point3(*q).y);
        }
        if lo < f32::MAX {
            lowest.push((lo, c.index, c.name.clone()));
        }
    }
    lowest.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("  lowest points (bot y):");
    for (y, i, name) in lowest.iter().take(5) {
        println!("    {y:7.3}  #{i:<2} {name}");
    }

    // Which components are turned by a motor somewhere up the tree?
    let mut powered = 0;
    for c in &r.components {
        let mut p = c.parent;
        let mut driven = false;
        while let Some(i) = p {
            if r.components[i].motor.is_some() {
                driven = true;
                break;
            }
            p = r.components[i].parent;
        }
        if driven {
            powered += 1;
            println!("  driven: {:2} {:?} {}", c.index, c.role, c.name);
        }
    }
    println!("driven components: {powered}");

    println!("assembled bodies: {}", r.parts.len());
    for (p, part) in r.parts.iter().enumerate() {
        let names: Vec<String> = part
            .members
            .iter()
            .map(|&i| format!("{}#{}", r.components[i].name, i))
            .collect();
        let joint = part
            .joint
            .map(|j| {
                format!(
                    " <- motor #{} rate={:.0} torque={:.0} {:?} limits=({:.2},{:.2}) axis=({:.2},{:.2},{:.2}) pivot=({:.2},{:.2},{:.2})",
                    j.motor,
                    j.rate,
                    j.max_torque,
                    j.use_kind,
                    j.limits[0],
                    j.limits[1],
                    j.axis.x,
                    j.axis.y,
                    j.axis.z,
                    j.pivot.x,
                    j.pivot.y,
                    j.pivot.z,
                )
            })
            .unwrap_or_default();
        println!(
            "  part {p:2} root=#{:2} parent={:?} members={} {}{joint}",
            part.root,
            part.parent,
            part.members.len(),
            names.join(", ")
        );
        if let Some(j) = part.joint {
            let child = &r.components[part.root];
            let motor = &r.components[j.motor];
            let att = motor.attachments.get(&child.parent_mount);
            println!(
                "       mount on #{:<2} id={:<2} ({} points on the motor) -> {:?}",
                j.motor,
                child.parent_mount,
                motor.attachments.len(),
                att.map(|a| format!(
                    "{} z=({:.2},{:.2},{:.2})",
                    a.name, a.transform[2][0], a.transform[2][1], a.transform[2][2]
                )),
            );
        }
    }
}
