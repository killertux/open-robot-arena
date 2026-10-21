//! Diagnostic: each robot's chassis body as built: mass and centre of mass in
//! the robot frame (for comparison with `assembly-*__live__000.json`).
//!
//! Usage: `comprobe <arena.py> <bot>...`
use ra2sim::{BotSetup, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    for bot in &a[1..] {
        let duel = Duel::new(
            &a[0],
            &[BotSetup {
                path: bot.clone(),
                yaw_deg: None,
                spawn: None,
            }],
        )?;
        let r = &duel.robots[0];
        for (i, c) in r.rig.components.iter().enumerate() {
            let m = r.rig.part_local[i];
            if !c.havok.phantom.is_empty() {
                let top = c
                    .havok
                    .phantom
                    .iter()
                    .map(|q| m.transform_point3(*q).y)
                    .fold(f32::MIN, f32::max);
                println!("  comp {i} {} phantom top {top:.3}", c.class);
            }
            for (k, pts) in c.havok.collision.iter().enumerate() {
                let local: Vec<glam::Vec3> = pts.iter().map(|q| m.transform_point3(*q)).collect();
                let tris = c
                    .havok
                    .collision_mesh
                    .get(k)
                    .cloned()
                    .flatten()
                    .unwrap_or_default();
                let [mean, bx, surf] = candidates(&local, &tris);
                let vol = ra2sim::massprops::hull_volume_centroid(&local).map(|v| v.1);
                let top = local.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                let low = local.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                println!(
                    "  comp {i} {} {} piece {k}: n {} tris {} y {low:.3}..{top:.3} mean {mean:.4} box {bx:.4} surf {surf:.4} hull {vol:.4?}",
                    c.class,
                    c.name,
                    local.len(),
                    tris.len()
                );
            }
        }
        for (p, part) in r.parts.iter().enumerate() {
            let s = duel.physics.state(part.body).unwrap();
            let com = duel.physics.center_of_mass(part.body).unwrap();
            let frame = r.rig.components[r.rig.parts[p].root].local;
            let local = s.rotation.inverse() * (com - s.position);
            let bot_space = frame.transform_point3(local);
            println!(
                "{bot} part {p} mass {:.2} com_bot ({:.4}, {:.4}, {:.4})",
                duel.physics.mass(part.body).unwrap_or(0.0),
                bot_space.x,
                bot_space.y,
                bot_space.z
            );
            if p == 0 {
                if let Some((_, _, inv)) = duel.physics.mass_properties(part.body) {
                    let i = inv.inverse();
                    println!("{bot} part 0 inertia {:?}", i.to_cols_array_2d());
                }
            }
        }
    }
    Ok(())
}

/// Candidate centres of a triangle mesh: vertex mean, box centre, surface
/// (area-weighted) centroid.
#[allow(dead_code)]
pub fn candidates(pts: &[glam::Vec3], tris: &[[u32; 3]]) -> [glam::Vec3; 3] {
    let n = pts.len().max(1) as f32;
    let mean = pts.iter().copied().sum::<glam::Vec3>() / n;
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for p in pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let (mut area, mut moment) = (0.0f32, glam::Vec3::ZERO);
    for t in tris {
        let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        let ar = 0.5 * (b - a).cross(c - a).length();
        area += ar;
        moment += ar * (a + b + c) / 3.0;
    }
    let (mut v, mut mv) = (0.0f32, glam::Vec3::ZERO);
    for t in tris {
        let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        let d = a.dot(b.cross(c)) / 6.0;
        v += d;
        mv += d * (a + b + c) / 4.0;
    }
    println!("    div volume {v:.5} centre {:.4}", mv / v);
    [mean, (lo + hi) * 0.5, moment / area.max(1e-9)]
}
