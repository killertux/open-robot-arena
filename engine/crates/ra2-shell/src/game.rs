//! Finding the game installation.
//!
//! The repository ships no game files: the user supplies an install and the
//! runner has to find it. `RA2_GAME_DIR` wins; otherwise the search walks up
//! from the working directory and from the crate looking for the `local/game`
//! directory this project is laid out around. A candidate is only accepted when
//! the three things the shell needs are there, and the error names them.

use std::path::{Path, PathBuf};

/// The files and directories the shell boots from, in the order the error names
/// them.
pub const REQUIRED: [&str; 3] = ["RA2.cfz", "UI", "Scripts"];

/// The environment variable that overrides the search.
pub const ENV: &str = "RA2_GAME_DIR";

/// The release the port is built and measured against: the final patch.
pub const SUPPORTED_VERSION: &str = "1.20";

/// Folders the game needs to be played (the shell alone boots without them, but
/// a match, the workshop or the audio do not).
pub const PLAYABLE: [&str; 5] = ["Components", "Arenas", "AI", "Sounds", "Music"];

/// The install's executable. The port never runs it, but it is the file a
/// player recognises their install folder by, so the messages name it.
pub const EXE: &str = "Robot Arena 2.exe";

fn has(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

fn is_install(dir: &Path) -> bool {
    REQUIRED.iter().all(|name| has(dir, name))
}

/// Where a Windows or Wine install usually is, for the "did you mean" hint and
/// the last-resort search.
fn usual_places() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut prefixes = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        prefixes.push(home.join(".wine/drive_c"));
        prefixes.push(home.join(".var/app/org.winehq.Wine/data/wine/drive_c"));
        prefixes.push(home.join(".local/share/wineprefixes/default/drive_c"));
    }
    prefixes.push(PathBuf::from("C:\\"));
    for prefix in prefixes {
        for programs in ["Program Files (x86)", "Program Files"] {
            for vendor in ["", "Infogrames", "Gabriel Interactive"] {
                let mut dir = prefix.join(programs);
                if !vendor.is_empty() {
                    dir = dir.join(vendor);
                }
                out.push(dir.join("Robot Arena 2"));
            }
        }
    }
    out
}

/// Check that a path is a game installation the shell can boot from. The
/// message says what is wrong (no such folder, a file, the wrong level of the
/// tree, or the files that are missing) and how to point the port at the
/// install.
pub fn check(path: &Path) -> Result<(), String> {
    let shown = path.display();
    let how = format!(
        "Point the port at the folder that holds {EXE}, {}, {} and {} \
         with --game DIR or the {ENV} environment variable.",
        REQUIRED[0], REQUIRED[1], REQUIRED[2]
    );
    if !path.exists() {
        return Err(format!("{shown} does not exist. {how}"));
    }
    if path.is_file() {
        let parent = path.parent().filter(|p| is_install(p));
        return Err(match parent {
            Some(dir) => format!(
                "{shown} is a file, not the install folder. Use its folder instead: {}",
                dir.display()
            ),
            None => format!("{shown} is a file, not the install folder. {how}"),
        });
    }
    if is_install(path) {
        return Ok(());
    }
    let missing: Vec<&str> = REQUIRED.iter().copied().filter(|n| !has(path, n)).collect();
    let mut hint = String::new();
    // One level down (the folder that contains the install) or up (a subfolder of it).
    let below = std::fs::read_dir(path).ok().and_then(|entries| {
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && is_install(p))
            .collect();
        dirs.sort();
        dirs.into_iter().next()
    });
    if let Some(inner) = below {
        hint = format!(" The install is one level down: {}.", inner.display());
    } else if let Some(parent) = path.parent().filter(|p| is_install(p)) {
        hint = format!(" The install is one level up: {}.", parent.display());
    }
    let exe = if has(path, EXE) {
        String::new()
    } else {
        format!(" ({EXE} is not there either.)")
    };
    Err(format!(
        "{shown} is not a Robot Arena 2 install the shell can boot: missing {}.{exe}{hint} {how}",
        missing.join(", ")
    ))
}

/// The version the install's `ReadMe.txt` states (`... version 1.20`), if it
/// states one.
pub fn version(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path.join("ReadMe.txt")).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    text.lines().take(40).find_map(|line| {
        let at = line.find("version ")? + "version ".len();
        let v: String = line[at..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        (v.contains('.') && v.chars().next().is_some_and(|c| c.is_ascii_digit())).then_some(v)
    })
}

/// Things that are wrong with an install that [`check`] accepts, as sentences
/// for the player: missing game folders, and a version other than the one the
/// port was measured against. They are warnings: the port still starts.
pub fn warnings(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let missing: Vec<&str> = PLAYABLE.iter().copied().filter(|n| !has(path, n)).collect();
    if !missing.is_empty() {
        out.push(format!(
            "{} has no {}: the menus start, but matches, the workshop or sound will fail. \
             Point --game at a complete install (the whole folder, not only RA2.cfz, UI and Scripts).",
            path.display(),
            missing.join(", ")
        ));
    }
    match version(path) {
        Some(v) if v == SUPPORTED_VERSION => {}
        Some(v) => out.push(format!(
            "this install says it is version {v}; the port is built and measured against \
             version {SUPPORTED_VERSION}, the final patch. Other versions may play differently \
             or fail to load. Install the {SUPPORTED_VERSION} patch for your copy."
        )),
        None if has(path, "ReadMe.txt") => {}
        None => out.push(format!(
            "no ReadMe.txt in {}, so the game version could not be checked; the port expects \
             version {SUPPORTED_VERSION}, the final patch.",
            path.display()
        )),
    }
    out
}

/// The game directory: `RA2_GAME_DIR`, else `local/game` above the working
/// directory or this crate, else the places a Windows/Wine install usually
/// lives, else a clear error naming every place tried.
pub fn find() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(ENV) {
        let dir = PathBuf::from(dir);
        check(&dir).map_err(|e| format!("{ENV}: {e}"))?;
        return Ok(dir);
    }
    let mut tried: Vec<PathBuf> = Vec::new();
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    for start in starts {
        let mut dir = start.as_path();
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() && check(&candidate).is_ok() {
                return Ok(candidate);
            }
            if !tried.contains(&candidate) {
                tried.push(candidate);
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }
    let usual = usual_places();
    for candidate in &usual {
        if candidate.is_dir() && is_install(candidate) {
            return Ok(candidate.clone());
        }
    }
    // The upward walk repeats itself per level; show only the nearest few.
    let near: Vec<String> = tried
        .iter()
        .take(3)
        .map(|p| format!("  {}", p.display()))
        .collect();
    let usual: Vec<String> = usual
        .iter()
        .take(2)
        .map(|p| format!("  {}", p.display()))
        .collect();
    Err(format!(
        "no Robot Arena 2 install found. The port needs your own copy of the game: a folder holding \
         {EXE}, {}, {}/ and {}/. Tell it where with --game DIR or {ENV}=DIR (the folder is remembered after a windowed run). \
         Looked in: {ENV} (not set), local/game above the working directory, e.g.\n{}\n and the usual Windows/Wine places, e.g.\n{}",
        REQUIRED[0],
        REQUIRED[1],
        REQUIRED[2],
        near.join("\n"),
        usual.join("\n")
    ))
}

/// Every `.gib` under a UI directory, relative to it with `/` separators.
pub fn screens(ui_root: &Path) -> Result<Vec<String>, String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| format!("{e}"))?.path();
            if path.is_dir() {
                walk(root, &path, out)?;
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gib"))
            {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(ui_root, ui_root, &mut out)?;
    out.sort();
    Ok(out)
}

/// Find one screen by a name a `Notes` field gives.
///
/// A note may name a whole path (`exhibition\arena.gib`) or only the file
/// (`arena.gib`). The first is looked up case-insensitively under `UI`; the
/// second is searched for across the shipped screens.
pub fn screen(ui_root: &Path, name: &str) -> Option<(PathBuf, String)> {
    let name = name.replace('\\', "/");
    let name = name.strip_prefix("UI/").unwrap_or(&name).trim();
    if name.contains('/') {
        let path = ra2fmt::find_ci(ui_root, name)?;
        let key = path
            .strip_prefix(ui_root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        return Some((path, key));
    }
    let want = name.to_ascii_lowercase();
    let mut matches = Vec::new();
    for rel in screens(ui_root).ok()? {
        let file = rel.rsplit('/').next().unwrap_or(&rel);
        if file.eq_ignore_ascii_case(&want) {
            matches.push(rel);
        }
    }
    // A bare name that the game ships more than once is not guessed: the note
    // itself was ambiguous and the caller has to name the directory.
    if matches.len() != 1 {
        return None;
    }
    let key = matches.pop()?;
    let path = ra2fmt::find_ci(ui_root, &key)?;
    Some((path, key))
}
