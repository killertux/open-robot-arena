//! Diagnostic: fire a robot's first burst motor from rest in the box and print
//! the hinge's angle and speed every substep (compare with
//! `weapons-scout-alarm__live__000`).
//!
//! Usage: `forktrace <game dir> [bot] [steps]`
use glam::Vec3;
use ra2sim::rig::JointUse;
use ra2sim::{BotSetup, DriveInput, Duel};

const DT: f32 = 1.0 / 60.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let game = std::path::PathBuf::from(&a[0]);
    let bot = a.get(1).cloned().unwrap_or("AI/Team0/Bot0.bot".into());
    let steps: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(14);
    // `FORKTRACE_SPAWN=x,z,yaw_deg` (default the box's centre, facing 270°);
    // `FORKTRACE_SPAWN=arena` keeps the arena's own start point.
    let spawn = std::env::var("FORKTRACE_SPAWN").unwrap_or_default();
    let f: Vec<f32> = spawn.split(',').filter_map(|v| v.parse().ok()).collect();
    let setup = BotSetup {
        path: game.join(&bot).to_string_lossy().to_string(),
        yaw_deg: if spawn == "arena" {
            None
        } else {
            Some(f.get(2).copied().unwrap_or(270.0))
        },
        spawn: if spawn == "arena" {
            None
        } else {
            Some([
                f.first().copied().unwrap_or(0.0),
                f.get(1).copied().unwrap_or(0.0),
            ])
        },
    };
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let mut d = Duel::new(&arena, std::slice::from_ref(&setup))?;
    // `FORKTRACE_SETTLE=<steps>` at rest first (default 60).
    let settle: usize = std::env::var("FORKTRACE_SETTLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    for _ in 0..settle {
        d.step_direct(&[DriveInput::default()], DT);
    }
    let r = &d.robots[0];
    let p = (0..r.rig.parts.len())
        .find(|&p| {
            r.rig.parts[p]
                .joint
                .is_some_and(|j| j.use_kind == JointUse::Burst)
        })
        .ok_or("no burst motor")?;
    let motor = r.rig.parts[p].joint.unwrap().motor;
    let w = &r.rig.wiring;
    let wire = w
        .wires
        .iter()
        .find(|x| x.component == motor)
        .ok_or("unwired")?;
    let name = w.controls[wire.controller].name.clone();
    let joint = r.parts[p].joint.unwrap();
    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    for i in 0..steps {
        d.step_direct(&[DriveInput::default()], DT);
        let r = &d.robots[0];
        let info = r.rig.parts[p].joint.unwrap();
        let parent = r.rig.parts[p].parent.unwrap();
        let sa = d.physics.state(r.parts[parent].body).unwrap();
        let sb = d.physics.state(r.parts[p].body).unwrap();
        let axis = sa.rotation
            * r.rig.components[r.rig.parts[parent].root]
                .local
                .inverse()
                .transform_vector3(info.axis);
        let w = (sb.angular_velocity - sa.angular_velocity).dot(axis.normalize_or(Vec3::Y));
        println!(
            "step {i}: angle {:.3} speed {:.2} chassis w ({:.2},{:.2},{:.2}) y {:.4}",
            d.physics.hinge_angle(joint),
            w,
            sa.angular_velocity.x,
            sa.angular_velocity.y,
            sa.angular_velocity.z,
            d.physics
                .center_of_mass(r.parts[parent].body)
                .unwrap_or_default()
                .y
        );
    }
    Ok(())
}
