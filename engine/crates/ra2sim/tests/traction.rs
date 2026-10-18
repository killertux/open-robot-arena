//! The 45-bot traction survey.
//!
//! Every stock AI robot is spawned on the box arena's own first start point and
//! driven at full throttle for three seconds; a robot passes when its chassis
//! travels 2 m or more. The count is the project's headline physics number
//! (`docs/05-validation-report.md` §6, `docs/06-deferred-fidelity.md` F2/F4).
//!
//! This test **prints the count and every failing robot with its distance**, so
//! a change that moves one bot across the line is visible in the log rather
//! than only in a changed assertion. The printed table is the measurement.
//!
//! Baseline on the tree before the contact-layer fixes (F4's self-collision
//! group and the internal-fitting hull): **28/45**. The docs' older figure of
//! 27/45 predates this harness; `Team9/Bot1` is the borderline robot (2.09 m
//! net, barely over the line). The measured result after the fixes is
//! **39/45**; the six that remain are named in `docs/06` F4.

use std::path::{Path, PathBuf};

use ra2sim::{BotSetup, DriveInput, Duel, Role};

const DT: f32 = 1.0 / 60.0;
const SECONDS: f32 = 3.0;
const PASS_M: f32 = 2.0;

fn game_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(PathBuf::from(dir));
    }
    // `engine/crates/ra2sim` → repository root.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for up in [3usize, 4] {
        let mut p = here.to_path_buf();
        for _ in 0..up {
            p.pop();
        }
        let p = p.join("local/game");
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// The 45 stock AI robots, in a stable order (`AI/Team0/Bot0` … `Team14/Bot2`).
fn stock_bots(game: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(teams) = std::fs::read_dir(game.join("AI")) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = teams
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("Team"))
                    .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut bots: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bot"))
            .collect();
        bots.sort();
        out.extend(bots);
    }
    out
}

/// One robot's three-second run: `(name, metres travelled)`.
///
/// The spawn is the arena's own first start point, facing the centre, exactly
/// where the duel puts a robot — the survey measures the robot as the game
/// fields it, not from a hand-picked line.
fn travel(arena: &Path, bot: &Path) -> Result<(String, f32), Box<dyn std::error::Error>> {
    let name = format!(
        "{}/{}",
        bot.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?"),
        bot.file_stem().and_then(|n| n.to_str()).unwrap_or("?")
    );
    let setup = BotSetup {
        path: bot.to_string_lossy().to_string(),
        yaw_deg: None,
        spawn: None,
    };
    let mut duel = Duel::new(&arena.to_string_lossy(), std::slice::from_ref(&setup))?;
    let start = duel.robots[0].position(&*duel.physics);
    let input = DriveInput {
        throttle: 1.0,
        steer: 0.0,
    };
    for _ in 0..(SECONDS / DT).round() as usize {
        duel.step_direct(&[input], DT);
    }
    let end = duel.robots[0].position(&*duel.physics);
    Ok((name, (end - start).length()))
}

/// A single robot's three-second run, sampled: displacement, yaw, body height
/// and the lowest attached part. `RA2_TRACTION_BOT=Team7/Bot1` selects one.
#[test]
fn one_bot_reports_its_trajectory_over_time() {
    let (Some(game), Ok(want)) = (game_dir(), std::env::var("RA2_TRACTION_BOT")) else {
        return;
    };
    let bot = stock_bots(&game)
        .into_iter()
        .find(|p| {
            let n = format!(
                "{}/{}",
                p.parent()
                    .and_then(|q| q.file_name())
                    .and_then(|q| q.to_str())
                    .unwrap_or("?"),
                p.file_stem().and_then(|q| q.to_str()).unwrap_or("?")
            );
            n == want
        })
        .unwrap_or_else(|| panic!("{want} not found"));
    let setup = BotSetup {
        path: bot.to_string_lossy().to_string(),
        yaw_deg: None,
        spawn: None,
    };
    let arena = game.join("Arenas/box.py");
    let mut duel = Duel::new(&arena.to_string_lossy(), std::slice::from_ref(&setup)).unwrap();
    let start = duel.robots[0].position(&*duel.physics);
    {
        let robot = &duel.robots[0];
        for p in 0..robot.rig.parts.len() {
            let (Some(info), Some(joint)) = (robot.rig.parts[p].joint, robot.parts[p].joint) else {
                continue;
            };
            if !matches!(info.use_kind, ra2sim::rig::JointUse::Wheel) {
                continue;
            }
            let c = &robot.rig.components[robot.rig.parts[p].root];
            let hub = c.local.transform_point3(glam::Vec3::ZERO);
            let worlds = robot.component_worlds(&*duel.physics);
            let low = robot.rig.parts[p]
                .members
                .iter()
                .flat_map(|&i| robot.rig.components[i].hull_points.iter())
                .map(|q| worlds[robot.rig.parts[p].root].transform_point3(*q).y)
                .fold(f32::MAX, f32::min);
            println!(
                "wheel part {p} {} rate={:.1} rad/s r={:.3} v={:.2} m/s axis=({:.2},{:.2},{:.2}) hub=({:.2},{:.2},{:.2}) low={low:.3} joint={joint}",
                c.name,
                info.rate,
                c.wheel_radius(),
                info.rate * c.wheel_radius(),
                info.axis.x,
                info.axis.y,
                info.axis.z,
                hub.x,
                hub.y,
                hub.z
            );
        }
    }
    let drive = DriveInput {
        throttle: 1.0,
        steer: 0.0,
    };
    for step in 0..180 {
        duel.step_direct(&[drive], DT);
        if step % 30 == 29 {
            let robot = &duel.robots[0];
            let worlds = robot.component_worlds(&*duel.physics);
            let low = |i: usize| {
                robot.rig.components[i]
                    .hull_points
                    .iter()
                    .map(|p| worlds[i].transform_point3(*p).y)
                    .fold(f32::MAX, f32::min)
            };
            let wheel = robot
                .rig
                .components
                .iter()
                .enumerate()
                .filter(|(_, c)| c.role == Role::Wheel)
                .map(|(i, _)| low(i))
                .fold(f32::MAX, f32::min);
            let mut worst = (f32::MAX, String::new());
            for (i, c) in robot.rig.components.iter().enumerate() {
                if c.hull_points.is_empty() || !robot.attached[i] {
                    continue;
                }
                let l = low(i);
                if l < worst.0 {
                    worst = (l, format!("{} ({:?})", c.name, c.role));
                }
            }
            let (_, rot, pos) = robot
                .transform(&*duel.physics)
                .to_scale_rotation_translation();
            let yaw = rot.to_euler(glam::EulerRot::YXZ).0.to_degrees();
            let v = robot.velocity(&*duel.physics);
            println!(
                "t={:.1}s d={:.2} m pos=({:.2},{:.2}) yaw={:.0}° v=({:.2},{:.2}) wheels={:.3} lowest {} {:.3} ({:.3} below)",
                (step + 1) as f32 * DT,
                pos.distance(start),
                pos.x,
                pos.z,
                yaw,
                v.x,
                v.z,
                wheel,
                worst.1,
                worst.0,
                wheel - worst.0
            );
        }
    }
}

/// The rest-pose height report behind F4's remaining failures: for every stock
/// robot, the lowest world point of each component against the lowest wheel.
/// A part below the wheel line can prop the robot up, which no amount of
/// friction or torque fixes; the print names it and its clearance.
#[test]
fn the_rest_pose_report_names_every_part_below_the_wheels() {
    let (Some(game), Ok(_)) = (game_dir(), std::env::var("RA2_TRACTION_REPORT")) else {
        return;
    };
    let arena = game.join("Arenas/box.py");
    let mut flagged = 0usize;
    for bot in stock_bots(&game) {
        let setup = BotSetup {
            path: bot.to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let Ok(duel) = Duel::new(&arena.to_string_lossy(), std::slice::from_ref(&setup)) else {
            continue;
        };
        let robot = &duel.robots[0];
        let worlds = robot.component_worlds(&*duel.physics);
        let low = |i: usize| {
            robot.rig.components[i]
                .hull_points
                .iter()
                .map(|p| worlds[i].transform_point3(*p).y)
                .fold(f32::MAX, f32::min)
        };
        let wheel = robot
            .rig
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.role == Role::Wheel)
            .map(|(i, _)| low(i))
            .fold(f32::MAX, f32::min);
        if wheel == f32::MAX {
            continue;
        }
        let name = format!(
            "{}/{}",
            bot.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("?"),
            bot.file_stem().and_then(|n| n.to_str()).unwrap_or("?")
        );
        let mut rows = Vec::new();
        for (i, c) in robot.rig.components.iter().enumerate() {
            if c.hull_points.is_empty() || !robot.attached[i] {
                continue;
            }
            let l = low(i);
            if l < wheel - 0.002 {
                rows.push(format!(
                    "{} ({:?}) low {:.3} = {:.3} m below the wheels",
                    c.name,
                    c.role,
                    l,
                    wheel - l
                ));
            }
        }
        if !rows.is_empty() {
            flagged += 1;
            println!("{name}: wheels at {wheel:.3}");
            for r in rows {
                println!("  {r}");
            }
        }
    }
    println!("{flagged} stock robots have a part below the wheel line at rest");
}

#[test]
fn the_traction_survey_prints_its_count_and_failures() {
    let Some(game) = game_dir() else {
        eprintln!("skipping the traction survey: no local/game (set RA2_GAME to run it)");
        return;
    };
    let arena = game.join("Arenas/box.py");
    let bots = stock_bots(&game);
    assert!(
        !bots.is_empty(),
        "no stock AI bots under {}",
        game.display()
    );

    let mut passed = 0usize;
    let mut failed: Vec<(String, f32)> = Vec::new();
    let mut rows: Vec<(String, f32)> = Vec::new();
    for bot in &bots {
        match travel(&arena, bot) {
            Ok((name, d)) => {
                if d >= PASS_M {
                    passed += 1;
                } else {
                    failed.push((name.clone(), d));
                }
                rows.push((name, d));
            }
            Err(e) => failed.push((format!("{}: {e}", bot.display()), 0.0)),
        }
    }
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "traction survey: {passed}/{} travel >= {PASS_M} m in {SECONDS} s",
        bots.len()
    );
    for (name, d) in &rows {
        println!("  {d:6.2} m  {name}");
    }
    if !failed.is_empty() {
        println!("failing ({}):", failed.len());
        for (name, d) in &failed {
            println!("  {d:6.2} m  {name}");
        }
    }

    assert_eq!(
        bots.len(),
        45,
        "the stock AI set is 45 robots; found {}",
        bots.len()
    );
    // The last measured count is 39/45; 38 is the floor because `Team3/Bot1`
    // sits on the 2 m line (2.00 m printed, just under it). A change that moves
    // the count must update this number *and* the docs; the per-bot table above
    // is the evidence.
    assert!(
        passed >= 38,
        "the traction survey regressed below its floor: {passed}/45 (last measured 39); failures: {failed:?}"
    );
}
