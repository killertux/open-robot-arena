//! `ra2-render` CLI — render a `.gmf`, an assembled `.bot`, or an arena `.py`
//! scene to a PNG.
//!
//! Usage:
//!   ra2-render <file.gmf|file.bot|arena.py> <out.png> [width] [height]
//!
//! Env: RA2_DEBUG=1  per-part bounds
//!      RA2_OBSTACLE=<name|all|none>  RA2_HIDE=<name,...>
//!      RA2_CAM=<ex,ey,ez,tx,ty,tz>|@<n|name>  RA2_CULL=off
//!      RA2_STILL=<x,y,z,pitch,yaw,fov>  a script-style camera (as in an arena's
//!        `AddStaticCamera`: position, (pitch, yaw) radians, vertical fov radians)

use std::process::ExitCode;

use ra2_render::{
    bounds, camera_override, collect_items, debug_print_items, look_at_lh, mvp_matrix,
    perspective_lh, scene_background, Renderer,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: ra2-render <file.gmf|file.bot|arena.py> <out.png> [width] [height]");
        eprintln!(
            "  env: RA2_OBSTACLE=<name|all|none>  RA2_HIDE=<name,...>  RA2_CAM=<ex,ey,ez,tx,ty,tz>"
        );
        return ExitCode::FAILURE;
    }
    let input = &args[0];
    let output = &args[1];
    let w: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let h: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(768);

    match run(input, output, w, h) {
        Ok((items, tris)) => {
            println!("rendered {input} -> {output} ({w}x{h}, {items} parts, {tris} tris)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    input: &str,
    output: &str,
    w: u32,
    h: u32,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let items = collect_items(input)?;
    if items.is_empty() {
        return Err("no renderable geometry found".into());
    }
    let ntris: usize = items.iter().map(|i| i.vertices.len() / 3).sum();
    debug_print_items(&items);

    let (min, max) = bounds(&items);
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(0.001);

    let renderer = Renderer::new()?;
    if let Some(bg) = scene_background(input) {
        renderer.set_background(bg);
    }
    let mvp = match still_camera(w, h) {
        Some(m) => m,
        None => mvp_matrix(center, radius, w, h, camera_override(input)),
    };
    let rgba = renderer.render(&items, mvp, w, h)?;

    image::save_buffer(output, &rgba, w, h, image::ColorType::Rgba8)?;
    Ok((items.len(), ntris))
}

/// `RA2_STILL=x,y,z,pitch,yaw,fov`: a camera the way the arena scripts state
/// one. Pitch looks down when positive, yaw turns from +Z towards +X.
fn still_camera(w: u32, h: u32) -> Option<glam::Mat4> {
    let spec = std::env::var("RA2_STILL").ok()?;
    let f: Vec<f32> = spec
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    if f.len() < 6 {
        return None;
    }
    let eye = glam::Vec3::new(f[0], f[1], f[2]);
    let (pitch, yaw, fov) = (f[3], f[4], f[5]);
    let dir = glam::Vec3::new(
        yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    );
    // `D3DXMatrixLookAtLH` with a world-up of +Y, as the original builds its
    // views. A pitch just past the zenith (`1.5708 > pi/2`, as the overhead
    // cameras use) flips the horizontal part of the forward vector, which is
    // what turns the overhead views the right way up.
    let view = look_at_lh(eye, eye + dir, glam::Vec3::Y);
    let proj = perspective_lh(fov, w as f32 / h as f32, 0.1, 2000.0);
    Some(proj * view)
}
