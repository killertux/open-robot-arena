//! Diagnostic: drive one robot and print speed / pitch / wheel spin every 0.1 s.
//!
//! ```text
//! drivelab <arena.py> <bot> "0-3:0,1;3-4.5:0,0;4.5-6.7:1,0" <seconds>
//! ```
use glam::Vec3;
use ra2sim::{BotSetup, DriveScript, Duel, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let script = DriveScript::parse(&a[2]);
    let secs: f32 = a[3].parse()?;
    let mut duel = Duel::new(
        &a[0],
        &[BotSetup {
            path: a[1].clone(),
            yaw_deg: std::env::var("YAW").ok().and_then(|v| v.parse().ok()),
            spawn: match (
                std::env::var("SPX")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok()),
                std::env::var("SPZ")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok()),
            ) {
                (Some(x), Some(z)) => Some([x, z]),
                _ => None,
            },
        }],
    )?;
    let steps = (secs / DEFAULT_STEP) as usize;
    for s in 0..steps {
        let t = s as f32 * DEFAULT_STEP;
        if std::env::var_os("RA2_CONTACTS").is_some() {
            let tuning = duel.tuning;
            duel.robots[0].drive(&mut *duel.physics, script.at(t), &tuning);
            duel.physics.step(DEFAULT_STEP);
            let ev = duel.physics.take_contacts();
            if s % 6 == 0 {
                let mut per: std::collections::BTreeMap<u32, (usize, f32)> = Default::default();
                for e in &ev {
                    let part = if duel.robots[0].parts.iter().any(|p| p.body == e.a) {
                        e.a
                    } else {
                        e.b
                    };
                    let n = per.entry(part).or_default();
                    n.0 += 1;
                    n.1 += e.normal.y.abs();
                }
                println!("  contacts t={t:.2}: {per:?}");
            }
        } else {
            duel.step_direct(&[script.at(t)], DEFAULT_STEP);
        }
        if s % 6 == 0 {
            let r = &duel.robots[0];
            let tf = r.transform(&*duel.physics);
            let f = tf.transform_vector3(r.rig.forward());
            let up = tf.transform_vector3(Vec3::Y);
            let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
            let pitch = f.y.asin().to_degrees();
            let roll = (-up.dot(flat.cross(Vec3::Y))).asin().to_degrees();
            let v = r.velocity(&*duel.physics);
            let av = duel
                .physics
                .state(r.body())
                .map(|s| s.angular_velocity)
                .unwrap_or_default();
            let wheels: Vec<String> = (1..r.parts.len())
                .filter_map(|p| duel.physics.state(r.parts[p].body))
                .map(|s| format!("{:.1}", s.angular_velocity.length()))
                .collect();
            println!(
                "{t:5.2} x={:.2} z={:.2} y={:.3} v={:6.2} fwd={:6.2} yawrate={:6.2} pitch={pitch:5.1} roll={roll:5.1} wheels={wheels:?}",
                tf.w_axis.x,
                tf.w_axis.z,
                tf.w_axis.y,
                v.length(),
                v.dot(flat),
                av.y
            );
        }
    }
    Ok(())
}
