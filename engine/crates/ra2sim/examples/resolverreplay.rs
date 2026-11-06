//! Diagnostic (docs/29 12be): our contact resolver on the original's own input. Reads the cases
//! `tools/oracle/sideways/resolver_cases.py` writes from a capture with `V` rows (the velocities the
//! resolver `0x53ed80` left), runs `havok17::resolver::resolve` on the chassis against a fixed arena
//! and prints the velocity change of the original against ours.
//!
//!   `resolverreplay cases.txt [trial] [first_step] [last_step]`
//!
//! `RESOLVER_ZERO_E=1` zeroes the restitution, `RESOLVER_MU=<x>` replaces the friction.
use glam::{Mat3, Quat, Vec3};
use ra2sim::backend::MotionType;
use ra2sim::havok17::body::Body;
use ra2sim::havok17::resolver::{resolve, Contact};

fn v3(f: &[f32]) -> Vec3 {
    Vec3::new(f[0], f[1], f[2])
}

fn body(motion: MotionType, f: &[f32]) -> Body {
    let mass = f[0];
    let rows = Mat3::from_cols_array(&f[10..19].try_into().unwrap()).transpose();
    let inertia = Mat3::from_cols_array(&f[19..28].try_into().unwrap());
    let inv = inertia.inverse();
    let mut b = Body {
        alive: true,
        motion,
        sensor: false,
        group: None,
        com: v3(&f[1..4]),
        rotation: Quat::from_mat3(&rows),
        velocity: v3(&f[4..7]),
        angular_velocity: v3(&f[7..10]),
        com_local: Vec3::ZERO,
        mass,
        inv_mass: 1.0 / mass,
        inv_inertia_local: inv,
        inv_inertia_world: Mat3::ZERO,
        force: Vec3::ZERO,
        torque: Vec3::ZERO,
        friction: 0.5,
        restitution: 0.2,
        active: true,
        freeze: Default::default(),
        system: None,
    };
    b.inv_inertia_world = rows * inv * rows.transpose();
    b
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1]).unwrap();
    let want_trial: Option<usize> = a.get(2).and_then(|s| s.parse().ok());
    let lo: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let hi: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let zero_e = std::env::var_os("RESOLVER_ZERO_E").is_some();
    let mu_over: Option<f32> = std::env::var("RESOLVER_MU")
        .ok()
        .and_then(|s| s.parse().ok());
    let lines = text.lines();
    let (mut trial, mut step, mut tol) = (0usize, 0usize, 0.1f32);
    let mut live: Option<Body> = None;
    let mut contacts: Vec<Contact> = Vec::new();
    let mut show = false;
    for l in lines {
        let t: Vec<&str> = l.split_whitespace().collect();
        match t[0] {
            "case" => {
                trial = t[1].parse().unwrap();
                step = t[2].parse().unwrap();
                tol = t[5].parse().unwrap();
                contacts.clear();
                show = want_trial.is_none_or(|w| w == trial) && step >= lo && step <= hi;
            }
            "body" => {
                let f: Vec<f32> = t[1..].iter().map(|x| x.parse().unwrap()).collect();
                live = Some(body(MotionType::Dynamic, &f));
            }
            "contact" => {
                let f: Vec<f32> = t[1..].iter().map(|x| x.parse().unwrap()).collect();
                let chassis_a = f[12] > 0.5;
                let (ia, ib) = if chassis_a { (0, 1) } else { (1, 0) };
                let mut mu = (f[7] * f[8]).sqrt();
                if let Some(m) = mu_over {
                    mu = m;
                }
                let mut e = f[9] * f[10];
                if zero_e {
                    e = 0.0;
                }
                contacts.push(Contact {
                    a: ia,
                    b: ib,
                    tag_a: ra2sim::backend::NO_TAG,
                    tag_b: ra2sim::backend::NO_TAG,
                    normal: v3(&f[0..3]),
                    point: v3(&f[3..6]),
                    distance: f[6],
                    friction: mu,
                    restitution: e,
                });
            }
            "post" => {
                if !show {
                    continue;
                }
                let f: Vec<f32> = t[1..].iter().map(|x| x.parse().unwrap()).collect();
                let b0 = live.clone().unwrap();
                let mut arena = b0.clone();
                arena.motion = MotionType::Static;
                arena.inv_mass = 0.0;
                arena.velocity = Vec3::ZERO;
                arena.angular_velocity = Vec3::ZERO;
                let mut bodies = vec![b0.clone(), arena];
                resolve(&mut bodies, &contacts, tol);
                let o = &bodies[0];
                println!(
                    "trial {trial} step {step:3} n {} | in v {:7.3} {:7.3} {:7.3} w {:7.3} {:7.3} {:7.3} | dw live {:7.3} {:7.3} {:7.3} ours {:7.3} {:7.3} {:7.3} | dv live {:7.3} {:7.3} {:7.3} ours {:7.3} {:7.3} {:7.3}",
                    contacts.len(),
                    b0.velocity.x, b0.velocity.y, b0.velocity.z,
                    b0.angular_velocity.x, b0.angular_velocity.y, b0.angular_velocity.z,
                    f[3] - b0.angular_velocity.x, f[4] - b0.angular_velocity.y, f[5] - b0.angular_velocity.z,
                    o.angular_velocity.x - b0.angular_velocity.x, o.angular_velocity.y - b0.angular_velocity.y, o.angular_velocity.z - b0.angular_velocity.z,
                    f[0] - b0.velocity.x, f[1] - b0.velocity.y, f[2] - b0.velocity.z,
                    o.velocity.x - b0.velocity.x, o.velocity.y - b0.velocity.y, o.velocity.z - b0.velocity.z,
                );
            }
            _ => {}
        }
    }
}
