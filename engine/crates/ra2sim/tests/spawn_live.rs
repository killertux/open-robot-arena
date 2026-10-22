//! Where a match puts the robots (docs/29 section 12r): each robot's origin
//! at the floor plus 0.3975 m, so it falls 0.20-0.21 m onto its wheels, as
//! the original's `spawn-*__live__000` captures hold it through the intro.

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

/// The chassis' centre of mass at the bell, live (box: Scout -1.6729,
/// ALARM -1.6052, Razor -1.6431, FlapJack -1.6590; Compressor: Scout 5.5445,
/// ALARM 5.6123). FlapJack comes down one lowering step (0.05 m) further in
/// ours: the turns its wheels get on its start point (docs/29 section 12w)
/// leave its traction phantoms a few tenths of a millimetre on the other side
/// of the tolerance; live some of its bodies take only part of the first
/// substep after the placement, which ours does not model.
#[test]
fn robots_start_where_the_original_holds_them() {
    let Some(game) = game_dir() else {
        eprintln!("skipping: no local/game (set RA2_GAME)");
        return;
    };
    let _scripts = ra2script::lock();
    for (arena, a, b, ya, yb) in [
        ("box.py", "Team0/Bot0", "Team0/Bot1", -1.6729, -1.6052),
        ("box.py", "Team10/Bot1", "Team8/Bot0", -1.6431, -1.6590),
        ("compressor.py", "Team0/Bot0", "Team0/Bot1", 5.5445, 5.6123),
    ] {
        let setup = |bot: &str| BotSetup {
            path: game
                .join(format!("AI/{bot}.bot"))
                .to_string_lossy()
                .to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let d = Duel::new(
            &game.join("Arenas").join(arena).to_string_lossy(),
            &[setup(a), setup(b)],
        )
        .expect("a duel");
        for (r, want) in [(0, ya), (1, yb)] {
            let step = if arena == "box.py" && b == "Team8/Bot0" && r == 1 {
                0.05
            } else {
                0.0
            };
            let y = d
                .physics
                .center_of_mass(d.robots[r].parts[0].body)
                .unwrap()
                .y;
            assert!(
                (y - want).abs() < 0.002 || (y - (want - step)).abs() < 0.002,
                "{arena} robot {r}: {y} (live {want})"
            );
        }
    }
}
