//! Diagnostic: ALARM (robot 1 of a box match, the Scout left at its spawn)
//! placed at a set tilt and spin and let go, as `frida/ra2_tipab.js` does in
//! the original (docs/29 section 12r): every body of the robot is held still
//! `HOLD` substeps 1 m above its rest pose (so no contact of the old pose is
//! left), then turned rigidly about the chassis' centre of mass and given the
//! rigid velocity field `v + w × r`.
//!
//! Usage:
//!   `tipprobe <arena.py> <botA> <botB> self <ax ay az> <deg> <w> <secs> [drop] [fire]`
//!     tilt about the chassis-frame axis (ax, ay, az) by `deg`, spin `w`
//!     rad/s about the same axis, the lowest hull point `drop` m above where
//!     it rests (default 0); fire the weapons `fire` s after the release.
//!   `tipprobe <arena.py> <botA> <botB> live <state.txt> <secs> [fire]`
//!     the robot's chassis exactly at the live placed state (`X` row bodies,
//!     one per line: mass, centre, velocity, angular velocity, rotation by
//!     columns), its other bodies rigidly with it and given the live
//!     velocities of the body of the same mass nearest them.
//!
//! Prints the chassis every substep after the release (`S` lines: time,
//! centre, up vector, angular velocity in world and chassis frames, linear
//! velocity); `RA2_H17_DUMP` writes the capture rows, and an `["X"]` row marks
//! the release.
use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

const HOLD: usize = 30;

/// The robot under test: `TIPPROBE_ROBOT` (default 1, ALARM in a box match).
fn robot_index() -> usize {
    std::env::var("TIPPROBE_ROBOT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

struct LiveBody {
    mass: f32,
    com: Vec3,
    vel: Vec3,
    ang: Vec3,
    rot: Quat,
    /// The original's local inertia tensor (row-major), when the state file carries it.
    inertia: Option<Mat3>,
}

/// The lowest point of the robot's solid geometry. A wheel's hull points take
/// in its traction phantom, whose ring reaches the floor at rest while the
/// solid wheel rests on its contacts 0.0975 m above it (live and ours): a
/// wheel counts from its lowest point that much higher.
const RING_BELOW_SOLID: f32 = 0.0975;

fn lowest(duel: &Duel) -> f32 {
    let r = &duel.robots[robot_index()];
    let worlds = r.component_worlds(&*duel.physics);
    r.rig
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
            if c.name.to_ascii_lowercase().contains("wheel") {
                lo + RING_BELOW_SOLID
            } else {
                lo
            }
        })
        .fold(f32::MAX, f32::min)
}

/// Every body of the robot: (handle, centre of mass, rotation).
fn snapshot(duel: &Duel) -> Vec<(u32, Vec3, Quat)> {
    snapshot_of(duel, robot_index())
}

fn snapshot_of(duel: &Duel, robot: usize) -> Vec<(u32, Vec3, Quat)> {
    duel.robots[robot]
        .parts
        .iter()
        .map(|p| {
            let s = duel.physics.state(p.body).unwrap();
            let c = duel.physics.center_of_mass(p.body).unwrap();
            (p.body, c, s.rotation)
        })
        .collect()
}

/// Move the snapshot rigidly: turned by `q` about the chassis' centre, the
/// chassis' centre at `at`, every body at `v + w × r`.
fn place(duel: &mut Duel, snap: &[(u32, Vec3, Quat)], q: Quat, at: Vec3, v: Vec3, w: Vec3) {
    let c0 = snap[0].1;
    for &(b, c, rot) in snap {
        let s = duel.physics.state(b).unwrap();
        let now = duel.physics.center_of_mass(b).unwrap();
        // The body origin's offset from its centre of mass, kept.
        let off = s.rotation.inverse() * (s.position - now);
        let com = at + q * (c - c0);
        let r = q * rot;
        duel.physics.set_transform(b, com + r * off, r);
        duel.physics.set_velocity(b, v + w.cross(com - at), w);
    }
}

/// The rotation of ours' body that puts its inertia tensor where the original's body of rotation `rl` and
/// local inertia `il` has it: `rl * g` for the turn of the cube `g` whose `g io g^T` is nearest `il`
/// (among equals, the one nearest `current`).
fn frame_of_ours(rl: Quat, il: Mat3, io: Mat3, current: Quat) -> Quat {
    let mut best = (f32::MAX, current);
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let scale = il
        .x_axis
        .x
        .abs()
        .max(il.y_axis.y.abs())
        .max(il.z_axis.z.abs());
    let mut cands = Vec::new();
    for p in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let g = Mat3::from_cols(axes[p[0]] * sx, axes[p[1]] * sy, axes[p[2]] * sz);
                    if g.determinant() > 0.0 {
                        let d = g * io * g.transpose() - il;
                        let err = d
                            .to_cols_array()
                            .iter()
                            .map(|x| x.abs())
                            .fold(0.0f32, f32::max)
                            / scale;
                        cands.push((err, Quat::from_mat3(&g)));
                    }
                }
            }
        }
    }
    let min_err = cands.iter().map(|c| c.0).fold(f32::MAX, f32::min);
    for (err, g) in cands {
        if err <= min_err + 0.02 {
            let q = (rl * g).normalize();
            let ang = q.angle_between(current);
            if ang < best.0 {
                best = (ang, q);
            }
        }
    }
    best.1
}

/// Puts a robot's bodies at the original's placed state (`live[0]` its chassis).
fn adopt(duel: &mut Duel, snap: &[(u32, Vec3, Quat)], live: &[LiveBody]) {
    let r0 = snap[0].2;
    let lc = &live[0];
    let q = lc.rot * r0.inverse();
    place(duel, snap, q, lc.com, lc.vel, lc.ang);
    for &(b, _, _) in &snap[1..] {
        let (m, _, _) = duel.physics.mass_properties(b).unwrap();
        let com = duel.physics.center_of_mass(b).unwrap();
        let best = live[1..]
            .iter()
            .filter(|l| (l.mass - m).abs() < 0.01)
            .min_by(|x, y| x.com.distance(com).total_cmp(&y.com.distance(com)));
        if let Some(l) = best {
            let s = duel.physics.state(b).unwrap();
            let (_, an) = (s.rotation.inverse() * l.rot).to_axis_angle();
            println!(
                "body {b} {m:.1} kg: live centre {:.4} away, rotation off {an:.3} rad",
                l.com.distance(com)
            );
            // `TIPPROBE_LIVE_POSE=1`: and its centre is the live one's exactly, and its rotation too when
            // the file carries the live inertia: the body frames of ours and of the original differ by a
            // turn of the cube, found from the two inertia tensors (docs/29 12be), so no joint starts
            // with the error of ours' rest pose against the original's.
            if std::env::var_os("TIPPROBE_LIVE_POSE").is_some() {
                let off = s.rotation.inverse() * (s.position - com);
                let mut rot = s.rotation;
                if let (Some(il), Some((_, _, inv))) = (l.inertia, duel.physics.mass_properties(b))
                {
                    rot = frame_of_ours(l.rot, il, inv.inverse(), s.rotation);
                }
                duel.physics.set_transform(b, l.com + rot * off, rot);
            }
            duel.physics.set_velocity(b, l.vel, l.ang);
        } else {
            println!("body {b} {m:.1} kg: no live body");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut duel = Duel::new(
        &a[0],
        &[
            BotSetup {
                path: a[1].clone(),
                yaw_deg: None,
                spawn: None,
            },
            BotSetup {
                path: a[2].clone(),
                yaw_deg: None,
                spawn: None,
            },
        ],
    )?;
    if std::env::var_os("TIPPROBE_SPAWN").is_some() {
        for (p, f) in ra2_render::start_points(&a[0])? {
            println!(
                "start point {:.4} {:.4} {:.4} facing {:.3} {:.3} {:.3}",
                p.x, p.y, p.z, f.x, f.y, f.z
            );
        }
        for (r, robot) in duel.robots.iter().enumerate() {
            let s = duel.physics.state(robot.parts[0].body).unwrap();
            println!(
                "robot {r} chassis body origin {:.4} {:.4} {:.4} rig.min.y {:.4}",
                s.position.x, s.position.y, s.position.z, robot.rig.min.y
            );
        }
    }
    let n = duel.robots.len();
    let idle = vec![DriveInput::default(); n];
    for i in 0..60 {
        if std::env::var_os("TIPPROBE_SPAWN").is_some() && i % 3 == 0 {
            // The match start: each chassis' centre height and speed.
            for (r, robot) in duel.robots.iter().enumerate() {
                let c = duel.physics.center_of_mass(robot.parts[0].body).unwrap();
                let s = duel.physics.state(robot.parts[0].body).unwrap();
                println!(
                    "spawn {:.3} robot {r} y {:.4} vy {:.3}",
                    i as f32 * DEFAULT_STEP,
                    c.y,
                    s.linear_velocity.y
                );
            }
        }
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let rest_low = lowest(&duel);
    if std::env::var_os("TIPPROBE_POINTS").is_some() {
        // Every component's hull points in the chassis' frame (centre of
        // mass origin): `pt <component> <part> x y z`.
        let r = &duel.robots[robot_index()];
        let worlds = r.component_worlds(&*duel.physics);
        let s = duel.physics.state(r.parts[0].body).unwrap();
        let c = duel.physics.center_of_mass(r.parts[0].body).unwrap();
        for (i, comp) in r.rig.components.iter().enumerate() {
            for p in &comp.hull_points {
                let l = s.rotation.inverse() * (worlds[i].transform_point3(*p) - c);
                println!(
                    "pt {i} {} {:.4} {:.4} {:.4}",
                    comp.name.replace(' ', "_"),
                    l.x,
                    l.y,
                    l.z
                );
            }
        }
    }
    let snap = snapshot(&duel);
    let c0 = snap[0].1;
    let r0 = snap[0].2;
    // Hold it 1 m up, still, so that no contact of the rest pose is left.
    // `TIPPROBE_PHASE=1`: one more held substep, so that the components'
    // once-a-frame tick falls on odd substeps after the release.
    let hold = HOLD + std::env::var("TIPPROBE_PHASE").map_or(0, |v| v.parse().unwrap_or(0));
    for _ in 0..hold {
        place(
            &mut duel,
            &snap,
            Quat::IDENTITY,
            c0 + Vec3::Y,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let mut scout_at: Option<Vec3> = None;
    let (secs, fire): (f32, Option<f32>) = match a[3].as_str() {
        "self" => {
            let axis_local = Vec3::new(a[4].parse()?, a[5].parse()?, a[6].parse()?).normalize();
            let deg: f32 = a[7].parse()?;
            let w: f32 = a[8].parse()?;
            let secs: f32 = a[9].parse()?;
            let drop: f32 = a.get(10).map(|s| s.parse()).transpose()?.unwrap_or(0.0);
            let fire = a.get(11).and_then(|s| s.parse().ok());
            let axis = r0 * axis_local;
            let q = Quat::from_axis_angle(axis, deg.to_radians());
            // Find the height that puts the lowest hull point where it rests.
            place(&mut duel, &snap, q, c0, Vec3::ZERO, Vec3::ZERO);
            let dy = rest_low + drop - lowest(&duel);
            // `TIPPROBE_COM_LIFT=<m>`: the chassis' centre that far above its rest
            // centre instead (the live trials release from 1 m, `dcentre` 0).
            let at = match std::env::var("TIPPROBE_COM_LIFT")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                Some(l) => c0 + Vec3::Y * l,
                None => c0 + Vec3::Y * dy,
            };
            // `TIPPROBE_COM_SHIFT=<dx>,<dz>`: and moved sideways by that (the live grids'
            // `dcentre`).
            let at = match std::env::var("TIPPROBE_COM_SHIFT") {
                Ok(v) => {
                    let p: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                    at + Vec3::new(
                        p.first().copied().unwrap_or(0.0),
                        0.0,
                        p.get(1).copied().unwrap_or(0.0),
                    )
                }
                Err(_) => at,
            };
            place(&mut duel, &snap, q, at, Vec3::ZERO, axis * w);
            scout_at = Some(at);
            println!(
                "placed centre {:.4} {:.4} {:.4} offset {:.4} {:.4} {:.4} q {:.5} {:.5} {:.5} {:.5} (w,x,y,z) axis {:.4} {:.4} {:.4} w {w}",
                at.x, at.y, at.z, at.x - c0.x, at.y - c0.y, at.z - c0.z, q.w, q.x, q.y, q.z, axis.x, axis.y, axis.z
            );
            (secs, fire)
        }
        "live" => {
            let live: Vec<LiveBody> = std::fs::read_to_string(&a[4])?
                .lines()
                .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                .map(|l| {
                    let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
                    let m = Mat3::from_cols_array(&[
                        v[10], v[11], v[12], v[13], v[14], v[15], v[16], v[17], v[18],
                    ]);
                    LiveBody {
                        mass: v[0],
                        com: Vec3::new(v[1], v[2], v[3]),
                        vel: Vec3::new(v[4], v[5], v[6]),
                        ang: Vec3::new(v[7], v[8], v[9]),
                        rot: Quat::from_mat3(&m).normalize(),
                        inertia: (v.len() >= 28).then(|| {
                            Mat3::from_cols_array(&v[19..28].try_into().unwrap()).transpose()
                        }),
                    }
                })
                .collect();
            let k = std::env::var_os("TIPPROBE_LIVE_ALARM")
                .and_then(|_| live.iter().position(|l| (l.mass - 232.116).abs() < 0.05))
                .unwrap_or(live.len());
            adopt(&mut duel, &snap, &live[..k]);
            if k < live.len() {
                // `TIPPROBE_LIVE_ALARM=1`: the second robot too (the hammer trials' `extra` group).
                let snap1 = snapshot_of(&duel, 1);
                adopt(&mut duel, &snap1, &live[k..]);
            }
            (a[5].parse()?, a.get(6).and_then(|s| s.parse().ok()))
        }
        m => return Err(format!("unknown mode {m}").into()),
    };
    // `TIPPROBE_ALARM=<azimuth deg>,<distance m>,<fire s>`: the other robot (1) is put `distance`
    // from the tipped one's rest centre in the horizontal direction `azimuth` (0 = +x, 90 = +z),
    // facing it, standing on the floor, and its `Fire` pressed `fire` s after the release (the
    // live trials' `extra` group, docs/29 12bd).
    let mut alarm_fire: Option<f32> = None;
    if let Ok(v) = std::env::var("TIPPROBE_ALARM") {
        let p: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        let (az, dist) = (p[0].to_radians(), p[1]);
        alarm_fire = p.get(2).copied();
        let snap1 = snapshot_of(&duel, 1);
        let c1 = snap1[0].1;
        let r1 = snap1[0].2;
        // `TIPPROBE_ALARM_REL=1`: from the tipped robot's placed centre rather than its rest centre.
        let base = if std::env::var_os("TIPPROBE_ALARM_REL").is_some() {
            scout_at.unwrap_or(c0)
        } else {
            c0
        };
        let target = Vec3::new(
            base.x + dist * az.cos(),
            c1.y + 0.03,
            base.z + dist * az.sin(),
        );
        let f0 = r1 * duel.robots[1].rig.forward();
        let u = Vec3::new(base.x - target.x, 0.0, base.z - target.z).normalize();
        let psi = u.x.atan2(u.z) - f0.x.atan2(f0.z);
        // Bodies are put in place at the release like the first group.
        place(
            &mut duel,
            &snap1,
            Quat::from_rotation_y(psi),
            target,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        println!("alarm placed at {:.3} {:.3} {:.3} yaw {:.1} deg (forward in the chassis frame {:?}, rest centre {:?}, q {:?})", target.x, target.y, target.z, psi.to_degrees(), duel.robots[1].rig.forward(), c1, r1);
    }
    if let Ok(v) = std::env::var("TIPPROBE_ALARM_FIRE") {
        alarm_fire = v.parse().ok();
    }
    ra2sim::havok17::dump_line(format_args!("[\"X\",0]"));
    let triggers: Vec<String> = duel.robots[robot_index()]
        .rig
        .wiring
        .controls
        .iter()
        .filter(|c| !c.name.is_empty())
        .map(|c| c.name.clone())
        .filter(|n| n != "LeftRight" && n != "Forward")
        .collect();
    // `TIPPROBE_DRIVE=step:throttle:turn,...`: from that substep after the
    // release the robot is driven with that command (-100..100, as the AI's
    // `Throttle`/`Turn`).
    let drive: Vec<(usize, f32, f32)> = std::env::var("TIPPROBE_DRIVE")
        .unwrap_or_default()
        .split(',')
        .filter_map(|d| {
            let v: Vec<&str> = d.split(':').collect();
            Some((
                v.first()?.parse().ok()?,
                v.get(1)?.parse().ok()?,
                v.get(2)?.parse().ok()?,
            ))
        })
        .collect();
    let mut command = DriveInput::default();
    // `TIPPROBE_AI=1`: from the release on the shipped brains drive both robots (as in a fight,
    // `Duel::step`), and each substep also prints the other robot's chassis (`A` lines).
    let ai = std::env::var_os("TIPPROBE_AI").is_some();
    // `TIPPROBE_AI_AT=<substeps>`: the brains are switched on that many substeps after the release (the live
    // capture's Python call takes a few frames: `AION` rows of `capture_tailfight.py`); until then the robots idle.
    let ai_at: usize = std::env::var("TIPPROBE_AI_AT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let steps = (secs / DEFAULT_STEP).round() as usize;
    let chassis = duel.robots[robot_index()].parts[0].body;
    for i in 0..steps {
        let t = i as f32 * DEFAULT_STEP;
        if fire.is_some_and(|f| (t - f).abs() < DEFAULT_STEP * 0.5) {
            let ev: Vec<(String, i32, f32)> =
                triggers.iter().map(|n| (n.clone(), 0, 1.0)).collect();
            duel.robots[robot_index()].apply_inputs(&ev);
        }
        if alarm_fire.is_some_and(|f| (t - f).abs() < DEFAULT_STEP * 0.5) {
            duel.robots[1].apply_inputs(&[("Fire".to_string(), 0, 1.0)]);
        }
        let s = duel.physics.state(chassis).unwrap();
        let c = duel.physics.center_of_mass(chassis).unwrap();
        let up = s.rotation * Vec3::Y;
        let wl = s.rotation.inverse() * s.angular_velocity;
        let mut sliders = String::new();
        {
            let r = &duel.robots[robot_index()];
            for (p, part) in r.parts.iter().enumerate() {
                if let (Some(j), Some(info)) = (part.joint, r.rig.parts[p].joint) {
                    if info.slider {
                        sliders += &format!(" s{p} {:.4}", duel.physics.slider_offset(j));
                    }
                }
            }
        }
        println!(
            "S {t:.4} c {:.4} {:.4} {:.4} up {:.4} {:.4} {:.4} w {:.4} {:.4} {:.4} wl {:.4} {:.4} {:.4} v {:.4} {:.4} {:.4}",
            c.x, c.y, c.z, up.x, up.y, up.z,
            s.angular_velocity.x, s.angular_velocity.y, s.angular_velocity.z,
            wl.x, wl.y, wl.z,
            s.linear_velocity.x, s.linear_velocity.y, s.linear_velocity.z
        );
        if !sliders.is_empty() {
            println!("J {t:.4}{sliders}");
        }
        if let Some(&(_, th, tu)) = drive.iter().rfind(|d| d.0 == i) {
            command = DriveInput::new(th / 100.0, tu / 100.0);
        }
        if ai {
            let other = 1 - robot_index();
            let ob = duel.robots[other].parts[0].body;
            let os = duel.physics.state(ob).unwrap();
            let oc = duel.physics.center_of_mass(ob).unwrap();
            let ou = os.rotation * Vec3::Y;
            println!(
                "A {t:.4} c {:.4} {:.4} {:.4} up {:.4} {:.4} {:.4}",
                oc.x, oc.y, oc.z, ou.x, ou.y, ou.z
            );
        }
        if ai && i >= ai_at {
            duel.step(DEFAULT_STEP);
        } else {
            let mut inputs = idle.clone();
            inputs[robot_index()] = command;
            duel.step_direct(&inputs, DEFAULT_STEP);
        }
    }
    Ok(())
}
