//! A career round loads the player's robot from the save folder
//! (`Teams/Team1/Bot3.bot`), never from under the install. The loaders find the
//! install's `Components` from a robot's own path unless `RA2_GAME` names it,
//! so `ra2_app::run` must name the install: without it the round exited with
//! "could not locate game root". Own test binary: it edits the process
//! environment and configures the process-wide save directory.

#![cfg(unix)]

use std::path::{Path, PathBuf};

fn game_dir() -> Option<PathBuf> {
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

fn first_bot(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bot"))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

#[test]
fn a_robot_in_the_save_folder_loads_after_the_app_names_the_install() {
    let Some(real) = game_dir() else { return };
    let Some(bot) = first_bot(&real.join("Robot Designs")) else {
        return;
    };
    let root = std::env::temp_dir().join(format!("ra2-saverobot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // A scratch install of symlinks: the real folder is only read.
    let install = root.join("install");
    std::fs::create_dir_all(&install).unwrap();
    for entry in std::fs::read_dir(&real).unwrap().flatten() {
        std::os::unix::fs::symlink(entry.path(), install.join(entry.file_name())).unwrap();
    }
    let save = root.join("save");
    let team_dir = save.join("Teams").join("Team1");
    std::fs::create_dir_all(&team_dir).unwrap();
    let saved = team_dir.join("Bot3.bot");
    std::fs::copy(&bot, &saved).unwrap();

    std::env::remove_var("RA2_GAME");
    // Nothing above the save folder is an install: the walk up from the robot fails.
    assert!(
        ra2_render::find_game_root(&saved).is_none(),
        "the scratch save folder must not look like an install"
    );
    let png = root.join("menu.png");
    ra2_app::run([
        "--game".to_string(),
        install.display().to_string(),
        "--save-dir".to_string(),
        save.display().to_string(),
        "--offscreen".to_string(),
        png.display().to_string(),
    ])
    .expect("the headless shell run completes");

    let found = ra2_render::find_game_root(&saved).expect("run() named the install in RA2_GAME");
    assert_eq!(found, install.canonicalize().unwrap());
    ra2_render::load_bot_components(&saved.to_string_lossy())
        .expect("the save-folder robot loads its components");
    std::fs::remove_dir_all(&root).ok();
}
