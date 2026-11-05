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
/// floor triangle gets the points of the distance query's simplexes (the
/// convex x mesh agent), each inside the tolerance and on the cube's bottom
/// face. With the face flat on the triangle the original's query ends on a
/// three-point simplex (a face against a face), whose point is a weighted mix
/// of the bottom corners and is reported for that detection only (docs/29
/// section 12ak); the corners of an earlier tie-break are the `RA2_GJK=gram`
/// answer.
#[test]
fn a_resting_box_is_held_by_its_manifold_points() {
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
        // On the cube's bottom face, half the gap above the floor.
        let r = k.point - centre;
        assert!(
            r.x.abs() < 0.52 && r.z.abs() < 0.52 && (r.y + 0.5).abs() < 0.1,
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
            &BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::MeshHull {
                    points,
                    indices,
                    raw: Vec::new(),
                },
            )
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

/// A thin box as a mesh collidable on a moving body, centred at `at`.
fn mesh_slab(physics: &mut Havok17Backend, at: Vec3) -> BodyHandle {
    let mut points = Vec::new();
    for x in [-0.5, 0.5] {
        for y in [-0.05, 0.05] {
            for z in [-0.5, 0.5] {
                points.push(Vec3::new(x, y, z));
            }
        }
    }
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
    physics
        .create_body(
            &BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::MeshHull {
                    points,
                    indices,
                    raw: Vec::new(),
                },
            )
            .at(at)
            .with_mass(30.0),
        )
        .unwrap()
}

/// Two moving meshes whose nearest edges are 0.08 m apart along a diagonal,
/// their triangles' boxes 0.057 m apart on two axes: the original walks both
/// trees with each box grown by half the tolerance (`FUN_00557870` called
/// from `FUN_00550100` with `0.5 · tol`), a whole tolerance between them, and
/// reports the pair. Grown by half the tolerance in all, the pair was lost:
/// the Scout's fork arm lost ALARM's side edge at 0.088-0.096 m, where the
/// live resolver kept pushing on it (docs/29 section 12s).
#[test]
fn two_moving_meshes_meet_within_a_whole_tolerance() {
    let mut physics = Havok17Backend::new().unwrap();
    let off = 0.08 / 2f32.sqrt();
    let a = mesh_slab(&mut physics, Vec3::ZERO);
    let b = mesh_slab(&mut physics, Vec3::new(1.0 + off, 0.1 + off, 0.0));
    let (contacts, bad) = physics.detect(TOLERANCE * 0.5);
    assert!(bad.is_empty());
    let pair: Vec<_> = contacts
        .iter()
        .filter(|k| {
            (k.a, k.b) == (a as usize, b as usize) || (k.a, k.b) == (b as usize, a as usize)
        })
        .collect();
    assert!(!pair.is_empty(), "no contact between the slabs");
    for k in &pair {
        assert!((k.distance - 0.08).abs() < 1e-3, "{k:?}");
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

/// The Combat Arena's collision box as the original's geometry holds it, and
/// the order the MOPP the original compiles from it returns its triangles in
/// (live, `mopp-scout-alarm-arenas__live__000`; docs/29 section 12v).
#[test]
fn the_combat_arenas_mopp_returns_its_triangles_in_the_live_order() {
    let (x, y0, y1, z0, z1) = (
        12.7f32,
        -2.264_982_2f32,
        13.735_018f32,
        -12.397_295f32,
        13.002_705f32,
    );
    let v = [
        Vec3::new(-x, y0, z0),
        Vec3::new(x, y0, z0),
        Vec3::new(-x, y0, z1),
        Vec3::new(x, y0, z1),
        Vec3::new(-x, y1, z0),
        Vec3::new(x, y1, z0),
        Vec3::new(-x, y1, z1),
        Vec3::new(x, y1, z1),
    ];
    let tris = [
        [0, 2, 3],
        [3, 1, 0],
        [4, 5, 7],
        [7, 6, 4],
        [0, 1, 5],
        [5, 4, 0],
        [1, 3, 7],
        [7, 5, 1],
        [3, 2, 6],
        [6, 7, 3],
        [2, 0, 4],
        [4, 6, 2],
    ];
    assert_eq!(
        mopp::leaf_order(&v, &tris),
        vec![10, 0, 11, 8, 9, 3, 1, 6, 5, 2, 4, 7]
    );
}

/// A contact names the collidable that made it: two boxes side by side in one body
/// (the Scout's round extender and the fork arm behind it, docs/29 section 12as) are
/// told apart by the compound children's tags, wherever the point lies.
#[test]
fn a_contact_reports_the_tags_of_the_colliding_pieces() {
    use crate::backend::{CompoundChild, NO_TAG};
    let boxed = |lo: Vec3, hi: Vec3| {
        let mut pts = Vec::new();
        for x in [lo.x, hi.x] {
            for y in [lo.y, hi.y] {
                for z in [lo.z, hi.z] {
                    pts.push(Vec3::new(x, y, z));
                }
            }
        }
        ShapeDesc::ConvexHull { points: pts }
    };
    let child = |lo: Vec3, hi: Vec3, tag: u32| CompoundChild {
        local_position: Vec3::ZERO,
        local_rotation: Quat::IDENTITY,
        shape: boxed(lo, hi),
        tag,
    };
    let mut physics = Havok17Backend::new().unwrap();
    // The target: a short box (tag 8) at z 0..0.25 and a longer one (tag 9) at z 0.25..0.65.
    let target = physics
        .create_body(
            &BodyDesc::new(
                MotionType::Static,
                ShapeDesc::Compound(vec![
                    child(Vec3::new(-0.05, -0.1, 0.0), Vec3::new(0.05, 0.1, 0.25), 8),
                    child(Vec3::new(-0.05, -0.1, 0.25), Vec3::new(0.05, 0.1, 0.65), 9),
                ]),
            )
            .with_friction(0.3),
        )
        .unwrap();
    // A box coming down the z axis onto the far end of the long piece.
    let striker = physics
        .create_body(
            &BodyDesc::new(
                MotionType::Dynamic,
                boxed(Vec3::splat(-0.1), Vec3::splat(0.1)),
            )
            .at(Vec3::new(0.0, 0.0, 0.85))
            .with_mass(10.0)
            .with_friction(0.3),
        )
        .unwrap();
    physics.set_velocity(striker, Vec3::new(0.0, 0.0, -3.0), Vec3::ZERO);
    let mut seen = Vec::new();
    for _ in 0..30 {
        physics.step(1.0 / 60.0);
        seen.extend(physics.take_contacts());
    }
    let tags: Vec<u32> = seen
        .iter()
        .map(|e| if e.a == target { e.tag_a } else { e.tag_b })
        .collect();
    assert!(!tags.is_empty(), "the boxes never met");
    assert!(tags.iter().all(|&t| t == 9), "{tags:?}");
    assert!(seen.iter().all(|e| {
        let other = if e.a == target { e.tag_b } else { e.tag_a };
        other == NO_TAG
    }));
}

/// A rod (60 kg) slid at its stop on a 300 kg housing. The original's prismatic controller is a
/// Havok action: it runs after a piece's integration and before the solver, and locks (rod
/// velocities to zero, limits to the offset) when the offset is within 0.05 of the stop it is
/// driven to, so the rod that arrives in that piece never meets the solver with its speed. Live
/// (`weapons-wideload-free__live__000`, docs/29 12ay): the housing is kicked once (1.5 m/s) by
/// the limit and not again when the lock takes the rod's remaining 5.5 m/s. Unarmed (the lock a
/// piece late) the limit absorbs it through the housing a second time.
fn rod_at_its_stop(armed: bool) -> (f32, f32) {
    let mut physics = Havok17Backend::new().unwrap();
    let mk = |physics: &mut Havok17Backend, mass: f32, x: f32| {
        physics
            .create_body(
                &BodyDesc::new(MotionType::Dynamic, ShapeDesc::Sphere { radius: 0.2 })
                    .at(Vec3::new(x, 10.0, 0.0))
                    .with_mass(mass),
            )
            .unwrap()
    };
    let housing = mk(&mut physics, 300.0, 0.0);
    let rod = mk(&mut physics, 60.0, 0.0);
    let mut desc = SliderDesc::new(housing, rod, Vec3::ZERO, Vec3::X);
    desc.limits = [-0.73, 0.0];
    let joint = physics.create_slider(&desc).unwrap();
    // The rod 0.3 m out, going to its stop at 12 m/s; both fall together.
    physics.set_transform(rod, Vec3::new(-0.43, 10.0, 0.0), Quat::IDENTITY);
    physics.set_velocity(rod, Vec3::new(-12.0, 0.0, 0.0), Vec3::ZERO);
    let mut locked_at = None;
    for i in 0..8 {
        if armed {
            physics.arm_slider_lock(joint, -0.73, 0.05);
        }
        physics.step(1.0 / 60.0);
        if locked_at.is_none() && physics.joint(joint).unwrap().saved_limits.is_some() {
            locked_at = Some(i);
        }
    }
    let h = physics.state(housing).unwrap().linear_velocity.x;
    let r = physics.state(rod).unwrap().linear_velocity.x;
    (h, r)
}

#[test]
fn a_rod_that_arrives_in_a_piece_is_locked_before_that_pieces_solver() {
    let (h_late, r_late) = rod_at_its_stop(false);
    let (h_pre, r_pre) = rod_at_its_stop(true);
    // Both end with the rod at rest against the housing; the pre-solver lock hands the housing
    // less of the rod's momentum (the lock takes the rod's speed without a reaction).
    assert!(
        (r_late - h_late).abs() < 0.5 && (r_pre - h_pre).abs() < 0.5,
        "{r_late} {h_late} {r_pre} {h_pre}"
    );
    assert!(
        h_pre.abs() + 0.3 < h_late.abs(),
        "late {h_late}, pre-solver {h_pre}"
    );
}
