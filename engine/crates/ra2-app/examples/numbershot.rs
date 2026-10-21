//! Photograph a floating damage number the way `tools/oracle/capture_numbers.py` photographs the
//! original's (docs/37 section 7): a human robot and an AI robot in the Combat Arena, a hazard
//! hit (`plus.damage`) on the human robot, the camera placed by `eye`, `target`, `fov` (the live
//! `ra2_camera.js` rows) and frames written every `gap` seconds of game time over the number's life.
//!
//! ```text
//! RA2_GAME=<game> numbershot <out_dir> <ex ey ez> <tx ty tz> <fov> <px py pz> [amount] [green]
//! ```
//!
//! Prints `NUMBER age x y` for every frame (the anchor's projection) and writes
//! `number_<n>.png`.

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
    if args.len() < 11 {
        return Err(
            "usage: numbershot <out_dir> ex ey ez tx ty tz fov px py pz [amount] [green]".into(),
        );
    }
    let game = PathBuf::from(std::env::var("RA2_GAME").map_err(|_| "set RA2_GAME")?);
    let out = PathBuf::from(&args[0]);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let num = |i: usize| -> f32 { args[i].parse().expect("a number") };
    let eye = Vec3::new(num(1), num(2), num(3));
    let target = Vec3::new(num(4), num(5), num(6));
    let fov = num(7);
    let point = Vec3::new(num(8), num(9), num(10));
    let amount = args.get(11).and_then(|s| s.parse().ok()).unwrap_or(25.0f32);
    let green = args.get(12).is_some_and(|s| s == "green");
    std::env::set_var("RA2_HAZARDS", "0");
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let cfg = MatchConfig::new(
        arena,
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(
                0,
                game.join("AI/Team0/Bot0.bot").to_string_lossy().to_string(),
            ),
            RobotEntry::ai(
                1,
                game.join("AI/Team0/Bot1.bot").to_string_lossy().to_string(),
            ),
        ],
    )
    .with_duration(Some(180.0));
    let matched = Match::new(cfg).map_err(|e| e.to_string())?;
    let mut session = Session::with_match_player(&game, matched)?;
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
    let d = (target - eye).normalize();
    let place = |session: &mut Session| {
        let cam = &mut session.matched.duel.scene_camera;
        cam.select(SceneCameraKind::Static);
        cam.apply(CameraCommand::Position(eye.into()));
        cam.apply(CameraCommand::Rotation([-d.y.asin(), d.x.atan2(d.z)]));
        cam.apply(CameraCommand::Fov(fov));
    };
    place(&mut session);
    // the hit: a hazard's red number on the human robot, or the attacker's green one
    if green {
        session
            .matched
            .duel
            .apply_hit_normal(0.0, Some((0, 0)), 1, 0, amount, point, Vec3::ZERO);
    } else {
        session
            .matched
            .duel
            .apply_hit_normal(0.0, None, 0, 0, amount, point, Vec3::ZERO);
    }
    let dt = session.matched.config.dt;
    let mut n = 0;
    let mut written = 0;
    for _ in 0..(1.4 / dt) as usize {
        session.step();
        place(&mut session);
        let numbers = session.matched.damage_numbers().to_vec();
        if let Some(num) = numbers.first() {
            // the first frame, then every 0.25 s
            if n % (0.25 / dt) as usize == 0 {
                let path = out.join(format!("number_{written}.png"));
                write_session_frame(&mut session, &renderer, &ui, size, &path)?;
                println!("NUMBER age {:.3} pos {:?}", num.age, num.position());
                written += 1;
            }
            n += 1;
        }
    }
    Ok(())
}
