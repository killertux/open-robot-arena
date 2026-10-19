use super::*;

fn floor(physics: &mut Havok17Backend) -> BodyHandle {
    let s = 20.0;
    physics
        .create_body(
            &BodyDesc::new(
                MotionType::Static,
                ShapeDesc::TriMesh {
                    vertices: vec![
                        Vec3::new(-s, 0.0, -s),
                        Vec3::new(s, 0.0, -s),
                        Vec3::new(s, 0.0, s),
                        Vec3::new(-s, 0.0, s),
                    ],
                    indices: vec![[0, 2, 1], [0, 3, 2]],
                },
            )
            .with_friction(0.3)
            .with_restitution(0.3),
        )
        .unwrap()
}

fn cube(physics: &mut Havok17Backend, y: f32) -> BodyHandle {
    let mut pts = Vec::new();
    for x in [-0.5, 0.5] {
        for yy in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                pts.push(Vec3::new(x, yy, z));
            }
        }
    }
    physics
        .create_body(
            &BodyDesc::new(MotionType::Dynamic, ShapeDesc::ConvexHull { points: pts })
                .at(Vec3::new(0.0, y, 0.0))
                .with_mass(100.0)
                .with_friction(0.5)
                .with_restitution(0.2),
        )
        .unwrap()
}

/// A resting body hovers inside the tolerance, where the live resolver sees
/// the Scout's and ALARM's resting contacts (0.083..0.0999 m).
#[test]
fn a_dropped_cube_comes_to_rest_inside_the_tolerance() {
    let mut physics = Havok17Backend::new().unwrap();
    floor(&mut physics);
    let c = cube(&mut physics, 2.0);
    let mut lows = Vec::new();
    for i in 0..360 {
        physics.step(1.0 / 60.0);
        if i > 240 {
            lows.push(physics.state(c).unwrap().position.y - 0.5);
        }
    }
    let lo = lows.iter().copied().fold(f32::MAX, f32::min);
    let hi = lows.iter().copied().fold(f32::MIN, f32::max);
    assert!(lo > 0.0 && hi < TOLERANCE * 1.02, "gap {lo}..{hi}");
    let s = physics.state(c).unwrap();
    assert!(s.linear_velocity.length() < 0.5, "{s:?}");
    // Flat on its face (the sequential impulses of the landing may turn it
    // about the vertical) and not spinning.
    assert!((s.rotation * Vec3::Y).dot(Vec3::Y) > 0.999, "{s:?}");
    assert!(s.angular_velocity.length() < 0.5, "{s:?}");
}

/// Coulomb friction from the impulses: a cube shoved along the floor slows at
/// about µ g (µ = sqrt(0.5 · 0.3), g = 12) and stops.
#[test]
fn a_shoved_cube_slides_to_a_stop() {
    let mut physics = Havok17Backend::new().unwrap();
    floor(&mut physics);
    let c = cube(&mut physics, 0.59);
    for _ in 0..60 {
        physics.step(1.0 / 60.0);
    }
    physics.set_velocity(c, Vec3::new(4.0, 0.0, 0.0), Vec3::ZERO);
    let x0 = physics.state(c).unwrap().position.x;
    for _ in 0..240 {
        physics.step(1.0 / 60.0);
    }
    let s = physics.state(c).unwrap();
    let travel = s.position.x - x0;
    let mu = (0.5f32 * 0.3).sqrt();
    let ideal = 16.0 / (2.0 * mu * 12.0);
    assert!(s.linear_velocity.length() < 0.2, "{s:?}");
    assert!(
        travel > 0.5 * ideal && travel < 2.0 * ideal,
        "slid {travel} vs {ideal}"
    );
}
