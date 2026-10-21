//! Photograph a Flamethrower the way `tools/oracle/capture_flame.py` photographs the
//! original's, HUD included: a robot with a flamethrower (`flamebot`) beside ALARM in
//! the Combat Arena (hazards off, as live), the jet fired at level 100, 50 and 20,
//! and a camera `fwd` metres ahead of the robot, `side` to its right and `up` above
//! it, looking at the point `look` metres ahead (the live script's `CAM`).
//!
//! ```text
//! RA2_GAME=<game> flameshot <flame.bot> <other.bot> <out_dir> [fwd side up look]
//! ```
//!
//! Frames `flame_<phase>_<n>.png`, 2.2-2.3 s of game time apart like the live shots.

use std::path::PathBuf;

use glam::Vec3;
use ra2_app::match_mode::{write_session_frame, Session};
use ra2_render::ui::UiRenderer;
use ra2_render::Renderer;
use ra2script::camera::CameraCommand;
use ra2sim::camera::SceneCameraKind;
use ra2sim::{DuelCamera, GameType, Match, MatchConfig, MatchState, RobotEntry};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: flameshot <flame.bot> <other.bot> <out_dir> [fwd side up look]".into());
    }
    let game = PathBuf::from(std::env::var("RA2_GAME").map_err(|_| "set RA2_GAME")?);
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let num = |i: usize, d: f32| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (fwd, side, up, look) = (num(3, 1.3), num(4, 1.9), num(5, 0.6), num(6, 1.3));
    // the live run cleared "Hazards enabled"
    std::env::set_var("RA2_HAZARDS", "0");
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let cfg = MatchConfig::new(
        arena,
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, args[0].clone()),
            RobotEntry::ai(1, args[1].clone()),
        ],
    )
    .with_duration(Some(180.0));
    let matched = Match::new(cfg).map_err(|e| e.to_string())?;
    let mut session = Session::with_match_player(&game, matched)?;
    // both AIs stopped, as live: robot 0 is ours to drive, ALARM stands still
    session.set_remote_drive(1, Some(ra2sim::DriveInput::default()));
    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let size = [1024u32, 768];
    session.set_layout(size);
    session.skip_intro();
    // the camera-name label shows for 3 s after a switch; switch during the countdown
    session.enter_camera(DuelCamera::Static);
    let mut guard = 0;
    while session.matched.state() != MatchState::Fight {
        session.step();
        guard += 1;
        if guard > 6000 {
            return Err("the fight never started".into());
        }
    }
    let dt = session.matched.config.dt;
    let frame_camera = |session: &mut Session| {
        let duel = &session.matched.duel;
        let r = &duel.robots[0];
        let a = duel
            .physics
            .center_of_mass(r.body())
            .unwrap_or_else(|| r.position(&*duel.physics));
        let f = duel
            .physics
            .state(r.body())
            .map(|s| s.rotation * r.rig.forward())
            .unwrap_or(Vec3::Z);
        let f = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        let (rx, rz) = (f.z, -f.x);
        let m = Vec3::new(
            a.x + fwd * f.x + side * rx,
            a.y + up,
            a.z + fwd * f.z + side * rz,
        );
        let t = Vec3::new(a.x + look * f.x, a.y + 0.15, a.z + look * f.z);
        let d = (t - m).normalize();
        let cam = &mut session.matched.duel.scene_camera;
        cam.select(SceneCameraKind::Static);
        cam.apply(CameraCommand::Position(m.into()));
        cam.apply(CameraCommand::Rotation([-d.y.asin(), d.x.atan2(d.z)]));
        cam.apply(CameraCommand::Fov(0.8));
        eprintln!(
            "CAM com {a:?} f {f:?} eye {m:?} pitch {} yaw {}",
            -d.y.asin(),
            d.x.atan2(d.z)
        );
    };
    // the live robot crept along (Throttle 8) so the immobility rule left it be
    session.matched.duel.robots[0].apply_inputs(&[("Forward".into(), 0, 8.0)]);
    for (name, level, hold, gap) in [
        ("l100", 100.0, 7.0, 2.2),
        ("l50", 50.0, 5.0, 2.3),
        ("l20", 20.0, 4.5, 2.3),
    ] {
        session.matched.duel.robots[0].apply_inputs(&[("Flame".into(), 0, level)]);
        let every = (gap / dt) as usize;
        for n in 0..(hold / dt) as usize {
            session.step();
            if n > 0 && n % every == 0 {
                frame_camera(&mut session);
                let path = out.join(format!("flame_{name}_{}.png", n / every));
                write_session_frame(&mut session, &renderer, &ui, size, &path)?;
            }
        }
        session.matched.duel.robots[0].apply_inputs(&[("Flame".into(), 0, 0.0)]);
        for _ in 0..(1.2 / dt) as usize {
            session.step();
        }
    }
    Ok(())
}
