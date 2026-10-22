//! Photograph two robots the way `tools/oracle/capture_shape.py` photographs the
//! original's, HUD included: both stand still in the Combat Arena (hazards off, AIs
//! stopped, as live) and each shot puts the camera `fwd` metres ahead of the robot,
//! `side` to its right and `up` above it, looking at the point `look` metres ahead
//! (docs/37 section 6's `CAM`; the list is the script's `SHOTS`).
//!
//! ```text
//! RA2_GAME=<game> shapeshot <bot0> <bot1> <out_dir>
//! ```
//!
//! Frames `shape_<k>.png`, `k` the index in the script's list.
//!
//! `RA2_SHAPE_POSES="x,y,z,heading;..."` (one entry per shot) replaces the robot's position and
//! heading in the camera formula by the ones a live capture's jsonl recorded (`pose`: location,
//! heading), so the camera sits where the original's did even though the robots there had moved
//! on from their spawn points; the robots themselves stay where ours stand.

use std::path::PathBuf;

use glam::Vec3;
use ra2_app::match_mode::{write_session_frame, Session};
use ra2_render::ui::UiRenderer;
use ra2_render::Renderer;
use ra2script::camera::CameraCommand;
use ra2sim::camera::SceneCameraKind;
use ra2sim::{DuelCamera, GameType, Match, MatchConfig, MatchState, RobotEntry};

/// `(player, fwd, side, up, look)`, as `tools/oracle/capture_shape.py`.
const SHOTS: [(usize, f32, f32, f32, f32); 8] = [
    (0, 3.2, 1.8, 1.2, 1.2),
    (0, 0.0, 3.2, 0.9, 0.5),
    (0, 3.6, 0.0, 1.0, 1.0),
    (1, 3.2, 1.8, 1.2, 0.5),
    (1, 3.2, -1.8, 1.2, 0.5),
    (1, -3.2, 1.8, 1.2, -0.5),
    (1, -3.2, -1.8, 1.2, -0.5),
    (1, 0.0, 3.4, 1.0, 0.0),
];

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: shapeshot <bot0> <bot1> <out_dir>".into());
    }
    let game = PathBuf::from(std::env::var("RA2_GAME").map_err(|_| "set RA2_GAME")?);
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
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
    session.set_remote_drive(0, Some(ra2sim::DriveInput::default()));
    session.set_remote_drive(1, Some(ra2sim::DriveInput::default()));
    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let size = [1024u32, 768];
    session.set_layout(size);
    session.skip_intro();
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
    for _ in 0..(1.5 / dt) as usize {
        session.step();
    }
    let poses: Vec<[f32; 4]> = std::env::var("RA2_SHAPE_POSES")
        .map(|v| {
            v.split(';')
                .filter_map(|e| {
                    let n: Vec<f32> = e.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (n.len() == 4).then(|| [n[0], n[1], n[2], n[3]])
                })
                .collect()
        })
        .unwrap_or_default();
    for (k, &(p, fwd, side, up, look)) in SHOTS.iter().enumerate() {
        let duel = &session.matched.duel;
        let r = &duel.robots[p];
        let a = duel
            .physics
            .center_of_mass(r.body())
            .unwrap_or_else(|| r.position(&*duel.physics));
        let f = duel
            .physics
            .state(r.body())
            .map(|s| s.rotation * r.rig.forward())
            .unwrap_or(Vec3::Z);
        let mut f = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        let mut a = a;
        if let Some(pose) = poses.get(k) {
            a = Vec3::new(pose[0], pose[1], pose[2]);
            f = Vec3::new(pose[3].sin(), 0.0, pose[3].cos());
        }
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
        eprintln!("shot {k}: robot {p} at {a:?} heading {f:?} eye {m:?}");
        let path = out.join(format!("shape_{k}.png"));
        write_session_frame(&mut session, &renderer, &ui, size, &path)?;
    }
    Ok(())
}
