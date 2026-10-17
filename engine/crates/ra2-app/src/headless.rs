//! The headless path: the same frame builder, written to a PNG.
//!
//! `--offscreen` boots the shell, applies the click script in order, builds the
//! final frame with [`crate::frame`], draws it with `ra2-render`'s UI pipeline
//! and reads it back. It is the CI entry point for M7 and the proof that the
//! windowed loop has no logic of its own.

use std::path::Path;

use ra2_shell::{Shell, Step};

use crate::frame::FrameRenderer;
use crate::SHELL_SIZE;

/// Apply a click script, in order, at the shell's own coordinates.
pub fn apply_clicks(shell: &mut Shell, clicks: &[[f32; 2]]) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for &[x, y] in clicks {
        steps.push(shell.click(x, y)?);
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
pub fn run(game: &Path, out: &Path, size: [u32; 2], clicks: &[[f32; 2]]) -> Result<(), String> {
    let mut shell = Shell::boot(game, SHELL_SIZE)?;
    println!(
        "booted {} from {} ({}x{})",
        shell.screen_id(),
        game.display(),
        shell.size()[0],
        shell.size()[1]
    );
    let steps = apply_clicks(&mut shell, clicks)?;
    for step in &steps {
        println!("{}", crate::describe(step));
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

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
