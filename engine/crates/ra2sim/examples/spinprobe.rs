//! Diagnostic: how far every hinged body of the robots has turned about its
//! hinge between the build and the hold (docs/29 section 12w). The original
//! places a robot by turning its chassis alone and lets the joints pull the
//! rest round (`duel::turn_into_place`), which leaves every wheel turned about
//! its axle by some angle; `tools/oracle/spincmp.py` takes the same from a
//! live `rest-*` capture.
//!
//! Usage: `spinprobe <arena.py> <botA> <botB>`; prints per robot and body
//! `P <robot> <part> <mass> <dx> <dy> <dz> <angle> <ax> <ay> <az>`: the body's
//! centre of mass at the build relative to the chassis', in the robot's frame,
//! and the turn from the build to now (robot frame, relative to the chassis),
//! as an angle about the hinge's axis (the axis of that turn printed too).
use glam::{Quat, Vec3};
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
    for (ri, r) in duel.robots.iter().enumerate() {
        let chassis_local = r.rig.components[r.rig.parts[0].root].local;
        let (_, c_local_rot, _) = chassis_local.to_scale_rotation_translation();
        let cs = duel.physics.state(r.body()).unwrap();
        // The robot's frame now: the chassis body's rotation less its own.
        let frame = cs.rotation * c_local_rot.inverse();
        let ccom = duel.physics.center_of_mass(r.body()).unwrap();
        let (cm, _, _) = duel.physics.mass_properties(r.body()).unwrap();
        println!(
            "R {ri} {:?} chassis {cm:.2} kg rot {:.4?} centre {:.3?}",
            r.rig.name, cs.rotation, ccom
        );
        for (p, part) in r.rig.parts.iter().enumerate().skip(1) {
            let s = duel.physics.state(r.parts[p].body).unwrap();
            let (m, _, _) = duel.physics.mass_properties(r.parts[p].body).unwrap();
            let com = duel.physics.center_of_mass(r.parts[p].body).unwrap();
            let local = r.rig.components[part.root].local;
            let (_, built, _) = local.to_scale_rotation_translation();
            let rel: Quat = frame.inverse() * s.rotation * built.inverse();
            let (ax, ang) = rel.to_axis_angle();
            let hinge = part.joint.map(|j| j.axis).unwrap_or(Vec3::ZERO);
            let signed = if ax.dot(hinge) < 0.0 { -ang } else { ang };
            let d = frame.inverse() * (com - ccom);
            println!(
                "P {ri} {p} {m:.2} {:.3} {:.3} {:.3} {:.4} {:.3} {:.3} {:.3}",
                d.x, d.y, d.z, signed, ax.x, ax.y, ax.z
            );
        }
    }
    // `LAND=<substeps>`: then let the robots fall and print every body's
    // centre height (and vertical speed) per substep, `L <step> <robot>
    // <y/vy>...`, chassis first.
    if let Some(n) = std::env::var("LAND")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        let idle = vec![ra2sim::DriveInput::default(); duel.robots.len()];
        for k in 0..n {
            for (ri, r) in duel.robots.iter().enumerate() {
                let cells: Vec<String> = r
                    .parts
                    .iter()
                    .filter_map(|p| {
                        let c = duel.physics.center_of_mass(p.body)?;
                        let s = duel.physics.state(p.body)?;
                        Some(format!("{:.4}/{:+.2}", c.y, s.linear_velocity.y))
                    })
                    .collect();
                println!("L {k} {ri} {}", cells.join(" "));
            }
            duel.step_direct(&idle, ra2sim::DEFAULT_STEP);
        }
    }
    Ok(())
}
