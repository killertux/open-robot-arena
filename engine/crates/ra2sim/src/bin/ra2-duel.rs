//! `ra2-duel` — two robots fight in an arena, headless.
//!
//! Drives both robots with a simple "ram the opponent" AI, simulates motors,
//! weapons and structural damage (parts get knocked off), and renders a frame
//! sequence plus a montage.
//!
//! Usage:
//!   ra2-duel <arena.py|arena.gmf> <botA> <botB> <out_dir> [seconds] [shots] [camera]
//!
//! camera: `wide` (default), `chase`, `top`
//! env:    RA2_OBSTACLE=crates  RA2_MASS_SCALE=1.0  RA2_CAM=<dx,dy,dz>
//!         RA2_DAMAGE=<scale>

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use glam::Vec3;
use ra2_render::Renderer;
use ra2sim::duel::{BotSetup, Duel, DuelCamera};
use ra2sim::montage;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("usage: ra2-duel <arena> <botA> <botB> <out_dir> [seconds] [shots] [camera]");
        eprintln!("  camera: wide (default) | chase | top");
        eprintln!("  env:    RA2_OBSTACLE=crates  RA2_MASS_SCALE=1.0  RA2_CAM=<dx,dy,dz>");
        return ExitCode::FAILURE;
    }
    let arena = args[0].clone();
    let out = PathBuf::from(&args[3]);
    let seconds: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let shots: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(16);
    let camera = args.get(6).cloned().unwrap_or_else(|| "wide".into());

    let bots = vec![
        BotSetup {
            path: args[1].clone(),
            yaw_deg: None,
            spawn: None,
        },
        BotSetup {
            path: args[2].clone(),
            yaw_deg: None,
            spawn: None,
        },
    ];

    match run(&arena, bots, &out, seconds, shots, &camera) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    arena: &str,
    bots: Vec<BotSetup>,
    out: &Path,
    seconds: f32,
    shots: usize,
    camera: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut duel = Duel::new(arena, &bots)?;
    if let Ok(scale) = std::env::var("RA2_DAMAGE") {
        if let Ok(v) = scale.parse() {
            duel.combat.material_scale = v;
        }
    }
    println!(
        "duel: {} robots, arena_tris={} props={} material_scale={:.3}",
        duel.robots.len(),
        duel.arena.triangles.len(),
        duel.props.len(),
        duel.combat.material_scale
    );

    let cam = match camera {
        "chase" => DuelCamera::Chase {
            robot: 0,
            distance: 4.5,
            height: 3.0,
        },
        "top" => DuelCamera::Top { height: 14.0 },
        "wide" => DuelCamera::Wide {
            offset: Vec3::new(6.0, 5.0, 6.0),
        },
        _ => DuelCamera::Auto,
    };
    let cam = match std::env::var("RA2_CAM") {
        Ok(spec) => {
            let v: Vec<f32> = spec
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            if v.len() >= 3 {
                DuelCamera::Wide {
                    offset: Vec3::new(v[0], v[1], v[2]),
                }
            } else {
                cam
            }
        }
        Err(_) => cam,
    };

    std::fs::create_dir_all(out)?;
    let renderer = Renderer::new()?;

    let dt = 1.0 / 60.0;
    let total = (seconds / dt).round().max(1.0) as usize;
    let shots = shots.max(1);
    let every = (total / shots).max(1);
    let (w, h) = (900u32, 560u32);

    let mut frames: Vec<PathBuf> = Vec::new();
    let mut log_at = 0usize;
    // Diagnostic: render the chassis crushed to a given fraction (F9), so the
    // deformation can be looked at without fighting a bout into it.
    if let Ok(v) = std::env::var("RA2_BUCKLE") {
        if let Ok(f) = v.parse::<f32>() {
            for i in 0..duel.robots.len() {
                let hp = duel.robots[i].rig.components[0].hitpoints;
                duel.robots[i].health[0] = hp * (1.0 - f.clamp(0.0, 1.0));
                duel.robots[i].buckle(&mut *duel.physics);
            }
            println!("  buckle: chassis crushed to {f:.2}");
        }
    }
    // `RA2_DUEL_DRIVE=ram` drives every robot straight ahead instead of running
    // the AI, turning the bout into a deterministic head-on collision. That is
    // how the damage model is observed and tested: with the AI's own approach
    // two stock bots frequently never touch at all.
    let direct: Vec<ra2sim::DriveInput> = if std::env::var("RA2_DUEL_DRIVE")
        .map(|v| v.eq_ignore_ascii_case("ram"))
        .unwrap_or(false)
    {
        println!("  drive: ram (straight ahead, AI off)");
        (0..duel.robots.len())
            .map(|_| ra2sim::DriveInput {
                throttle: 1.0,
                steer: 0.0,
            })
            .collect()
    } else {
        Vec::new()
    };
    for step in 0..=total {
        if step > 0 {
            if direct.is_empty() {
                duel.step(dt);
            } else {
                duel.step_direct(&direct, dt);
            }
        }
        // Report new breakage.
        while log_at < duel.log.len() {
            println!("  {}", duel.log[log_at]);
            log_at += 1;
        }
        if step % every == 0 || step == total {
            if std::env::var("RA2_DEBUG").is_ok() && duel.robots.len() >= 2 {
                for row in ra2sim::combat::report(&*duel.physics, &duel.robots, 0, 1, &duel.combat)
                {
                    println!("    {row}");
                }
                // Which bodies of A actually touch which of B? The damage model
                // asks about the two robots' *root* bodies only, and a robot is
                // several hinged parts, so this is the check that matters.
                let (a, b) = (&duel.robots[0], &duel.robots[1]);
                let mut hits = Vec::new();
                for (i, pa) in a.parts.iter().enumerate() {
                    for (j, pb) in b.parts.iter().enumerate() {
                        if duel.physics.in_contact(pa.body, pb.body) {
                            hits.push(format!("{i}x{j}"));
                        }
                    }
                }
                println!(
                    "    parts: a={} b={} touching: [{}]",
                    a.parts.len(),
                    b.parts.len(),
                    hits.join(" ")
                );
            }
            let png = duel.render(&renderer, &cam, w, h)?;
            let path = out.join(format!("frame_{:03}.png", frames.len()));
            montage::save_png(&path, &png, w, h)?;
            let alive: Vec<String> = duel
                .robots
                .iter()
                .map(|r| {
                    let hp: f32 = r
                        .health
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| r.attached[*i])
                        .map(|(_, v)| v.max(0.0))
                        .sum();
                    let p = r.position(&*duel.physics);
                    let v = r.velocity(&*duel.physics).length();
                    let spin = duel
                        .physics
                        .state(r.body())
                        .map(|s| s.angular_velocity.y)
                        .unwrap_or(0.0);
                    format!(
                        "{}{} p=({:.1},{:.1}) v={:.1} w={:.1} hp={}",
                        r.rig.name,
                        if r.is_alive() { "" } else { " KO" },
                        p.x,
                        p.z,
                        v,
                        spin,
                        hp as i32
                    )
                })
                .collect();
            println!(
                "  t={:5.2}s  debris={:3}  gap={:6.2}  dmg={:?}  [{}]  {}",
                duel.time,
                duel.debris.len(),
                duel.closest_gap(),
                duel.damage_taken
                    .iter()
                    .map(|d| *d as i32)
                    .collect::<Vec<_>>(),
                alive.join(" "),
                path.display()
            );
            frames.push(path);
        }
    }

    let strip = out.join("strip.png");
    montage::montage(&frames, &strip, 4, 2)?;
    println!("wrote {} frames + {}", frames.len(), strip.display());
    Ok(())
}
