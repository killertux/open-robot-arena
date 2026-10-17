//! The packaging metadata, checked against the runner it ships.
//!
//! The tarball itself is exercised by `tools/smoke_package.sh` (it unpacks the
//! artifact and runs the shell); these tests keep the *sources* the package is
//! built from honest: the launcher names the files `ra2-shell` requires, the
//! package script copies every binary and licence, and the player-facing text
//! states the Python requirement and what is missing.

use std::path::{Path, PathBuf};

use ra2_shell::game;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn read(relative: &str) -> String {
    let path = repo().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The launcher's search and error text must agree with the runner's own
/// constants: the same environment variable, and all three required names.
/// The package script ships the entry point and the four tools; the smoke
/// script drives the two routes and the boot-frame comparison.
#[test]
fn the_package_ships_the_runner_and_its_required_files() {
    let launcher = read("packaging/run.sh");
    assert!(
        launcher.contains(game::ENV),
        "the launcher must honour {}",
        game::ENV
    );
    for name in game::REQUIRED {
        assert!(
            launcher.contains(name),
            "the launcher never names {name}, which ra2-shell requires"
        );
    }
    assert!(launcher.contains("--check-python"));
    assert!(launcher.contains("libpython3.12.so.1.0"));

    let package = read("tools/package.sh");
    for bin in ["ra2-shell", "ra2-render", "ra2-sim", "ra2-duel", "ra2"] {
        assert!(
            package.contains(bin),
            "tools/package.sh does not ship {bin}"
        );
    }
    for notice in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "THIRD_PARTY.md"] {
        assert!(
            package.contains(notice),
            "tools/package.sh does not copy {notice}"
        );
    }

    let smoke = read("tools/smoke_package.sh");
    for frame in [
        "00-mainmenu.png",
        "01-select-team.png",
        "02-team-hq.png",
        "03-robot-inventory.png",
        "04-botlab-overview.png",
        "05-botlab-chassis.png",
        "06-chassis-step1.png",
    ] {
        assert!(smoke.contains(frame), "the smoke test never checks {frame}");
    }
    assert!(
        smoke.contains("uishot"),
        "the boot frame is not compared with an independent render"
    );
    assert!(smoke.contains("RA2_GAME_DIR"));
}

/// The player's README and the desktop entry are part of the deliverable, and
/// the licence files the Cargo manifests promise actually exist.
#[test]
fn the_player_text_names_the_game_the_python_and_the_licence() {
    let readme = read("packaging/README.txt.in");
    for name in game::REQUIRED {
        assert!(readme.contains(name), "README.txt does not name {name}");
    }
    assert!(readme.contains("CPython 3.12"));
    assert!(readme.contains("libpython3.12"));
    assert!(
        readme.contains("does NOT include the game"),
        "README.txt must say the game is not included"
    );

    let desktop = read("packaging/ra2-open-robot-arena.desktop");
    assert!(desktop.contains("[Desktop Entry]"));
    assert!(desktop.contains("Exec=ra2-open-robot-arena"));
    assert!(desktop.contains("Icon=ra2-open-robot-arena"));
    assert!(desktop.contains("Type=Application"));

    assert!(repo().join("packaging/ra2-open-robot-arena.svg").is_file());
    assert!(repo().join("LICENSE-MIT").is_file());
    assert!(repo().join("LICENSE-APACHE").is_file());
    let notice = read("NOTICE");
    assert!(notice.contains("MIT"));
    assert!(notice.contains("Apache"));
    assert!(notice.contains("NOT part of this project"));
}
