//! The 45-bot traction survey.
//!
//! Every stock AI robot is spawned on the box arena's own first start point and
//! driven at full throttle for three seconds; a robot passes when its chassis
//! travels 2 m or more. The count is the project's headline physics number
//! (`docs/05-validation-report.md` §6, `docs/06-deferred-fidelity.md` F2/F4).
//!
//! This test **prints the count and every failing robot with its distance**, so
//! a change that moves one bot across the line is visible in the log rather
//! than only in a changed assertion. The assertion is the last recorded count;
//! the printed table is the measurement.
//!
//! Baseline on the tree before the contact-layer fixes (F4's self-collision
//! group and the internal-fitting hull): **28/45**. The docs' older figure of
//! 27/45 predates this harness; `Team9/Bot1` is the borderline robot (2.09 m
//! net, barely over the line). Fourteen of the eighteen/nineteen failures were
//! robots whose own chassis parts dragged on the floor, which is what the
//! fixes below address.

use std::path::{Path, PathBuf};

use ra2sim::{BotSetup, DriveInput, Duel};

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
    // The last recorded count after the F4 contact-layer fixes. A change that
    // moves it must update this number *and* the docs; the per-bot table above
    // is the evidence.
    assert!(
        passed >= 38,
        "the traction survey regressed: {passed}/45 (was 38/45); failures: {failed:?}"
    );
}
