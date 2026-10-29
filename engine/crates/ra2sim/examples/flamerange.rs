//! Diagnostic: the hit rate of a Flamethrower against distance (docs/37 section 6), the counterpart of
//! `tools/oracle/capture_flame_look.py` `range`: the flame robot and ALARM stand still, ALARM `gap` metres
//! (origin to origin, along the flame robot's heading) ahead and facing it, the jet at level 100 for
//! `secs` seconds.  Prints the particle hits and the hit points ALARM lost.
//!
//! Usage: flamerange <arena.py> <flame.bot> <alarm.bot> secs gap...
//!        flamerange <arena.py> <flame.bot> <alarm.bot> secs @sx,sz,sh,ax,az,ah ...   (each phase's live poses:
//!        centre-of-mass x z and heading of the flame robot, then of ALARM, as `plus` reports them)
use glam::Vec3;
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let secs: f32 = a[3].parse()?;
    let base = [-9.85f32, 10.08];
    let dir = [
        std::f32::consts::FRAC_1_SQRT_2,
        -std::f32::consts::FRAC_1_SQRT_2,
    ];
    for arg in &a[4..] {
        let (target, g): (Option<[f32; 6]>, f32) = if let Some(t) = arg.strip_prefix('@') {
            let v: Vec<f32> = t.split(',').map(|x| x.parse().unwrap()).collect();
            (Some([v[0], v[1], v[2], v[3], v[4], v[5]]), 0.0)
        } else {
            (None, arg.parse().unwrap())
        };
        let mut spawn = match target {
            Some(t) => [[t[0], t[1]], [t[3], t[4]]],
            None => [base, [base[0] + g * dir[0], base[1] + g * dir[1]]],
        };
        let yaws = match target {
            Some(t) => [t[2].to_degrees() + 180.0, t[5].to_degrees() + 180.0],
            None => [-45.0, 135.0],
        };
        // a first build tells where the centres of mass fall, the second moves the spawns by the error
        for pass in 0..2 {
            let mut duel = Duel::new(
                &a[0],
                &[
                    BotSetup {
                        path: a[1].clone(),
                        yaw_deg: Some(yaws[0]),
                        spawn: Some(spawn[0]),
                    },
                    BotSetup {
                        path: a[2].clone(),
                        yaw_deg: Some(yaws[1]),
                        spawn: Some(spawn[1]),
                    },
                ],
            )?;
            let still = [DriveInput::default(), DriveInput::default()];
            for _ in 0..(3.0 / DEFAULT_STEP) as usize {
                duel.step_direct(&still, DEFAULT_STEP);
            }
            let d = |duel: &Duel| -> f32 {
                let p = |i: usize| {
                    duel.physics
                        .center_of_mass(duel.robots[i].body())
                        .unwrap_or(Vec3::ZERO)
                };
                let (x, y) = (p(0), p(1));
                ((x.x - y.x).powi(2) + (x.z - y.z).powi(2)).sqrt()
            };
            if let (Some(t), 0) = (target, pass) {
                for (i, sp) in spawn.iter_mut().enumerate() {
                    let c = duel
                        .physics
                        .center_of_mass(duel.robots[i].body())
                        .unwrap_or(Vec3::ZERO);
                    let want = if i == 0 { [t[0], t[1]] } else { [t[3], t[4]] };
                    *sp = [sp[0] + want[0] - c.x, sp[1] + want[1] - c.z];
                }
                continue;
            }
            if target.is_some() && pass == 0 {
                continue;
            }
            let before: f32 = duel.robots[1].health.iter().sum();
            let dist = d(&duel);
            if std::env::var("FLAME_LOG").is_ok() {
                let o = duel
                    .physics
                    .center_of_mass(duel.robots[0].body())
                    .unwrap_or(Vec3::ZERO);
                let fwd = duel
                    .physics
                    .state(duel.robots[0].body())
                    .map(|s| s.rotation * duel.robots[0].rig.forward())
                    .unwrap_or(Vec3::Z);
                let l = (fwd.x * fwd.x + fwd.z * fwd.z).sqrt();
                for i in 0..2 {
                    println!(
                        "COM {i} {:?}",
                        duel.physics.center_of_mass(duel.robots[i].body())
                    );
                }
                for j in duel.robots[0].flame_jets(&*duel.physics) {
                    let d = j.position - o;
                    println!(
                        "JET along {:.3} lat {:.3} h {:.3} vel {:?}",
                        (d.x * fwd.x + d.z * fwd.z) / l,
                        (d.x * fwd.z - d.z * fwd.x) / l,
                        d.y,
                        j.velocity
                    );
                }
            }
            duel.robots[0].apply_inputs(&[("Flame".into(), 0, 100.0)]);
            // FLAME_DT=<s> steps the world (and so the particles) at a coarser interval, as the live frames were
            let dt: f32 = std::env::var("FLAME_DT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_STEP);
            let n = (secs / dt) as usize;
            let trace = std::env::var("FLAME_TRACE").is_ok();
            for step in 0..n {
                duel.step_direct(&still, dt);
                if trace && step % 15 == 0 {
                    for j in duel.robots[0].flame_jets(&*duel.physics) {
                        println!(
                            "TRACE t {:.2} nozzle {:.3} {:.3} {:.3} vel {:.2} {:.2} {:.2}",
                            step as f32 * dt,
                            j.position.x,
                            j.position.y,
                            j.position.z,
                            j.velocity.x,
                            j.velocity.y,
                            j.velocity.z
                        );
                    }
                }
            }
            duel.robots[0].apply_inputs(&[("Flame".into(), 0, 0.0)]);
            if std::env::var("FLAME_LOG").is_ok() {
                let o = duel
                    .physics
                    .center_of_mass(duel.robots[0].body())
                    .unwrap_or(Vec3::ZERO);
                let f = duel.robots[0].rig.forward();
                let fwd = duel
                    .physics
                    .state(duel.robots[0].body())
                    .map(|s| s.rotation * f)
                    .unwrap_or(f);
                let (fx, fz) = (fwd.x, fwd.z);
                let l = (fx * fx + fz * fz).sqrt();
                for (v, c, p) in &duel.flame_hit_log {
                    let along = ((p.x - o.x) * fx + (p.z - o.z) * fz) / l;
                    let lat = ((p.x - o.x) * fz - (p.z - o.z) * fx) / l;
                    println!(
                        "HIT {v} {c} {} along {along:.2} lat {lat:.2} h {:.2}",
                        duel.robots[*v].rig.components[*c].name,
                        p.y - o.y
                    );
                }
            }
            let after: f32 = duel.robots[1].health.iter().sum();
            println!(
                "gap {g:.1} centres {dist:.2} m: {} hits in {secs} s ({:.1}/s), ALARM lost {:.1}",
                duel.flame_hits,
                duel.flame_hits as f32 / secs,
                before - after
            );
            break;
        }
    }
    Ok(())
}
