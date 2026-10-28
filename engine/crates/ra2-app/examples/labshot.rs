//! Drive our Bot Lab with the same step script the live oracle gets (`ra2_drive.py` steps),
//! so side-by-sides compare the same clicks:
//!
//! ```text
//! labshot <scratch_game> <cell|-> <steps...>
//! ```
//!
//! Steps: `click X Y`, `move X Y`, `shot PATH`, `key C` (a typed character, `Return`,
//! `BackSpace`), `status`, `back` (Back to HQ: saves into the team) and `tab`. The scratch
//! game must be a copy made by `tools/labgame.sh`; `ORA_SAVE_DIR` is set to it here.

use std::path::{Path, PathBuf};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    let mut i = 2;
    let num = |s: &String| s.parse::<f32>().map_err(|e| format!("{s}: {e}"));
    while i < args.len() {
        match args[i].as_str() {
            "click" => {
                let (x, y) = (num(&args[i + 1])?, num(&args[i + 2])?);
                builder.mouse_moved(x, y);
                builder.click(x, y)?;
                i += 3;
            }
            "move" => {
                builder.mouse_moved(num(&args[i + 1])?, num(&args[i + 2])?);
                i += 3;
            }
            "cam" => {
                builder.set_lab_cam(ra2_app::build_mode::LabCam {
                    pitch: num(&args[i + 1])?,
                    yaw: num(&args[i + 2])?,
                    dist: num(&args[i + 3])?,
                });
                i += 4;
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
            "key" => {
                match args[i + 1].as_str() {
                    "Return" => {
                        builder.enter()?;
                    }
                    "BackSpace" => builder.backspace(),
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
