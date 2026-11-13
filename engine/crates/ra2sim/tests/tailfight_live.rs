//! A fight from a placed state, against the original's (`docs/29-havok17-backend.md` section
//! 12bg).
//!
//! `tools/oracle/capture_tailfight.py` stood the Scout on its tail in the Octagon (hazards on),
//! put ALARM 2.5 m away facing it and let both go with the shipped AIs: `frida/ra2_tipab.js`
//! recorded the placed state of every body (`X` rows) and the Scout's lean (`up.y`) at every
//! substep. Run from the same state with the brains stepping from the release (`Duel::step`:
//! the brain's weapon logic is in its `Tick`, which the original ran while the capture had its
//! `Think` put off, so ALARM's `Fire` came on the Scout's entry into its zone and the hammer was
//! moving 7 substeps after the release), the Scout's lean follows the original's to 0.05 over the
//! first 16 substeps (the five states of `tests/data/tailfight_live.txt`: largest differences
//! 0.014-0.045) and the hammer is on its way by then (15.7 m/s against the original's 6.9 m/s at
//! substep 7 and 14-18 at the blow). With the brains off until later, as the first version of
//! the tool had them, the hammer's blow at that substep is missing.

use std::path::{Path, PathBuf};

use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, Duel, DEFAULT_STEP};

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(PathBuf::from(dir));
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for up in [3usize, 4] {
        let mut p = here.to_path_buf();
        for _ in 0..up {
            p.pop();
        }
        let p = p.join("local/game");
        if p.join("Components").is_dir() {
            return Some(p);
        }
    }
    None
}

struct LiveBody {
    mass: f32,
    com: Vec3,
    vel: Vec3,
    ang: Vec3,
    rot: Quat,
    inertia: Option<Mat3>,
}

struct Trial {
    label: String,
    bodies: Vec<LiveBody>,
    lean: Vec<f32>,
}

fn trials() -> Vec<Trial> {
    let text = include_str!("data/tailfight_live.txt");
    let mut out = Vec::new();
    let mut cur: Option<Trial> = None;
    for line in text.lines() {
        if let Some(l) = line.strip_prefix("trial ") {
            cur = Some(Trial {
                label: l.to_string(),
                bodies: Vec::new(),
                lean: Vec::new(),
            });
        } else if let Some(l) = line.strip_prefix("b ") {
            let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            let m = Mat3::from_cols_array(&[
                v[10], v[11], v[12], v[13], v[14], v[15], v[16], v[17], v[18],
            ]);
            cur.as_mut().unwrap().bodies.push(LiveBody {
                mass: v[0],
                com: Vec3::new(v[1], v[2], v[3]),
                vel: Vec3::new(v[4], v[5], v[6]),
                ang: Vec3::new(v[7], v[8], v[9]),
                rot: Quat::from_mat3(&m).normalize(),
                inertia: (v.len() >= 28)
                    .then(|| Mat3::from_cols_array(&v[19..28].try_into().unwrap()).transpose()),
            });
        } else if let Some(l) = line.strip_prefix("lean ") {
            cur.as_mut().unwrap().lean = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
        } else if line == "end" {
            out.extend(cur.take());
        }
    }
    out
}

fn snapshot(duel: &Duel, robot: usize) -> Vec<(u32, Vec3, Quat)> {
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

/// Every body rigidly turned by `q` about the chassis' centre, the chassis' centre at `at`.
fn place(duel: &mut Duel, snap: &[(u32, Vec3, Quat)], q: Quat, at: Vec3, v: Vec3, w: Vec3) {
    let c0 = snap[0].1;
    for &(b, c, rot) in snap {
        let s = duel.physics.state(b).unwrap();
        let now = duel.physics.center_of_mass(b).unwrap();
        let off = s.rotation.inverse() * (s.position - now);
        let com = at + q * (c - c0);
        let r = q * rot;
        duel.physics.set_transform(b, com + r * off, r);
        duel.physics.set_velocity(b, v + w.cross(com - at), w);
    }
}

/// The rotation of ours' body that puts its inertia tensor where the original's body of rotation
/// `rl` and local inertia `il` has it: `rl * g` for the turn of the cube `g` whose `g io g^T` is
/// nearest `il` (among equals, the one nearest `current`; `examples/tipprobe.rs`).
fn frame_of_ours(rl: Quat, il: Mat3, io: Mat3, current: Quat) -> Quat {
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
    let mut best = (f32::MAX, current);
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

/// Puts a robot's bodies at the original's placed state (`live[0]` its chassis): the chassis
/// exactly, the others at the live centre of the body of the same mass nearest them, in the live
/// frame found from the two inertia tensors.
fn adopt(duel: &mut Duel, snap: &[(u32, Vec3, Quat)], live: &[LiveBody]) {
    let lc = &live[0];
    let q = lc.rot * snap[0].2.inverse();
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
            let off = s.rotation.inverse() * (s.position - com);
            let mut rot = s.rotation;
            if let (Some(il), Some((_, _, inv))) = (l.inertia, duel.physics.mass_properties(b)) {
                rot = frame_of_ours(l.rot, il, inv.inverse(), s.rotation);
            }
            duel.physics.set_transform(b, l.com + rot * off, rot);
            duel.physics.set_velocity(b, l.vel, l.ang);
        }
    }
}

/// Runs a trial from the live placed state with the brains stepping (`brains`) or the robots
/// idle, and returns the Scout's lean at the placed state and after each of `steps` substeps,
/// and the fastest the hammer rod of ALARM (the 33 kg body) moved in them.
fn run(game: &Path, t: &Trial, brains: bool, steps: usize) -> (Vec<f32>, f32) {
    let _scripts = ra2script::lock();
    std::env::set_var("RA2_HAZARDS", "1");
    let arena = game.join("Arenas/octagon.py").to_string_lossy().to_string();
    let bot = |p: &str| BotSetup {
        path: game.join(p).to_string_lossy().to_string(),
        yaw_deg: None,
        spawn: None,
    };
    let mut duel = Duel::new(
        &arena,
        &[bot("AI/Team0/Bot0.bot"), bot("AI/Team0/Bot1.bot")],
    )
    .expect("a duel");
    let idle = vec![ra2sim::DriveInput::default(); 2];
    for _ in 0..60 {
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let snap0 = snapshot(&duel, 0);
    let c0 = snap0[0].1;
    for _ in 0..30 {
        place(
            &mut duel,
            &snap0,
            Quat::IDENTITY,
            c0 + Vec3::Y,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let k = t
        .bodies
        .iter()
        .position(|l| (l.mass - 232.116).abs() < 0.05)
        .expect("ALARM's chassis");
    adopt(&mut duel, &snap0, &t.bodies[..k]);
    let snap1 = snapshot(&duel, 1);
    adopt(&mut duel, &snap1, &t.bodies[k..]);
    let chassis = duel.robots[0].parts[0].body;
    let rods: Vec<u32> = duel.robots[1]
        .parts
        .iter()
        .map(|p| p.body)
        .filter(|&b| duel.physics.mass(b).is_some_and(|m| (m - 33.0).abs() < 0.5))
        .collect();
    let mut lean = Vec::new();
    let mut rod_max = 0.0f32;
    for _ in 0..=steps {
        let s = duel.physics.state(chassis).unwrap();
        lean.push((s.rotation * Vec3::Y).y);
        for &r in &rods {
            rod_max = rod_max.max(duel.physics.state(r).unwrap().linear_velocity.length());
        }
        if brains {
            duel.step(DEFAULT_STEP);
        } else {
            duel.step_direct(&idle, DEFAULT_STEP);
        }
    }
    (lean, rod_max)
}

fn available() -> Option<PathBuf> {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return None;
    };
    if std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17") {
        eprintln!("skipped: not the havok17 backend");
        return None;
    }
    Some(game)
}

/// The lean of the Scout on its tail follows the original's through the first 16 substeps, with
/// the brains stepping from the release.
#[test]
fn the_scouts_lean_follows_the_originals_in_the_first_16_substeps_of_a_fight() {
    let Some(game) = available() else { return };
    let mut worst = 0.0f32;
    for t in trials() {
        let (lean, _) = run(&game, &t, true, 16);
        let d = (0..=16)
            .map(|i| (lean[i] - t.lean[i]).abs())
            .fold(0.0f32, f32::max);
        eprintln!("{}: largest lean difference {d:.4}", t.label);
        assert!(d < 0.06, "{}: lean differs by {d}", t.label);
        worst = worst.max(d);
    }
    assert!(worst > 0.0);
}

/// ALARM's hammer is moving fast by the 8th substep of the first fight state (the original's rod
/// was at 6.9 m/s along the line to the Scout at substep 7, `Fire` pressed by the brain's `Tick`
/// on the Scout's entry into its zone), as it is with the brains stepping from the release and
/// is not with them off.
#[test]
fn alarms_hammer_is_on_its_way_when_the_brains_step_from_the_release() {
    let Some(game) = available() else { return };
    let t = trials()
        .into_iter()
        .find(|t| t.label == "s0,0_az_-90")
        .expect("the trial");
    let (_, with) = run(&game, &t, true, 12);
    let (_, without) = run(&game, &t, false, 12);
    eprintln!("hammer rod fastest: brains {with:.2} m/s, brains off {without:.2} m/s");
    assert!(with > 3.0, "rod {with} m/s");
    assert!(without < 2.0, "rod without the brains {without} m/s");
}
