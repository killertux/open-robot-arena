//! `ra2-sim` — headless M2 physics sandbox.
//!
//! Drops a robot into an arena, drives it with a scripted input timeline,
//! steps the physics world, and renders a frame sequence (plus a montage) so
//! the motion can be verified without a display.
//!
//! Usage:
//!   ra2-sim <arena.py|arena.gmf> <bot> <out_dir> [seconds] [shots] [drive]
//!
//! drive: "<start>-<end>:<throttle>,<steer>; ..."  e.g. "0-2:1,0;2-4:1,1;4-6:-1,0"
//! Env:   RA2_OBSTACLE=crates ... (arena props), RA2_MASS=<kg>
//!
//! Writes <out_dir>/frame_000.png ... and <out_dir>/strip.png.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use glam::Vec3;
use ra2_render::Renderer;
use ra2sim::{CameraMode, DriveScript, Sandbox};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: ra2-sim <arena.py|arena.gmf> <bot> <out_dir> [seconds] [shots] [drive]");
        eprintln!("  drive: \"0-2:1,0;2-4:1,1;4-6:-1,0\"  (start-end:throttle,steer)");
        eprintln!("  env:   RA2_OBSTACLE=crates  RA2_MASS=<kg>  RA2_YAW=<deg>");
        eprintln!("         RA2_SPAWN=<x,z>  RA2_CHASE=<dx,dy,dz>");
        return ExitCode::FAILURE;
    }
    let arena = &args[0];
    let bot = &args[1];
    let out = PathBuf::from(&args[2]);
    let seconds: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let shots: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(12);
    let script = args
        .get(5)
        .cloned()
        .unwrap_or_else(|| "0-2:1,0;2-4:1,1;4-6:1,-1;6-8:0,0".to_string());

    match run(arena, bot, &out, seconds, shots, &script) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    arena: &str,
    bot: &str,
    out: &Path,
    seconds: f32,
    shots: usize,
    script: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut sandbox = Sandbox::new(arena, bot)?;
    let drive = DriveScript::parse(script);
    println!("drive script: {script}");

    std::fs::create_dir_all(out)?;
    let renderer = Renderer::new()?;

    let dt = 1.0 / 60.0;
    let total_steps = (seconds / dt).round().max(1.0) as usize;
    let shots = shots.max(1);
    let every = (total_steps / shots).max(1);

    let (w, h) = (960u32, 600u32);
    let camera = match std::env::var("RA2_CHASE") {
        Ok(spec) => {
            let v: Vec<f32> = spec
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            if v.len() >= 3 {
                CameraMode::Fixed(Vec3::new(v[0], v[1], v[2]))
            } else {
                CameraMode::chase_for(sandbox.bot.half_extent().length())
            }
        }
        Err(_) => CameraMode::chase_for(sandbox.bot.half_extent().length()),
    };

    let mut frames: Vec<PathBuf> = Vec::new();
    for step in 0..=total_steps {
        if step % every == 0 || step == total_steps {
            let input = drive.at(sandbox.time);
            if step > 0 {
                sandbox.step(input, dt);
            }
            let png = sandbox.render(&renderer, &camera, w, h)?;
            let path = out.join(format!("frame_{:03}.png", frames.len()));
            image::save_buffer(&path, &png, w, h, image::ColorType::Rgba8)?;
            let p = sandbox.bot_position();
            println!(
                "  t={:5.2}s  pos=({:7.2},{:7.2},{:7.2})  speed={:5.2} m/s  {}",
                sandbox.time,
                p.x,
                p.y,
                p.z,
                sandbox.bot_speed(),
                path.display()
            );
            frames.push(path);
        } else {
            sandbox.step(drive.at(sandbox.time), dt);
        }
    }

    let strip = out.join("strip.png");
    montage(&frames, &strip)?;
    println!("wrote {} frames + {}", frames.len(), strip.display());
    Ok(())
}

/// Tile the frames into a single image for quick inspection.
fn montage(frames: &[PathBuf], out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if frames.is_empty() {
        return Ok(());
    }
    let first = image::open(&frames[0])?.to_rgba8();
    let (tw, th) = (first.width() / 2, first.height() / 2);
    let cols = 4usize;
    let rows = frames.len().div_ceil(cols);
    let mut canvas = image::RgbaImage::from_pixel(
        tw * cols as u32,
        th * rows as u32,
        image::Rgba([18, 18, 22, 255]),
    );
    for (i, f) in frames.iter().enumerate() {
        let img = image::open(f)?.to_rgba8();
        let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
        let (x, y) = ((i % cols) as u32 * tw, (i / cols) as u32 * th);
        image::imageops::overlay(&mut canvas, &small, x as i64, y as i64);
    }
    canvas.save(out)?;
    Ok(())
}
