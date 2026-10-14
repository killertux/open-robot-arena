//! Print each component's world origin and world axes under every mount rule.
//!
//! `cargo run --release --example mountdump -- <bot> [component-index ...]`
//!
//! F1 (docs/06-deferred-fidelity.md): a numeric read-out of what each candidate
//! placement rule produces, so rules can be compared without rendering.

use glam::Vec3;
use ra2_render::{component_worlds_mounted, load_bot_components, MountMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = args.first().cloned().unwrap_or_default();
    let want: Vec<usize> = args[1..].iter().filter_map(|s| s.parse().ok()).collect();
    let (bot, _root, loaded) = load_bot_components(&input)?;
    println!(
        "{} v{}: {} components",
        bot.name,
        bot.version,
        bot.components.len()
    );
    let modes = [
        ("stored", MountMode::Stored),
        ("frames", MountMode::Frames),
        ("frames-flip", MountMode::FramesFlipped),
        ("stored-own", MountMode::StoredOwn),
        ("frames-stored", MountMode::FramesStored),
    ];
    for (name, mode) in modes {
        println!("=== {name} ===");
        let world = component_worlds_mounted(&bot, &loaded, mode);
        let mut below = f32::MAX;
        for (i, c) in bot.components.iter().enumerate() {
            let m = world[i];
            let o = m.transform_point3(Vec3::ZERO);
            below = below.min(o.y);
            if !want.is_empty() && !want.contains(&i) {
                continue;
            }
            let ax = |v: Vec3| {
                let a = m.transform_vector3(v);
                format!("({:5.2},{:5.2},{:5.2})", a.x, a.y, a.z)
            };
            let file = loaded
                .get(i)
                .and_then(|l| l.as_ref())
                .map(|l| l.label.clone())
                .unwrap_or_default();
            println!(
                "  {i:2} p={:>3} at={:6.3},{:6.3},{:6.3} X={} Y={} Z={}  {}",
                c.parent,
                o.x,
                o.y,
                o.z,
                ax(Vec3::X),
                ax(Vec3::Y),
                ax(Vec3::Z),
                file
            );
        }
        println!("  --> lowest component origin y = {below:.3}");
    }
    Ok(())
}
