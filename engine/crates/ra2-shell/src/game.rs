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

/// Check that a path is a game installation the shell can boot from.
pub fn check(path: &Path) -> Result<(), String> {
    let mut missing = Vec::new();
    for name in REQUIRED {
        if !path.join(name).exists() {
            missing.push(name);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} is not a Robot Arena 2 install the shell can boot: missing {}",
        path.display(),
        missing.join(", ")
    ))
}

/// The game directory: `RA2_GAME_DIR`, else `local/game` above the working
/// directory or this crate, else a clear error.
pub fn find() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(ENV) {
        let dir = PathBuf::from(dir);
        check(&dir)?;
        return Ok(dir);
    }
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
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }
    Err(format!(
        "no Robot Arena 2 install found (set {ENV}). The shell needs a directory holding {}, UI/ and Scripts/; \
         the repository's usual place is local/game beside the workspace.",
        REQUIRED[0]
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
