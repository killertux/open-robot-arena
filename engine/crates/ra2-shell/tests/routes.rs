//! The measurement: the boot screen and the two routes are the shipped data.
//!
//! The runner's boot constant, its small table of purpose-named links and its
//! route scripts are all checked here against the game's own files. Nothing in
//! this test draws: the frames are the runner's job; the screen sequence is the
//! data's.

use std::path::{Path, PathBuf};

use ra2_shell::game;
use ra2_shell::nav::{self, Navigation};
use ra2_shell::script::{self, Command};
use ra2_shell::Shell;

fn button<'a>(gib: &'a ra2fmt::gib::Gib, name: &str) -> &'a ra2fmt::gib::Widget {
    gib.walk()
        .find(|w| w.name() == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

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

fn route(name: &str) -> Vec<Command> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("routes")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    script::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The boot screen is the game's own main menu in three shipped places: the
/// executable's UI table names `mainmenu\mainmenu.gib` beside `Main Menu Win`,
/// the file's root *is* `Main Menu Win`, and the bottom navigation bar's
/// `MainMenu button` returns to it.
#[test]
fn the_boot_screen_is_the_main_menu_the_data_names() {
    let Some(game) = game_dir() else { return };
    let ui = game.join("UI");

    let exe = std::fs::read(game.join("Robot Arena 2.exe")).expect("the game executable");
    let exe = String::from_utf8_lossy(&exe);
    assert!(
        exe.contains("mainmenu\\mainmenu.gib"),
        "the executable names the boot screen"
    );
    assert!(
        exe.contains("Main Menu Win"),
        "the executable names the main menu's window class"
    );

    let (path, key) = game::screen(&ui, nav::BOOT_SCREEN).expect("the boot screen is shipped");
    assert_eq!(key, "mainmenu/mainmenu.gib");
    let gib = ra2fmt::gib::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(gib.widgets().next().unwrap().name(), "Main Menu Win");

    let nav_gib =
        ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("common/bottomnav.gib")).unwrap())
            .unwrap();
    let main = nav_gib
        .walk()
        .find(|w| w.name() == "MainMenu button")
        .expect("the nav bar's MainMenu button");
    assert_eq!(main.notes().trim(), "click to return to main menu");
    assert_eq!(
        nav::link(main.notes()).map(|l| l.screen),
        Some(nav::BOOT_SCREEN)
    );

    // The shipped boot file is the script layer, not the screen graph: it
    // imports the arenas and brains and registers the callback. It names no
    // `.gib` at all, which is why the screen comes from the executable and the
    // `.gib` data above.
    let main_py = std::fs::read_to_string(game.join("Scripts/Main.py")).unwrap();
    assert!(main_py.contains("import Arenas"));
    assert!(main_py.contains("import AI"));
    assert!(main_py.contains("giScript.register_function(dispatchEvent)"));
    assert!(
        !main_py.contains(".gib"),
        "Main.py is not where the boot screen is named"
    );
}

/// The two routes' transitions are the strings the screens carry, and the
/// targets they name are shipped screens with the roots those notes imply.
#[test]
fn the_routes_transitions_are_read_from_the_screens() {
    let Some(game) = game_dir() else { return };
    let ui = game.join("UI");

    let mainmenu =
        ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("mainmenu/mainmenu.gib")).unwrap())
            .unwrap();
    assert_eq!(
        button(&mainmenu, "exhibition button").notes().trim(),
        "link to exhibition setup"
    );
    assert_eq!(
        button(&mainmenu, "Team HQ button").notes().trim(),
        "link to team HQ"
    );
    assert_eq!(
        nav::link("link to exhibition setup").map(|l| l.screen),
        Some("exhibition/exhib_setup.gib")
    );
    assert_eq!(
        nav::link("link to team HQ").map(|l| l.screen),
        Some("teamhq/select_team.gib"),
        "the main menu's team link opens the picker"
    );

    // The picker's own Continue says the next screen, and the team HQ's Robots
    // tab says its file and slot.
    let picker =
        ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("teamhq/select_team.gib")).unwrap())
            .unwrap();
    assert!(button(&picker, "continue button")
        .notes()
        .trim()
        .starts_with("click to go to team hq with selected team"));
    assert_eq!(
        nav::link(button(&picker, "continue button").notes()).map(|l| l.screen),
        Some("teamhq/teamhq.gib")
    );

    let hq = ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("teamhq/teamhq.gib")).unwrap())
        .unwrap();
    assert_eq!(
        nav::navigation(button(&hq, "robots button").notes()),
        Some(Navigation::File {
            screen: "hq_robots.gib".into(),
            slot: Some("hq content group".into())
        })
    );

    // The robot inventory's workshop button names the workshop, and the
    // workshop's root says what it is.
    let robots =
        ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("teamhq/hq_robots.gib")).unwrap())
            .unwrap();
    assert_eq!(
        nav::link(button(&robots, "workshop button").notes()).map(|l| l.screen),
        Some("botlab/botlabwin.gib")
    );
    let lab =
        ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("botlab/botlabwin.gib")).unwrap())
            .unwrap();
    assert_eq!(lab.widgets().next().unwrap().name(), "botlab win");

    // The exhibition setup's Arena button names its file and its slot; the
    // loaded file's list names its content.
    let setup = ra2fmt::gib::parse(
        &std::fs::read_to_string(ui.join("exhibition/exhib_setup.gib")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        nav::navigation(button(&setup, "arena button").notes()),
        Some(Navigation::File {
            screen: "arena.gib".into(),
            slot: Some("content group".into())
        })
    );
    let arena =
        ra2fmt::gib::parse(&std::fs::read_to_string(ui.join("exhibition/arena.gib")).unwrap())
            .unwrap();
    let list = arena
        .walk()
        .find(|w| w.name() == ra2ui::arenas::LIST)
        .expect("the arena listbox");
    assert_eq!(list.notes().trim(), "populate with arena names");
}

/// The bot lab route, taken from the shipped input file: main menu → team
/// picker → team HQ → robot inventory → bot lab → chassis tab → its first step.
#[test]
fn the_bot_lab_route_is_the_screen_sequence_the_data_names() {
    let Some(game) = game_dir() else { return };
    let commands = route("botlab.txt");
    let mut shell = Shell::boot(&game, [1280.0, 1005.0]).unwrap();
    assert_eq!(shell.screen_id(), "mainmenu/mainmenu.gib");

    let mut clicks = 0;
    for command in &commands {
        match command {
            Command::Click { x, y } => {
                shell.click(*x, *y).unwrap();
                clicks += 1;
            }
            Command::ClickNamed { name } => {
                shell.click_named(name).unwrap();
                clicks += 1;
            }
            Command::Wait { .. } | Command::Shot { .. } => {}
        }
        match clicks {
            1 => {
                assert_eq!(shell.screen_id(), "teamhq/select_team.gib");
                assert_eq!(
                    shell.trace().last().unwrap().note.as_deref(),
                    Some("link to team HQ")
                );
            }
            2 => {
                assert_eq!(shell.screen_id(), "teamhq/teamhq.gib");
            }
            3 => {
                assert_eq!(shell.screen_id(), "teamhq/teamhq.gib");
                assert_eq!(shell.content_id().as_deref(), Some("teamhq/hq_robots.gib"));
            }
            4 => {
                assert_eq!(shell.screen_id(), "botlab/botlabwin.gib");
                assert_eq!(shell.content_id().as_deref(), Some("botlab/overview.gib"));
            }
            5 => {
                assert_eq!(shell.tab(), Some("chassis button"));
                assert_eq!(shell.content_id().as_deref(), Some("botlab/chassis.gib"));
            }
            6 => {
                assert_eq!(shell.step(), Some("structure design button"));
                assert_eq!(
                    shell.content_id().as_deref(),
                    Some("botlab/chassis_structure1.gib")
                );
            }
            _ => {}
        }
    }
    assert_eq!(clicks, 6, "the route's clicks");
    // The chassis step's screen and its layer are in the content slot.
    let root = shell
        .document()
        .widgets()
        .next()
        .expect("the composed workshop");
    let slot = ra2ui::workshop::find(root, ra2ui::workshop::CONTENT_SLOT).unwrap();
    assert!(slot.children.iter().any(|w| w.name() == "chassis split"));
    assert!(slot
        .children
        .iter()
        .any(|w| w.name() == "steps and snap group"));
}

/// The exhibition route: main menu → exhibition setup → the arena picker, with
/// the list filled from the game's own arena scripts.
#[test]
fn the_arena_picker_is_the_screen_and_the_arenas_the_data_names() {
    let Some(game) = game_dir() else { return };
    let commands = route("exhibition.txt");
    let mut shell = Shell::boot(&game, [1280.0, 1005.0]).unwrap();
    assert_eq!(shell.screen_id(), "mainmenu/mainmenu.gib");

    let mut clicks = 0;
    for command in &commands {
        match command {
            Command::Click { x, y } => {
                shell.click(*x, *y).unwrap();
                clicks += 1;
            }
            Command::ClickNamed { name } => {
                shell.click_named(name).unwrap();
                clicks += 1;
            }
            Command::Wait { .. } | Command::Shot { .. } => {}
        }
        match clicks {
            1 => {
                assert_eq!(shell.screen_id(), "exhibition/exhib_setup.gib");
                assert_eq!(
                    shell.trace().last().unwrap().note.as_deref(),
                    Some("link to exhibition setup")
                );
            }
            2 => {
                assert_eq!(shell.screen_id(), "exhibition/exhib_setup.gib");
                assert_eq!(shell.content_id().as_deref(), Some("exhibition/arena.gib"));
                assert_eq!(
                    shell.trace().last().unwrap().note.as_deref(),
                    Some("click to load arena.gib into content group")
                );
            }
            _ => {}
        }
    }
    assert_eq!(clicks, 2);

    // The picker's rows are the arena scripts' own names, in their order.
    let arenas = ra2ui::arenas::all(&game).unwrap();
    assert_eq!(
        arenas.len(),
        12,
        "the picker offers the twelve stock arenas"
    );
    let root = shell.document().widgets().next().expect("the setup screen");
    let list = ra2ui::workshop::find(root, ra2ui::arenas::LIST).expect("the arena listbox");
    let rows: Vec<&str> = list
        .children
        .iter()
        .filter(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
        .filter_map(|c| c.field("_Text").map(|f| f.text()))
        .collect();
    // The setup screen opens on DEATHMATCH, so the picker lists that type's
    // arenas (the original's eight rows, Bridge of Doom first).
    let available = ra2ui::arenas::available(&game, "DEATHMATCH").unwrap();
    assert_eq!(
        rows,
        available
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        "one row per DEATHMATCH arena, in the order the game's own picker sorts"
    );
    assert!(rows.contains(&"Combat Arena"), "{rows:?}");
}

/// A pixel click works as well as a named one: the main menu's Exhibition button
/// is at 639,466 in the 1280x1005 design screen.
#[test]
fn a_click_at_the_menu_buttons_pixel_opens_its_screen() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1280.0, 1005.0]).unwrap();
    let step = shell.click(639.0, 466.0).unwrap();
    assert_eq!(step.screen, "exhibition/exhib_setup.gib");
    assert_eq!(step.note.as_deref(), Some("link to exhibition setup"));

    let mut shell = Shell::boot(&game, [1280.0, 1005.0]).unwrap();
    let step = shell.click(639.0, 544.0).unwrap();
    assert_eq!(step.screen, "teamhq/select_team.gib");
    assert_eq!(step.note.as_deref(), Some("link to team HQ"));
}

/// The discovery error names the files the shell needs, and a valid install
/// passes the check.
#[test]
fn the_game_directory_error_names_what_the_shell_needs() {
    let dir = std::env::temp_dir().join(format!("ra2-shell-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let error = game::check(&dir).unwrap_err();
    for name in game::REQUIRED {
        assert!(error.contains(name), "{error}");
    }
    std::fs::remove_dir(&dir).ok();
    if let Some(game) = game_dir() {
        game::check(&game).unwrap();
    }
}

/// A wrong `--game` is explained, not just rejected: a missing folder, a file
/// (the exe), a folder one level too high, each says what to do.
#[test]
fn a_wrong_game_path_says_what_to_do() {
    let root = std::env::temp_dir().join(format!("ra2-shell-wrong-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let install = root.join("Robot Arena 2");
    std::fs::create_dir_all(install.join("UI")).unwrap();
    std::fs::create_dir_all(install.join("Scripts")).unwrap();
    std::fs::write(install.join("RA2.cfz"), b"x").unwrap();
    std::fs::write(install.join("Robot Arena 2.exe"), b"x").unwrap();

    let missing = game::check(&root.join("nope")).unwrap_err();
    assert!(
        missing.contains("does not exist") && missing.contains("--game"),
        "{missing}"
    );
    let exe = game::check(&install.join("Robot Arena 2.exe")).unwrap_err();
    assert!(
        exe.contains("is a file") && exe.contains(&*install.to_string_lossy()),
        "{exe}"
    );
    let above = game::check(&root).unwrap_err();
    assert!(
        above.contains("one level down") && above.contains("RA2.cfz"),
        "{above}"
    );
    game::check(&install).unwrap();
    std::fs::remove_dir_all(&root).ok();
}

/// The arena tab's description is the selected arena's class docstring, in the
/// `Glyph` that carries the `DisplayBox` on `description grp`'s child (the
/// file's own "hammers" text is only the editor's placeholder).
#[test]
fn the_arena_description_follows_the_selected_row() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.click_named("exhibition button").unwrap();
    shell.click_named("arena button").unwrap();
    let arenas = ra2ui::arenas::available(&game, "DEATHMATCH").unwrap();
    let shown = |shell: &Shell| -> Vec<String> {
        let root = shell.document().widgets().next().unwrap();
        let group = ra2ui::workshop::find(root, "description grp").unwrap();
        let mut out = Vec::new();
        fn walk(w: &ra2fmt::gib::Widget, out: &mut Vec<String>) {
            if w.kind == ra2fmt::gib::WidgetKind::Glyph {
                if let Some(t) = w.field("_Text") {
                    out.push(t.text().to_string());
                }
            }
            for c in &w.children {
                walk(c, out);
            }
        }
        walk(group, &mut out);
        out
    };
    assert_eq!(arenas[0].name, "Bridge of Doom");
    assert!(arenas[0]
        .description
        .starts_with("Travel across the bridge"));
    assert_eq!(shown(&shell), vec![arenas[0].description.clone()]);
    // Row 1 (Combat Arena) is 18 px below row 0 in the 1024x768 layout.
    shell.click(320.0, 188.0).unwrap();
    assert_eq!(arenas[1].name, "Combat Arena");
    assert_eq!(shown(&shell), vec![arenas[1].description.clone()]);
    assert!(!shown(&shell)[0].contains("hammers and other nice"));
}

/// The robot picker's cells draw the snapshot each `.bot` carries, not the
/// "No Robot Preview" placeholder (`exhibition\sample_bot.bmp`).
#[test]
fn the_robot_picker_cells_carry_the_bots_snapshots() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    for name in [
        "exhibition button",
        "robots button",
        "computer player button",
    ] {
        shell.click_named(name).unwrap();
    }
    let keys: Vec<&String> = shell
        .previews()
        .keys()
        .filter(|k| k.starts_with("#bot-preview:picker-"))
        .collect();
    assert!(
        keys.len() >= 3,
        "one registered snapshot per robot of the first team: {keys:?}"
    );
}

/// The bottom bar's Help button opens the help window with the gib the exe's
/// help switch (`FUN_004e4ab0`) names for the screen, and its Close button
/// ends it.
#[test]
fn the_help_button_opens_the_screens_help_window() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.click_named("exhibition button").unwrap();
    let step = shell.click_named("Help button").unwrap();
    assert_eq!(step.screen, "help/helpframe.gib");
    assert!(
        step.note.as_deref().unwrap_or("").contains("help"),
        "{:?}",
        step.note
    );
    let doc = shell.document();
    let texts: Vec<String> = doc
        .walk()
        .filter_map(|w| w.field("_Text").map(|f| f.text().to_string()))
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("Exhibition")),
        "the exhibition help is in the frame: {texts:?}"
    );
    shell.click_named("close button").unwrap();
    assert_eq!(shell.screen_id(), "exhibition/exhib_setup.gib");

    // The bot lab: each tab has its own help (ids 4-9).
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    for name in [
        "Team HQ button",
        "continue button",
        "robots button",
        "workshop button",
    ] {
        shell.click_named(name).unwrap();
    }
    shell.click_named("wiring button").unwrap();
    let step = shell.click_named("Help button").unwrap();
    assert_eq!(step.screen, "help/helpframe.gib");
    assert!(
        step.note.as_deref().unwrap_or("").contains("(8)"),
        "{:?}",
        step.note
    );
}
