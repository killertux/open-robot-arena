//! Time the offscreen frame path and ten match steps.
//!
//! ```text
//! cargo run -q --release -p ra2-app --example perf -- <game> [frames]
//! ```
//!
//! The frame measurement is the same loop `--offscreen` uses: resolve the shell
//! at 1280×1005, draw the UI scene at that size, and read the pixels back. The
//! match measurement builds the shipped `--match` default (Combat Arena, two AI
//! bots, DEATHMATCH) and times ten fixed `Match::step` calls.  Both numbers are
//! wall-clock on this machine; wgpu's `HighPerformance` request selects the
//! NVIDIA MX350 (Vulkan, DiscreteGpu).  This is a stills-throughput number, not
//! a frame-rate promise (docs/12-oracle.md §4).

use std::path::PathBuf;
use std::time::Instant;

use ra2_app::frame::FrameRenderer;
use ra2_app::match_mode;
use ra2_app::SHELL_SIZE;
use ra2_render::ui::UiRenderer;
use ra2_render::Renderer;
use ra2_shell::Shell;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let game = PathBuf::from(args.next().ok_or("usage: perf <game> [frames]")?);
    let frames: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(30);
    let target = [SHELL_SIZE[0] as u32, SHELL_SIZE[1] as u32];

    // --- the offscreen frame path -----------------------------------------
    // Report the adapter the same request selects, then open the renderer.
    {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))?;
        let info = adapter.get_info();
        println!(
            "adapter:         {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
    }
    let renderer = Renderer::new()?;
    let ui = UiRenderer::new(renderer.device());
    let mut shell = Shell::boot(&game, SHELL_SIZE)?;
    let mut frames_renderer = FrameRenderer::open(&game)?;
    for _ in 0..3 {
        let frame = frames_renderer.frame(&mut shell, target);
        ui.render_at(renderer.device(), renderer.queue(), &frame.scene, target)?;
    }
    let start = Instant::now();
    for _ in 0..frames {
        let frame = frames_renderer.frame(&mut shell, target);
        ui.render_at(renderer.device(), renderer.queue(), &frame.scene, target)?;
    }
    let frame_time = start.elapsed().as_secs_f64() / frames as f64;
    println!(
        "offscreen frame: {:.2} ms/frame, {:.1} frames/s ({} frames, {}x{})",
        frame_time * 1000.0,
        1.0 / frame_time,
        frames,
        target[0],
        target[1]
    );

    // --- ten match steps ---------------------------------------------------
    let options =
        match_mode::Options::new(&game, None, Vec::new(), Some(30.0), None, None, target)?;
    let mut session = match_mode::Session::new(&options)?;
    session.step(); // finish the first countdown tick outside the timing
    let start = Instant::now();
    for _ in 0..10 {
        session.step();
    }
    let step_time = start.elapsed().as_secs_f64() / 10.0;
    println!(
        "match step:      {:.3} ms/step, {:.1} steps/s (10 steps, state {:?}, clock {:.2}s)",
        step_time * 1000.0,
        1.0 / step_time,
        session.matched.state(),
        session.matched.clock()
    );
    Ok(())
}
