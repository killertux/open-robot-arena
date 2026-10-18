//! `ra2-render` CLI — render a `.gmf`, an assembled `.bot`, or an arena `.py`
//! scene to a PNG.
//!
//! Usage:
//!   ra2-render <file.gmf|file.bot|arena.py> <out.png> [width] [height]
//!
//! Env: RA2_DEBUG=1  per-part bounds
//!      RA2_OBSTACLE=<name|all|none>  RA2_HIDE=<name,...>
//!      RA2_CAM=<ex,ey,ez,tx,ty,tz>|@<n|name>  RA2_CULL=off

use std::process::ExitCode;

use ra2_render::{bounds, camera_override, collect_items, debug_print_items, mvp_matrix, Renderer};

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
    let mvp = mvp_matrix(center, radius, w, h, camera_override(input));
    let rgba = renderer.render(&items, mvp, w, h)?;

    image::save_buffer(output, &rgba, w, h, image::ColorType::Rgba8)?;
    Ok((items.len(), ntris))
}
