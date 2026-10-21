//! Diagnostic: settle a robot, roll it over about its forward axis, and watch
//! it come to rest (and, optionally, fire its weapons every second the way the
//! shipped AIs' `InvertHandler`s do).
//!
//! Usage: `invertprobe <arena.py> <bot> <roll degrees> <seconds> [fire]`
//!
//! Prints every 0.1 s the bot origin's and the chassis centre of mass's
//! height, the up vector and the components whose hulls reach lowest.
use glam::{Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let roll: f32 = a[2].parse::<f32>()?.to_radians();
    let secs: f32 = a[3].parse()?;
    let fire = a.get(4).is_some_and(|s| s == "fire");
    let mut duel = Duel::new(
        &a[0],
        &[BotSetup {
            path: a[1].clone(),
            yaw_deg: Some(0.0),
            spawn: Some([0.0, 0.0]),
        }],
    )?;
    if std::env::var("INVERTPROBE_DUMP").is_ok() {
        let c = &duel.robots[0].rig.components[0];
        for p in &c.hull_points {
            println!("pt {:.3} {:.3} {:.3}", p.x, p.y, p.z);
        }
    }
    for _ in 0..60 {
        duel.step_direct(&[DriveInput::default()], DEFAULT_STEP);
    }
    let floor = {
        let r = &duel.robots[0];
        let worlds = r.component_worlds(&*duel.physics);
        r.rig
            .components
            .iter()
            .enumerate()
            .flat_map(|(i, c)| c.hull_points.iter().map(move |p| (i, *p)))
            .map(|(i, p)| worlds[i].transform_point3(p).y)
            .fold(f32::MAX, f32::min)
    };
    // Roll every body about the chassis centre of mass, lifted clear.
    {
        let r = &duel.robots[0];
        let tf = r.transform(&*duel.physics);
        let fwd = tf.transform_vector3(r.rig.forward()).normalize();
        // `INVERTPROBE_PITCH=1`: about the robot's side axis instead.
        let axis = if std::env::var("INVERTPROBE_PITCH").is_ok() {
            fwd.cross(Vec3::Y).normalize()
        } else {
            fwd
        };
        let q = Quat::from_axis_angle(axis, roll);
        let pivot = duel.physics.center_of_mass(r.parts[0].body).unwrap();
        let lift = Vec3::Y * 1.0;
        let bodies: Vec<_> = r.parts.iter().map(|p| p.body).collect();
        for b in bodies {
            let s = duel.physics.state(b).unwrap();
            let pos = pivot + q * (s.position - pivot) + lift;
            duel.physics.set_transform(b, pos, q * s.rotation);
            duel.physics.set_velocity(b, Vec3::ZERO, Vec3::ZERO);
        }
    }
    let trigger: Vec<String> = {
        let w = &duel.robots[0].rig.wiring;
        w.controls
            .iter()
            .filter(|c| !c.name.is_empty())
            .map(|c| c.name.clone())
            .collect()
    };
    println!(
        "floor y {floor:.3}; controls {trigger:?}; bodies {:?}",
        duel.robots[0]
            .parts
            .iter()
            .map(|p| p.body)
            .collect::<Vec<_>>()
    );
    let steps = (secs / DEFAULT_STEP) as usize;
    for i in 0..steps {
        let t = i as f32 * DEFAULT_STEP;
        if fire && i > 0 && i % 60 == 0 {
            let names: Vec<(String, i32, f32)> = trigger
                .iter()
                .filter(|n| n.as_str() != "LeftRight" && n.as_str() != "Forward")
                .map(|n| (n.clone(), 0, 1.0))
                .collect();
            duel.robots[0].apply_inputs(&names);
        }
        duel.step_direct(&[DriveInput::default()], DEFAULT_STEP);
        if i % 6 != 0 {
            continue;
        }
        let r = &duel.robots[0];
        let tf = r.transform(&*duel.physics);
        let up = tf.transform_vector3(Vec3::Y).normalize();
        let com = duel.physics.center_of_mass(r.parts[0].body).unwrap();
        let s = duel.physics.state(r.parts[0].body).unwrap();
        let worlds = r.component_worlds(&*duel.physics);
        let mut low: Vec<(f32, usize)> = r
            .rig
            .components
            .iter()
            .enumerate()
            .filter(|(i, _)| r.attached.get(*i).copied().unwrap_or(true))
            .map(|(i, c)| {
                let lo = c
                    .hull_points
                    .iter()
                    .map(|p| worlds[i].transform_point3(*p).y)
                    .fold(f32::MAX, f32::min);
                (lo, i)
            })
            .filter(|(lo, _)| lo.is_finite())
            .collect();
        low.sort_by(|x, y| x.0.total_cmp(&y.0));
        let lowest = low[0].0;
        let touching: Vec<String> = low
            .iter()
            .filter(|(lo, _)| *lo < lowest + 0.03)
            .map(|(lo, i)| format!("#{i} {} {:.3}", r.rig.components[*i].name, lo - floor))
            .collect();
        println!(
            "{t:5.2} origin {:6.3} com {:6.3} up ({:5.2},{:5.2},{:5.2}) w {:5.2} v {:5.2} | {}",
            tf.w_axis.y - floor,
            com.y - floor,
            up.x,
            up.y,
            up.z,
            s.angular_velocity.length(),
            s.linear_velocity.length(),
            touching.join(", ")
        );
    }
    Ok(())
}
