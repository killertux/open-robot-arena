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
        ra2fmt::saves::bind(&root, &root);
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
    // The tab opens with its first row selected, so the list's own choice
    // stands and no fallback note is needed.
    assert_eq!(choice.arena_note, None);
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

/// The list's own `+Scroller` and the wheel are both real, but the list is 15 rows
/// tall (282 px / 18, the capture), so the eight DEATHMATCH arenas fit: the bar
/// is hidden and neither its arrow nor the wheel moves the viewport. (Scrolling a
/// list that overflows is `ra2ui::input`'s own test, with the file's `Rows 6`.)
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
        assert!(
            !bar.widget.visible(),
            "hidden: eight rows fit the 15-row list"
        );
        (bar.frame, list.frame)
    };
    let more = [bar.x + bar.w / 2.0, bar.bottom() - 4.0];
    shell.click(more[0], more[1]).unwrap();
    assert_eq!(shell.arena_scroll(), Some(0));
    let wheel = [list.x + list.w / 2.0, list.y + 20.0];
    shell.scroll_at(wheel[0], wheel[1], 2);
    assert_eq!(shell.arena_scroll(), Some(0), "nothing to scroll");
}

/// Row 6 is inside the 15 rows the picker shows, so the list does not scroll and
/// the click lands on `Parking Lot`.
#[test]
fn arena_index_six_scrolls_the_picker_and_selects_parking_lot() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.arena_index = Some(6);
    let (shell, _) = play_mode::select_setup(&options).expect("the click flow");
    assert_eq!(
        shell.arena_row(),
        Some((6, 8)),
        "the seventh row of the eight DEATHMATCH arenas"
    );
    assert_eq!(
        shell.arena_scroll(),
        Some(0),
        "row 6 is in view: the list shows 15 rows"
    );
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.arena_name, "Parking Lot");
    assert_eq!(choice.arena_script, "parkinglot.py");
    assert_eq!(choice.arena_row, Some((6, 8)));
}

/// The time dropdown's displayed value is data (`3`, the constructor's default; the
/// option list is `No limit`, 1-5, 10, 15, 20, `FUN_004733f0`). The screen's value is the match length, and `--seconds`/`--time`
/// override it explicitly.
#[test]
fn the_time_dropdowns_displayed_value_is_the_match_length() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.arena_script = Some("box.py".to_string());
    let (mut shell, _) = play_mode::select_setup(&options).expect("the click flow");
    shell.click_named("gametype button").expect("Game Settings");
    assert_eq!(shell.time_text(), Some("3"));
    assert_eq!(shell.time_choice(), Some(Some(180.0)));
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.time_text.as_deref(), Some("3"));
    assert_eq!(choice.time_seconds, Some(180.0));

    let (session, _) = play_mode::session_from_shell(&game, &shell, None, None).unwrap();
    assert_eq!(session.matched.config.duration, Some(180.0));
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

/// The time dropdown opens its list (`No limit`, 1-5, 10, 15, 20: the
/// constructor `FUN_004733f0`), a row picks the match length, a click elsewhere
/// closes the list without changing it, and the choice survives a trip to the
/// arena tab and back.
#[test]
fn the_time_dropdown_opens_its_list_and_a_row_sets_the_match_length() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let mut options = options(&game);
    options.arena_script = Some("box.py".to_string());
    let (mut shell, _) = play_mode::select_setup(&options).expect("the click flow");
    shell.click_named("gametype button").expect("Game Settings");
    let frame_of = |shell: &ra2_shell::Shell, name: &str| {
        let placed = shell.placements();
        placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name)
            .map(|p| p.frame)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let rows_of = |shell: &ra2_shell::Shell| -> Vec<String> {
        let placed = shell.placements();
        let dropdown = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "time dropdown")
            .expect("the dropdown");
        let list = dropdown
            .walk()
            .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::ListBox)
            .expect("its list");
        list.widget
            .children
            .iter()
            .filter(|c| c.kind == ra2fmt::gib::WidgetKind::Text && c.visible())
            .filter_map(|c| c.field("_Text").map(|f| f.text().to_string()))
            .collect()
    };
    assert!(rows_of(&shell).is_empty(), "closed: no rows");
    let field = frame_of(&shell, "time dropdown");
    // The arrow opens the list; the rows are the constructor's, five shown and the
    // sixth cut off at the clip (live `exhibition-time-open`).
    shell.click(field.x + field.w - 8.0, field.y + 9.0).unwrap();
    assert_eq!(rows_of(&shell), ["No limit", "1", "2", "3", "4", "5"]);
    // A click away closes it and changes nothing.
    shell
        .click(field.x + field.w + 200.0, field.y + 9.0)
        .unwrap();
    assert!(rows_of(&shell).is_empty());
    assert_eq!(shell.time_choice(), Some(Some(180.0)));
    // Open again and pick `No limit`, then `4`.
    for (row, expected) in [(0usize, None), (4, Some(240.0))] {
        shell.click(field.x + field.w - 8.0, field.y + 9.0).unwrap();
        let rowh = 18.0;
        shell
            .click(field.x + 20.0, field.y + field.h + rowh * row as f32 + 9.0)
            .unwrap();
        assert!(rows_of(&shell).is_empty(), "a pick closes the list");
        assert_eq!(shell.time_choice(), Some(expected));
    }
    assert_eq!(shell.time_text(), Some("4"));
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert_eq!(choice.time_seconds, Some(240.0));
    // The arena tab replaces the tab; the choice is still the player's.
    shell.click_named("arena button").expect("Arena tab");
    shell.click_named("gametype button").expect("Game Settings");
    assert_eq!(shell.time_text(), Some("4"));
}

/// The weight-class filter (`FUN_004733f0`'s `weight class optiongroup`):
/// choosing a class empties every slot whose robot is of another class
/// (`FUN_0047b320`) and the pickers then dim and refuse the robots outside it
/// (`FUN_004ea040`: alpha 0.3, not selectable). `All Classes` allows everything.
#[test]
fn the_weight_class_filter_empties_slots_and_dims_the_pickers_robots() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let options = options(&game);
    let (mut shell, _) = play_mode::select_setup(&options).expect("the click flow");
    let class_of = |shell: &ra2_shell::Shell, slot: usize| -> Option<i32> {
        let pick = shell.slot_pick(slot)?;
        let side = if slot < 2 {
            ra2ui::teams::Side::Player
        } else {
            ra2ui::teams::Side::Computer
        };
        let path = ra2ui::teams::bot_path(&game, side, pick.team, pick.bot);
        ra2ui::teams::bot_info(&path).map(|(_, class)| class)
    };
    let (own, other) = (class_of(&shell, 0), class_of(&shell, 2));
    assert!(own.is_some() && other.is_some(), "both slots hold a robot");
    // Choose the class of the player's robot.
    let (name, class) = match own.unwrap() {
        0 => ("lightweight cb", 0),
        1 => ("mediumweight cb", 1),
        _ => ("heavyweight cb", 2),
    };
    shell.click_named("gametype button").expect("Game Settings");
    shell.click_named(name).expect("the class");
    assert_eq!(shell.weight_choice(), Some(class as u8));
    assert!(shell.slot_pick(0).is_some(), "a robot of the class stays");
    assert_eq!(
        shell.slot_pick(2).is_some(),
        other == Some(class),
        "a robot of another class is ejected"
    );
    // The choice survives a tab trip.
    shell.click_named("arena button").expect("Arena");
    shell.click_named("gametype button").expect("Game Settings");
    assert_eq!(shell.weight_choice(), Some(class as u8));
    // The picker dims what the filter refuses.
    shell.click_named("robots button").expect("Robots");
    shell.click_region("player1 group").expect("the picker");
    let dimmed = |shell: &ra2_shell::Shell| {
        shell
            .placements()
            .iter()
            .flat_map(|p| p.walk())
            .filter(|p| {
                p.widget.kind == ra2fmt::gib::WidgetKind::Image
                    && p.widget.field("_ForeColor").map(|f| f.text()) == Some("1 1 1 0.3")
            })
            .count()
    };
    let allowed_somewhere = dimmed(&shell);
    shell.click_named("cancel button").unwrap();
    shell.click_named("gametype button").unwrap();
    shell.click_named("all classes cb").unwrap();
    assert_eq!(shell.weight_choice(), None);
    shell.click_named("robots button").unwrap();
    shell.click_region("player1 group").expect("the picker");
    let baseline = dimmed(&shell);
    assert!(
        allowed_somewhere > baseline,
        "the filter dims robots ({allowed_somewhere} images against {baseline} without it)"
    );
}

/// A duel leaves slots 2 and 4 `OPEN` (the lobby's `noplayer.gib` word); only
/// BATTLE ROYAL and TEAM MATCH fill all four.
#[test]
fn a_duel_reports_slots_two_and_four_as_open() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let (shell, _) = play_mode::select_setup(&options(&game)).expect("the click flow");
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert!(choice.slot_line(0).starts_with("slot 1: player team"));
    assert!(choice.slot_line(1).starts_with("slot 2: OPEN"));
    assert!(choice.slot_line(2).starts_with("slot 3: computer team"));
    assert!(choice.slot_line(3).starts_with("slot 4: OPEN"));
    let mut royal = options(&game);
    royal.game_type = Some(GameType::BattleRoyal);
    let (shell, _) = play_mode::select_setup(&royal).expect("the four pickers");
    let choice = play_mode::choice_from_shell(&game, &shell).expect("the choice");
    assert!((0..4).all(|slot| !choice.slot_line(slot).contains("OPEN")));
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
