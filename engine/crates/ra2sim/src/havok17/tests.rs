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
    // Horizontally: between steps a resting body carries the -g·dt the last
    // integration gave it, which the next step's resolution takes back.
    let v = s.linear_velocity;
    assert!(v.x.hypot(v.z) < 0.2, "{s:?}");
    assert!(
        travel > 0.5 * ideal && travel < 2.0 * ideal,
        "slid {travel} vs {ideal}"
    );
}

/// The game's `Lock` (`FUN_00480e20`): both hinge limits at the current angle.
/// A horizontal arm hinged to a fixed body sags under gravity while free, and
/// holds where it was locked; unlocked, it swings again.
#[test]
fn a_locked_hinge_holds_its_angle_and_releases() {
    let mut physics = Havok17Backend::new().unwrap();
    let base = physics
        .create_body(&BodyDesc::new(
            MotionType::Static,
            ShapeDesc::Box {
                half_extents: Vec3::splat(0.1),
            },
        ))
        .unwrap();
    let arm = physics
        .create_body(
            &BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::Box {
                    half_extents: Vec3::new(0.5, 0.05, 0.05),
                },
            )
            .at(Vec3::new(0.8, 0.0, 0.0))
            .with_mass(10.0),
        )
        .unwrap();
    let mut desc = HingeDesc::new(base, arm, Vec3::ZERO, Vec3::Z);
    // The arm starts 0.3 m out, clear of the base by more than the tolerance.
    desc.anchor_b = Vec3::new(-0.8, 0.0, 0.0);
    let hinge = physics.create_hinge(&desc).unwrap();
    for _ in 0..6 {
        physics.step(1.0 / 60.0);
    }
    // Locked while it swings: it stops within a step or two of where the lock
    // caught it, then holds.
    let at_lock = physics.hinge_angle(hinge);
    assert!(physics.lock_joint(hinge, true));
    for _ in 0..60 {
        physics.step(1.0 / 60.0);
    }
    let held = physics.hinge_angle(hinge);
    assert!(
        (held - at_lock).abs() < 0.05,
        "the lock let it fall: {at_lock} -> {held}"
    );
    for _ in 0..120 {
        physics.step(1.0 / 60.0);
    }
    let after = physics.hinge_angle(hinge);
    assert!(
        (after - held).abs() < 0.005,
        "a locked arm moved: {held} -> {after}"
    );
    physics.lock_joint(hinge, false);
    for _ in 0..30 {
        physics.step(1.0 / 60.0);
    }
    let free = physics.hinge_angle(hinge);
    assert!(
        (free - held).abs() > 0.3,
        "the released arm did not swing: {held} -> {free}"
    );
}

/// A burst motor's stroke: the servo's target runs on at the motor's speed
/// past the stop, and once it is more than half a turn ahead the servo pulls
/// the short way round. The limit is the hinge's own angle all the same: the
/// arm stays at its stop instead of being put a turn below the lower limit and
/// spun round (Sentinel's wedge, 50 rad/s against 1.67 rad of travel; docs/29
/// section 12l).
#[test]
fn a_hinge_driven_past_its_stop_stays_at_it() {
    let mut physics = Havok17Backend::new().unwrap();
    let base = physics
        .create_body(&BodyDesc::new(
            MotionType::Static,
            ShapeDesc::Box {
                half_extents: Vec3::splat(0.1),
            },
        ))
        .unwrap();
    let arm = physics
        .create_body(
            &BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::Box {
                    half_extents: Vec3::new(0.3, 0.05, 0.05),
                },
            )
            .at(Vec3::new(0.6, 0.0, 0.0))
            .with_mass(28.0),
        )
        .unwrap();
    let mut desc = HingeDesc::new(base, arm, Vec3::ZERO, Vec3::Z);
    desc.anchor_b = Vec3::new(-0.6, 0.0, 0.0);
    desc.limits = [0.0, 1.67];
    desc.max_torque = 3600.0;
    let hinge = physics.create_hinge(&desc).unwrap();
    physics.drive_hinge(hinge, 50.0);
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for i in 0..60 {
        physics.step(1.0 / 60.0);
        let a = physics.hinge_angle(hinge);
        if i > 8 {
            lo = lo.min(a);
            hi = hi.max(a);
        }
    }
    // The limit impulse works on the arm's inertia about its centre, so it
    // stops it softly (section 11's locked-hinge note); what it must not do is
    // let it round past the lower stop.
    assert!(lo > 0.5 && hi < 3.0, "the arm went round: {lo}..{hi}");
    let w = physics.state(arm).unwrap().angular_velocity.length();
    assert!(w < 10.0, "the arm spins at {w} rad/s");
}

/// The agents' points, not every corner near the floor: a box resting on a
/// floor triangle is held by the corners that have been its closest point
/// (the convex x mesh manifold), each once, all inside the tolerance.
#[test]
fn a_resting_box_is_held_by_its_manifold_corners() {
    let mut physics = Havok17Backend::new().unwrap();
    floor(&mut physics);
    let c = cube(&mut physics, 0.59);
    for _ in 0..120 {
        physics.step(1.0 / 60.0);
    }
    let mine: Vec<_> = physics
        .contacts
        .iter()
        .filter(|k| k.a == c as usize || k.b == c as usize)
        .collect();
    assert!(!mine.is_empty() && mine.len() <= 4, "{mine:?}");
    let centre = physics.state(c).unwrap().position;
    for (n, k) in mine.iter().enumerate() {
        assert!(k.distance >= 0.0 && k.distance < TOLERANCE, "{k:?}");
        // At a bottom corner of the cube, half the gap above the floor.
        let r = k.point - centre;
        assert!(
            (r.x.abs() - 0.5).abs() < 0.02 && (r.z.abs() - 0.5).abs() < 0.02,
            "{k:?}"
        );
        for other in &mine[n + 1..] {
            assert!(k.point.distance(other.point) > 0.1, "{k:?} {other:?}");
        }
    }
}

/// A wheel dropped onto the floor skidding (moving, not turning) is spun up
/// by the floor's friction toward rolling: the points are the hub's lowest
/// corners, so nothing ahead of the axle brakes the spin.
#[test]
fn a_skidding_wheel_spins_up_toward_rolling() {
    let mut physics = Havok17Backend::new().unwrap();
    floor(&mut physics);
    let (r, h) = (0.25f32, 0.07f32);
    let mut pts = Vec::new();
    for k in 0..16 {
        let a = k as f32 * std::f32::consts::TAU / 16.0;
        for z in [-h, h] {
            pts.push(Vec3::new(a.cos() * r, a.sin() * r, z));
        }
    }
    let w = physics
        .create_body(
            &BodyDesc::new(MotionType::Dynamic, ShapeDesc::ConvexHull { points: pts })
                .at(Vec3::new(0.0, r + 0.09, 0.0))
                .with_mass(20.0)
                .with_friction(0.5)
                .with_restitution(0.0),
        )
        .unwrap();
    physics.set_velocity(w, Vec3::new(3.0, 0.0, 0.0), Vec3::ZERO);
    for _ in 0..90 {
        physics.step(1.0 / 60.0);
    }
    let s = physics.state(w).unwrap();
    // Rolling toward +x turns the wheel about -z.
    let roll = s.linear_velocity.x / r;
    let spin = -s.angular_velocity.z;
    assert!(s.linear_velocity.x > 0.5, "{s:?}");
    assert!(
        spin > 0.6 * roll && spin < 1.3 * roll,
        "spin {spin} vs rolling {roll}: {s:?}"
    );
}

/// A mesh collidable on a moving body (`ShapeDesc::MeshHull`, the game's
/// chassis and `SIMULATION_GEOMETRY 6` parts) meets the floor through the mesh
/// x mesh agent: one point per pair of triangles within the tolerance, none
/// within 3 mm of another.
#[test]
fn a_mesh_plate_rests_on_the_floor_with_distinct_points() {
    let mut physics = Havok17Backend::new().unwrap();
    floor(&mut physics);
    let mut points = Vec::new();
    for x in [-0.5, 0.5] {
        for y in [-0.05, 0.05] {
            for z in [-0.4, 0.4] {
                points.push(Vec3::new(x, y, z));
            }
        }
    }
    // The box's twelve triangles (points indexed x*4 + y*2 + z).
    let quads = [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ];
    let indices: Vec<[u32; 3]> = quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    let plate = physics
        .create_body(
            &BodyDesc::new(MotionType::Dynamic, ShapeDesc::MeshHull { points, indices })
                .at(Vec3::new(0.0, 0.14, 0.0))
                .with_mass(30.0)
                .with_friction(0.5)
                .with_restitution(0.0),
        )
        .unwrap();
    for _ in 0..120 {
        physics.step(1.0 / 60.0);
    }
    let s = physics.state(plate).unwrap();
    assert!(
        s.position.y > 0.05 && s.position.y < 0.05 + TOLERANCE,
        "{s:?}"
    );
    let mine: Vec<_> = physics
        .contacts
        .iter()
        .filter(|k| k.a == plate as usize || k.b == plate as usize)
        .collect();
    assert!(!mine.is_empty(), "the plate fell through");
    for (n, k) in mine.iter().enumerate() {
        assert!(k.distance > 0.0 && k.distance < TOLERANCE, "{k:?}");
        for other in &mine[n + 1..] {
            assert!(k.point.distance(other.point) > 0.003, "{k:?} {other:?}");
        }
    }
}

/// The hinge motor's target angle survives the motor being switched off and
/// on (`0x5355a0` only moves it by whole turns): live, the Scout's fork locks
/// at -1.18 rad with its target at -2.59 and the cocking motor starts from
/// that same target 29 substeps later (`firetiming-scout-alarm__live__000`).
#[test]
fn a_motor_switched_back_on_keeps_its_old_target() {
    let mut physics = Havok17Backend::new().unwrap();
    let base = physics
        .create_body(&BodyDesc::new(
            MotionType::Static,
            ShapeDesc::Box {
                half_extents: Vec3::splat(0.1),
            },
        ))
        .unwrap();
    let arm = physics
        .create_body(
            &BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::Box {
                    half_extents: Vec3::new(0.5, 0.05, 0.05),
                },
            )
            .at(Vec3::new(0.8, 0.0, 0.0))
            .with_mass(10.0),
        )
        .unwrap();
    let mut desc = HingeDesc::new(base, arm, Vec3::ZERO, Vec3::Y);
    desc.anchor_b = Vec3::new(-0.8, 0.0, 0.0);
    desc.max_torque = 200.0;
    let hinge = physics.create_hinge(&desc).unwrap();
    let target = |p: &Havok17Backend| p.joints[hinge as usize].target;
    // Driven for half a second: the servo's target runs ahead of the arm.
    physics.drive_hinge(hinge, 2.0);
    for _ in 0..30 {
        physics.step(1.0 / 60.0);
    }
    physics.drive_hinge(hinge, 0.0);
    let parked = target(&physics);
    // Off, the arm coasts and the target stays where it was.
    for _ in 0..20 {
        physics.step(1.0 / 60.0);
    }
    assert_eq!(target(&physics), parked);
    let angle = physics.hinge_angle(hinge);
    physics.drive_hinge(hinge, 2.0);
    let resumed = target(&physics);
    assert!(
        (resumed - parked).abs() < 1e-6 && (resumed - angle).abs() > 1e-3,
        "the target was reset: parked {parked}, resumed {resumed}, angle {angle}"
    );
}

/// `Lock(1)` stops the driven body dead in the world (`0x480e66..0x480e89`:
/// its velocity and angular velocity set to the zero vector), not only
/// relative to its parent (docs/29 section 12r).
#[test]
fn a_lock_stops_the_driven_body_in_the_world() {
    let mut physics = Havok17Backend::new().unwrap();
    let base = physics
        .create_body(&BodyDesc::new(
            MotionType::Static,
            ShapeDesc::Box {
                half_extents: Vec3::splat(0.1),
            },
        ))
        .unwrap();
    let arm = physics
        .create_body(
            &BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::Box {
                    half_extents: Vec3::new(0.5, 0.05, 0.05),
                },
            )
            .at(Vec3::new(0.8, 0.0, 0.0))
            .with_mass(10.0),
        )
        .unwrap();
    let mut desc = HingeDesc::new(base, arm, Vec3::ZERO, Vec3::Z);
    desc.anchor_b = Vec3::new(-0.8, 0.0, 0.0);
    let hinge = physics.create_hinge(&desc).unwrap();
    for _ in 0..10 {
        physics.step(1.0 / 60.0);
    }
    let swinging = physics.state(arm).unwrap();
    assert!(swinging.angular_velocity.length() > 0.5, "the arm swings");
    physics.lock_joint(hinge, true);
    let held = physics.state(arm).unwrap();
    assert_eq!(held.linear_velocity, Vec3::ZERO);
    assert_eq!(held.angular_velocity, Vec3::ZERO);
    // Locking again while locked changes nothing; unlocking leaves it as is.
    physics.set_velocity(arm, Vec3::X, Vec3::ZERO);
    physics.lock_joint(hinge, true);
    assert_eq!(physics.state(arm).unwrap().linear_velocity, Vec3::X);
}

/// A phantom is listed only while it overlaps (docs/29 sections 12d, 12i,
/// 12r): a sphere 5 cm above the floor touches nothing, inside the 0.1 m
/// tolerance or not; let down 5 cm into it, it touches.
#[test]
fn a_phantom_touches_only_what_it_overlaps() {
    let mut physics = Havok17Backend::new().unwrap();
    floor(&mut physics);
    let make = |physics: &mut Havok17Backend, x: f32, y: f32| {
        physics
            .create_body(
                &BodyDesc::new(
                    MotionType::Static,
                    ShapeDesc::Box {
                        half_extents: Vec3::splat(0.05),
                    },
                )
                .at(Vec3::new(x, y, 0.0))
                .with_phantom(ShapeDesc::Sphere { radius: 0.5 }),
            )
            .unwrap()
    };
    let above = make(&mut physics, -3.0, 0.55);
    let into = make(&mut physics, 3.0, 0.45);
    physics.step(1.0 / 60.0);
    assert_eq!(
        physics.phantom_touch(above),
        None,
        "5 cm clear of the floor"
    );
    let n = physics.phantom_touch(into).expect("5 cm into the floor");
    assert!(n.y.abs() > 0.9, "the floor's normal: {n}");
}
