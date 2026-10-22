//! Photograph the in-match chat box (docs/38 section 11): a network-style match HUD with
//! chat lines in `chat displaybox` and the entry open, rendered at 1024x768.
//!
//! ```text
//! RA2_GAME=<game> chatshot <out_dir> [line ...]
//! ```
//!
//! Frames `chat_lines.png` (lines only, entry closed) and `chat_open.png` (entry open with
//! a typed line).

use std::path::PathBuf;

use ra2_app::match_mode::{write_session_frame, Session};
use ra2_render::ui::UiRenderer;
use ra2_render::Renderer;
use ra2sim::{DuelCamera, GameType, Match, MatchConfig, MatchState, RobotEntry};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Err("usage: chatshot <out_dir> [line ...]".into());
    }
    let game = PathBuf::from(std::env::var("RA2_GAME").map_err(|_| "set RA2_GAME")?);
    let out = PathBuf::from(&args[0]);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let lines: Vec<String> = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        vec![
            "[Bruno] hello there".into(),
            "[Guest] gg, nice flipper".into(),
            "[Bruno] this is a much longer line of chat text that has to wrap inside the box"
                .into(),
            "[Guest] ok".into(),
        ]
    };
    std::env::set_var("RA2_HAZARDS", "0");
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let bot = |n: &str| game.join(n).to_string_lossy().to_string();
    let cfg = MatchConfig::new(
        arena,
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, bot("AI/Team8/Bot0.bot")),
            RobotEntry::ai(1, bot("AI/Team5/Bot2.bot")),
        ],
    )
    .with_duration(Some(180.0));
    let matched = Match::new(cfg).map_err(|e| e.to_string())?;
    let mut session = Session::with_match_player(&game, matched)?;
    session.enable_chat();
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
    for l in lines {
        session.push_chat_line(l);
    }
    session.step();
    write_session_frame(
        &mut session,
        &renderer,
        &ui,
        size,
        &out.join("chat_lines.png"),
    )?;
    session.toggle_chat();
    for c in "typing a reply".chars() {
        session.chat_type(c);
    }
    session.step();
    write_session_frame(
        &mut session,
        &renderer,
        &ui,
        size,
        &out.join("chat_open.png"),
    )?;
    Ok(())
}
