//! The exhibition flow's evidence, in its own test process.
//!
//! `ra2script` has process-global match state, so a full match run belongs in
//! an integration test rather than beside the unit tests that also drive
//! matches. These two tests share one mutex and run in this binary only.

use std::path::PathBuf;

use ra2_app::play_mode::{self, Options};
use ra2_app::SHELL_SIZE;
use ra2_shell::Shell;
use ra2sim::r#match::MatchState;
use ra2sim::GameType;

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

/// A scripted drive moves robot 0 at least a metre, the match reaches `Over`,
/// and the result screen and the three frames exist at the design size.
#[test]
fn a_scripted_drive_moves_robot_zero_and_reaches_the_results() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let out = std::env::temp_dir().join(format!("ra2-play-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let options = Options {
        game,
        drive: "0-3:1,0".into(),
        seconds: 5.0,
        out: out.clone(),
        size: [1280, 1005],
        arena_script: Some("box.py".into()),
        arena_index: None,
        game_type: None,
        team: None,
        bot: None,
        opponent_team: None,
        opponent_bot: None,
        time: None,
    };
    let outcome = play_mode::run_headless(&options).expect("the play run completes");
    assert!(
        outcome.distance >= 1.0,
        "the scripted drive moved robot 0 {:.2} m",
        outcome.distance
    );
    assert!(outcome.over, "the match reached Over");
    assert_eq!(outcome.state, MatchState::Results);
    for name in ["play-setup.png", "play-drive.png", "play-results.png"] {
        let path = out.join(name);
        let image = image::open(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .into_rgba8();
        assert_eq!(image.dimensions(), (1280, 1005), "{name}");
    }
    std::fs::remove_dir_all(&out).ok();
}

/// Clicking `start game button` is the game's own note ("start the game"),
/// not one of the shell's navigation links: the screen stays where it is and a
/// real match can be built from the choices underneath.
#[test]
fn the_start_button_starts_a_match_and_is_not_a_shell_link() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let previous = std::env::var("RA2_HAZARDS").ok();
    let mut shell = Shell::boot(&game, SHELL_SIZE).unwrap();
    shell.click_named("exhibition button").unwrap();
    shell.click_named("arena button").unwrap();
    let arenas = ra2ui::arenas::all(&game).unwrap();
    let row = arenas.iter().position(|a| a.script == "box.py").unwrap();
    shell
        .click_list_row(ra2ui::arenas::LIST, row)
        .expect("the arena row");
    shell.click_named("gametype button").unwrap();

    let before = shell.screen_id();
    let step = shell.click_named(play_mode::START_BUTTON).unwrap();
    assert_eq!(step.hit_widget(), Some(play_mode::START_BUTTON));
    assert_eq!(step.screen, before, "Start Game is not a screen link");
    assert_eq!(step.note, None, "the note is not a navigation");
    assert_eq!(shell.screen_id(), "exhibition/exhib_setup.gib");

    let (session, choice) = play_mode::session_from_shell(&game, &shell, Some(1.0), None).unwrap();
    assert_eq!(choice.arena_script, "box.py");
    assert_eq!(choice.game_type, GameType::Deathmatch);
    assert_eq!(session.matched.state(), MatchState::Countdown);
    assert_eq!(session.matched.config.robots.len(), 2);
    assert!(session.matched.config.robots[0]
        .bot_path
        .ends_with("Teams/Team0/Bot0.bot"));
    match previous {
        Some(value) => std::env::set_var("RA2_HAZARDS", value),
        None => std::env::remove_var("RA2_HAZARDS"),
    }
}
