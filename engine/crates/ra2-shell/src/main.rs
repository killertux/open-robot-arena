//! The shell's entry point: boot the main menu and drive it headlessly.
//!
//! ```text
//! cargo run -q -p ra2-shell -- --route botlab --out ra2-shell-frames
//! cargo run -q -p ra2-shell -- --script my-input.txt --out ra2-shell-frames
//! ```
//!
//! The game is found through `RA2_GAME_DIR` or the repository's `local/game`;
//! the boot screen and every transition come from the shipped `.gib` files (see
//! [`ra2_shell::nav`]). The shipped `Scripts/Main.py` is booted through
//! `ra2script` first, so the script layer behind the shell is the game's own;
//! `--no-python` skips that when only the screens are wanted.

use std::path::{Path, PathBuf};

use ra2_shell::game;
use ra2_shell::script::{self, Command};
use ra2_shell::shell::Shell;

/// The two routes the runner ships.
const BOTLAB: &str = include_str!("../routes/botlab.txt");
const EXHIBITION: &str = include_str!("../routes/exhibition.txt");

fn usage() -> String {
    "usage: ra2-shell [--game DIR] (--route botlab|exhibition | --script FILE) \
     [--out DIR] [--size WxH] [--no-python]\n\
     \n\
     The routes are scripted input files: click x y (or click \"Widget Name\"), \
     wait n, shot name.png."
        .to_string()
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ra2-shell: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut game_dir: Option<PathBuf> = None;
    let mut script_path: Option<PathBuf> = None;
    let mut route: Option<String> = None;
    // The system temp directory, not `/tmp`: the latter is a drive-root path on
    // Windows. The default is otherwise the same on every platform.
    let mut out = std::env::temp_dir().join("ra2-shell");
    let mut size = [1280.0f32, 1005.0];
    let mut python = true;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game" => {
                game_dir = Some(PathBuf::from(next(&mut args, "--game")?));
            }
            "--script" => {
                script_path = Some(PathBuf::from(next(&mut args, "--script")?));
            }
            "--route" => {
                route = Some(next(&mut args, "--route")?);
            }
            "--out" => {
                out = PathBuf::from(next(&mut args, "--out")?);
            }
            "--size" => {
                let value = next(&mut args, "--size")?;
                size = parse_size(&value)?;
            }
            "--no-python" => python = false,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            other => return Err(format!("unknown argument {other:?}\n{}", usage())),
        }
    }

    let text = match (&route, &script_path) {
        (Some(route), None) => match route.as_str() {
            "botlab" => BOTLAB.to_string(),
            "exhibition" => EXHIBITION.to_string(),
            other => {
                return Err(format!(
                    "unknown route {other:?}; the shipped routes are botlab and exhibition"
                ))
            }
        },
        (None, Some(path)) => {
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
        }
        (Some(_), Some(_)) => return Err("pass --route or --script, not both".into()),
        (None, None) => return Err(usage()),
    };
    let commands = script::parse(&text)?;

    // The interpreter changes the process's working directory to the game root,
    // exactly as the original did, so every path we keep is made absolute first.
    let out = absolute(&out)?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let game = match game_dir {
        Some(dir) => {
            game::check(&dir)?;
            dir.canonicalize()
                .map_err(|e| format!("{}: {e}", dir.display()))?
        }
        None => game::find()?,
    };

    if python {
        boot_python(&game)?;
    }

    let mut shell = Shell::boot(&game, size)?;
    println!(
        "booted {} from {} ({}x{})",
        ra2_shell::nav::BOOT_SCREEN,
        game.display(),
        size[0],
        size[1]
    );
    for command in commands {
        match command {
            Command::Click { x, y } => {
                let step = shell.click(x, y)?;
                report(&step);
            }
            Command::ClickNamed { name } => {
                let step = shell.click_named(&name)?;
                report(&step);
            }
            Command::Wait { ticks } => {
                println!("wait {ticks}: the shell has no time-dependent drawing");
            }
            Command::Shot { name } => {
                let path = out.join(&name);
                let shot = shell.shot(&path)?;
                println!(
                    "shot {} ({}x{}, {} quads over {} textures{})",
                    shot.path.display(),
                    shot.size[0],
                    shot.size[1],
                    shot.quads,
                    shot.textures,
                    if shot.missing == 0 {
                        String::new()
                    } else {
                        format!(", {} missing", shot.missing)
                    }
                );
            }
        }
    }
    println!(
        "finished on {} (content: {})",
        shell.screen_id(),
        shell.content_id().unwrap_or_else(|| "-".into())
    );
    Ok(())
}

/// Boot the shipped `Scripts/Main.py`, the same entry point the game runs. That
/// file imports every arena and brain and registers `dispatchEvent`; the screens
/// themselves are the data this runner then places.
fn boot_python(game: &Path) -> Result<(), String> {
    let _guard = ra2script::lock();
    let host = ra2script::ScriptHost::boot(game).map_err(|e| e.to_string())?;
    pyo3::Python::with_gil(|py| host.boot_shell(py))
        .map_err(|e| format!("Scripts/Main.py: {e}"))?;
    println!("the shipped Scripts/Main.py booted");
    Ok(())
}

fn report(step: &ra2_shell::shell::Step) {
    println!(
        "  {} -> {} [{}] at {:.1},{:.1}{}",
        step.command,
        step.screen,
        if step.hit.is_empty() {
            "nothing"
        } else {
            &step.hit
        },
        step.point[0],
        step.point[1],
        match (&step.note, step.content.as_deref()) {
            (_, Some(content)) => format!(", content {content}"),
            (Some(note), None) => format!(", note {note:?}"),
            (None, None) => String::new(),
        }
    );
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value\n{}", usage()))
}

fn parse_size(value: &str) -> Result<[f32; 2], String> {
    let (w, h) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("--size wants WxH, got {value:?}"))?;
    let w = w
        .parse()
        .map_err(|_| format!("--size width is not a number: {w:?}"))?;
    let h = h
        .parse()
        .map_err(|_| format!("--size height is not a number: {h:?}"))?;
    Ok([w, h])
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    Ok(cwd.join(path))
}
