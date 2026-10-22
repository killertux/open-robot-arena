//! Draw the particles of one live frame of the original's Flamethrower with our renderer (docs/37 section 6):
//! the scene of `flameshot` (the flame robot and ALARM at their spawn points, HUD, hazards off, AIs
//! stopped), the camera of the live frame, and exactly the particles the original drew in it
//! (`frida/ra2_flame.js` `draw` rows: position, half-size, colour, in its draw order).
//!
//! ```text
//! RA2_GAME=<game> flamereplay <flame.bot> <other.bot> <frames.txt> <out_dir>
//! ```
//!
//! `frames.txt`: `frame <name> ex ey ez pitch yaw fov`, then one `p x y z size r g b a` line per particle.

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
        return Err("usage: flamereplay <flame.bot> <other.bot> <frames.txt> <out_dir>".into());
    }
    let game = PathBuf::from(std::env::var("RA2_GAME").map_err(|_| "set RA2_GAME")?);
    let out = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
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
    session.set_remote_drive(0, Some(ra2sim::DriveInput::default()));
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
    // frames.txt: `frame <name> ex ey ez pitch yaw fov` then `p x y z size r g b a` lines
    let text = std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?;
    type Frame = (String, [f32; 6], Vec<(Vec3, f32, [f32; 4])>);
    let mut frames: Vec<Frame> = Vec::new();
    for line in text.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| w.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        match w.first().copied() {
            Some("frame") => frames.push((
                w[1].to_string(),
                [n(2), n(3), n(4), n(5), n(6), n(7)],
                Vec::new(),
            )),
            Some("p") => {
                if let Some(last) = frames.last_mut() {
                    last.2
                        .push((Vec3::new(n(1), n(2), n(3)), n(4), [n(5), n(6), n(7), n(8)]));
                }
            }
            _ => {}
        }
    }
    for (name, c, list) in frames {
        let cam = &mut session.matched.duel.scene_camera;
        cam.select(SceneCameraKind::Static);
        cam.apply(CameraCommand::Position([c[0], c[1], c[2]]));
        cam.apply(CameraCommand::Rotation([c[3], c[4]]));
        cam.apply(CameraCommand::Fov(c[5]));
        session.matched.duel.fx.replay(Some(&list));
        write_session_frame(
            &mut session,
            &renderer,
            &ui,
            size,
            &out.join(format!("{name}.png")),
        )?;
    }
    Ok(())
}
