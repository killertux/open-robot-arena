//! Photograph the particle effects: run a match into its fight, spawn each kind
//! of effect beside the first robot (or follow a real hit of more than 10
//! damage if one happens first), and write frames from a close camera.
//!
//! ```text
//! fxshot <arena.py> <botA> <botB> <out_dir> [frames] [every]
//! ```
//!
//! Frames are `fx_000.png ...`, every `every`-th physics step (default 3) after
//! the effects start. Needs a GPU adapter (wgpu) and the game data
//! (`RA2_GAME`).

use std::path::PathBuf;

use glam::Vec3;
use ra2_render::Renderer;
use ra2sim::{DuelCamera, GameType, Match, MatchConfig, MatchState, RobotEntry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("usage: fxshot <arena.py> <botA> <botB> <out_dir> [frames] [every]");
        std::process::exit(2);
    }
    let out = PathBuf::from(&args[3]);
    let frames: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(24);
    let every: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(3);
    std::fs::create_dir_all(&out)?;
    let cfg = MatchConfig::new(
        args[0].clone(),
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, args[1].clone()),
            RobotEntry::ai(1, args[2].clone()),
        ],
    )
    .with_duration(Some(60.0));
    let mut matched = Match::new(cfg)?;
    matched.duel.record_impacts = true;
    let renderer = Renderer::new()?;
    let (w, h) = (1024u32, 768u32);
    let mut step = 0usize;
    while matched.state() != MatchState::Fight || matched.clock() < 1.0 {
        matched.step();
        step += 1;
        assert!(step < 5000, "the fight never started");
    }
    let physics = &*matched.duel.physics;
    let a = matched.duel.robots[0].position(physics);
    let b = matched.duel.robots[1].position(physics);
    // Where `DuelCamera::Wide` looks: the midpoint of the fight.
    let base = Vec3::new((a.x + b.x) / 2.0, a.y.min(b.y), (a.z + b.z) / 2.0);
    println!("effects at {base:?}");
    // The kinds of effect, side by side along the camera's right.
    let fx = &mut matched.duel.fx;
    fx.sparks(
        40.0,
        base + Vec3::new(-1.0, 0.3, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    );
    fx.debris(
        30.0,
        base + Vec3::new(-0.3, 0.3, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    );
    fx.smoke_puff(
        25,
        base + Vec3::new(0.4, 0.2, 0.0),
        Vec3::Y,
        Vec3::new(0.5, 0.0, 0.5),
    );
    fx.motor_smoke(1, base + Vec3::new(1.0, 0.2, 0.0), true);
    fx.flame(
        2,
        base + Vec3::new(1.8, 0.2, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.3, 0.0, 0.3),
        true,
    );
    let camera = DuelCamera::Wide {
        offset: Vec3::new(0.0, 1.5, -4.5),
    };
    for n in 0..frames * every {
        matched.step();
        if let Some(i) = matched.duel.impacts.iter().find(|i| i.damage > 10.0) {
            println!(
                "real hit {:.1} at {:?} normal {:?}",
                i.damage, i.point, i.normal
            );
        }
        if n % every == 0 {
            let png = matched.duel.render(&renderer, &camera, w, h)?;
            image::save_buffer(
                out.join(format!("fx_{:03}.png", n / every)),
                &png,
                w,
                h,
                image::ColorType::Rgba8,
            )?;
        }
    }
    Ok(())
}
