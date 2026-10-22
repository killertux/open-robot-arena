//! How far a match's placement turns each wheel about its axle (docs/29
//! section 12w). The original builds a robot at the arena's origin, turns its
//! chassis alone onto the start point's heading and steps the world while the
//! joints pull the other bodies round (`0x4dcff0`, `0x4dee20`), which leaves
//! every wheel turned about its axle by some angle; live
//! (`spinup-*__live__000`) the angles below, for the bodies whose first
//! substep after the placement was a whole one.

use std::path::{Path, PathBuf};

use glam::Quat;
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

/// Every part's turn from the build to the hold, in the robot's frame, as an
/// angle in degrees (0..180).
fn turns(d: &Duel, robot: usize) -> Vec<f32> {
    let r = &d.robots[robot];
    let (_, c_local, _) = r.rig.components[r.rig.parts[0].root]
        .local
        .to_scale_rotation_translation();
    let frame = d.physics.state(r.body()).unwrap().rotation * c_local.inverse();
    r.rig
        .parts
        .iter()
        .enumerate()
        .map(|(p, part)| {
            let (_, built, _) = r.rig.components[part.root]
                .local
                .to_scale_rotation_translation();
            let now = d.physics.state(r.parts[p].body).unwrap().rotation;
            let rel: Quat = frame.inverse() * now * built.inverse();
            rel.angle_between(Quat::IDENTITY).to_degrees()
        })
        .collect()
}

#[test]
fn wheels_turn_about_their_axles_as_the_original_places_them() {
    let Some(game) = game_dir() else {
        eprintln!("skipping: no local/game (set RA2_GAME)");
        return;
    };
    if std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17") {
        eprintln!("skipped: not the havok17 backend");
        return;
    }
    let _scripts = ra2script::lock();
    let setup = |bot: &str| BotSetup {
        path: game
            .join(format!("AI/{bot}.bot"))
            .to_string_lossy()
            .to_string(),
        yaw_deg: None,
        spawn: None,
    };
    // (robots, robot, [(part, live degrees)])
    type Case<'a> = (&'a str, &'a str, usize, &'a [(usize, f32)]);
    let cases: [Case; 3] = [
        ("Team0/Bot0", "Team0/Bot1", 0, &[(1, 14.52), (2, 2.14)]),
        (
            "Team0/Bot0",
            "Team0/Bot1",
            1,
            &[(1, 66.75), (2, 10.57), (3, 85.71), (4, 10.50)],
        ),
        (
            "Team12/Bot1",
            "Team12/Bot2",
            1,
            &[(1, 4.22), (2, 173.64), (3, 137.99), (4, 39.59)],
        ),
    ];
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    for (a, b, robot, want) in cases {
        let d = Duel::new(&arena, &[setup(a), setup(b)]).expect("a duel");
        let got = turns(&d, robot);
        for &(p, live) in want {
            assert!(
                (got[p] - live).abs() < 2.5,
                "{a} v {b}, robot {robot} part {p}: turned {:.2} degrees, live {live}",
                got[p]
            );
        }
    }
}
