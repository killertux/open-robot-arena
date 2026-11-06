//! The player's routes through the windowed client, driven headlessly
//! (docs/36-playthrough.md). Each test walks the shell the way a mouse does and
//! hands off to the live modes through `ra2_app::flow`, exactly as the window
//! loop does.
//!
//! A career writes the game's own `Teams/` and the workshop writes the robot it
//! edits, so every test that saves runs on a scratch game: the tables and the
//! player's team folder are copies, everything else links to the real install.
//! Scripted matches hold `ra2script::lock()`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use ra2_app::flow;
use ra2_app::hq_mode;
use ra2_shell::Shell;
use ra2fmt::gib::Block;
use ra2sim::r#match::MatchState;
use ra2sim::DriveInput;

/// The windowed client's shell size on the fullscreen 1280x1024 config: the
/// shell lays out natively at the window, not at the 1280x1005 design size.
const WINDOW: [f32; 2] = [1280.0, 1024.0];

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        let dir = PathBuf::from(dir);
        return dir.is_dir().then_some(dir);
    }
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

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A scratch install: `Teams/` (tables, logos, the player's robots) and
/// `Robot Designs/` copied, `AI/Teams.txt` copied, everything else linked.
struct ScratchGame(PathBuf);

impl ScratchGame {
    fn new(tag: &str) -> Option<ScratchGame> {
        let real = game_dir()?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("ra2-playthrough-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for entry in std::fs::read_dir(&real).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let from = entry.path();
            let to = root.join(&name);
            match name.to_str() {
                Some("Teams") | Some("Robot Designs") => copy_dir(&from, &to),
                Some("AI") => {
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
                }
                _ => symlink(&from, &to),
            }
        }
        ra2fmt::saves::bind(&root, &root);
        Some(ScratchGame(root))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchGame {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) {
    std::os::unix::fs::symlink(from, to).unwrap();
}

#[cfg(not(unix))]
fn symlink(from: &Path, to: &Path) {
    if from.is_dir() {
        copy_dir(from, to);
    } else {
        std::fs::copy(from, to).unwrap();
    }
}

/// The shell as the window boots it: native layout, the career's HQ data and
/// the event hook.
fn window_shell(game: &Path) -> Shell {
    let mut shell = Shell::boot(game, WINDOW).unwrap();
    shell.set_hq_view(hq_mode::shell_view(game).unwrap());
    shell.set_enter_hook(hq_mode::shell_enter_hook(game).unwrap());
    shell.set_team_hook(hq_mode::shell_team_hook(game));
    shell.set_scrap_hook(hq_mode::shell_scrap_hook(game));
    shell.set_export_hook(hq_mode::shell_export_hook(game));
    shell.set_import_hook(hq_mode::shell_import_hook(game));
    shell.set_later_hook(hq_mode::shell_later_hook(game));
    shell
}

/// The frame of a placed widget on the shell's current page.
fn frame_of(shell: &Shell, name: &str) -> ra2ui::Frame {
    let placed = ra2ui::resolve(shell.document(), shell.size());
    placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == name)
        .map(|p| p.frame)
        .unwrap_or_else(|| panic!("{name} is not on {}", shell.screen_id()))
}

/// At the window's 1280x1024 the schedule's cells are its rectangle divided
/// evenly (as drawn), so the third cell is the third event; a click on the
/// robot inventory selects a robot and never replaces the HQ with the
/// `robot_cell.gib` template its note names (the client used to exit on it).
#[test]
fn the_hq_grids_hit_the_cells_they_draw_at_the_window_size() {
    let Some(scratch) = ScratchGame::new("grids") else {
        return;
    };
    let game = scratch.path();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button"] {
        shell.click_named(name).unwrap();
    }
    // Robot inventory: the second roster robot's cell.
    let grid = frame_of(&shell, ra2ui::hq::ROBOT_GRID);
    let point = [grid.x + grid.w * 0.25, grid.y + grid.h * 0.5];
    assert_eq!(flow::inventory_cell_at(&shell, point), Some(1));
    let step = shell.click(point[0], point[1]).unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib", "{step:?}");
    assert_eq!(shell.window_depth(), 0);

    // Event schedule: the third cell of the first row.
    shell.click_named("events button").unwrap();
    let grid = frame_of(&shell, ra2ui::hq::EVENT_GRID);
    let view = hq_mode::shell_view(game).unwrap();
    shell
        .click(grid.x + grid.w * (2.5 / 3.0), grid.y + grid.h * (0.5 / 3.0))
        .unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");
    assert_eq!(shell.window_depth(), 1, "the join window is open");
    // The join window names the third event.
    let named = window_text(&shell, "event name");
    assert_eq!(named.as_deref(), Some(view.events[2].event.name.as_str()));
}

/// The join window's `event name` text, read through the shell's placements.
fn window_text(shell: &Shell, name: &str) -> Option<String> {
    let mut found = None;
    for placement in shell.placements() {
        for p in placement.walk() {
            if p.widget.name() == name {
                found = ra2ui::input::field_of(p.widget, "_Text")
                    .map(str::to_string)
                    .or_else(|| {
                        p.widget
                            .children
                            .iter()
                            .find_map(|c| ra2ui::input::field_of(c, "_Text").map(str::to_string))
                    });
            }
        }
    }
    found
}

/// Team HQ career, end to end through the shell: join an event with a chosen
/// robot, `Play Next Round!` starts the event's match with the player driving
/// that robot (keys reach it), the round is awarded and saved, and the flow
/// hands the player back to the shell's HQ with the season moved on.
#[test]
fn an_event_entered_in_the_shell_is_played_by_the_player_and_saved() {
    let Some(scratch) = ScratchGame::new("event") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let previous = std::env::var("RA2_ARENA_INTROS").ok();
    std::env::set_var("RA2_ARENA_INTROS", "0");
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button", "events button"] {
        shell.click_named(name).unwrap();
    }
    let grid = frame_of(&shell, ra2ui::hq::EVENT_GRID);
    shell
        .click(grid.x + grid.w / 6.0, grid.y + grid.h / 6.0)
        .unwrap();
    assert_eq!(shell.window_depth(), 1);
    // The join window's robot grid: the second robot (Sting Ray).
    let placed = shell.placements();
    let robots = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == "robot grid")
        .map(|p| p.frame)
        .expect("the join window's robot grid");
    shell
        .click(robots.x + robots.w * 1.5 / 6.0, robots.y + robots.h / 2.0)
        .unwrap();
    shell.click_named("enter event button").unwrap();
    assert_eq!(shell.base_screen_id(), flow::BRACKET_SCREEN);

    // The repairs bench that follows a round is drawn over this screen and its bottom bar is the
    // shell's: the app routes the bar's Main Menu to it (live `event-repairs-damaged`).
    let main_menu = shell
        .placements()
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == "MainMenu button")
        .map(|p| p.frame)
        .expect("the bottom bar's Main Menu button");
    let at = [
        main_menu.x + main_menu.w / 2.0,
        main_menu.y + main_menu.h / 2.0,
    ];
    assert_eq!(
        flow::nav_control_at(&shell, at).as_deref(),
        Some("MainMenu button")
    );
    assert_eq!(flow::nav_control_at(&shell, [640.0, 400.0]), None);

    // Entering writes nothing; the shell remembers the event and the robot
    // column for the round's start.
    let career = ra2sim::career::Career::load(game).unwrap();
    assert!(matches!(
        career.phase,
        ra2sim::career::SeasonPhase::Schedule
    ));
    let view = hq_mode::shell_view(game).unwrap();
    assert_eq!(shell.entered_event(), Some((view.events[0].id, 1)));
    let entered = career.player().robots[1];

    // `Play Next Round!`.
    let step = shell.click_named("play button").unwrap();
    assert_eq!(step.hit_widget(), Some("play button"));
    let mut hq = flow::start_event_match(game, shell.entered_event(), 4.0).unwrap();
    {
        let session = hq.match_session_mut().expect("the event match");
        assert!(
            session.player_controlled,
            "the player drives an event match"
        );
        assert!(session.matched.config.robots[0]
            .bot_path
            .ends_with(&format!("Bot{entered}.bot")));
        session.skip_intro();
        // Keys held reach the robot's own wiring.
        let mut input = DriveInput::default();
        input.keys.set(ra2sim::controls::dik::W, true);
        session.set_drive(input);
        let mut guard = 0;
        while session.matched.state() != MatchState::Results && guard < 400 {
            session.advance(0.05);
            guard += 1;
        }
        assert_eq!(session.matched.state(), MatchState::Results);
        let report = session.report();
        assert_eq!(report.robots[0].team_name, career.team_name());
    }
    hq.finish_match().unwrap();
    // Award (a loss ends the event) or the next round's bracket: either way the
    // round is in the saved career.
    let saved = ra2sim::career::Career::load(game).unwrap();
    assert!(
        saved.history.len() == 1 || saved.bracket().is_some_and(|b| b.round > 0),
        "the round was recorded"
    );
    // A win: the repair bench if the robot was hurt, then the bracket's
    // `Finish Later`; a loss: the award's `continue button`.
    let mut guard = 0;
    while !hq.on_hq_screen() && guard < 8 {
        let point = if hq.on_event_screen() {
            later_point(&hq)
        } else {
            continue_point(&mut hq)
        };
        hq.click(point).unwrap();
        guard += 1;
    }
    assert!(hq.on_hq_screen());
    flow::return_to_shell_hq(&mut shell).unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");
    match previous {
        Some(value) => std::env::set_var("RA2_ARENA_INTROS", value),
        None => std::env::remove_var("RA2_ARENA_INTROS"),
    }
}

/// A point on the award/repairs screen's own continue/finished control.
fn continue_point(hq: &mut hq_mode::Session) -> [f32; 2] {
    hq.control_point(&["continue button", "finished button"])
        .expect("a continue control")
}

fn later_point(hq: &hq_mode::Session) -> [f32; 2] {
    hq.control_point(&["later button"])
        .expect("the later button")
}

/// Bot lab from the HQ: the workshop opens on the robot the player selected,
/// an attached component is written back to that team slot on `Back to HQ`,
/// and the edited robot is the one an exhibition match spawns.
#[test]
fn a_robot_edited_in_the_workshop_is_saved_to_its_slot_and_fights() {
    let Some(scratch) = ScratchGame::new("lab") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button"] {
        shell.click_named(name).unwrap();
    }
    let grid = frame_of(&shell, ra2ui::hq::ROBOT_GRID);
    let point = [grid.x + grid.w * 0.25, grid.y + grid.h * 0.5];
    let pick = flow::inventory_cell_at(&shell, point);
    assert_eq!(pick, Some(1));
    // Workshop is dim until a cell is selected (docs/36 9.25).
    shell.click_named("workshop button").unwrap();
    assert!(shell.lab().is_none());
    shell.click(point[0], point[1]).unwrap();
    shell.click_named("workshop button").unwrap();
    assert!(shell.lab().is_some());

    let mut builder = flow::open_workshop(game, pick).unwrap();
    let target = builder.team_target().unwrap().to_path_buf();
    assert!(
        target.ends_with("Teams/Team0/Bot1.bot"),
        "{}",
        target.display()
    );
    let before = builder.build().components().len();
    builder.click_named("components button").unwrap();
    builder.click_named("mechanics button").unwrap();
    builder
        .place_component_free("Components\\ztek1.txt")
        .unwrap();
    assert_eq!(builder.build().components().len(), before + 1);
    builder.click_named("back button").unwrap();
    assert!(builder.shell().lab().is_none(), "Back to HQ leaves the lab");
    assert_eq!(
        flow::lab_exit_control(builder.shell()).as_deref(),
        Some("back button")
    );
    let saved = flow::close_workshop(&mut builder, &mut shell).unwrap();
    assert_eq!(saved.as_deref(), Some(target.as_path()));
    shell.click_named("back button").unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");

    let bot = ra2fmt::bot::parse(&std::fs::read(&target).unwrap()).unwrap();
    assert_eq!(bot.components.len(), before + 1);

    // The exhibition's player picker reads the same slot.
    let options = ra2_app::play_mode::Options {
        game: game.to_path_buf(),
        drive: "0-1:1,0".into(),
        seconds: 1.0,
        out: std::env::temp_dir(),
        size: [1280, 1005],
        arena_script: Some("box.py".into()),
        arena_index: None,
        game_type: None,
        team: Some(0),
        bot: Some(1),
        opponent_team: None,
        opponent_bot: None,
        time: None,
    };
    let (setup, _) = ra2_app::play_mode::select_setup(&options).unwrap();
    let (session, _) =
        ra2_app::play_mode::session_from_shell(game, &setup, Some(1.0), None).unwrap();
    assert!(session.matched.config.robots[0]
        .bot_path
        .ends_with("Teams/Team0/Bot1.bot"));
    assert_eq!(
        session.matched.duel.robots[0].rig.components.len(),
        before + 1
    );
}

/// New Team: a click focuses the shipped editable name box, typing fills it,
/// a label text box takes no typing, and OK makes the team: the first empty
/// record of `Teams/Teams.txt` is written at once (`FUN_004ebd40` ->
/// message 200 -> `FUN_00502720`), with its `Teams/Team<N>/` folder, and the
/// new row is selected. `continue button` opens *that* team's HQ, a fresh
/// boot lists it, and Delete Team's Yes clears the record on disk.
#[test]
fn a_new_team_is_typed_saved_selected_and_deleted() {
    let Some(scratch) = ScratchGame::new("newteam") else {
        return;
    };
    let game = scratch.path();
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    let rows_before = select_rows(&shell);
    shell.click_named("new team button").unwrap();
    assert_eq!(shell.window_depth(), 1);
    shell.click_named("name input textbox").unwrap();
    assert_eq!(shell.text_focus(), Some("name input textbox"));
    for c in "Robo Rats".chars() {
        assert!(shell.type_char(c));
    }
    assert!(shell.backspace());
    assert!(shell.type_char('s'));
    assert_eq!(shell.focused_text().as_deref(), Some("Robo Rats"));
    shell.click_named("window caption").unwrap();
    assert_eq!(shell.text_focus(), None, "a caption is not editable");
    // The logo list is `Teams\*.bmp` (`FUN_00468760`); the third row is the
    // team's logo.
    let mut logos: Vec<String> = std::fs::read_dir(game.join("Teams"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.to_ascii_lowercase().ends_with(".bmp"))
        .collect();
    logos.sort_by_key(|n| n.to_ascii_lowercase());
    shell.click_list_row("logo listbox", 2).unwrap();
    shell.click_named("ok button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    assert_eq!(select_rows(&shell), rows_before + 1);

    // On disk at once: the record and its robot folder.
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    let slot = table
        .teams
        .iter()
        .position(|t| t.name == "Robo Rats")
        .expect("the new team is in Teams/Teams.txt");
    assert!(game.join(format!("Teams/Team{slot}")).is_dir());
    assert_eq!(table.teams[slot].logo, format!("Teams\\{}", logos[2]));

    // The new row is selected, so Continue opens the new team's HQ.
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");
    assert_eq!(shell.team_slot(), Some(slot));
    assert_eq!(hq_mode::selected_team(game), Some(slot));
    assert!(document_has_text(&shell, "Robo Rats"));
    assert!(!document_has_text(&shell, "The Rookies"));

    // A fresh boot lists it, selected (the last team, as the original does); the first row is
    // still The Rookies' HQ.
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    assert_eq!(select_rows(&shell), rows_before + 1);
    click_select_row(&mut shell, 0);
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.team_slot(), Some(0));
    assert!(document_has_text(&shell, "The Rookies"));

    // Delete Team on the new row: Yes clears the record on disk.
    shell.click_named("MainMenu button").unwrap();
    shell.click_named("Team HQ button").unwrap();
    click_select_row(&mut shell, rows_before);
    shell.click_named("delete team button").unwrap();
    assert_eq!(shell.window_depth(), 1);
    shell.click_named("yes button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    assert_eq!(select_rows(&shell), rows_before);
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    assert!(!table.teams.iter().any(|t| t.name == "Robo Rats"));
    assert!(!table.teams[slot].is_named());
    assert_eq!(table.teams.len(), 16);
}

/// The HQ's `Edit Team` opens the same window as New Team, captioned `Edit
/// Robotics Team` and filled from the HQ's team (capture `teamhq-editteam`);
/// OK rewrites that record (name, motto, logo) and keeps its robots.
#[test]
fn edit_team_shows_the_teams_own_record_and_ok_rewrites_it() {
    let Some(scratch) = ScratchGame::new("editteam") else {
        return;
    };
    let game = scratch.path();
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");
    shell.click_named("edit team button").unwrap();
    assert_eq!(shell.window_depth(), 1);
    assert!(document_has_text(&shell, "Edit Robotics Team"));
    assert!(document_has_text(&shell, "The Rookies"));
    shell.click_named("name input textbox").unwrap();
    for c in "!".chars() {
        assert!(shell.type_char(c));
    }
    shell.click_named("ok button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    assert_eq!(table.teams[0].name, "The Rookies!");
    assert!(
        !table.teams[0].robots.is_empty(),
        "the record keeps its robots"
    );
}

/// Continuing with a team other than the first opens that team's HQ: its
/// name, its roster, its career (`FUN_004eba60` selects the grid's team).
#[test]
fn the_selected_team_is_the_one_the_hq_opens() {
    let Some(scratch) = ScratchGame::new("selectteam") else {
        return;
    };
    let game = scratch.path();
    let slot = ra2ui::teams::create_team(game, "Second Team", "Motto", "Teams\\star.bmp")
        .unwrap()
        .expect("an empty slot");
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    assert_eq!(select_rows(&shell), 2);
    click_select_row(&mut shell, 1);
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.team_slot(), Some(slot));
    assert!(document_has_text(&shell, "Second Team"));
    let view = hq_mode::shell_view(game).unwrap();
    assert_eq!(view.team.name, "Second Team");
    assert!(view.robots.is_empty(), "a new team has no robots yet");
    // The career the event flow opens is the same team's.
    let career = hq_mode::load_career(game).unwrap();
    assert_eq!(career.player_slot, slot);
}

/// Click the select-team grid's `row`-th team cell.
fn click_select_row(shell: &mut Shell, row: usize) {
    let placed = ra2ui::resolve(shell.document(), shell.size());
    let cell = placed
        .iter()
        .flat_map(|p| p.walk())
        .filter(|p| p.widget.name() == "team cell")
        .nth(row)
        .map(|p| p.frame)
        .unwrap_or_else(|| panic!("no team cell {row}"));
    shell
        .click(cell.x + cell.w / 2.0, cell.y + cell.h / 2.0)
        .unwrap();
}

/// Whether any widget of the current page shows `text`.
fn document_has_text(shell: &Shell, text: &str) -> bool {
    fn walk(widget: &ra2fmt::gib::Widget, text: &str) -> bool {
        ra2ui::input::field_of(widget, "_Text").is_some_and(|t| t.trim() == text)
            || widget.children.iter().any(|c| walk(c, text))
    }
    shell.document().blocks.iter().any(|b| match b {
        Block::Widget(root) => walk(root, text),
        _ => false,
    })
}

fn select_rows(shell: &Shell) -> usize {
    shell
        .document()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Widget(root) => {
                ra2ui::workshop::find(root, ra2ui::teams::SELECT_GRID).map(|grid| {
                    grid.children
                        .iter()
                        .filter(|c| c.name() != "Sel Bar" && c.name() != "High Bar")
                        .count()
                })
            }
            _ => None,
        })
        .unwrap_or(0)
}

/// The main menu's `Quit button` is the executable's quit binding, so the
/// window exits on it; the multiplayer screen's buttons reach their windows
/// (navigation only).
#[test]
fn quit_and_the_multiplayer_buttons_are_bound() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, WINDOW).unwrap();
    let step = shell.click_named("Quit button").unwrap();
    assert!(flow::quits("mainmenu/mainmenu.gib", &step));
    let step = shell.click_named("Multiplayer button").unwrap();
    assert!(!flow::quits("mainmenu/mainmenu.gib", &step));
    assert_eq!(shell.base_screen_id(), "multiplayer/multiplayer.gib");
    shell.click_named("IP connect button").unwrap();
    assert_eq!(shell.window_depth(), 1);
    shell.click_named("cancel button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    shell.click_named("Help button").unwrap();
    assert_eq!(shell.window_depth(), 1);
    shell.click_named("close button").unwrap();
    shell.click_named("Create Game button").unwrap();
    assert_eq!(shell.base_screen_id(), "multiplayer/mp_create.gib");
    // It opens on its settings tab (`FUN_0049a000` loads mpc_settings.gib).
    assert_eq!(
        shell.content_id().as_deref(),
        Some("multiplayer/mpc_settings.gib")
    );
    assert!(
        document_has_text(&shell, "Server Name:") || shell.point_for("server name textbox").is_ok()
    );
    shell.click_named("cancel game button").unwrap();
    assert_eq!(shell.base_screen_id(), "multiplayer/multiplayer.gib");
}

/// The intro's `Skip Intro` button is a click target, not only the Esc key.
#[test]
fn the_skip_intro_button_skips_the_arena_intro() {
    let Some(game) = game_dir() else { return };
    let _guard = ra2script::lock();
    let previous = std::env::var("RA2_ARENA_INTROS").ok();
    let previous_cfg = std::env::var_os("RA2_CONFIG");
    std::env::remove_var("RA2_ARENA_INTROS");
    // The shipped settings (`ArenaIntros=1`), read only.
    std::env::set_var("RA2_CONFIG", game.join("ra2.cfg"));
    let mut shell = Shell::boot(&game, ra2_app::SHELL_SIZE).unwrap();
    shell.click_named("exhibition button").unwrap();
    let (mut session, _) =
        ra2_app::play_mode::session_from_shell(&game, &shell, Some(5.0), None).unwrap();
    // `ArenaIntros=1` (the shipped config) starts the arena's intro.
    let intro = session.matched.state() == MatchState::Intro;
    match previous_cfg {
        Some(value) => std::env::set_var("RA2_CONFIG", value),
        None => std::env::remove_var("RA2_CONFIG"),
    }
    assert!(intro, "the shipped config plays the arena intro");
    {
        let layout = session.layout();
        // Not on the button: nothing.
        assert!(!session.skip_button_at([5.0, 5.0]));
        let hud = ra2ui::workshop::load(&game.join("UI"), "common/gamewindow.gib").unwrap();
        let placed = ra2ui::resolve(&hud, layout);
        let button = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "skip button")
            .map(|p| p.frame)
            .expect("the skip button");
        let centre = [button.x + button.w / 2.0, button.y + button.h / 2.0];
        assert!(session.skip_button_at(centre));
        session.skip_intro();
        assert_eq!(session.matched.state(), MatchState::Countdown);
    }
    match previous {
        Some(value) => std::env::set_var("RA2_ARENA_INTROS", value),
        None => std::env::remove_var("RA2_ARENA_INTROS"),
    }
}

/// Esc on the shell's screens and windows does nothing (`0x5ee6b0`): it does
/// not quit, leave the screen or close a window.
#[test]
fn escape_on_the_shell_does_nothing() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, WINDOW).unwrap();
    for route in [&[][..], &["exhibition button"][..], &["Options button"][..]] {
        let mut shell_here = Shell::boot(&game, WINDOW).unwrap();
        for name in route {
            shell_here.click_named(name).unwrap();
        }
        let (screen, depth) = (shell_here.screen_id(), shell_here.window_depth());
        assert!(!flow::shell_escape(&mut shell_here), "Esc does not quit");
        assert_eq!(shell_here.screen_id(), screen);
        assert_eq!(shell_here.window_depth(), depth);
    }
    assert!(!flow::shell_escape(&mut shell));
}

/// An event match is as long as the session's exhibition time: the bracket
/// never sets one (`FUN_0046c720`), so a fresh session's is the constructor's
/// 3 minutes.
#[test]
fn an_event_match_lasts_the_sessions_match_time() {
    let Some(game) = game_dir() else { return };
    let shell = Shell::boot(&game, WINDOW).unwrap();
    assert_eq!(flow::event_match_seconds(&shell), 180.0);
}

/// The pause dialog's `options button` opens `options win` over the match:
/// the shell's window stack holds it (drawn alone over the match frame), and
/// its OK closes it again.
#[test]
fn the_match_options_window_is_a_shell_modal() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, WINDOW).unwrap();
    shell.click_named("exhibition button").unwrap();
    let base = shell.base_screen_id();
    shell.open_modal("common/options.gib").unwrap();
    assert_eq!(shell.window_depth(), 1);
    assert_eq!(shell.screen_id(), "common/options.gib");
    let windows = shell.window_placements();
    assert!(!windows.is_empty());
    assert!(
        windows.len() < shell.placements().len(),
        "the screen is not drawn"
    );
    shell.click_named("cancel button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    assert_eq!(shell.base_screen_id(), base);
}

/// The options window's resolution and camera dropdowns open lists (the
/// display's modes; `watch`/`static`/`orbit`) and a picked row is the setting:
/// the file's `FSDisplay*`/`FS3DDisplay*` pair and `DefaultCamera`.
#[test]
fn the_options_dropdowns_list_their_rows_and_a_row_sets_the_value() {
    let Some(game) = game_dir() else { return };
    let cfg = std::env::temp_dir().join(format!("ra2-options-dd-{}.cfg", std::process::id()));
    std::fs::copy(game.join("ra2.cfg"), &cfg).unwrap();
    let mut settings = ra2_app::options::Options::open(&cfg);
    settings.set_display_modes(&[[640, 480], [1024, 768], [1280, 1024], [1920, 1080]]);
    let mut shell = Shell::boot(&game, WINDOW).unwrap();
    shell.click_named("exhibition button").unwrap();
    shell.open_modal("common/options.gib").unwrap();
    settings.register_dropdowns(&mut shell);
    let field = |shell: &Shell, name: &str| {
        let placed = shell.placements();
        placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name)
            .map(|p| p.frame)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let rows = |shell: &Shell, name: &str| -> Vec<String> {
        let placed = shell.placements();
        let dropdown = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name)
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
    let resolution = field(&shell, "resolution dropdown");
    shell
        .click(resolution.x + resolution.w - 8.0, resolution.y + 9.0)
        .unwrap();
    assert_eq!(
        rows(&shell, "resolution dropdown"),
        [
            "640x480x32",
            "1024x768x32",
            "1280x1024x32",
            "1920x1080x32",
            "640x480x16",
            "1024x768x16"
        ]
    );
    // The fourth row.
    shell
        .click(
            resolution.x + 20.0,
            resolution.y + resolution.h + 3.0 * 18.0 + 9.0,
        )
        .unwrap();
    assert!(rows(&shell, "resolution dropdown").is_empty());
    let changes = settings.absorb_picks(&shell.take_dropdown_picks());
    assert_eq!(changes, ["FSDisplay=1920x1080"]);
    assert_eq!(settings.cfg().fs_display_width(), 1920);
    assert_eq!(settings.cfg().fs_display_height(), 1080);
    // The camera list.
    let camera = field(&shell, "Camera dropdown");
    shell
        .click(camera.x + camera.w - 8.0, camera.y + 9.0)
        .unwrap();
    assert_eq!(
        rows(&shell, "Camera dropdown"),
        ["watch", "orbit", "static"]
    );
    shell
        .click(camera.x + 20.0, camera.y + camera.h + 1.0 * 18.0 + 9.0)
        .unwrap();
    settings.absorb_picks(&shell.take_dropdown_picks());
    assert_eq!(settings.cfg().default_camera(), "orbit");
    std::fs::remove_file(&cfg).ok();
}

/// `Finish Later` before any round (`FUN_0046b9b0` -> `FUN_0046c720`): the
/// event entered through the join window is committed to the team record, so a
/// fresh shell on the same install shows it in progress and its bracket opens.
#[test]
fn finish_later_keeps_the_entered_event_in_the_team_record() {
    let Some(scratch) = ScratchGame::new("later") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button", "events button"] {
        shell.click_named(name).unwrap();
    }
    let grid = frame_of(&shell, ra2ui::hq::EVENT_GRID);
    shell
        .click(grid.x + grid.w / 6.0, grid.y + grid.h / 6.0)
        .unwrap();
    // No robot is chosen when the window opens (live): Enter does nothing until one is clicked.
    shell.click_named("enter event button").unwrap();
    assert_ne!(shell.base_screen_id(), flow::BRACKET_SCREEN);
    let robots = frame_of(&shell, "robot grid");
    shell
        .click(robots.x + robots.w / 12.0, robots.y + robots.h / 2.0)
        .unwrap();
    shell.click_named("enter event button").unwrap();
    assert_eq!(shell.base_screen_id(), flow::BRACKET_SCREEN);
    let before = ra2sim::career::Career::load(game).unwrap();
    assert!(
        matches!(before.phase, ra2sim::career::SeasonPhase::Schedule),
        "entering writes nothing"
    );
    let (event, _) = shell.entered_event().unwrap();
    shell.click_named("later button").unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");

    let saved = ra2sim::career::Career::load(game).unwrap();
    assert_eq!(
        saved.phase,
        ra2sim::career::SeasonPhase::InEvent { event },
        "the entered event is in the team record"
    );
    assert!(saved.history.is_empty(), "no round was played");
    // A fresh shell (a restart): the event is the one in progress.
    let view = hq_mode::shell_view(game).unwrap();
    let row = view.events.iter().find(|row| row.id == event).unwrap();
    assert_eq!(row.state, ra2ui::hq::EventState::InProgress);
    // Leaving again changes nothing further.
    let again = window_shell(game);
    drop(again);
    assert_eq!(
        ra2sim::career::Career::load(game).unwrap().phase,
        ra2sim::career::SeasonPhase::InEvent { event }
    );
}

/// A robot built from an empty inventory selection joins the team when the
/// lab is left: a free `Bot<n>.bot` slot is written and the record's
/// `Robots:` line lists it (the selected cell, even when nothing was built).
#[test]
fn a_new_workshop_robot_joins_the_roster() {
    let Some(scratch) = ScratchGame::new("newbot") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button"] {
        shell.click_named(name).unwrap();
    }
    // The inventory has six cells: scrap a robot so that a `New Robot` cell exists.
    let first = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots[0] as usize;
    flow::scrap_prompt(&mut shell, Some(first)).unwrap();
    shell.click_named("yes button").unwrap();
    let roster_before = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots
        .clone();

    // Nothing built: nothing added. The Workshop needs a cell: the first `New Robot` one.
    let free = (0..6_i32)
        .find(|slot| !roster_before.contains(slot))
        .expect("a free slot");
    let grid = frame_of(&shell, ra2ui::hq::ROBOT_GRID);
    let (column, row) = (free / 3, free % 3);
    shell
        .click(
            grid.x + grid.w * (column as f32 + 0.5) / 2.0,
            grid.y + grid.h * (row as f32 + 0.5) / 3.0,
        )
        .unwrap();
    shell.click_named("workshop button").unwrap();
    let mut builder = flow::open_workshop(game, Some(free as usize)).unwrap();
    assert!(builder.team_target().is_none());
    builder.click_named("back button").unwrap();
    // Live: leaving with nothing built still saves the selected cell (`Bot<cell>.bot`, `Enter Bot Name`).
    let empty = flow::close_workshop(&mut builder, &mut shell)
        .unwrap()
        .expect("an empty new robot is saved too");
    assert!(
        empty.ends_with(format!("Teams/Team0/Bot{free}.bot")),
        "{}",
        empty.display()
    );
    let bot = ra2fmt::bot::parse(&std::fs::read(&empty).unwrap()).unwrap();
    assert_eq!(bot.name, "Enter Bot Name");
    shell.click_named("back button").unwrap();

    // A part attached to it (the cell is selected again, now a robot's).
    shell
        .click(
            grid.x + grid.w * (column as f32 + 0.5) / 2.0,
            grid.y + grid.h * (row as f32 + 0.5) / 3.0,
        )
        .unwrap();
    shell.click_named("workshop button").unwrap();
    let mut builder = flow::open_workshop(game, Some(free as usize)).unwrap();
    // A blank robot's later tabs are dim until it has a chassis shape.
    builder.draw_square_chassis().unwrap();
    builder.click_named("components button").unwrap();
    builder.click_named("mechanics button").unwrap();
    builder
        .place_component_free("Components\\ztek1.txt")
        .unwrap();
    builder.click_named("back button").unwrap();
    // The robot lands in the selected cell, over the scrapped robot's file that stayed on disk.
    let saved = flow::close_workshop(&mut builder, &mut shell)
        .unwrap()
        .expect("the new robot is saved to the team");
    assert!(
        saved.ends_with(format!("Teams/Team0/Bot{free}.bot")),
        "{}",
        saved.display()
    );
    assert!(saved.is_file());
    let roster = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots
        .clone();
    assert_eq!(roster.len(), roster_before.len() + 1);
    assert!(roster.contains(&free));
    let view = hq_mode::shell_view(game).unwrap();
    assert_eq!(view.robots.len(), roster.len());
}

/// The robot entered in the team's current event is not editable
/// (`FUN_004fee90`'s guard and message).
#[test]
fn the_robot_in_an_event_is_not_editable() {
    let Some(scratch) = ScratchGame::new("lockedbot") else {
        return;
    };
    let game = scratch.path();
    let mut career = ra2sim::career::Career::new(game, hq_mode::SEED).unwrap();
    let event = career.schedule[0];
    career.enter_event(event).unwrap();
    career.player_mut().event_robot = career.player().robots[1];
    career.save(game).unwrap();
    assert!(flow::robot_in_event(game, 1));
    assert!(!flow::robot_in_event(game, 0));
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    shell
        .show_message("Robot Arena 2", flow::ROBOT_IN_EVENT)
        .unwrap();
    assert_eq!(shell.window_depth(), 1);
    shell.click_named("ok button").unwrap();
    assert_eq!(shell.window_depth(), 0);
}

/// The inventory's `scrap button` (`FUN_004ff0c0`, docs/36 9.24): the question is Yes/No,
/// No keeps the robot, Yes takes it off the team's list in `Teams.txt` and leaves its `.bot`
/// file where it was; the robot entered in an event only gets the OK box.
#[test]
fn scrapping_a_robot_unlists_it_and_keeps_the_file() {
    let Some(scratch) = ScratchGame::new("scrap") else {
        return;
    };
    let game = scratch.path();
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    shell.click_named("continue button").unwrap();
    let before = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots
        .clone();
    assert!(before.len() >= 2, "the team has robots to scrap");
    let slot = before[1] as usize;
    let name = shell.hq_robot_name(slot).unwrap().to_string();
    assert!(!name.is_empty());
    let file = hq_mode::player_robot_path(game, 0, before[1]);
    assert!(file.is_file());

    // No: nothing changes.
    flow::scrap_prompt(&mut shell, Some(slot)).unwrap();
    assert_eq!(shell.window_depth(), 1);
    // (The box's body draws blank, as in the original's capture `scrap-box`; the question is
    // the text handed to the box.)
    assert!(document_has_text(&shell, "Team HQ"));
    shell.click_named("no button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    assert_eq!(table.teams[0].robots, before);

    // Yes: the roster entry is gone, the file stays.
    flow::scrap_prompt(&mut shell, Some(slot)).unwrap();
    shell.click_named("yes button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    let mut expect = before.clone();
    expect.remove(1);
    assert_eq!(table.teams[0].robots, expect);
    assert!(file.is_file(), "the .bot stays on disk");
    // The other robots keep their slots; the scrapped cell is empty.
    assert_eq!(shell.hq_robot_name(slot), None);
    assert!(shell.hq_robot_name(before[0] as usize).is_some());
}

/// The inventory's Export Bot and Import Bot (docs/36 9.25). Export writes `Robot Designs/<name>.bot`, the robot
/// loaded and saved again as 1.12 (byte-identical to the live exports `teamhq-export-*`); Import puts a design
/// into an empty cell as `Bot<slot>.bot`, the same re-save, and the slot rejoins `Robots:`. The buttons dim by
/// selection (live: none selected, all four dim; a robot, Import dim; a `New Robot` cell, Export and Scrap dim).
#[test]
fn export_and_import_bot_resave_the_design_and_the_buttons_follow_the_selection() {
    let Some(scratch) = ScratchGame::new("exportimport") else {
        return;
    };
    let game = scratch.path();
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    shell.click_named("continue button").unwrap();
    let dimmed = |shell: &Shell, name: &str| {
        let placed = ra2ui::resolve(shell.document(), shell.size());
        let widget = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name)
            .unwrap()
            .widget;
        ra2ui::input::button_dimmed(widget)
    };
    let buttons = |shell: &Shell| {
        [
            "workshop button",
            "scrap button",
            "export button",
            "import button",
        ]
        .map(|name| dimmed(shell, name))
    };
    assert_eq!(buttons(&shell), [true; 4], "nothing selected: all dim");

    // Select robot slot 4 (cell column 1, row 1): Import dims, the others light.
    let cell = frame_of(&shell, "robot grid");
    let point = |slot: usize| {
        let (column, row) = (slot / 3, slot % 3);
        [
            cell.x + cell.w * (column as f32 + 0.5) / 2.0,
            cell.y + cell.h * (row as f32 + 0.5) / 3.0,
        ]
    };
    let p = point(4);
    shell.click(p[0], p[1]).unwrap();
    assert_eq!(buttons(&shell), [false, false, false, true]);

    // Export: the window's file name is the robot's name; OK writes the re-saved design.
    let name = shell.hq_robot_name(4).unwrap().to_string();
    shell.ask_export(4).unwrap();
    assert_eq!(shell.window_depth(), 1);
    shell.click_named("ok button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    let exported = game.join("Robot Designs").join(format!("{name}.bot"));
    let source = hq_mode::player_robot_path(game, 0, 4);
    let expect = ra2fmt::bot::parse(&std::fs::read(&source).unwrap())
        .unwrap()
        .upgraded()
        .write();
    assert_eq!(std::fs::read(&exported).unwrap(), expect);
    assert!(std::fs::read(&exported).unwrap().starts_with(b"1.12\n"));

    // Scrap it, select the empty cell: Export and Scrap dim, Workshop and Import light.
    flow::scrap_prompt(&mut shell, Some(4)).unwrap();
    shell.click_named("yes button").unwrap();
    assert_eq!(buttons(&shell), [true; 4], "a refill clears the selection");
    shell.click(p[0], p[1]).unwrap();
    assert_eq!(buttons(&shell), [false, true, true, false]);

    // Import the exported design (the list is the designs in byte order) into the empty cell.
    shell.ask_import(4).unwrap();
    assert_eq!(shell.window_depth(), 1);
    let list = frame_of(&shell, "file list");
    let designs = ra2fmt::saves::list(game, "Robot Designs");
    let mut designs: Vec<_> = designs
        .into_iter()
        .filter(|n| n.ends_with(".bot"))
        .collect();
    designs.sort();
    let row = designs
        .iter()
        .position(|n| *n == format!("{name}.bot"))
        .unwrap();
    shell
        .click(list.x + 20.0, list.y + 18.0 * row as f32 + 9.0)
        .unwrap();
    shell.click_named("ok button").unwrap();
    assert_eq!(shell.window_depth(), 0);
    assert_eq!(shell.hq_robot_name(4), Some(name.as_str()));
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    assert!(table.teams[0].robots.windows(2).all(|w| w[0] < w[1]));
    assert!(table.teams[0].robots.contains(&4));
    assert_eq!(std::fs::read(&source).unwrap(), expect);
    assert_eq!(buttons(&shell), [true; 4]);
}

/// Click the cell of inventory slot `slot` (down each column first).
fn click_inventory_slot(shell: &mut Shell, slot: usize) {
    let grid = frame_of(shell, "robot grid");
    let (column, row) = (slot / 3, slot % 3);
    shell
        .click(
            grid.x + grid.w * (column as f32 + 0.5) / 2.0,
            grid.y + grid.h * (row as f32 + 0.5) / 3.0,
        )
        .unwrap();
}

/// The whole career, scripted: a new team, a robot imported into its empty cell and one built in the
/// workshop, an event joined with it and fought round by round with the player's keys, the result
/// awarded and saved, the app restarted on the saved files (the event is still open), a second event
/// refused until the first is done, the first forfeited, and the next one entered.
#[test]
fn a_whole_career_new_team_to_forfeit_survives_a_restart() {
    let Some(scratch) = ScratchGame::new("career") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    std::env::set_var("RA2_ARENA_INTROS", "0");

    // New team.
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    shell.click_named("new team button").unwrap();
    shell.click_named("name input textbox").unwrap();
    for c in "Career Crew".chars() {
        shell.type_char(c);
    }
    shell.click_named("ok button").unwrap();
    shell.click_named("continue button").unwrap();
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    let slot = table
        .teams
        .iter()
        .position(|t| t.name == "Career Crew")
        .unwrap();
    assert_eq!(shell.team_slot(), Some(slot));
    assert!(
        table.teams[slot].robots.is_empty(),
        "a new team owns no robot"
    );

    // Robot Inventory: import a design into cell 0, build another in cell 1.
    let design = ra2fmt::saves::list(game, "Robot Designs")
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".bot"))
        .min()
        .expect("a shipped design");
    click_inventory_slot(&mut shell, 0);
    shell.ask_import(0).unwrap();
    let mut designs: Vec<_> = ra2fmt::saves::list(game, "Robot Designs")
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".bot"))
        .collect();
    designs.sort();
    let row = designs.iter().position(|n| *n == design).unwrap();
    let list = frame_of(&shell, "file list");
    shell
        .click(list.x + 20.0, list.y + 18.0 * row as f32 + 9.0)
        .unwrap();
    shell.click_named("ok button").unwrap();
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    assert_eq!(
        table.teams[slot].robots,
        vec![0],
        "the import joins the roster"
    );
    assert!(hq_mode::player_robot_path(game, slot, 0).is_file());

    click_inventory_slot(&mut shell, 1);
    shell.click_named("workshop button").unwrap();
    assert!(shell.lab().is_some());
    let mut builder = flow::open_workshop(game, Some(1)).unwrap();
    builder.click_named("back button").unwrap();
    let saved = flow::close_workshop(&mut builder, &mut shell)
        .unwrap()
        .unwrap();
    assert!(saved.is_file());
    shell.click_named("back button").unwrap();
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    assert_eq!(table.teams[slot].robots, vec![0, 1]);

    drop(shell);

    // Event A: joined with the imported robot, fought with the keys held; whatever the result, the
    // round is awarded and saved.
    let mut shell = hq_shell(game, slot);
    let a = join_event(&mut shell, 0, 0);
    assert_eq!(shell.entered_event().map(|e| e.0), Some(a));
    let played = fight_event(game, &mut shell, None);
    let career = hq_mode::load_career(game).unwrap();
    eprintln!(
        "A: {played:?} history {} points {}",
        career.history.len(),
        career.season_points()
    );

    // A restart: the same career, event A completed, nothing in progress.
    let mut shell = hq_shell(game, slot);
    let view = hq_mode::shell_view(game).unwrap();
    let state =
        |view: &ra2ui::hq::View, id: usize| view.events.iter().find(|r| r.id == id).unwrap().state;
    assert_eq!(state(&view, a), ra2ui::hq::EventState::Completed);

    // Upgrade in the workshop between events: a part on the imported robot, written back to its slot.
    click_inventory_slot(&mut shell, 0);
    shell.click_named("workshop button").unwrap();
    let mut builder = flow::open_workshop(game, Some(0)).unwrap();
    let before = builder.build().components().len();
    builder.click_named("components button").unwrap();
    builder.click_named("mechanics button").unwrap();
    builder
        .place_component_free("Components\\ztek1.txt")
        .unwrap();
    assert_eq!(builder.build().components().len(), before + 1);
    builder.click_named("back button").unwrap();
    flow::close_workshop(&mut builder, &mut shell)
        .unwrap()
        .unwrap();
    shell.click_named("back button").unwrap();
    let upgraded =
        ra2fmt::bot::parse(&std::fs::read(hq_mode::player_robot_path(game, slot, 0)).unwrap())
            .unwrap();
    eprintln!(
        "upgraded robot has {} components",
        upgraded.components.len()
    );

    // Event B: won round by round (the harness forces the result), `Finish Later` in between,
    // resumed after a restart, to the trophy.
    let b = join_event(&mut shell, 1, 0);
    let first = fight_event(game, &mut shell, Some(true));
    assert!(
        first.is_none(),
        "a won round 1 of event B leaves rounds to play"
    );
    let career = hq_mode::load_career(game).unwrap();
    assert_eq!(
        career.phase,
        ra2sim::career::SeasonPhase::InEvent { event: b }
    );
    assert_eq!(career.history.len(), 1);

    // `Finish Later`, quit, boot again: B is in progress, every other event is disabled.
    shell.click_named("later button").unwrap();
    assert_eq!(shell.base_screen_id(), "teamhq/teamhq.gib");
    drop(shell);
    let mut shell = hq_shell(game, slot);
    let view = hq_mode::shell_view(game).unwrap();
    assert_eq!(state(&view, b), ra2ui::hq::EventState::InProgress);
    let open = view
        .events
        .iter()
        .filter(|r| r.state == ra2ui::hq::EventState::Available)
        .count();
    assert_eq!(open, 0, "no other event can be joined while B is open");
    // The robot of an open event cannot be edited or scrapped.
    assert!(flow::robot_in_event(game, 0));

    // Resume B from the schedule and fight on, winning, until the trophy.
    shell.click_named("events button").unwrap();
    click_event_cell(
        &mut shell,
        view.events.iter().position(|r| r.id == b).unwrap(),
    );
    assert_eq!(
        shell.base_screen_id(),
        flow::BRACKET_SCREEN,
        "an open event opens its bracket"
    );
    let mut record = None;
    for _ in 0..8 {
        record = fight_event(game, &mut shell, Some(true));
        if record.is_some() {
            break;
        }
    }
    let record = record.expect("winning every round decides the event");
    assert!(record.won, "{record:?}");
    let career = hq_mode::load_career(game).unwrap();
    assert_eq!(career.history.len(), 2);
    assert_eq!(career.phase, ra2sim::career::SeasonPhase::Schedule);
    assert!(!career.prizes.is_empty(), "a won event earns its trophy");
    assert!(
        career.season_points() > 80,
        "points {}",
        career.season_points()
    );
    eprintln!(
        "B won: {record:?}; prizes {:?}; points {}",
        career.prizes,
        career.season_points()
    );

    // Event C is joined and forfeited from the bracket: it closes as a loss, the robot is free again.
    let mut shell = hq_shell(game, slot);
    let c = join_event(&mut shell, 2, 0);
    let step = shell.click_named("forfeit button").unwrap();
    assert_eq!(step.hit_widget(), Some("forfeit button"));
    flow::forfeit_event(game, shell.entered_event()).unwrap();
    let career = hq_mode::load_career(game).unwrap();
    assert_eq!(career.history.len(), 3);
    assert_eq!(career.history.last().map(|r| r.event), Some(c));
    assert!(!career.history.last().unwrap().won);
    assert_eq!(career.phase, ra2sim::career::SeasonPhase::Schedule);
    assert!(!flow::robot_in_event(game, 0));
    eprintln!("C forfeited; points {}", career.season_points());
}

/// A freshly booted shell opened on team `slot`'s Team HQ (a restart of the app).
fn hq_shell(game: &Path, slot: usize) -> Shell {
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    // A boot lists the first team selected; the player picks theirs.
    let table = ra2fmt::team::read_table(&game.join("Teams/Teams.txt")).unwrap();
    let row = table.teams[..slot].iter().filter(|t| t.is_named()).count();
    click_select_row(&mut shell, row);
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.team_slot(), Some(slot));
    shell
}

/// Click the schedule grid's `row`-th cell (row-major, as the HQ numbers them).
fn click_event_cell(shell: &mut Shell, row: usize) {
    let placed = ra2ui::resolve(shell.document(), shell.size());
    let grid = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == ra2ui::hq::EVENT_GRID)
        .unwrap();
    let geometry = ra2ui::Grid::from_widget(grid.widget).unwrap();
    let (columns, rows) = (
        geometry.columns.max(1) as usize,
        geometry.rows.max(1) as usize,
    );
    let (x, y) = (
        grid.frame.x + grid.frame.w * ((row % columns) as f32 + 0.5) / columns as f32,
        grid.frame.y + grid.frame.h * ((row / columns) as f32 + 0.5) / rows as f32,
    );
    shell.click(x, y).unwrap();
}

/// Join the schedule's event `row` with the join window's robot `column`; the bracket opens.
fn join_event(shell: &mut Shell, row: usize, column: usize) -> usize {
    shell.click_named("events button").unwrap();
    click_event_cell(shell, row);
    assert_eq!(shell.window_depth(), 1, "the join window opens");
    let robots = frame_of(shell, "robot grid");
    shell
        .click(
            robots.x + robots.w * (column as f32 + 0.5) / 6.0,
            robots.y + robots.h / 2.0,
        )
        .unwrap();
    shell.click_named("enter event button").unwrap();
    assert_eq!(shell.base_screen_id(), flow::BRACKET_SCREEN);
    shell.entered_event().unwrap().0
}

/// Fight the entered event's next round (the player holds W for four seconds), award it, and walk the
/// award/repair screens back to the HQ or the bracket. Returns the event's record when the round decided it.
fn fight_event(
    game: &Path,
    shell: &mut Shell,
    forced: Option<bool>,
) -> Option<ra2sim::career::EventRecord> {
    let mut hq = flow::start_event_match(game, shell.entered_event(), 4.0).unwrap();
    hq.set_forced_win(forced);
    {
        let session = hq.match_session_mut().unwrap();
        session.skip_intro();
        let mut input = DriveInput::default();
        input.keys.set(ra2sim::controls::dik::W, true);
        session.set_drive(input);
        let mut guard = 0;
        while session.matched.state() != MatchState::Results && guard < 400 {
            session.advance(0.05);
            guard += 1;
        }
        assert_eq!(session.matched.state(), MatchState::Results);
    }
    hq.finish_match().unwrap();
    let over = hq.decided_event().cloned();
    let mut guard = 0;
    while !hq.on_hq_screen() && !hq.on_event_screen() && guard < 8 {
        let point = continue_point(&mut hq);
        hq.click(point).unwrap();
        guard += 1;
    }
    over
}

/// A robot built from nothing in the Bot Lab (docs/36 section 9.27): the lab opens blank on an empty
/// cell (no chassis, weight 0.0, Classification `None`, Components, Wiring, Paint Shop and Test Robot
/// dim), the baseplate outline is drawn and closed on the blueprint, step 2 and Finished extrude it,
/// the tabs light, a control board, two motors and their wheels are placed, an analog control is
/// dragged onto the controller (W and S), the motors are wired to it in the connection window, the
/// robot is named, saved by Back to HQ into the selected cell, and then it drives in a match.
#[test]
fn a_robot_built_from_nothing_is_wired_saved_and_fights() {
    use ra2_app::build_mode::PlaceOn;
    let Some(scratch) = ScratchGame::new("fromnothing") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button"] {
        shell.click_named(name).unwrap();
    }
    let first = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots[0] as usize;
    flow::scrap_prompt(&mut shell, Some(first)).unwrap();
    shell.click_named("yes button").unwrap();
    let free = first;
    let mut b = flow::open_workshop(game, Some(free)).unwrap();
    b.set_size([1024.0, 768.0]).unwrap();

    // Blank: nothing built, the later tabs dim and deaf.
    assert!(b.build().is_blank());
    assert_eq!(b.build().stats().mass, 1.0);
    b.click_named("components button").unwrap();
    assert_eq!(b.tab(), Some("overview button"), "a dim tab takes no click");

    // Chassis: Structure Design, step 1, a square drawn and closed, step 2, Finished.
    b.click_named("chassis button").unwrap();
    b.click_named("structure design button").unwrap();
    for p in [[200, 200], [800, 200], [800, 800], [200, 800], [200, 200]] {
        let at = b.outline_point_for(p).unwrap();
        b.click(at[0], at[1]).unwrap();
    }
    assert_eq!(b.build().chassis().unwrap().bottom.len(), 4);
    b.click_named("step 2 button").unwrap();
    assert!(!b.build().is_blank(), "step 2 extrudes the outline");
    b.click_named("finished button").unwrap();
    assert_eq!(b.tab(), Some("chassis button"));
    let mass = b.build().stats().mass;
    assert!((mass - 94.1).abs() < 0.2, "aluminium square chassis {mass}");

    // Components: board, battery, two motors on the baseplate, a wheel on each axle.
    b.click_named("components button").unwrap();
    b.place_component(
        "Components\\controlboard.txt",
        PlaceOn::Baseplate([0.0, 0.3]),
    )
    .unwrap_or_else(|_| {
        b.click_named("power button").unwrap();
        b.place_component_free("Components\\controlboard.txt")
            .unwrap()
    });
    let mut motors = Vec::new();
    for x in [-0.55f32, 0.55] {
        b.click_named("mechanics button").unwrap();
        let motor = b
            .place_component("Components\\ztek1.txt", PlaceOn::Baseplate([x, -0.2]))
            .or_else(|_| b.place_component_free("Components\\ztek1.txt"))
            .unwrap();
        let axle = b
            .build()
            .component_info(motor)
            .and_then(|i| {
                i.mounts
                    .iter()
                    .find(|(_, a)| a.types.iter().any(|t| t == "axle_m"))
                    .map(|(id, _)| *id)
            })
            .unwrap_or_else(|| {
                panic!(
                    "a motor axle: component {motor} of {:?}",
                    b.build()
                        .components()
                        .iter()
                        .map(|c| c.file.clone())
                        .collect::<Vec<_>>()
                )
            });
        b.click_named("mobility button").unwrap();
        b.place_component(
            "Components\\miniwheel.txt",
            PlaceOn::Mount {
                parent: motor,
                mount: axle,
            },
        )
        .unwrap();
        motors.push(motor);
    }
    assert!(b.build().has_control_board());

    // Wiring: drag ANALOG onto a cell, type the keys, OK.
    b.click_named("wiring button").unwrap();
    assert_eq!(b.tab(), Some("wiring button"), "the board lights Wiring");
    let icon = b.point_of("new analog").unwrap();
    b.click(icon[0], icon[1]).unwrap();
    let cell = b.control_cell_point([0, 0]).unwrap();
    b.mouse_released(cell[0], cell[1]);
    assert!(b.dialog_open(), "the drop opens the Analog Control window");
    for (field, key) in [("input1", 'w'), ("input2", 's')] {
        let at = b.dialog_point_for(field).unwrap();
        b.click(at[0], at[1]).unwrap();
        b.type_char(key);
    }
    let ok = b.dialog_point_for("ok button").unwrap();
    b.click(ok[0], ok[1]).unwrap();
    assert!(!b.dialog_open());
    let control = b.build().board().control_at([0, 0]).unwrap().clone();
    assert_eq!(control.name, "New Analog");
    assert_eq!(control.key_label(), "W - S");

    // Select the control, click each motor in the 3D view, pick Spin Clockwise on both inputs.
    b.click(cell[0], cell[1]).unwrap();
    // The lab's camera starts level; the player turns it (the middle button) to look down at the parts.
    b.set_lab_cam(ra2_app::build_mode::LabCam {
        pitch: -std::f32::consts::FRAC_PI_2,
        yaw: 0.0,
        dist: 5.0,
    });
    for motor in &motors {
        let at = b.part_point(*motor).unwrap();
        b.click(at[0], at[1]).unwrap();
        assert!(
            b.dialog_open(),
            "a click on the motor opens its connection window"
        );
        for (field, row) in [("input1 map", 1usize), ("input2 map", 2)] {
            let at = b.dialog_point_for(field).unwrap();
            b.click(at[0], at[1]).unwrap();
            let rows = b.dialog_list_point(row).unwrap();
            b.click(rows[0], rows[1]).unwrap();
        }
        let ok = b.dialog_point_for("ok button").unwrap();
        b.click(ok[0], ok[1]).unwrap();
    }
    assert!(
        b.build().board().bindings.len() >= 4,
        "{:?}",
        b.build().board().bindings
    );

    // Name it on the Overview and leave.
    b.click_named("overview button").unwrap();
    // Live: a key reaches the name box after a click on it and is added to the text there, so the
    // placeholder `Enter Bot Name` is rubbed out first.
    b.type_char('x');
    assert_eq!(
        b.name_text(),
        "Enter Bot Name",
        "a key before the click goes nowhere"
    );
    b.click_named("bot name textbox").unwrap();
    b.type_char('!');
    assert_eq!(
        b.name_text(),
        "Enter Bot Name!",
        "the first key is added, not a replacement"
    );
    for _ in 0.."Enter Bot Name!".len() {
        b.backspace();
    }
    for c in "Bolt".chars() {
        b.type_char(c);
    }
    b.click_named("back button").unwrap();
    let saved = flow::close_workshop(&mut b, &mut shell).unwrap().unwrap();
    assert!(saved.ends_with(format!("Teams/Team0/Bot{free}.bot")));
    let bot = ra2fmt::bot::parse(&std::fs::read(&saved).unwrap()).unwrap();
    assert_eq!(bot.name, "Bolt");
    assert_eq!(bot.class, 0);
    assert!(bot.components.len() >= 6);
    assert!(bot.bindings.len() >= 4);
    assert_eq!(bot.controllers.len(), 1);

    // It fights: robot 0 is the new robot, driven forward through its own wiring.
    let options = ra2_app::play_mode::Options {
        game: game.to_path_buf(),
        drive: "0-3:1,0".into(),
        seconds: 4.0,
        out: std::env::temp_dir().join(format!("ra2-fromnothing-{}", std::process::id())),
        size: [1280, 1005],
        arena_script: Some("box.py".into()),
        arena_index: None,
        game_type: None,
        team: Some(0),
        bot: Some(free as i32),
        opponent_team: None,
        opponent_bot: None,
        time: None,
    };
    let outcome = ra2_app::play_mode::run_headless(&options).expect("the play run completes");
    assert_eq!(outcome.player_bot, "Bolt");
    assert!(
        outcome.distance > 0.5,
        "the built robot drove {:.2} m",
        outcome.distance
    );
}

/// A team with no robots: the join window opens, and `Enter Event!` does nothing
/// (`FUN_00481f20` returns at once while its chosen robot, `+0x16c`, is -1; the
/// constructor `0x481b60` leaves it -1 and shows no message). The window stays open and the
/// event is not entered.
#[test]
fn enter_event_without_a_robot_does_nothing() {
    let Some(scratch) = ScratchGame::new("norobots") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    ra2ui::teams::create_team(game, "Empty Team", "Motto", "Teams\\star.bmp")
        .unwrap()
        .expect("an empty slot");
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    click_select_row(&mut shell, 1);
    shell.click_named("continue button").unwrap();
    shell.click_named("events button").unwrap();
    click_event_cell(&mut shell, 0);
    assert_eq!(shell.window_depth(), 1, "the join window opens");
    let base = shell.base_screen_id().to_string();
    for _ in 0..2 {
        shell.click_named("enter event button").unwrap();
        assert_eq!(shell.window_depth(), 1, "the window stays open, no box");
        assert_eq!(shell.base_screen_id(), base);
        assert!(shell.entered_event().is_none());
    }
    shell.click_named("cancel button").unwrap();
    assert_eq!(shell.window_depth(), 0);
}

/// A fresh Select Team window selects the LAST team, whichever team was played
/// before (live after a restart of the original: nothing is saved, `FUN_004ec2c0`
/// selects count - 1 while the grid's selection is -1); within a run the window
/// keeps the row it had.
#[test]
fn select_team_opens_on_the_last_team_and_keeps_the_pick_within_a_run() {
    let Some(scratch) = ScratchGame::new("selectlast") else {
        return;
    };
    let game = scratch.path();
    ra2ui::teams::create_team(game, "Second Team", "Motto", "Teams\\star.bmp")
        .unwrap()
        .expect("an empty slot");
    let selected = |shell: &Shell| -> Option<usize> {
        shell.document().blocks.iter().find_map(|b| match b {
            Block::Widget(root) => ra2ui::workshop::find(root, ra2ui::teams::SELECT_GRID)
                .and_then(ra2ui::input::selected_row),
            _ => None,
        })
    };
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    assert_eq!(selected(&shell), Some(1), "a fresh window: the last team");
    // Play the first team, come back: the pick is kept while the client runs.
    click_select_row(&mut shell, 0);
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.team_slot(), Some(0));
    shell.click_named("select team button").unwrap();
    assert_eq!(selected(&shell), Some(0));
    // A restart forgets it (the original writes nothing): the last team again.
    let mut shell = window_shell(game);
    shell.click_named("Team HQ button").unwrap();
    assert_eq!(selected(&shell), Some(1));
    shell.click_named("continue button").unwrap();
    assert_eq!(shell.team_slot(), Some(1));
}

/// The lab's `Test Robot` tab (docs/36 9.27): the robot alone in the practice arena, driven by its own
/// wiring, with the `Obstacles` list and RESET; leaving the tab drops the arena and entering it builds a
/// new one.
#[test]
fn the_test_robot_tab_is_a_practice_arena_with_obstacles_and_reset() {
    let Some(scratch) = ScratchGame::new("practice") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut b = flow::open_workshop(game, Some(0)).unwrap();
    b.set_size([1024.0, 768.0]).unwrap();
    let run = |b: &mut ra2_app::build_mode::Builder, seconds: f32| {
        let mut left = seconds;
        while left > 0.0 {
            b.advance(left.min(1.0 / 60.0));
            left -= 1.0 / 60.0;
        }
    };
    let at = |b: &mut ra2_app::build_mode::Builder| {
        let arena = b.arena_mut().expect("the practice arena is up");
        arena.matched.duel.robots[0].position(&*arena.matched.duel.physics)
    };
    assert!(!b.arena_active(), "no arena outside the tab");
    b.click_named("test robot button").unwrap();
    run(&mut b, 2.0);
    assert!(b.arena_active());
    let start = at(&mut b);
    // The model's start point (0.306, 2.163, -0.948): the robot has dropped onto the floor there.
    assert!(
        (start.x - 0.3).abs() < 0.3 && (start.z + 0.95).abs() < 0.3,
        "{start:?}"
    );
    assert!(start.y < 0.6, "{start:?}");
    // The frame is the arena's 3D view under the status box and the Obstacles list.
    let scene = b.scene([1024, 768]).unwrap();
    assert!(scene
        .textures
        .iter()
        .any(|t| t.label == "lab practice arena"));

    // W is the robot's forward key: it runs for the far wall (live: z 16.4 in about 1.6 s).
    let mut keys = b.drive();
    keys.keys.set(17, true);
    b.set_drive(keys);
    run(&mut b, 1.8);
    let far = at(&mut b);
    assert!(far.z > 8.0, "held W: {far:?}");

    keys.keys.set(17, false);
    b.set_drive(keys);
    // The Obstacles list builds a new arena (robot back on its start point); so does RESET.
    b.click_named("blocks cb").unwrap();
    assert_eq!(b.arena_mut().unwrap().obstacle(), "blocks");
    run(&mut b, 0.5);
    let again = at(&mut b);
    assert!((again.z + 0.95).abs() < 0.5, "{again:?}");
    run(&mut b, 1.0);
    b.click_named("reset button").unwrap();
    run(&mut b, 0.5);
    let reset = at(&mut b);
    assert!((reset.z + 0.95).abs() < 0.5, "{reset:?}");
    assert_eq!(
        b.arena_mut().unwrap().obstacle(),
        "blocks",
        "RESET keeps the obstacle"
    );

    // Leaving the tab drops it; coming back starts over with no obstacle.
    b.click_named("overview button").unwrap();
    b.advance(0.1);
    assert!(!b.arena_active());
    b.click_named("test robot button").unwrap();
    run(&mut b, 0.2);
    assert_eq!(b.arena_mut().unwrap().obstacle(), "none");
}

/// Chassis step 1 judges every click by the editing area's rules too and, with the outline closed,
/// drags its points (live `botlab-step1-*`, docs/36 9.27f): a point on an edge, one that makes the
/// outline cross itself and one that makes a corner too sharp are refused with no alert (nothing
/// was accepted yet, and a point added to the list clears the latch); after accepted drags a refused
/// place raises the alert.
#[test]
fn chassis_step_one_refuses_points_by_the_editing_areas_rules_and_drags_them_once_closed() {
    let Some(scratch) = ScratchGame::new("steponerules") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button"] {
        shell.click_named(name).unwrap();
    }
    let first = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots[0] as usize;
    flow::scrap_prompt(&mut shell, Some(first)).unwrap();
    shell.click_named("yes button").unwrap();
    let mut b = flow::open_workshop(game, Some(first)).unwrap();
    b.set_size([1024.0, 768.0]).unwrap();
    b.click_named("chassis button").unwrap();
    b.click_named("structure design button").unwrap();
    let click = |b: &mut ra2_app::build_mode::Builder, p: [i32; 2]| {
        let at = b.outline_point_for(p).unwrap();
        b.mouse_moved(at[0], at[1]);
        b.click(at[0], at[1]).unwrap();
        b.mouse_released(at[0], at[1]);
    };
    for p in [[200, 200], [800, 200], [800, 800], [200, 800]] {
        click(&mut b, p);
    }
    assert_eq!(b.build().chassis().unwrap().bottom.len(), 4);
    // on the first edge / crossing the right edge / a corner too sharp at the last point
    for p in [[500, 200], [900, 500], [700, 700]] {
        click(&mut b, p);
        assert_eq!(
            b.build().chassis().unwrap().bottom.len(),
            4,
            "{p:?} is refused"
        );
        assert!(b.alert().is_none(), "{p:?}: the latch is clear, no alert");
    }
    // close it on the first point; the baseplate's points now drag
    click(&mut b, [200, 200]);
    let drag = |b: &mut ra2_app::build_mode::Builder, from: [i32; 2], to: [i32; 2], steps: i32| {
        let a = b.outline_point_for(from).unwrap();
        b.mouse_moved(a[0], a[1]);
        b.click(a[0], a[1]).unwrap();
        let z = b.outline_point_for(to).unwrap();
        for k in 1..=steps {
            let f = k as f32 / steps as f32;
            b.mouse_moved(
                (a[0] + (z[0] - a[0]) * f).round(),
                (a[1] + (z[1] - a[1]) * f).round(),
            );
        }
        b.mouse_released(z[0], z[1]);
    };
    drag(&mut b, [200, 200], [300, 300], 4);
    assert_eq!(b.build().chassis().unwrap().bottom[0], [300, 300]);
    assert!(b.alert().is_none());
    // now a refused place raises the alert and the point stays: the last point onto the first edge
    // (the point follows the cursor to the last place the rules took and stops there)
    drag(&mut b, [200, 800], [500, 250], 3);
    assert_ne!(b.build().chassis().unwrap().bottom[3], [500, 250]);
    assert_eq!(
        b.alert(),
        Some("Points should not be too close to other points or lines")
    );
}

/// Chassis step 2 as the original drives it (docs/36 section 9.27e): a press on a top point takes
/// it, the point follows the mouse (snapped to the grid) while the button is down, the editing
/// area's three alerts and the lab's bend alert refuse a move and leave the point where it was,
/// the tip line is screen 4's, the mesh and the weight follow, step 1 and step 2 again start the
/// extrusion over, and Finished keeps what was dragged. Positions are the live ones
/// (`botlab-adjust-*`): the triangle (200,200) (800,200) (500,800), its first top point to
/// (400,300) weighs 55.8 kg, its second one may go to (800,400) and not to (800,500).
#[test]
fn chassis_step_two_drags_top_points_and_refuses_what_bends() {
    let Some(scratch) = ScratchGame::new("adjusttop") else {
        return;
    };
    let game = scratch.path();
    let _guard = ra2script::lock();
    let mut shell = window_shell(game);
    for name in ["Team HQ button", "continue button"] {
        shell.click_named(name).unwrap();
    }
    let first = ra2fmt::team::read_table(&game.join("Teams/Teams.txt"))
        .unwrap()
        .teams[0]
        .robots[0] as usize;
    flow::scrap_prompt(&mut shell, Some(first)).unwrap();
    shell.click_named("yes button").unwrap();
    let mut b = flow::open_workshop(game, Some(first)).unwrap();
    b.set_size([1024.0, 768.0]).unwrap();
    b.click_named("chassis button").unwrap();
    b.click_named("structure design button").unwrap();
    for p in [[200, 200], [800, 200], [500, 800], [200, 200]] {
        let at = b.outline_point_for(p).unwrap();
        b.click(at[0], at[1]).unwrap();
    }
    assert_eq!(b.build().chassis().unwrap().bottom.len(), 3);
    b.click_named("step 2 button").unwrap();
    assert_eq!(
        b.tip_line(),
        "Click and drag points to adjust the sides of your chassis"
    );

    // a drag: press on point 0, move in three steps, release
    let drag = |b: &mut ra2_app::build_mode::Builder, from: [i32; 2], to: [i32; 2], steps: i32| {
        let a = b.outline_point_for(from).unwrap();
        b.mouse_moved(a[0], a[1]);
        b.click(a[0], a[1]).unwrap();
        let z = b.outline_point_for(to).unwrap();
        for k in 1..=steps {
            let f = k as f32 / steps as f32;
            b.mouse_moved(
                (a[0] + (z[0] - a[0]) * f).round(),
                (a[1] + (z[1] - a[1]) * f).round(),
            );
        }
        b.mouse_released(z[0], z[1]);
    };
    drag(&mut b, [200, 200], [400, 300], 6);
    assert_eq!(b.build().chassis().unwrap().top[0], [400, 300]);
    assert_eq!(b.build().chassis().unwrap().bottom[0], [200, 200]);
    let mass = b.build().stats().mass;
    assert!((mass - 55.8).abs() < 0.1, "live Overview 55.8, {mass}");
    assert!(b.alert().is_none());

    // a second point: (800,400) is fine, (800,500) bends a side too much and goes back
    drag(&mut b, [800, 200], [800, 400], 4);
    assert_eq!(b.build().chassis().unwrap().top[1], [800, 400]);
    drag(&mut b, [800, 400], [800, 500], 3);
    assert_eq!(b.build().chassis().unwrap().top[1], [800, 400]);
    assert_eq!(
        b.alert(),
        Some("Can't move that point because it will bend one side too much")
    );
    // the alert falls after its two seconds and the screen's tip is back
    b.advance(2.5);
    assert!(b.alert().is_none());

    // the outline alerts show once a move was accepted: the third point onto the first edge
    drag(&mut b, [500, 800], [600, 360], 1);
    assert_eq!(b.build().chassis().unwrap().top[2], [500, 800]);
    assert_eq!(
        b.alert(),
        Some("Points should not be too close to other points or lines")
    );

    // step 1 and step 2 again: the extrusion starts over
    b.click(965.0, 635.0).unwrap();
    assert_eq!(
        b.tip_line(),
        "Click the Components button to add parts to your robot",
        "step 1 is not screen 4"
    );
    b.click(996.0, 635.0).unwrap();
    let ch = b.build().chassis().unwrap();
    assert_eq!(ch.top, ch.bottom);
    assert!((ch.height - 0.825).abs() < 1e-6);

    // Finished keeps a dragged point
    drag(&mut b, [200, 200], [300, 200], 3);
    b.click_named("finished button").unwrap();
    assert_eq!(b.build().chassis().unwrap().top[0], [300, 200]);
}
