//! Photograph a Flamethrower the way `tools/oracle/capture_flame.py` photographs the
//! original's: a robot with a flamethrower (`flamebot`) beside ALARM in the Combat
//! Arena, the AIs stopped, the jet fired at level 100, 50 and 20 and as a burst, and
//! a camera that sits `fwd` metres ahead of the robot, `side` to its right and `up`
//! above it and looks at the point `look` metres ahead (the live script's `CAM`).
//!
//! ```text
//! flameshot <arena.py> <flame.bot> <other.bot> <out_dir> [fwd side up look]
//! ```
//!
//! Frames `flame_<phase>_<n>.png`; the phases and their times follow the live run
//! (0.6 s apart from the moment the jet is switched on). Needs a GPU adapter and the
//! game data (`RA2_GAME`).

use std::path::PathBuf;

use glam::Vec3;
use ra2_render::Renderer;
use ra2script::camera::CameraCommand;
use ra2sim::camera::SceneCameraKind;
use ra2sim::{DuelCamera, GameType, Match, MatchConfig, MatchState, RobotEntry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!(
            "usage: flameshot <arena.py> <flame.bot> <other.bot> <out_dir> [fwd side up look]"
        );
        std::process::exit(2);
    }
    let out = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&out)?;
    let num = |i: usize, d: f32| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (fwd, side, up, look) = (num(4, 1.3), num(5, 1.9), num(6, 0.6), num(7, 1.3));
    let cfg = MatchConfig::new(
        args[0].clone(),
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, args[1].clone()),
            RobotEntry::ai(1, args[2].clone()),
        ],
    )
    .with_duration(Some(120.0));
    let mut matched = Match::new(cfg)?;
    let renderer = Renderer::new()?;
    let (w, h) = (1024u32, 768u32);
    let mut step = 0usize;
    while matched.state() != MatchState::Fight {
        matched.step();
        step += 1;
        assert!(step < 5000, "the fight never started");
    }
    let dt = matched.config.dt;
    let camera_for = |matched: &Match| {
        // The live script's `CAM`: relative to the robot's heading, looking at the
        // point `look` metres ahead of it, 0.15 m up.
        let duel = &matched.duel;
        let r = &duel.robots[0];
        let a = duel
            .physics
            .center_of_mass(r.body())
            .unwrap_or_else(|| r.position(&*duel.physics));
        let fwd_v = duel
            .physics
            .state(r.body())
            .map(|s| s.rotation * r.rig.forward())
            .unwrap_or(Vec3::Z);
        let (fx, fz) = {
            let f = Vec3::new(fwd_v.x, 0.0, fwd_v.z).normalize_or_zero();
            (f.x, f.z)
        };
        let (rx, rz) = (fz, -fx);
        let m = Vec3::new(
            a.x + fwd * fx + side * rx,
            a.y + up,
            a.z + fwd * fz + side * rz,
        );
        let t = Vec3::new(a.x + look * fx, a.y + 0.15, a.z + look * fz);
        let d = (t - m).normalize();
        (m, [-d.y.asin(), d.x.atan2(d.z)])
    };
    let shoot =
        |matched: &mut Match, name: &str, n: usize| -> Result<(), Box<dyn std::error::Error>> {
            let (eye, rotation) = camera_for(matched);
            let cam = &mut matched.duel.scene_camera;
            cam.select(SceneCameraKind::Static);
            cam.apply(CameraCommand::Position(eye.into()));
            cam.apply(CameraCommand::Rotation(rotation));
            cam.apply(CameraCommand::Fov(0.8));
            let png = matched.duel.render(&renderer, &DuelCamera::Static, w, h)?;
            image::save_buffer(
                out.join(format!("flame_{name}_{n}.png")),
                &png,
                w,
                h,
                image::ColorType::Rgba8,
            )?;
            Ok(())
        };
    // creep along like the live run (Throttle 8) so the immobility rule leaves it be
    let drive = |matched: &mut Match, on: bool| {
        matched.duel.robots[0].apply_inputs(&[("Forward".into(), 0, if on { 8.0 } else { 0.0 })]);
    };
    drive(&mut matched, true);
    // the live shots came 2.2-2.3 s of game time apart (a screenshot takes about 3 s of the
    // game's 0.7x wall clock), so the tank has run down by the later ones
    for (name, level, hold, gap) in [
        ("l100", 100.0, 7.0, 2.2),
        ("l50", 50.0, 5.0, 2.3),
        ("l20", 20.0, 4.5, 2.3),
    ] {
        matched.duel.robots[0].apply_inputs(&[("Flame".into(), 0, level)]);
        let steps = (hold / dt) as usize;
        let every = (gap / dt) as usize;
        for n in 0..steps {
            matched.step();
            if n % every == 0 && n > 0 {
                shoot(&mut matched, name, n / every)?;
            }
        }
        matched.duel.robots[0].apply_inputs(&[("Flame".into(), 0, 0.0)]);
        for _ in 0..(1.2 / dt) as usize {
            matched.step();
        }
    }
    matched.duel.robots[0].apply_inputs(&[("FlameBurst".into(), 0, 100.0)]);
    for n in 0..8 {
        matched.step();
        shoot(&mut matched, "burst", n)?;
    }
    println!("done");
    Ok(())
}
