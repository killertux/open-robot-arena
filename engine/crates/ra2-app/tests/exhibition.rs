//! The exhibition setup's real choices: the shipped robot pickers, the arena
//! filter, the list scroller and the match-time value.
//!
//! These tests drive the same click flow `--play` does. The full match and its
//! frames stay in `play.rs`; one test here builds a `MatchConfig` to prove the
//! chosen records and `.bot` slots reach the sim.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use ra2_app::play_mode::{self, Options};
use ra2_shell::Pick;
use ra2fmt::gib::Block;
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

fn options(game: &Path) -> Options {
    Options {
        game: game.to_path_buf(),
        drive: String::new(),
        seconds: 1.0,
        out: std::env::temp_dir().join("ra2-exhibition-unused"),
        size: [320, 251],
        arena_script: None,
        arena_index: None,
        game_type: None,
        team: None,
        bot: None,
        opponent_team: None,
        opponent_bot: None,
        time: None,
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A game directory whose player table is a copy this test can fill, with the
/// shipped team 0 roster reachable as team 5. Everything else is a symlink to
/// the real install, so no game file is copied or written.
struct TestGame(PathBuf);

impl TestGame {
    fn new(tag: &str, real: &Path) -> Option<TestGame> {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("ra2-exhibition-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for entry in std::fs::read_dir(real).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let from = entry.path();
            let to = root.join(&name);
            if name == "Teams" {
                std::fs::create_dir_all(&to).unwrap();
                for child in std::fs::read_dir(&from).unwrap() {
                    let child = child.unwrap();
                    let child_to = to.join(child.file_name());
                    if child.file_name() == "Teams.txt" {
                        std::fs::copy(child.path(), child_to).unwrap();
                    } else {
                        symlink(&child.path(), &child_to);
                    }
                }
                // The shipped only-populated player team, reachable as team 5.
                symlink(&from.join("Team0"), &to.join("Team5"));
                let table_path = to.join("Teams.txt");
                let mut table = ra2fmt::team::read_table(&table_path).unwrap();
                table.teams[5].name = "Test Five".to_string();
                table.teams[5].logo = table.teams[0].logo.clone();
                table.teams[5].robots = table.teams[0].robots.clone();
                table.write_to(&table_path).unwrap();
            } else {
                symlink(&from, &to);
            }
        }
        Some(TestGame(root))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestGame {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn symlink(from: &Path, to: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(from, to).unwrap();
    #[cfg(not(unix))]
    {
        if from.is_dir() {
            std::os::windows::fs::symlink_dir(from, to).unwrap();
        } else {
            std::os::windows::fs::symlink_file(from, to).unwrap();
        }
    }
}

/// The player picker's team 5 robot 1 and the computer picker's team 3 robot 0
/// reach the match: the spawned robots are those `.bot` files, not the defaults.
#[test]
fn choosing_team_five_bot_one_and_opponent_team_three_bot_zero_changes_the_robots() {
    let Some(real) = game_dir() else { return };
    let _guard = ra2script::lock();
    let Some(fixture) = TestGame::new("teams", &real) else {
        return;
    };
    let game = fixture.path();
    let mut options = options(game);
    options.team = Some(5);
    options.bot = Some(1);
    options.opponent_team = Some(3);
    options.opponent_bot = Some(0);

    let (shell, steps) = play_mode::select_setup(&options).expect("the click flow");
    assert!(steps.len() > 12, "the pickers' own clicks: {steps:?}");
    assert_eq!(shell.player_pick(), Some(Pick { team: 5, bot: 1 }));
    assert_eq!(shell.opponent_pick(), Some(Pick { team: 3, bot: 0 }));

    let (session, choice) =
        play_mode::session_from_shell(game, &shell, Some(1.0), None).expect("the match");
    let config = &session.matched.config;
    assert!(
        config.robots[0].bot_path.ends_with("Teams/Team5/Bot1.bot"),
        "{}",
        config.robots[0].bot_path
    );
    assert!(
        config.robots[1].bot_path.ends_with("AI/Team3/Bot0.bot"),
        "{}",
        config.robots[1].bot_path
    );
    let report = session.report();
    assert_eq!(report.robots[0].bot_name, choice.player.bot_name);
    assert_eq!(report.robots[1].bot_name, choice.opponent.bot_name);
    // Both differ from the debug defaults this client would otherwise spawn.
    assert_ne!(report.robots[0].bot_name, "C.R.U.S.H.");
    assert_ne!(report.robots[1].bot_name, "Roly Poly");
}

/// `KING OF THE HILL` filters the arena list to the two arenas whose
/// `game_types` name it, and the picker's fallback is `hilltop`.
#[test]
fn king_of_the_hill_filters_the_picker_and_picks_hilltop() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let available = ra2ui::arenas::available(&game, "KING OF THE HILL").unwrap();
    assert_eq!(
        available
            .iter()
            .map(|arena| arena.script.as_str())
            .collect::<Vec<_>>(),
        vec!["hilltop.py", "king.py"],
        "the original's enumAvailable(KING OF THE HILL)"
    );
    assert!(!available.iter().any(|arena| arena.script == "box.py"));

    let mut options = options(&game);
    options.game_type = Some(GameType::KingOfTheHill);
    let (shell, _) = play_mode::select_setup(&options).expect("the click flow");
    assert_eq!(shell.game_type_choice(), Some("KING OF THE HILL"));
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.game_type, GameType::KingOfTheHill);
    assert_eq!(choice.arena_script, "hilltop.py");
    assert_eq!(choice.arena_name, "Hill Top Arena");
    assert!(
        choice
            .arena_note
            .as_deref()
            .is_some_and(|note| note.contains("supports KING OF THE HILL")),
        "{:?}",
        choice.arena_note
    );
    assert_eq!(choice.arena_row, Some((0, 2)));
}

/// A box-arena row selected while the type was still `DEATHMATCH` is rejected
/// when Game Settings then selects `KING OF THE HILL`; the run says so and uses
/// `hilltop`.
#[test]
fn a_game_type_that_the_selected_arena_does_not_support_is_replaced_and_named() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.arena_script = Some("box.py".to_string());
    let (mut shell, _) = play_mode::select_setup(&options).expect("the click flow");
    assert_eq!(shell.arena_choice(), Some("box.py"));
    shell.click_named("gametype button").expect("Game Settings");
    shell
        .click_named("kingofhill checkbox")
        .expect("the option group's own member");
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.game_type, GameType::KingOfTheHill);
    assert_eq!(choice.arena_script, "hilltop.py");
    let note = choice.arena_note.expect("a named replacement");
    assert!(note.contains("Combat Arena"), "{note}");
    assert!(note.contains("does not support KING OF THE HILL"), "{note}");
}

/// The list's own `+Scroller` and the wheel are both real: the bar is shown
/// when the twelve rows do not fit, its `More` arrow moves a row, and the wheel
/// over the list does the same.
#[test]
fn the_arena_lists_scroller_and_wheel_move_the_viewport() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut shell = ra2_shell::Shell::boot(&game, ra2_app::SHELL_SIZE).unwrap();
    shell.click_named("exhibition button").unwrap();
    shell.click_named("arena button").unwrap();
    assert_eq!(shell.arena_scroll(), Some(0));
    let (bar, list) = {
        let placed = shell.placements();
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == ra2ui::arenas::LIST)
            .expect("the arena list");
        let bar = list
            .attachments
            .iter()
            .find(|a| a.attachment.kind == ra2fmt::gib::AttachmentKind::Scroller)
            .and_then(|a| {
                a.body
                    .walk()
                    .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::ScrollBar)
            })
            .expect("the list's scroll bar");
        assert!(bar.widget.visible(), "shown because 12 rows > 6");
        (bar.frame, list.frame)
    };
    // The `More` arrow is the bar's bottom 16 pixels.
    let more = [bar.x + bar.w / 2.0, bar.bottom() - 4.0];
    shell.click(more[0], more[1]).unwrap();
    assert_eq!(shell.arena_scroll(), Some(1));
    // The wheel over the list body.
    let wheel = [list.x + list.w / 2.0, list.y + 20.0];
    assert!(shell.scroll_at(wheel[0], wheel[1], 2));
    assert_eq!(shell.arena_scroll(), Some(3));
}

/// Row 10 is past the six rows the picker shows; the list scrolls and the click
/// lands on `Parking Lot`.
#[test]
fn arena_index_ten_scrolls_the_picker_and_selects_parking_lot() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.arena_index = Some(10);
    let (shell, _) = play_mode::select_setup(&options).expect("the click flow");
    assert_eq!(
        shell.arena_row(),
        Some((10, 12)),
        "the tenth row of the twelve"
    );
    assert_eq!(
        shell.arena_scroll(),
        Some(5),
        "six visible rows means row 10 needs a five-row offset"
    );
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.arena_name, "Parking Lot");
    assert_eq!(choice.arena_script, "parkinglot.py");
    assert_eq!(choice.arena_row, Some((10, 12)));
}

/// The time dropdown's displayed value is data (`5 minutes`); its option list
/// is not. The screen's value is the match length, and `--seconds`/`--time`
/// override it explicitly.
#[test]
fn the_time_dropdowns_displayed_value_is_the_match_length() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.arena_script = Some("box.py".to_string());
    let (mut shell, _) = play_mode::select_setup(&options).expect("the click flow");
    shell.click_named("gametype button").expect("Game Settings");
    assert_eq!(shell.time_text(), Some("5 minutes"));
    assert_eq!(shell.time_choice(), Some(Some(300.0)));
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.time_text.as_deref(), Some("5 minutes"));
    assert_eq!(choice.time_seconds, Some(300.0));

    let (session, _) = play_mode::session_from_shell(&game, &shell, None, None).unwrap();
    assert_eq!(session.matched.config.duration, Some(300.0));
    let (session, _) = play_mode::session_from_shell(&game, &shell, Some(2.0), None).unwrap();
    assert_eq!(
        session.matched.config.duration,
        Some(2.0),
        "--seconds is the headless override"
    );
    let (session, _) = play_mode::session_from_shell(&game, &shell, None, Some(12.0)).unwrap();
    assert_eq!(
        session.matched.config.duration,
        Some(12.0),
        "--time replaces the dropdown's value"
    );
}

/// The four `playerN group` slots all open the shipped pickers, and a BATTLE
/// ROYAL carries all four robots on four distinct spawn points. The results
/// screen lists the same four names in its four slots, and slot 1 is the driven
/// robot.
#[test]
fn a_battle_royal_carries_four_slots_four_spawns_and_four_result_rows() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.game_type = Some(GameType::BattleRoyal);
    let (shell, _) = play_mode::select_setup(&options).expect("the four pickers");
    for slot in 0..4 {
        assert!(
            shell.slot_pick(slot).is_some(),
            "slot {} kept its pick",
            slot + 1
        );
    }

    let (session, choice) =
        play_mode::session_from_shell(&game, &shell, Some(1.0), None).expect("a battle royal");
    assert_eq!(choice.slots.len(), 4);
    assert_eq!(session.player_robot, 0, "slot 1 is the driven robot");
    let config = &session.matched.config;
    assert_eq!(config.game_type, GameType::BattleRoyal);
    assert_eq!(config.bot_count, 4);
    assert_eq!(config.robots.len(), 4);
    assert_eq!(
        config.robots.iter().map(|r| r.team).collect::<Vec<_>>(),
        vec![0, 1, 2, 3],
        "a free-for-all gives each slot its own team id"
    );
    let paths: std::collections::HashSet<_> =
        config.robots.iter().map(|r| r.bot_path.clone()).collect();
    assert_eq!(paths.len(), 4, "four different .bot files");
    assert_eq!(session.matched.duel.robots.len(), 4);

    let mut spawns: Vec<[f32; 3]> = Vec::new();
    for robot in &session.matched.duel.robots {
        let position = robot.position(&*session.matched.duel.physics);
        let position = [position.x, position.y, position.z];
        assert!(
            spawns.iter().all(|other: &[f32; 3]| {
                let dx = other[0] - position[0];
                let dy = other[1] - position[1];
                let dz = other[2] - position[2];
                (dx * dx + dy * dy + dz * dz).sqrt() >= 1.0
            }),
            "two robots share a start point: {position:?}"
        );
        spawns.push(position);
    }

    let report = session.report();
    let rows = report.results();
    assert_eq!(rows.len(), 4);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.bot_name, report.robots[index].bot_name);
        assert!(
            row.bot_preview_image.is_some(),
            "slot {} carries the .bot's own picture",
            index + 1
        );
    }

    // The shipped results screen fills all four slots with those names.
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
    assert_eq!(ra2ui::results::fill(root, &template, &rows), 4);
    let placed = ra2ui::resolve(&screen, ra2ui::DESIGN_SCREEN);
    for (index, row) in rows.iter().enumerate() {
        let slot = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == ra2ui::results::SLOTS[index])
            .unwrap();
        let name = slot
            .walk()
            .find(|p| p.widget.name() == ra2ui::results::BOT_NAME)
            .expect("the result cell's bot name");
        let text = name
            .widget
            .children
            .iter()
            .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
            .and_then(|c| c.field("_Text"))
            .map(|f| f.text())
            .unwrap_or("");
        assert_eq!(text, row.bot_name, "result slot {}", index + 1);
    }
}

/// A TEAM MATCH is 2v2 with the screen's own pairs: slots 1 and 2 (team 1
/// group) are team 0, slots 3 and 4 (team 2 group) team 1. Slot 1 stays driven,
/// and a teammate's hit scores nothing.
#[test]
fn a_two_v_two_team_match_uses_the_screens_side_pairs_and_ignores_teammate_hits() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.game_type = Some(GameType::TeamMatch);
    let (shell, _) = play_mode::select_setup(&options).expect("the four pickers");
    let (mut session, choice) =
        play_mode::session_from_shell(&game, &shell, Some(2.0), None).expect("a team match");
    assert_eq!(session.player_robot, 0, "slot 1 is the driven robot");

    let config = &session.matched.config;
    assert_eq!(config.game_type, GameType::TeamMatch);
    assert_eq!(config.robots.len(), 4);
    assert_eq!(
        config.robots.iter().map(|r| r.team).collect::<Vec<_>>(),
        vec![0, 0, 1, 1],
        "slots 1+2 vs slots 3+4"
    );
    assert_eq!(
        session
            .matched
            .duel
            .robots
            .iter()
            .map(|r| r.team)
            .collect::<Vec<_>>(),
        vec![0, 0, 1, 1],
        "the spawned robots carry the screen's pairs"
    );
    assert_ne!(
        choice.slots[0].path, choice.slots[1].path,
        "the two player-side slots are different robots"
    );
    assert_ne!(
        choice.slots[2].path, choice.slots[3].path,
        "the two computer-side slots are different robots"
    );

    session.skip_intro();
    while session.matched.state() == MatchState::Countdown {
        session.matched.step();
    }
    assert_eq!(session.matched.state(), MatchState::Fight);

    let before = ra2script::points(0);
    session.matched.hit(0, 1, 0, 40.0);
    assert_eq!(
        ra2script::points(0),
        before,
        "hitting the team's own second robot adds nothing"
    );
    session.matched.hit(0, 2, 0, 40.0);
    assert_eq!(
        ra2script::points(0),
        before + 40,
        "hitting the other team scores its damage"
    );
}
