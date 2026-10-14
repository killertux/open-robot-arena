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
                format!("motor power={:.0} rate={:.0} rad/s", m.power, m.rate)
            }
            _ => String::new(),
        };
        println!(
            "  {i:2} p={parent:2} {role:9} {class:12} {name:22} m={mass:5.1} hp={hp:5.0}{damageable} rot={rot} {extra}",
            i = c.index,
            role = format!("{:?}", c.role),
            class = c.class,
            name = c.name,
            mass = c.mass,
            hp = c.hitpoints,
            damageable = if c.damageable { "" } else { " (indestructible)" },
            rot = c.rotor_items.len(),
        );
    }
    let thetas = vec![0.0; r.components.len()];
    let worlds = r.worlds(&thetas);
    println!(
        "worlds: {} (all finite: {})",
        worlds.len(),
        worlds.iter().all(|m| m.is_finite())
    );

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
}
