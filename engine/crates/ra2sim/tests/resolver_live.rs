//! The contact resolver on the original's own input (`docs/29-havok17-backend.md` section 12be).
//!
//! `data/resolver_live.txt` holds resolver entries of the Scout's tail-stand landings in
//! `tipab-scout-tail-corner-box__live__000` (`tools/oracle/sideways/resolver_cases.py`): the chassis'
//! state and its contacts as `0x53ed80` received them, and the velocities it left (the `V` rows of
//! `frida/ra2_tipab.js`). Ours, run on the same input, gives them to 1e-3.
use glam::{Mat3, Quat, Vec3};
use ra2sim::backend::{MotionType, NO_TAG};
use ra2sim::havok17::body::Body;
use ra2sim::havok17::resolver::{resolve, Contact};

fn v3(f: &[f32]) -> Vec3 {
    Vec3::new(f[0], f[1], f[2])
}

fn chassis(f: &[f32], motion: MotionType) -> Body {
    let rows = Mat3::from_cols_array(&f[10..19].try_into().unwrap()).transpose();
    let inv = Mat3::from_cols_array(&f[19..28].try_into().unwrap()).inverse();
    let dynamic = motion == MotionType::Dynamic;
    Body {
        alive: true,
        motion,
        sensor: false,
        group: None,
        com: v3(&f[1..4]),
        rotation: Quat::from_mat3(&rows),
        velocity: if dynamic { v3(&f[4..7]) } else { Vec3::ZERO },
        angular_velocity: if dynamic { v3(&f[7..10]) } else { Vec3::ZERO },
        com_local: Vec3::ZERO,
        mass: f[0],
        inv_mass: if dynamic { 1.0 / f[0] } else { 0.0 },
        inv_inertia_local: inv,
        inv_inertia_world: rows * inv * rows.transpose(),
        force: Vec3::ZERO,
        torque: Vec3::ZERO,
        friction: 0.5,
        restitution: 0.2,
        active: true,
        freeze: Default::default(),
        system: None,
    }
}

#[test]
fn the_resolver_gives_the_originals_velocities_on_the_originals_input() {
    let text = include_str!("data/resolver_live.txt");
    let (mut body, mut contacts, mut tol, mut n, mut worst) = (None, Vec::new(), 0.1f32, 0, 0.0f32);
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        let nums =
            |from: usize| -> Vec<f32> { t[from..].iter().map(|x| x.parse().unwrap()).collect() };
        match t[0] {
            "case" => {
                tol = t[5].parse().unwrap();
                contacts.clear();
            }
            "body" => body = Some(nums(1)),
            "contact" => {
                let f = nums(1);
                let a_is_chassis = f[12] > 0.5;
                contacts.push(Contact {
                    a: if a_is_chassis { 0 } else { 1 },
                    b: if a_is_chassis { 1 } else { 0 },
                    tag_a: NO_TAG,
                    tag_b: NO_TAG,
                    normal: v3(&f[0..3]),
                    point: v3(&f[3..6]),
                    distance: f[6],
                    friction: (f[7] * f[8]).sqrt(),
                    restitution: f[9] * f[10],
                });
            }
            "post" => {
                let post = nums(1);
                let f = body.take().unwrap();
                let mut bodies = vec![
                    chassis(&f, MotionType::Dynamic),
                    chassis(&f, MotionType::Static),
                ];
                resolve(&mut bodies, &contacts, tol);
                let dv = (bodies[0].velocity - v3(&post[0..3])).abs().max_element();
                let dw = (bodies[0].angular_velocity - v3(&post[3..6]))
                    .abs()
                    .max_element();
                worst = worst.max(dv).max(dw);
                n += 1;
            }
            _ => {}
        }
    }
    assert!(n > 50, "{n} cases");
    assert!(worst < 2e-3, "worst velocity difference {worst}");
}
