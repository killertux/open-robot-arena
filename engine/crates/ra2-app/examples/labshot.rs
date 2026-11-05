//! Drive our Bot Lab with the same step script the live oracle gets (`ra2_drive.py` steps),
//! so side-by-sides compare the same clicks:
//!
//! ```text
//! labshot <scratch_game> <cell|-> <steps...>
//! ```
//!
//! `-f FILE` (before the game) reads the steps from a route file instead, the very files `ra2_drive.py -f` gets
//! (`tools/oracle/routes/botlab/*.steps`, several files are read in order): `wait`, `hide` and `frames` are dropped,
//! `dragh X0 Y0 X1 Y1 [BUTTON] [STEPS] [HOLD]` becomes `drag`, `SHOTS/` in a `shot` path is the `LABSHOT_SHOTS`
//! directory, and `key NAME down|up` is a held robot key (W 17, A 30, S 31, D 32, space 57, the arrows).
//!
//! Steps: `click X Y`, `move X Y`, `shot PATH`, `key C` (a typed character, `Return`,
//! `BackSpace`), `status`, `back` (Back to HQ: saves into the team) and `tab`. The scratch
//! game must be a copy made by `tools/labgame.sh`; `ORA_SAVE_DIR` is set to it here.

use std::path::{Path, PathBuf};

fn main() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    while args.first().is_some_and(|a| a == "-f") {
        files.push(args.get(1).cloned().ok_or("-f needs a file")?);
        args.drain(..2);
    }
    if args.len() < 2 {
        return Err("usage: labshot <scratch_game> <cell|-> <steps...>".into());
    }
    let game = PathBuf::from(&args[0]);
    std::env::set_var("ORA_SAVE_DIR", &game);
    ra2fmt::saves::bind(&game, &game);
    let cell = args[1].parse::<usize>().ok();
    ra2_app::hq_mode::select_team(&game, 0);
    let mut builder = ra2_app::flow::open_workshop(&game, cell)?;
    builder.set_size([1024.0, 768.0])?;
    let mut route = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?;
        route.extend(route_tokens(&text));
    }
    // the route files' steps come first, then the steps on the command line
    let rest = args.split_off(2);
    args.extend(route);
    args.extend(rest);
    let mut i = 2;
    let num = |s: &String| s.parse::<f32>().map_err(|e| format!("{s}: {e}"));
    while i < args.len() {
        match args[i].as_str() {
            "click" => {
                let (x, y) = (num(&args[i + 1])?, num(&args[i + 2])?);
                builder.mouse_moved(x, y);
                builder.click(x, y)?;
                // the window sends the release too (a held picker or slider would otherwise follow the
                // next click)
                builder.mouse_released(x, y);
                i += 3;
            }
            "move" => {
                builder.mouse_moved(num(&args[i + 1])?, num(&args[i + 2])?);
                i += 3;
            }
            "drag" => {
                // `drag X0 Y0 X1 Y1 STEPS [HOLD]`: press on (X0,Y0), move to (X1,Y1) over STEPS mouse
                // events (positions rounded to whole pixels as `ra2_drive.py dragh` does), HOLD more
                // events at the end point, release. Chassis step 2's point drag.
                let (x0, y0, x1, y1) = (
                    num(&args[i + 1])?,
                    num(&args[i + 2])?,
                    num(&args[i + 3])?,
                    num(&args[i + 4])?,
                );
                let steps = num(&args[i + 5])? as usize;
                let hold = args
                    .get(i + 6)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let used = if args
                    .get(i + 6)
                    .and_then(|s| s.parse::<usize>().ok())
                    .is_some()
                {
                    7
                } else {
                    6
                };
                builder.mouse_moved(x0, y0);
                builder.click(x0, y0)?;
                for k in 1..=steps {
                    let f = k as f32 / steps as f32;
                    builder.mouse_moved((x0 + (x1 - x0) * f).round(), (y0 + (y1 - y0) * f).round());
                }
                for _ in 0..hold {
                    builder.mouse_moved(x1, y1);
                }
                builder.mouse_released(x1, y1);
                i += used;
            }
            "alert" => {
                println!("alert: {:?}", builder.alert());
                i += 1;
            }
            "cam" => {
                // `cam PITCH YAW [DIST]`: the lab camera (live: written into the tab's camera object)
                let dist = args.get(i + 3).and_then(|s| s.parse::<f32>().ok());
                builder.set_lab_cam(ra2_app::build_mode::LabCam {
                    pitch: num(&args[i + 1])?,
                    yaw: num(&args[i + 2])?,
                    dist: dist.unwrap_or(builder.lab_cam().dist),
                });
                i += if dist.is_some() { 4 } else { 3 };
            }
            "hit" => {
                for line in builder.debug_hit(num(&args[i + 1])?, num(&args[i + 2])?) {
                    println!("{line}");
                }
                i += 3;
            }
            "tick" => {
                // `tick SECONDS`: the practice arena runs that much wall time in 1/60 s slices.
                let seconds = num(&args[i + 1])?;
                let mut left = seconds;
                while left > 0.0 {
                    let slice = left.min(1.0 / 60.0);
                    builder.advance(slice);
                    left -= slice;
                }
                i += 2;
            }
            "keys" => {
                // `keys DOWN|UP SCANCODE`: a robot key (W 17, A 30, S 31, D 32, Space 57).
                let mut input = builder.drive();
                input.keys.set(
                    args[i + 2].parse::<i32>().map_err(|e| e.to_string())?,
                    args[i + 1] == "down",
                );
                builder.set_drive(input);
                i += 3;
            }
            "cues" => {
                // `cues`: the practice arena's sound cues since the last call.
                if let Some((cues, _)) = builder.take_arena_cues() {
                    for cue in cues {
                        println!("cue {cue:?}");
                    }
                }
                i += 1;
            }
            "arenashot" => {
                let size = [1024u32, 580];
                let pixels = builder.arena_frame(size)?;
                image::RgbaImage::from_raw(size[0], size[1], pixels)
                    .ok_or("bad frame")?
                    .save(&args[i + 1])
                    .map_err(|e| e.to_string())?;
                i += 2;
            }
            "where" => {
                if let Some(arena) = builder.arena_mut() {
                    let robot = &arena.matched.duel.robots[0];
                    let p = robot.position(&*arena.matched.duel.physics);
                    println!(
                        "robot at {:.3} {:.3} {:.3} t={:.2}",
                        p.x,
                        p.y,
                        p.z,
                        arena.matched.clock()
                    );
                    let c = &arena.matched.duel.scene_camera;
                    println!(
                        "camera {:?} eye {:?} rot {:?} fov {:.4} range {:?}",
                        c.kind, c.position, c.rotation, c.fov, c.fov_range
                    );
                }
                i += 1;
            }
            "shot" => {
                builder.shot(Path::new(&args[i + 1]), [1024, 768])?;
                i += 2;
            }
            "key" if matches!(args.get(i + 2).map(String::as_str), Some("down" | "up")) => {
                // `key NAME down|up`: a robot key held or let go (the practice arena reads it)
                let code =
                    key_code(&args[i + 1]).ok_or_else(|| format!("no key {}", args[i + 1]))?;
                let mut input = builder.drive();
                input.keys.set(code, args[i + 2] == "down");
                builder.set_drive(input);
                i += 3;
            }
            "key" => {
                match args[i + 1].as_str() {
                    "Return" => {
                        builder.enter()?;
                    }
                    "BackSpace" => builder.backspace(),
                    "space" => builder.type_char(' '),
                    s => s.chars().for_each(|c| builder.type_char(c)),
                }
                i += 2;
            }
            "back" => {
                builder.click(55.0, 18.0)?;
                let saved = ra2_app::flow::close_workshop(
                    &mut builder,
                    &mut ra2_shell::Shell::boot(&game, [1024.0, 768.0])?,
                )?;
                println!("back: saved {saved:?}");
                i += 1;
            }
            "place" => {
                builder.click_named(&format!("{} button", args[i + 1]))?;
                let at = builder.place_component_free(&args[i + 2])?;
                println!(
                    "placed {} as component {at}: {:?}",
                    args[i + 2],
                    builder
                        .build()
                        .components()
                        .iter()
                        .map(|c| c.file.clone())
                        .collect::<Vec<_>>()
                );
                i += 3;
            }
            "release" => {
                builder.mouse_released(num(&args[i + 1])?, num(&args[i + 2])?);
                i += 3;
            }
            "named" => {
                builder.click_named(&args[i + 1])?;
                i += 2;
            }
            "frame" => {
                let placed = builder.shell().placements();
                for p in placed.iter().flat_map(|p| p.walk()) {
                    if p.widget.name() == args[i + 1] {
                        println!("frame {}: {:?}", args[i + 1], p.frame);
                    }
                }
                i += 2;
            }
            "drawn" => {
                // `drawn X Y`: every widget under the point in draw order (back to front)
                let (px, py) = (num(&args[i + 1])?, num(&args[i + 2])?);
                let placed = builder.shell().placements();
                for p in ra2ui::draw_order(&placed) {
                    if p.frame.contains_point(px, py) {
                        println!(
                            "drawn {:?} {:?} order {} frame {:?}",
                            p.widget.kind,
                            p.widget.name(),
                            p.widget.child_order(),
                            p.frame
                        );
                    }
                }
                i += 3;
            }
            "hover" => {
                println!("hover: {:?}", builder.drag_hover());
                i += 1;
            }
            "status" => {
                let stats = builder.build().stats();
                println!(
                    "status: tab={:?} mass={:.1} class={:?} parts={} {}",
                    builder.tab(),
                    stats.mass,
                    stats.computed_class,
                    builder.build().components().len(),
                    builder.status()
                );
                i += 1;
            }
            other => return Err(format!("unknown step {other}")),
        }
    }
    Ok(())
}

/// The route file's tokens: comments cut, the live-only steps (`wait`, `hide`, `frames`) dropped, `dragh` as `drag`.
fn route_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.is_empty() {
            continue;
        }
        let shots = std::env::var("LABSHOT_SHOTS").unwrap_or_else(|_| ".".into());
        match t[0] {
            "wait" | "hide" | "frames" => {}
            // Back to HQ saves into the cell: ours does it in `back`
            "click" if t.get(1..3) == Some(&["55", "18"]) => out.push("back".to_string()),
            "dragh" | "drag" => {
                // x0 y0 x1 y1 [BUTTON] [STEPS] [HOLD] -> x0 y0 x1 y1 STEPS HOLD
                let nums: Vec<&str> = t[5..].to_vec();
                let (steps, hold) = match nums.len() {
                    0 => ("10", "0"),
                    1 => ("10", "0"),
                    2 => (nums[1], "0"),
                    _ => (nums[1], nums[2]),
                };
                out.push("drag".to_string());
                out.extend(t[1..5].iter().map(|s| s.to_string()));
                out.push(steps.to_string());
                out.push(hold.to_string());
            }
            "shot" => {
                out.push("shot".to_string());
                out.push(t[1].replacen("SHOTS", &shots, 1));
            }
            _ => out.extend(t.iter().map(|s| s.to_string())),
        }
    }
    out
}

/// The dinput scan code of a robot key by X keysym name.
fn key_code(name: &str) -> Option<i32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "w" => 17,
        "a" => 30,
        "s" => 31,
        "d" => 32,
        "space" => 57,
        "up" => 200,
        "down" => 208,
        "left" => 203,
        "right" => 205,
        _ => return None,
    })
}
