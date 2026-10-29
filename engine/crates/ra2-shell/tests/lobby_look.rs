//! The pre-match lobby's left column is 286 px wide at 1024x768, not the 286.72 that 0.28 of the
//! screen makes (live `mp-lobby-host-chat`: its frames end at x 265, docs/38 §16.6).

use std::path::PathBuf;

use ra2_shell::{Shell, PREMATCH};

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/game");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[test]
fn the_lobbys_left_column_is_a_whole_number_of_pixels_wide() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.goto(PREMATCH).unwrap();
    let edge = shell
        .placements()
        .iter()
        .find_map(|p| {
            p.find("player settings group")
                .map(|g| g.frame.x + g.frame.w)
        })
        .expect("the settings group");
    // 8 + 0.9 x 286 is the original's column 265 (the unrounded 286.72 gives 266).
    assert_eq!(edge, 265.0);
}
