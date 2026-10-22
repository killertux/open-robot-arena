//! Diagnostic: the floor's height under a point of an arena, on a grid
//! (`floorprobe <arena.py> <x> <z> <half> <n>`): what the physics stands on.
use glam::Vec3;
use ra2sim::Duel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let x: f32 = a[1].parse()?;
    let z: f32 = a[2].parse()?;
    let half: f32 = a[3].parse()?;
    let n: i32 = a[4].parse()?;
    let duel = Duel::new(&a[0], &[])?;
    for j in -n..=n {
        let mut row = String::new();
        for i in -n..=n {
            let p = Vec3::new(
                x + half * i as f32 / n as f32,
                std::env::var("FROM")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(20.0),
                z + half * j as f32 / n as f32,
            );
            match duel.physics.ray_cast(p, -Vec3::Y, 60.0) {
                Some(h) => row += &format!(" {:8.4}", h.point.y),
                None => row += "        -",
            }
        }
        println!("z {:+.2}:{row}", z + half * j as f32 / n as f32);
    }
    Ok(())
}
