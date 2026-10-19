//! The headless path: the same frame builder, written to a PNG.
//!
//! `--offscreen` boots the shell, applies the click script in order, builds the
//! final frame with [`crate::frame`], draws it with `ra2-render`'s UI pipeline
//! and reads it back. It is the CI entry point for M7 and the proof that the
//! windowed loop has no logic of its own.
//!
//! The audio policy is the windowed one over a silent [`crate::audio::AudioSystem`]
//! (`Audio::without_device`): the same music switches and click calls are
//! recorded and printed, never sent to a sound card. That is deliberate for a
//! headless evidence run and for `cargo test`.

use std::path::Path;

use ra2_shell::{Shell, Step};

use crate::audio::AudioSystem;
use crate::frame::FrameRenderer;

/// Apply a click script, in order, at the shell's own coordinates.
pub fn apply_clicks(shell: &mut Shell, clicks: &[[f32; 2]]) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for &[x, y] in clicks {
        steps.push(shell.click(x, y)?);
    }
    Ok(steps)
}

/// Apply a click script by widget name. The shell resolves each name to a
/// reachable point with the same hit test a mouse uses.
pub fn apply_named_clicks(shell: &mut Shell, names: &[String]) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for name in names {
        steps.push(shell.click_named(name)?);
    }
    Ok(steps)
}

/// Build, draw and save the current screen.
pub fn render_png(
    shell: &mut Shell,
    game: &Path,
    out: &Path,
    size: [u32; 2],
) -> Result<(), String> {
    use ra2_render::ui::UiRenderer;
    use ra2_render::Renderer;

    let mut frames = FrameRenderer::open(game)?;
    let frame = frames.frame(shell, size);
    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new(renderer.device());
    let pixels = ui
        .render_at(renderer.device(), renderer.queue(), &frame.scene, size)
        .map_err(|e| e.to_string())?;
    let image = image::RgbaImage::from_raw(size[0], size[1], pixels)
        .ok_or_else(|| "the readback did not fill the image".to_string())?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    image
        .save(out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(())
}

/// The `--offscreen` run: boot, click, draw, report.
pub fn run(
    game: &Path,
    out: &Path,
    size: [u32; 2],
    clicks: &[[f32; 2]],
    click_names: &[String],
    audio_report: bool,
) -> Result<(), String> {
    let mut shell = Shell::boot(game, [size[0] as f32, size[1] as f32])?;
    if let Some(view) = crate::hq_mode::shell_view(game) {
        shell.set_hq_view(view);
    }
    println!(
        "booted {} from {} ({}x{})",
        shell.screen_id(),
        game.display(),
        shell.size()[0],
        shell.size()[1]
    );
    let cfg = crate::options::read_cfg(game);
    let mut audio = AudioSystem::new_offscreen(game, &cfg);
    audio.on_screen(&shell.screen_id());
    let mut settings = crate::options::Options::open_game(game);

    let mut steps = Vec::new();
    if click_names.is_empty() {
        for &point in clicks {
            stage_options(&shell, &mut settings, &mut audio, point)?;
            steps.push(shell.click(point[0], point[1])?);
            let step = steps.last().unwrap().clone();
            finish_step(&mut shell, &mut settings, &mut audio, &step)?;
        }
    } else {
        for name in click_names {
            // `click_named` resolves this same point and then clicks it; the
            // options controls need the point before the click.
            // `@x,y` is a point in the render size (1024x768 here), so a route can
            // mix named controls with list rows and other unnamed targets.
            let at = name.strip_prefix('@').map(parse_point).transpose()?;
            let point = match at {
                Some(point) => point,
                None => shell.point_for(name)?,
            };
            stage_options(&shell, &mut settings, &mut audio, point)?;
            steps.push(match at {
                Some(point) => shell.click(point[0], point[1])?,
                None => shell.click_named(name)?,
            });
            let step = steps.last().unwrap().clone();
            finish_step(&mut shell, &mut settings, &mut audio, &step)?;
        }
    }

    render_png(&mut shell, game, out, size)?;
    println!(
        "finished on {} (content: {})",
        shell.screen_id(),
        shell.content_id().unwrap_or_else(|| "-".into())
    );
    println!(
        "wrote {} ({}x{}, {} clicks)",
        out.display(),
        size[0],
        size[1],
        steps.len()
    );
    // A capture of the shell is the music (and the clicks) for as long as the
    // recording we compare against: `RA2_AUDIO_SECONDS`, default 13.3 s.
    let seconds = std::env::var("RA2_AUDIO_SECONDS")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(13.3);
    audio.finish_capture(seconds)?;
    print!("{}", audio.report(audio_report));
    Ok(())
}

fn parse_point(text: &str) -> Result<[f32; 2], String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("click point {text:?} is not x,y"))?;
    let num = |v: &str| v.trim().parse::<f32>().map_err(|e| format!("{v:?}: {e}"));
    Ok([num(x)?, num(y)?])
}

/// Stage the settings control under `point`, if the options window is open.
fn stage_options(
    shell: &Shell,
    settings: &mut crate::options::Options,
    audio: &mut AudioSystem,
    point: [f32; 2],
) -> Result<(), String> {
    if !crate::options::Options::is_screen(&shell.screen_id()) {
        return Ok(());
    }
    let Some(hit) = crate::options::hit(shell, point) else {
        return Ok(());
    };
    if let Some(change) = settings.click(&hit) {
        println!("options: {change}");
        settings.save()?;
        audio.apply_cfg(settings.cfg());
    }
    Ok(())
}

/// The audio and settings consequences of a shell click.
fn finish_step(
    shell: &mut Shell,
    settings: &mut crate::options::Options,
    audio: &mut AudioSystem,
    step: &Step,
) -> Result<(), String> {
    println!("{}", crate::describe(step));
    audio.shell_step(step);
    match step.note.as_deref() {
        Some(crate::options::OPEN_NOTE) => {
            if let Err(error) = settings.reload() {
                eprintln!("options: {error}; keeping the staged values");
            }
        }
        Some(crate::options::OK_NOTE) => {
            settings.save()?;
            audio.apply_cfg(settings.cfg());
        }
        Some(crate::options::CANCEL_NOTE) => {
            settings.cancel()?;
            audio.apply_cfg(settings.cfg());
        }
        _ => {}
    }
    let screen = shell.screen_id();
    if crate::options::Options::is_screen(&screen) {
        if let Some(document) = shell.document_mut() {
            settings.sync(document);
        }
    }
    audio.on_screen(&screen);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::SHELL_SIZE;

    fn game_dir() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// The scripted route and the named route are the same route: the points
    /// `click_named` finds are fed to the pixel-click path and the resulting
    /// screens match step for step. This is the offscreen path's click half.
    #[test]
    fn the_offscreen_click_script_lands_where_the_named_route_lands() {
        let Some(game) = game_dir() else { return };
        let mut named = Shell::boot(&game, SHELL_SIZE).unwrap();
        let first = named.click_named("Team HQ button").unwrap();
        let second = named.click_named("continue button").unwrap();
        assert_eq!(named.screen_id(), "teamhq/teamhq.gib");

        let clicks = [first.point, second.point];
        let mut scripted = Shell::boot(&game, SHELL_SIZE).unwrap();
        let steps = apply_clicks(&mut scripted, &clicks).unwrap();

        assert_eq!(scripted.screen_id(), named.screen_id());
        let scripted_screens: Vec<&str> = steps.iter().map(|s| s.screen.as_str()).collect();
        let named_screens: Vec<&str> = named.trace().iter().map(|s| s.screen.as_str()).collect();
        assert_eq!(scripted_screens, named_screens);
        assert_eq!(steps[0].note.as_deref(), Some("link to team HQ"));
        assert_eq!(
            steps[1].note.as_deref(),
            Some("click to go to team hq with selected team (disable button if no team selected)")
        );
    }

    /// M7.5/M7.6: main menu → Team HQ → robots tab → bot lab is four
    /// activations and the three shipped tracks, with no restart inside the HQ
    /// group (the two teamhq screens share `Music/Team HQ`).
    #[test]
    fn the_named_route_switches_the_three_tracks_and_clicks_once_per_activation() {
        let Some(game) = game_dir() else { return };
        let mut shell = Shell::boot(&game, SHELL_SIZE).unwrap();
        let cfg = crate::options::read_cfg(&game);
        let mut audio = AudioSystem::new_silent(&game, &cfg);
        audio.on_screen(&shell.screen_id());

        let names = [
            "Team HQ button",
            "continue button",
            "robots button",
            "workshop button",
        ]
        .map(str::to_string);
        let steps = apply_named_clicks(&mut shell, &names).unwrap();
        for step in &steps {
            audio.shell_step(step);
            audio.on_screen(&step.screen);
        }
        assert_eq!(shell.screen_id(), "botlab/botlabwin.gib");
        let log = audio.audio().play_log();
        let music: Vec<_> = log
            .entries()
            .iter()
            .filter(|entry| entry.kind == ra2_audio::PlayKind::Music)
            .map(|entry| entry.path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(music.len(), 3, "{music:?}");
        assert!(music[0].ends_with("Music/Main UI/Enter the Battlefield.mp3"));
        assert!(music[1].ends_with("Music/Team HQ/Robot Tribal Dance.mp3"));
        assert!(music[2].ends_with("Music/Workshop/Deep inside the Bot Lab.mp3"));
        let clicks = log
            .entries()
            .iter()
            .filter(|entry| entry.kind == ra2_audio::PlayKind::Sound2d)
            .count();
        assert_eq!(clicks, 4, "one click per activation, not per mouse-down");
        println!("{}", audio.report(true));
    }

    /// At 1280×1005 the offscreen PNG is the shell's own `shot`, pixel for
    /// pixel. (Both paths use the same PNG encoder, but the check compares the
    /// decoded RGBA bytes, so an encoder change cannot hide a picture change.)
    #[test]
    fn the_offscreen_png_matches_the_shells_own_shot() {
        let Some(game) = game_dir() else { return };
        let dir = std::env::temp_dir().join(format!("ra2-app-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shot_path = dir.join("shot.png");
        let app_path = dir.join("app.png");

        let mut shell = Shell::boot(&game, SHELL_SIZE).unwrap();
        shell.click(573.5, 534.8).unwrap();
        assert_eq!(shell.screen_id(), "teamhq/select_team.gib");

        let shot = shell.shot(&shot_path).unwrap();
        assert_eq!(shot.size, [1280, 1005]);
        render_png(&mut shell, &game, &app_path, [1280, 1005]).unwrap();

        let shot = image::open(&shot_path).unwrap().into_rgba8();
        let app = image::open(&app_path).unwrap().into_rgba8();
        assert_eq!(shot.dimensions(), app.dimensions());
        assert_eq!(
            shot.as_raw(),
            app.as_raw(),
            "the offscreen frame is not the shell's own shot"
        );

        std::fs::remove_file(&shot_path).ok();
        std::fs::remove_file(&app_path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
