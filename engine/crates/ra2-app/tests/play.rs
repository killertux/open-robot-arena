//! The exhibition flow's evidence, in its own test process.
//!
//! `ra2script` has process-global match state, so a full match run belongs in
//! an integration test rather than beside the unit tests that also drive
//! matches. These two tests share one mutex and run in this binary only.

use std::path::PathBuf;

use ra2_app::play_mode::{self, Options};
use ra2_app::SHELL_SIZE;
use ra2_shell::Shell;
use ra2fmt::gib::Block;
use ra2sim::r#match::MatchState;
use ra2sim::GameType;

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
    // The setup opens on DEATHMATCH, so the list is that type's arenas.
    let arenas = ra2ui::arenas::available(&game, "DEATHMATCH").unwrap();
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

    let (mut session, choice) =
        play_mode::session_from_shell(&game, &shell, Some(1.0), None).unwrap();
    assert_eq!(choice.arena_script, "box.py");
    assert_eq!(choice.game_type, GameType::Deathmatch);
    // `ArenaIntros=1` starts the arena's intro; skipping it is the countdown.
    session.skip_intro();
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

/// A four-bot BATTLE ROYAL runs through the shipped setup and writes a results
/// frame whose four cells each draw the `.bot` file's own snapshot.
///
/// The pixel check samples a tenth into each preview frame and compares it with
/// the same texel of the decoded picture, so a placeholder, a blank fill or a
/// mixed-up cell fails.
#[test]
fn a_battle_royal_writes_four_result_slots_with_the_bots_pictures() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let out = std::env::temp_dir().join(format!("ra2-play-br-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let options = Options {
        game: game.clone(),
        drive: "0-1:1,0".into(),
        seconds: 2.0,
        out: out.clone(),
        size: [1280, 1005],
        arena_script: Some("box.py".into()),
        arena_index: None,
        game_type: Some(GameType::BattleRoyal),
        team: None,
        bot: None,
        opponent_team: None,
        opponent_bot: None,
        time: None,
    };
    let outcome = play_mode::run_headless(&options).expect("the battle royal run completes");
    assert_eq!(outcome.report.robots.len(), 4);
    assert_eq!(outcome.report.game_type, "BATTLE ROYAL");
    assert_eq!(
        outcome
            .report
            .robots
            .iter()
            .map(|robot| robot.team)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );

    let results = outcome.report.results();
    assert_eq!(results.len(), 4);
    let mut keys = std::collections::HashSet::new();
    for row in &results {
        assert!(row.bot_preview_image.is_some());
        keys.insert(row.bot_preview.clone());
    }
    assert_eq!(keys.len(), 4, "four different preview textures");

    // The shipped results screen, filled from the same rows, gives the frames
    // the cells draw at.
    let ui = game.join("UI");
    let mut screen = ra2ui::workshop::load(&ui, "common/matchresults.gib").unwrap();
    let template = ra2ui::cells::template(&ui, ra2ui::results::TEMPLATE).unwrap();
    let root = screen
        .blocks
        .iter_mut()
        .find_map(|b| match b {
            Block::Widget(w) => Some(w),
            _ => None,
        })
        .unwrap();
    assert_eq!(ra2ui::results::fill(root, &template, &results), 4);
    let placed = ra2ui::resolve(&screen, ra2ui::DESIGN_SCREEN);
    let pixels = image::open(out.join("play-results.png"))
        .expect("play-results.png")
        .into_rgba8();
    assert_eq!(pixels.dimensions(), (1280, 1005));

    let mut checked = 0;
    for (index, row) in results.iter().enumerate() {
        let slot = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == ra2ui::results::SLOTS[index])
            .unwrap();
        let preview = slot
            .walk()
            .find(|p| p.widget.name() == ra2ui::results::BOT_PREVIEW)
            .expect("the result cell's preview");
        let picture = row.bot_preview_image.as_ref().unwrap();
        let (sx, sy) = (picture.width / 10, picture.height / 10);
        let source = ((sy * picture.width + sx) * 4) as usize;
        let expected = [
            picture.rgba[source],
            picture.rgba[source + 1],
            picture.rgba[source + 2],
        ];
        let x = (preview.frame.x + preview.frame.w * 0.1).floor() as u32;
        let y = (preview.frame.y + preview.frame.h * 0.1).floor() as u32;
        let actual = pixels.get_pixel(x, y);
        for channel in 0..3 {
            assert!(
                (i32::from(actual[channel]) - i32::from(expected[channel])).abs() <= 2,
                "result slot {} at ({x},{y}) is {:?}, not {:?} from {}",
                index + 1,
                actual,
                expected,
                row.bot_name
            );
        }
        assert!(
            !(actual[0] == 255 && actual[1] == 255 && actual[2] == 255),
            "slot {} is not the white fill",
            index + 1
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
    std::fs::remove_dir_all(&out).ok();
}

/// A 2v2 TEAM MATCH runs the four slots as the screen's pairs and writes its
/// results frame at the design size.
#[test]
fn a_team_match_runs_two_versus_two_and_writes_its_results() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let out = std::env::temp_dir().join(format!("ra2-play-team-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let options = Options {
        game,
        drive: "0-1:1,0".into(),
        seconds: 2.0,
        out: out.clone(),
        size: [1280, 1005],
        arena_script: Some("box.py".into()),
        arena_index: None,
        game_type: Some(GameType::TeamMatch),
        team: None,
        bot: None,
        opponent_team: None,
        opponent_bot: None,
        time: None,
    };
    let outcome = play_mode::run_headless(&options).expect("the team match run completes");
    assert_eq!(outcome.report.robots.len(), 4);
    assert_eq!(outcome.report.game_type, "TEAM MATCH");
    assert_eq!(
        outcome
            .report
            .robots
            .iter()
            .map(|robot| robot.team)
            .collect::<Vec<_>>(),
        vec![0, 0, 1, 1],
        "slots 1+2 against slots 3+4"
    );
    let rows = outcome.report.results();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].team_name, rows[1].team_name);
    assert_eq!(rows[2].team_name, rows[3].team_name);
    assert_ne!(rows[0].team_name, rows[2].team_name);
    let frame = image::open(out.join("play-results.png"))
        .expect("play-results.png")
        .into_rgba8();
    assert_eq!(frame.dimensions(), (1280, 1005));
    std::fs::remove_dir_all(&out).ok();
}
