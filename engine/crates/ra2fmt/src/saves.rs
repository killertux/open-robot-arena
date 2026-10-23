//! The per-user save location: an overlay over the game install.
//!
//! Careers, teams, robot designs, custom textures and settings are the
//! player's, not the install's. Reads look in the save directory first and
//! then in the install; every write goes only to the save directory, under the
//! same relative path and in the same file format the install uses (so a user
//! can copy saves between this client and the original game). A deletion
//! never touches the install: a file that exists only in the install is hidden
//! by a line in `deleted.txt` at the save directory's root.
//!
//! The directory is, in order: a binding made with [`bind`] for one install
//! (tests, scratch runs), the `--save-dir` the app configured with
//! [`configure`], `ORA_SAVE_DIR`, then the platform default
//! (`$XDG_DATA_HOME/open-robot-arena` or `~/.local/share/open-robot-arena`;
//! `%APPDATA%\open-robot-arena`; `~/Library/Application Support/open-robot-arena`).
//! Binding an install to itself gives the old in-place behaviour.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The tombstone list at the save directory's root, one `/`-separated path per line.
pub const TOMBSTONES: &str = "deleted.txt";
/// The environment variable that overrides the platform default.
pub const ENV: &str = "ORA_SAVE_DIR";

static BINDINGS: Mutex<Vec<(PathBuf, PathBuf)>> = Mutex::new(Vec::new());
static CONFIGURED: Mutex<Option<PathBuf>> = Mutex::new(None);
static ISOLATED: Mutex<Option<PathBuf>> = Mutex::new(None);

fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Use `save` as the save directory for the install `game` (rather than the
/// configured one). `bind(dir, dir)` makes an install keep its own saves.
pub fn bind(game: &Path, save: &Path) {
    let game = key(game);
    let mut bindings = BINDINGS.lock().unwrap();
    bindings.retain(|(g, _)| *g != game);
    bindings.push((game, key(save)));
}

/// Set the save directory used for installs with no binding (`--save-dir`).
pub fn configure(save: Option<PathBuf>) {
    *CONFIGURED.lock().unwrap() = save;
}

/// For tests that read the real install: use an empty save directory of this
/// process (never created unless something writes to it) instead of
/// `ORA_SAVE_DIR` or the user's platform default, so a result does not depend on
/// the player's saves. A binding ([`bind`]) or `--save-dir` ([`configure`]) still
/// wins. Idempotent; call it from the test's install lookup.
pub fn isolate_for_tests() {
    let mut iso = ISOLATED.lock().unwrap();
    if iso.is_none() {
        *iso = Some(std::env::temp_dir().join(format!("ora-test-save-{}", std::process::id())));
    }
}

/// The platform default save directory.
pub fn default_dir() -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".local").join("share")))
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("open-robot-arena")
}

/// The save directory for the install `game`.
pub fn save_dir(game: &Path) -> PathBuf {
    let k = key(game);
    if let Some((_, save)) = BINDINGS.lock().unwrap().iter().find(|(g, _)| *g == k) {
        return save.clone();
    }
    if let Some(dir) = CONFIGURED.lock().unwrap().clone() {
        return dir;
    }
    if let Some(dir) = ISOLATED.lock().unwrap().clone() {
        return dir;
    }
    if let Some(dir) = std::env::var_os(ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    default_dir()
}

fn in_place(game: &Path) -> bool {
    key(&save_dir(game)) == key(game)
}

/// A relative path as the tombstone list spells it.
fn norm(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn tombstones(game: &Path) -> Vec<String> {
    std::fs::read_to_string(save_dir(game).join(TOMBSTONES))
        .map(|t| {
            t.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn write_tombstones(game: &Path, lines: &[String]) -> io::Result<()> {
    let path = save_dir(game).join(TOMBSTONES);
    if lines.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, lines.join("\n") + "\n")
}

fn hidden(lines: &[String], rel: &str) -> bool {
    lines.iter().any(|l| {
        l == rel
            || rel
                .strip_prefix(l.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })
}

/// Whether `rel` (relative to the install) was deleted by the player.
pub fn is_deleted(game: &Path, rel: &Path) -> bool {
    !in_place(game) && hidden(&tombstones(game), &norm(rel))
}

/// The path to read `rel` from: the save copy when there is one, otherwise the
/// install's. A deleted install file resolves to the (absent) save path.
pub fn resolve(game: &Path, rel: impl AsRef<Path>) -> PathBuf {
    let rel = rel.as_ref();
    let saved = save_dir(game).join(rel);
    if in_place(game) || saved.exists() || is_deleted(game, rel) {
        return saved;
    }
    game.join(rel)
}

/// The path a write of `rel` goes to (always in the save directory).
pub fn target(game: &Path, rel: impl AsRef<Path>) -> PathBuf {
    save_dir(game).join(rel)
}

/// Map a path under the install to the same place in the save directory; a
/// path outside the install is returned unchanged.
pub fn redirect(game: &Path, path: &Path) -> PathBuf {
    match path.strip_prefix(game) {
        Ok(rel) => target(game, rel),
        Err(_) => match (key(path).strip_prefix(key(game)), path.is_absolute()) {
            (Ok(rel), true) => target(game, rel),
            _ => path.to_path_buf(),
        },
    }
}

/// Prepare a write to `path` (a path under the install, or already in the save
/// directory): the redirected path, its folder created, and a tombstone for it
/// lifted. Every write the client does goes through this.
pub fn prepare_write(game: &Path, path: &Path) -> io::Result<PathBuf> {
    let out = redirect(game, path);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !in_place(game) {
        if let Ok(rel) = out.strip_prefix(save_dir(game)) {
            let rel = norm(rel);
            let lines = tombstones(game);
            if lines.contains(&rel) {
                let kept: Vec<String> = lines.into_iter().filter(|l| *l != rel).collect();
                write_tombstones(game, &kept)?;
            }
        }
    }
    Ok(out)
}

/// Write `bytes` to `path` (under the install) in the save directory.
pub fn write(game: &Path, path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<PathBuf> {
    let out = prepare_write(game, path)?;
    std::fs::write(&out, bytes)?;
    Ok(out)
}

/// Copy `from` (read through the overlay) to `to` (written to the save dir).
pub fn copy(game: &Path, from: &Path, to: &Path) -> io::Result<()> {
    let out = prepare_write(game, to)?;
    let src = if from.starts_with(game) {
        resolve(game, from.strip_prefix(game).unwrap())
    } else {
        from.to_path_buf()
    };
    if src != out {
        std::fs::copy(src, out)?;
    }
    Ok(())
}

/// Delete `rel`: the save copy is removed, and an install file stays where it
/// is but is hidden from now on.
pub fn remove(game: &Path, rel: impl AsRef<Path>) -> io::Result<()> {
    let rel = rel.as_ref();
    let saved = save_dir(game).join(rel);
    match std::fs::remove_file(&saved) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    if !in_place(game) && game.join(rel).exists() {
        let mut lines = tombstones(game);
        let n = norm(rel);
        if !lines.contains(&n) {
            lines.push(n);
            write_tombstones(game, &lines)?;
        }
    }
    Ok(())
}

/// The file names in `rel` (a folder): the save folder's and the install's,
/// without the ones the player deleted.
pub fn list(game: &Path, rel: impl AsRef<Path>) -> Vec<String> {
    let rel = rel.as_ref();
    let mut names: Vec<String> = Vec::new();
    for dir in [save_dir(game).join(rel), game.join(rel)] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !names.contains(&name) && !is_deleted(game, &rel.join(&name)) {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("ra2saves-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (game, save) = (base.join("game"), base.join("save"));
        std::fs::create_dir_all(game.join("Teams")).unwrap();
        std::fs::write(game.join("Teams/Teams.txt"), "install").unwrap();
        bind(&game, &save);
        (game, save)
    }

    #[test]
    fn reads_overlay_and_writes_go_to_save() {
        let (game, save) = scratch("rw");
        let p = game.join("Teams/Teams.txt");
        assert_eq!(resolve(&game, "Teams/Teams.txt"), p);
        write(&game, &p, "mine").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "install");
        assert_eq!(
            resolve(&game, "Teams/Teams.txt"),
            save.join("Teams/Teams.txt")
        );
        assert_eq!(
            std::fs::read_to_string(resolve(&game, "Teams/Teams.txt")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn delete_hides_install_file_until_rewritten() {
        let (game, save) = scratch("del");
        remove(&game, "Teams/Teams.txt").unwrap();
        assert!(game.join("Teams/Teams.txt").is_file());
        assert!(!resolve(&game, "Teams/Teams.txt").exists());
        assert!(!list(&game, "Teams").contains(&"Teams.txt".to_string()));
        assert_eq!(
            std::fs::read_to_string(save.join(TOMBSTONES)).unwrap(),
            "Teams/Teams.txt\n"
        );
        write(&game, &game.join("Teams/Teams.txt"), "back").unwrap();
        assert!(!save.join(TOMBSTONES).exists());
        assert!(list(&game, "Teams").contains(&"Teams.txt".to_string()));
    }

    #[test]
    fn in_place_binding_is_the_old_behaviour() {
        let (game, _) = scratch("inplace");
        bind(&game, &game);
        let p = game.join("Teams/Teams.txt");
        write(&game, &p, "x").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "x");
        remove(&game, "Teams/Teams.txt").unwrap();
        assert!(!p.exists());
    }

    #[test]
    fn isolated_tests_ignore_the_env_and_the_platform_default() {
        let game = std::env::temp_dir().join(format!("ra2saves-{}-iso/game", std::process::id()));
        isolate_for_tests();
        let dir = save_dir(&game);
        assert!(dir.starts_with(std::env::temp_dir()));
        assert_ne!(dir, default_dir());
        assert!(!dir.exists(), "isolation must not create the directory");
        // an explicit --save-dir still wins
        configure(Some(PathBuf::from("/explicit")));
        assert_eq!(save_dir(&game), PathBuf::from("/explicit"));
        configure(None);
    }
}
