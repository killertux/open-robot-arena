//! Diagnostic (docs/29 section 12bi): a robot held still `lift` m above its rest pose, one wheel
//! (or all) set spinning about its axle, then let go in the air; prints every substep each hinged
//! part's angular velocity about its hinge axis (`W <step> <part>...`) and the chassis' own.
//! The live twin is `tools/oracle/frida/ra2_wheelspin.js`.
//!
//! Usage: `wheelspin <arena.py> <botA> <botB> <robot> <rad/s> <lift m> <steps> [only-part]`
use glam::Vec3;
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

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
    let ri: usize = a[3].parse()?;
    let w: f32 = a[4].parse()?;
    let lift: f32 = a[5].parse()?;
    let steps: usize = a[6].parse()?;
    let only: Option<usize> = a.get(7).and_then(|s| s.parse().ok());
    let idle = vec![DriveInput::default(); duel.robots.len()];
    for _ in 0..60 {
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let bodies: Vec<u32> = duel.robots[ri].parts.iter().map(|p| p.body).collect();
    let snap: Vec<(Vec3, glam::Quat)> = bodies
        .iter()
        .map(|&b| {
            (
                duel.physics.center_of_mass(b).unwrap(),
                duel.physics.state(b).unwrap().rotation,
            )
        })
        .collect();
    {
        // `A <part> <mass> <com offset in the chassis body frame> <hinge axis in the same frame>`.
        let r = &duel.robots[ri];
        let (_, c_rot, _) = r.rig.components[r.rig.parts[0].root]
            .local
            .to_scale_rotation_translation();
        for (i, &b) in bodies.iter().enumerate() {
            let (m, _, _) = duel.physics.mass_properties(b).unwrap();
            let d = snap[0].1.inverse() * (snap[i].0 - snap[0].0);
            let axis = r.rig.parts[i].joint.map(|j| c_rot.inverse() * j.axis);
            println!("A {i} {m:.3} {:.3?} {:.4?}", d.to_array(), axis);
        }
    }
    let put = |duel: &mut Duel, spin: bool| {
        for (i, &b) in bodies.iter().enumerate() {
            let s = duel.physics.state(b).unwrap();
            let now = duel.physics.center_of_mass(b).unwrap();
            let off = s.rotation.inverse() * (s.position - now);
            let com = snap[i].0 + Vec3::Y * lift;
            duel.physics
                .set_transform(b, com + snap[i].1 * off, snap[i].1);
            let mut av = Vec3::ZERO;
            if spin && i > 0 && only.is_none_or(|o| o == i) {
                if let Some(j) = duel.robots[ri].rig.parts[i].joint {
                    let (_, c_rot, _) = duel.robots[ri].rig.components
                        [duel.robots[ri].rig.parts[0].root]
                        .local
                        .to_scale_rotation_translation();
                    let frame = snap[0].1 * c_rot.inverse();
                    av = frame * j.axis * w;
                }
            }
            duel.physics.set_velocity(b, Vec3::ZERO, av);
        }
    };
    let hold: usize = std::env::var("WS_HOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    for _ in 0..hold {
        put(&mut duel, false);
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    put(&mut duel, true);
    // `WS_AIR=e1,e2,...`: instead of spinning a wheel, drive the robot forward at full throttle from the
    // release and, after the first call, set every driven hinge's servo error (target - angle) to those
    // values (docs/29 section 12bi: the live targets are stale from earlier drives).
    let air: Option<Vec<f32>> = std::env::var("WS_AIR")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect());
    for k in 0..steps {
        if let Some(e) = &air {
            let mut inputs = idle.clone();
            inputs[ri] = DriveInput::new(1.0, 0.0);
            duel.step_direct(&inputs, DEFAULT_STEP);
            if k == 0 {
                let n = duel.physics.joint_errors().len();
                let mut errs = vec![[0.0f32; 2]; n];
                for (i, x) in e.iter().enumerate() {
                    if i < n {
                        errs[i][0] = *x;
                    }
                }
                duel.physics.set_joint_errors(&errs);
            }
        } else {
            duel.step_direct(&idle, DEFAULT_STEP);
        }
        let r = &duel.robots[ri];
        let (_, c_rot, _) = r.rig.components[r.rig.parts[0].root]
            .local
            .to_scale_rotation_translation();
        let cs = duel.physics.state(r.parts[0].body).unwrap();
        let frame = cs.rotation * c_rot.inverse();
        let mut line = format!("W {k}");
        for (p, part) in r.rig.parts.iter().enumerate().skip(1) {
            let Some(j) = part.joint else { continue };
            let s = duel.physics.state(r.parts[p].body).unwrap();
            let rel = s.angular_velocity - cs.angular_velocity;
            line += &format!(" {p}:{:.3}", rel.dot(frame * j.axis));
        }
        println!("{line} chassis_w {:.3?}", cs.angular_velocity);
    }
    Ok(())
}
