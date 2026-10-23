//! The save directory takes every write: a career save, a team created,
//! edited and deleted, and a robot saved all run against a read-only copy of
//! an install, and nothing in the install changes.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn set_mode(path: &Path, readonly: bool) {
    let meta = std::fs::symlink_metadata(path).unwrap();
    if meta.file_type().is_symlink() {
        return;
    }
    if meta.is_dir() {
        // Directories first when making them writable, last when locking.
        if !readonly {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        for entry in std::fs::read_dir(path).unwrap().flatten() {
            set_mode(&entry.path(), readonly);
        }
        if readonly {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).unwrap();
        }
    } else {
        let mode = if readonly { 0o444 } else { 0o644 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
}

/// Every path under `dir` with its size, mtime and bytes.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (u64, SystemTime, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (u64, SystemTime, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let meta = std::fs::metadata(&path).unwrap();
            if meta.is_dir() {
                out.insert(path.clone(), (0, meta.modified().unwrap(), Vec::new()));
                walk(&path, out);
            } else {
                out.insert(
                    path.clone(),
                    (
                        meta.len(),
                        meta.modified().unwrap(),
                        std::fs::read(&path).unwrap(),
                    ),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &mut out);
    out
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.exists() {
            set_mode(&self.0, false);
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn career_team_and_robot_saves_leave_a_read_only_install_untouched() {
    let Some(real) = game_dir() else { return };
    let root = std::env::temp_dir().join(format!("ra2-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let scratch = Scratch(root.clone());
    let (install, save) = (root.join("install"), root.join("save"));
    for name in ["Teams", "AI", "Robot Designs"] {
        if real.join(name).is_dir() {
            copy_tree(&real.join(name), &install.join(name));
        }
    }
    std::fs::create_dir_all(install.join("AI")).unwrap();
    for name in ["Events.txt", "ra2.cfg"] {
        if real.join(name).is_file() {
            std::fs::copy(real.join(name), install.join(name)).unwrap();
        }
    }
    // The shipped AI folders are scripts and robots, not tables; only the
    // table and a team's robots matter here.
    set_mode(&install, true);
    let before = snapshot(&install);
    ra2fmt::saves::bind(&install, &save);

    // A career save: both tables and the sidecar.
    let career = ra2sim::career::Career::load(&install).expect("the career loads");
    career.save(&install).expect("the career saves");
    assert!(save.join("Teams/Teams.txt").is_file());
    assert!(save.join("AI/Teams.txt").is_file());
    assert!(save.join("Teams/career.dat").is_file());

    // Team create, edit and delete.
    let slot = ra2ui::teams::create_team(&install, "Saved Team", "A motto", "Teams\\star.bmp")
        .unwrap()
        .expect("an empty team slot");
    ra2ui::teams::update_team(&install, slot, "Renamed", "Another", "Teams\\star.bmp").unwrap();
    let named = |side| {
        ra2ui::teams::rosters(&install, side)
            .unwrap()
            .into_iter()
            .filter(|r| r.named)
            .map(|r| r.name)
            .collect::<Vec<_>>()
    };
    assert!(named(ra2ui::teams::Side::Player).contains(&"Renamed".to_string()));

    // A robot saved into the new team: the file lands in the save directory.
    let path = ra2ui::teams::add_robot(&install, slot).unwrap();
    assert!(path.starts_with(&save), "{}", path.display());
    let bot = std::fs::read(real.join("Teams/Team0/Bot0.bot"))
        .or_else(|_| std::fs::read(real.join("AI/Team1/Bot0.bot")));
    if let Ok(bot) = bot {
        let written = ra2fmt::saves::write(&install, &path, bot).unwrap();
        assert!(written.is_file() && written.starts_with(&save));
    }

    // Deleting the shipped team hides it without touching the install.
    ra2ui::teams::delete_team(&install, 0).unwrap();
    assert!(!ra2ui::teams::rosters(&install, ra2ui::teams::Side::Player).unwrap()[0].named);
    // A design saved through the overlay, and a deleted install file.
    let design = install.join("Robot Designs").join("mine.bot");
    ra2fmt::saves::write(&install, &design, b"design").unwrap();
    assert!(save.join("Robot Designs/mine.bot").is_file());
    assert!(ra2fmt::saves::list(&install, "Robot Designs").contains(&"mine.bot".to_string()));
    ra2fmt::saves::remove(&install, "Teams/career.dat").unwrap();

    let after = snapshot(&install);
    assert_eq!(before, after, "the install changed");
    drop(scratch);
}
