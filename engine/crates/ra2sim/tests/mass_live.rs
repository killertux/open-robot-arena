//! The Havok bodies' mass properties against the original, measured live
//! (`docs/29-havok17-backend.md` section 12f).
//!
//! `tests/data/mass_live.tsv` holds, for 14 stock robots, the chassis body's
//! mass and centre of mass in the robot frame (`assembly-*__live__000`), and
//! for five of them every wheel record's offset as `AddWheel` stored it
//! (`magic-*__live__*`, record `+0x2c`, chassis frame).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glam::Vec3;
use ra2sim::{BotSetup, Duel};

fn game_dir() -> Option<PathBuf> {
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

struct Row {
    kind: String,
    index: usize,
    mass: f32,
    live: Vec3,
}

fn rows() -> BTreeMap<String, Vec<Row>> {
    let mut out: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for l in include_str!("data/mass_live.tsv").lines() {
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = l.split('\t').collect();
        let n = |i: usize| f[i].trim().parse::<f32>().expect("number");
        out.entry(f[1].to_string()).or_default().push(Row {
            kind: f[0].to_string(),
            index: f[2].parse().expect("index"),
            mass: n(3),
            live: Vec3::new(n(4), n(5), n(6)),
        });
    }
    out
}

#[test]
fn chassis_bodies_and_wheel_records_match_the_original() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return;
    };
    if std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17") {
        eprintln!("skipped: not the havok17 backend");
        return;
    }
    let _scripts = ra2script::lock();
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let mut worst = (0.0f32, 0.0f32);
    for (bot, rows) in rows() {
        let setup = BotSetup {
            path: game.join(&bot).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let d = Duel::new(&arena, std::slice::from_ref(&setup)).expect("a duel");
        let r = &d.robots[0];
        let chassis = r.parts[0].body;
        let s = d.physics.state(chassis).expect("state");
        let com = d.physics.center_of_mass(chassis).expect("centre");
        let frame = r.rig.components[r.rig.parts[0].root].local;
        let ours = frame.transform_point3(s.rotation.inverse() * (com - s.position));
        let mass = d.physics.mass(chassis).expect("mass");
        for row in rows {
            if row.kind == "chassis" {
                assert!(
                    (mass - row.mass).abs() < 0.05,
                    "{bot}: chassis {mass} kg, live {}",
                    row.mass
                );
                let e = (ours - row.live).length();
                worst.0 = worst.0.max(e);
                // Within 7 mm (Tornado is the worst, 6.2 mm; eight robots within
                // 0.1 mm).
                assert!(
                    e < 0.007,
                    "{bot}: centre of mass {ours} live {} ({e} m)",
                    row.live
                );
            } else {
                let w = &r.magic.as_ref().expect("wheel records").wheels;
                let point = w.get(row.index).expect("a record").point;
                let e = (point - row.live).length();
                worst.1 = worst.1.max(e);
                assert!(
                    e < 0.002,
                    "{bot} wheel {}: {point} live {} ({e} m)",
                    row.index,
                    row.live
                );
            }
        }
    }
    eprintln!(
        "worst chassis centre {:.4} m, worst wheel record {:.4} m",
        worst.0, worst.1
    );
}

/// Eigenvalues of a symmetric 3x3 matrix, ascending (Jacobi rotations).
fn principal(m: glam::Mat3) -> [f32; 3] {
    let mut a = m.to_cols_array_2d().map(|c| c.map(f64::from));
    for _ in 0..64 {
        let (mut p, mut q) = (0, 1);
        for (i, j) in [(0, 2), (1, 2)] {
            if a[i][j].abs() > a[p][q].abs() {
                (p, q) = (i, j);
            }
        }
        if a[p][q].abs() < 1e-12 {
            break;
        }
        let th = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
        let (c, s) = (th.cos(), th.sin());
        for row in a.iter_mut() {
            let (kp, kq) = (row[p], row[q]);
            row[p] = c * kp - s * kq;
            row[q] = s * kp + c * kq;
        }
        let (rp, rq) = (a[p], a[q]);
        for k in 0..3 {
            a[p][k] = c * rp[k] - s * rq[k];
            a[q][k] = s * rp[k] + c * rq[k];
        }
    }
    let mut e = [a[0][0] as f32, a[1][1] as f32, a[2][2] as f32];
    e.sort_by(f32::total_cmp);
    e
}

/// The Scout's fork arm (extender 2 kg, fork 12, the burst motor's rotor 12):
/// live its body weighs 26 kg with principal moments (0.171, 2.513, 2.573)
/// kg·m² and the fork controller's load cap is 462.4 N·m
/// (`bodies-scout-alarm__live__000`, `hingectl-scout-alarm-box__live__000`).
/// The fork's collision mesh is wound inside out; its solid still counts,
/// centre and inertia from its own triangles (docs/29 §12g).
#[test]
fn the_scout_fork_arm_matches_the_original() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return;
    };
    if std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17") {
        eprintln!("skipped: not the havok17 backend");
        return;
    }
    let _scripts = ra2script::lock();
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let setup = BotSetup {
        path: game.join("AI/Team0/Bot0.bot").to_string_lossy().to_string(),
        yaw_deg: None,
        spawn: None,
    };
    let d = Duel::new(&arena, std::slice::from_ref(&setup)).expect("a duel");
    let r = &d.robots[0];
    let p = (0..r.rig.parts.len())
        .find(|&p| {
            r.rig.parts[p]
                .joint
                .is_some_and(|j| j.use_kind == ra2sim::rig::JointUse::Burst)
        })
        .expect("the fork's body");
    let (mass, _, inv) = d
        .physics
        .mass_properties(r.parts[p].body)
        .expect("mass properties");
    assert!((mass - 26.0).abs() < 0.01, "fork body {mass} kg");
    let e = principal(inv.inverse());
    for (ours, live) in e.iter().zip([0.171f32, 2.513, 2.573]) {
        assert!(
            (ours - live).abs() < 0.005,
            "principal moments {e:?}, live (0.171, 2.513, 2.573)"
        );
    }
    let cap = r.hinge_settings(p).expect("fork controller").max_torque;
    assert!((cap - 462.4).abs() < 1.0, "load cap {cap}, live 462.4");
}
