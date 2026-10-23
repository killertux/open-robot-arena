//! A robot's placement and landing against the original (docs/29 section
//! 12x): the weight a body carries into its first substep after the
//! placement, the empty first substep after the bell, and where a `Steering`
//! unit holds its hubs.

use std::path::{Path, PathBuf};

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
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn havok17() -> bool {
    !std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17")
}

fn duel(game: &Path, a: &str, b: &str) -> Duel {
    let setup = |bot: &str| BotSetup {
        path: game
            .join(format!("AI/{bot}.bot"))
            .to_string_lossy()
            .to_string(),
        yaw_deg: None,
        spawn: None,
    };
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    Duel::new(&arena, &[setup(a), setup(b)]).expect("a duel")
}

/// Live (`spinup-*__live__000`, `pieces-*__live__000`), the first substep
/// after the placement moves a body by `-0.2 m/s · m_then / m`, `m_then` its
/// mass at the build's last wake: REVENGE's wheels 10 of 27 kg, its blade all
/// of its 30, Raptor's top 12 of 45, the Scout's fork arm 12 of 26, ALARM's
/// piston rod 8 of 33, Berserker's second wheel 10 of 27.
#[test]
fn a_body_that_grew_after_the_builds_last_wake_keeps_its_smaller_weight() {
    let Some(game) = game_dir() else {
        eprintln!("skipping: no local/game (set RA2_GAME)");
        return;
    };
    if !havok17() {
        return;
    }
    let _scripts = ra2script::lock();
    // (robots, robot, [(mass, live share)])
    type Case<'a> = (&'a str, &'a str, usize, &'a [(f32, f32)]);
    let cases: [Case; 5] = [
        (
            "Team12/Bot1",
            "Team12/Bot2",
            0,
            &[(27.0, 10.0 / 27.0), (30.0, 1.0)],
        ),
        (
            "Team12/Bot1",
            "Team12/Bot2",
            1,
            &[(45.0, 12.0 / 45.0), (28.0, 1.0)],
        ),
        (
            "Team0/Bot0",
            "Team0/Bot1",
            0,
            &[(26.0, 12.0 / 26.0), (27.0, 1.0)],
        ),
        (
            "Team0/Bot0",
            "Team0/Bot1",
            1,
            &[(33.0, 8.0 / 33.0), (27.0, 1.0)],
        ),
        ("Team0/Bot2", "Team10/Bot0", 1, &[(72.0, 1.0)]),
    ];
    for (a, b, robot, want) in cases {
        let d = duel(&game, a, b);
        let r = &d.robots[robot];
        for &(mass, share) in want {
            let found = r.parts.iter().enumerate().skip(1).any(|(p, part)| {
                let m = d.physics.mass(part.body).unwrap_or(0.0);
                (m - mass).abs() < 0.5 && (r.build_share[p] - share).abs() < 1e-3
            });
            assert!(
                found,
                "{a} v {b} robot {robot}: no {mass} kg part with share {share:.3} ({:?})",
                r.build_share
            );
        }
    }
}

/// Held through the countdown a body gathers no weight, so the first substep
/// after the bell integrates nothing (live S rows: every body of both robots
/// keeps its velocity 0 through that integration) and the fall starts in the
/// second at -0.2 m/s. REVENGE has nothing else moving it then; Raptor's
/// weapon motor takes hold within these substeps (live in the third), so of
/// Raptor only the chassis is checked.
#[test]
fn the_first_substep_after_the_bell_moves_nothing() {
    let Some(game) = game_dir() else {
        eprintln!("skipping: no local/game (set RA2_GAME)");
        return;
    };
    if !havok17() {
        return;
    }
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "Team12/Bot1", "Team12/Bot2");
    d.hold_robots(true);
    d.hold_robots(false);
    let idle = vec![ra2sim::DriveInput::default(); d.robots.len()];
    let vy = |d: &Duel| -> Vec<f32> {
        d.robots[0]
            .parts
            .iter()
            .map(|p| p.body)
            .chain([d.robots[1].body()])
            .map(|b| d.physics.state(b).unwrap().linear_velocity.y)
            .collect()
    };
    d.step_direct(&idle, ra2sim::DEFAULT_STEP);
    assert!(vy(&d).iter().all(|v| v.abs() < 0.01), "{:?}", vy(&d));
    d.step_direct(&idle, ra2sim::DEFAULT_STEP);
    let after = vy(&d);
    assert!(
        after.iter().all(|v| (v + 0.2).abs() < 0.03),
        "the second substep falls at gravity's pace: {after:?}"
    );
}

/// Sentinel's wheel constraints hold its steering hubs at their own pivots,
/// on the hub line 0.058 m inboard of the points the wheels mount on (live,
/// atom `+0x58`: 0.8281 and 0.8362 m from the chassis' centre of mass across,
/// the hubs' centres 0.1404 and 0.1386 m further out; `carsteering2.gmf`'s
/// `Carwheel01`/`Carwheel02`).
#[test]
fn a_steering_unit_holds_its_hubs_at_its_wheel_constraints_pivots() {
    let Some(game) = game_dir() else {
        eprintln!("skipping: no local/game (set RA2_GAME)");
        return;
    };
    let _scripts = ra2script::lock();
    let rig =
        ra2sim::rig::load(game.join("AI/Team0/Bot2.bot").to_str().unwrap()).expect("Sentinel");
    let mut seen = 0;
    for part in &rig.parts {
        let Some(j) = part.joint else { continue };
        if !rig.components[j.motor]
            .class
            .eq_ignore_ascii_case("steering")
        {
            continue;
        }
        seen += 1;
        let off = j.anchor - j.pivot;
        assert!(
            (off.length() - 0.058).abs() < 0.003,
            "hub anchor {:?} from its mount point {:?}",
            j.anchor,
            j.pivot
        );
        assert!(
            off.normalize().dot(j.axis).abs() > 0.999,
            "the anchor lies on the hub line"
        );
        assert!(off.dot(j.pivot.with_y(0.0)) < 0.0, "inboard");
    }
    assert_eq!(seen, 2, "two hubs");
}
