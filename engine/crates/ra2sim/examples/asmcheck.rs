//! Diagnostic: assemble one robot and drive it, printing per-body motion.
//!
//! Usage: cargo run --release -p ra2sim --example asmcheck -- <arena> <bot> [seconds]

use glam::{Quat, Vec3};
use ra2sim::backend::{MotionType, PhysicsBackend};
use ra2sim::jolt::JoltBackend;
use ra2sim::robot::Robot;
use ra2sim::scene::{self, USER_ARENA};
use ra2sim::sim::{DriveInput, DriveTuning};

fn main() {
    let arena_path = std::env::args()
        .nth(1)
        .expect("usage: <arena> <bot> [secs]");
    let bot_path = std::env::args()
        .nth(2)
        .expect("usage: <arena> <bot> [secs]");
    let secs: f32 = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3.0);

    std::env::set_var("RA2_OBSTACLE", "none");
    let arena = scene::load_arena(&arena_path).expect("arena");
    let mut physics: Box<dyn PhysicsBackend> =
        Box::new(JoltBackend::new(arena.gravity).expect("world"));
    let (verts, indices) = arena.triangle_soup();
    physics
        .create_body(
            &ra2sim::backend::BodyDesc::new(
                MotionType::Static,
                ra2sim::backend::ShapeDesc::TriMesh {
                    vertices: verts,
                    indices,
                },
            )
            .with_friction(arena.friction)
            .with_user_data(USER_ARENA),
        )
        .expect("arena body");

    let rig = ra2sim::rig::load(&bot_path).expect("bot");
    println!(
        "rig: {} components, {} bodies, mass {:.0} kg",
        rig.components.len(),
        rig.parts.len(),
        rig.mass
    );
    let rig_names: Vec<String> = rig
        .parts
        .iter()
        .map(|p| rig.components[p.root].name.clone())
        .collect();

    let tuning = DriveTuning::default();
    println!(
        "tuning: max_yaw={:.1} rad/s (top speed comes from each motor's `power`)",
        tuning.max_yaw_rate
    );

    // Drive maths, once.
    let forward = Vec3::new(0.0, 0.0, -1.0);
    println!("\npart  use    rate torque | axle(bot)              roll(bot)              r     target_w  lowest_y");
    for (p, part) in rig.parts.iter().enumerate() {
        let Some(info) = part.joint else {
            println!("{p:4}  (no joint) root={}", rig.components[part.root].name);
            continue;
        };
        let c = &rig.components[part.root];
        let motor = &rig.components[info.motor];
        let child_z = c.local.transform_vector3(Vec3::Z).normalize_or_zero();
        let child_o = c.local.transform_point3(Vec3::ZERO);
        let motor_o = motor.local.transform_point3(Vec3::ZERO);
        println!(
            "    childZ=({:5.2},{:5.2},{:5.2}) childO=({:5.2},{:5.2},{:5.2}) motorO=({:5.2},{:5.2},{:5.2})",
            child_z.x, child_z.y, child_z.z, child_o.x, child_o.y, child_o.z, motor_o.x, motor_o.y, motor_o.z
        );
        let axle = info.axis;
        let roll = axle.cross(Vec3::Y).normalize_or_zero();
        let hub = c.local.transform_point3(Vec3::ZERO);
        // The wheel's top speed: its own motor's rate, at its own radius.
        let v = info.rate * c.wheel_radius();
        let v_hub = forward * v;
        let target = v_hub.dot(roll) / c.wheel_radius().max(0.05);
        let mut lowest = f32::MAX;
        for &i in &part.members {
            for q in &rig.components[i].hull_points {
                lowest = lowest.min(rig.part_local[i].transform_point3(*q).y);
            }
        }
        println!(
            "{p:4}  {:<6} {:4.0} {:<6.0} | ({:5.2},{:5.2},{:5.2}) ({:5.2},{:5.2},{:5.2}) {:.3} {:8.2}  {:.3}  motor={} hub=({:.2},{:.2},{:.2})",
            format!("{:?}", info.use_kind),
            info.rate,
            info.max_torque,
            axle.x, axle.y, axle.z,
            roll.x, roll.y, roll.z,
            c.wheel_radius(),
            target,
            lowest,
            rig.components[info.motor].name,
            hub.x, hub.y, hub.z,
        );
    }

    // Spawn above the floor.
    let floor = 0.0;
    let position = Vec3::new(0.0, floor + 0.5, 0.0);
    let mut robot =
        Robot::spawn(&mut *physics, rig, 0, position, Quat::IDENTITY, 1.0).expect("spawn");

    let dt = 1.0 / 60.0;
    let steps = (secs / dt) as usize;
    for step in 0..=steps {
        if step > 0 {
            robot.drive(
                &mut *physics,
                DriveInput {
                    throttle: 1.0,
                    steer: 0.0,
                },
                &tuning,
            );
            physics.step(dt);
        }
        if step == 0 {
            let worlds = robot.component_worlds(&*physics);
            let names: Vec<String> = robot
                .rig
                .components
                .iter()
                .map(|c| c.name.clone())
                .collect();
            let mut rows: Vec<(f32, usize, String)> = Vec::new();
            for (i, c) in robot.rig.components.iter().enumerate() {
                if c.hull_points.is_empty() {
                    continue;
                }
                let mut lo = f32::MAX;
                for q in &c.hull_points {
                    lo = lo.min(worlds[i].transform_point3(*q).y);
                }
                rows.push((lo, i, names[i].clone()));
            }
            rows.sort_by(|a, b| a.0.total_cmp(&b.0));
            println!("\nlowest points (world y):");
            for (lo, i, n) in rows.iter().take(8) {
                println!("   {lo:7.3}  #{i:2} {n}");
            }
        }
        if step % 30 == 0 {
            let bot = robot.transform(&*physics);
            let p = bot.w_axis.truncate();
            let v = robot.velocity(&*physics);
            println!(
                "\nt={:4.2}s bot=({:6.2},{:6.2},{:6.2}) v=({:5.2},{:5.2},{:5.2})",
                step as f32 * dt,
                p.x,
                p.y,
                p.z,
                v.x,
                v.y,
                v.z
            );
            for (i, part) in robot.parts.iter().enumerate() {
                let st = physics.state(part.body);
                let (bp, av) = st
                    .map(|s| (s.position, s.angular_velocity))
                    .unwrap_or((Vec3::ZERO, Vec3::ZERO));
                let angle = part.joint.map(|j| physics.hinge_angle(j)).unwrap_or(0.0);
                println!(
                    "   part {i} {:22} alive={} pos=({:6.2},{:6.2},{:6.2}) w=({:6.1},{:6.1},{:6.1}) hinge={:6.2}",
                    rig_names[i], part.alive, bp.x, bp.y, bp.z, av.x, av.y, av.z, angle
                );
            }
        }
    }
}
