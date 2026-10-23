//! Diagnostic (docs/29 section 12y): the original builds a robot one component
//! per frame at the arena's origin, and each component's body first appears
//! there in its own frame, active, 58 substeps before it is moved onto the
//! robot. This prints, per component in `.bot` order, the bounding box of its
//! collision pieces in its own frame and every movable body of the robot built
//! so far (bot space) whose box overlaps it, with the gap between the boxes
//! along each axis (negative: overlap).
//!
//! Usage: `buildwakes <bot>...`

use glam::Vec3;
use ra2sim::rig;

fn aabb(points: impl Iterator<Item = Vec3>) -> Option<(Vec3, Vec3)> {
    points.fold(None, |acc, p| match acc {
        None => Some((p, p)),
        Some((lo, hi)) => Some((lo.min(p), hi.max(p))),
    })
}

fn main() {
    for path in std::env::args().skip(1) {
        let r = match rig::load(&path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        println!("robot {:?} ({} components)", r.name, r.components.len());
        // Every collision piece of component `i` (each its own collidable),
        // in the component's frame.
        let pieces = |i: usize, rotor: bool| -> Vec<Vec<Vec3>> {
            let h = &r.components[i].havok;
            let mut v: Vec<Vec<Vec3>> = Vec::new();
            if !rotor {
                v.extend(h.collision.iter().cloned());
            }
            if rotor || h.collision.is_empty() {
                v.extend(h.rotor_collision.iter().cloned());
            }
            if v.is_empty() && !rotor && !r.components[i].hull_points.is_empty() {
                v.push(r.components[i].hull_points.clone());
            }
            v
        };
        let margin: f32 = std::env::var("MARGIN")
            .ok()
            .and_then(|m| m.parse().ok())
            .unwrap_or(0.0);
        for k in 0..r.components.len() {
            let c = &r.components[k];
            let own: Vec<(Vec3, Vec3)> = pieces(k, false)
                .into_iter()
                .filter_map(|p| aabb(p.into_iter()))
                .collect();
            if own.is_empty() {
                println!("  {k:2} {:24} no geometry", c.name);
                continue;
            }
            let mut hits = Vec::new();
            for (p, part) in r.parts.iter().enumerate().skip(1) {
                if part.root >= k {
                    continue;
                }
                let mut boxes: Vec<(usize, (Vec3, Vec3))> = Vec::new();
                for &i in part.members.iter().filter(|&&i| i < k) {
                    let m = r.components[i].local;
                    for pc in pieces(i, false) {
                        if let Some(b) = aabb(pc.iter().map(|q| m.transform_point3(*q))) {
                            boxes.push((i, b));
                        }
                    }
                }
                if let Some(info) = part.joint {
                    if info.motor < k {
                        let m = r.components[info.motor].rotor_local;
                        for pc in pieces(info.motor, true) {
                            if let Some(b) = aabb(pc.iter().map(|q| m.transform_point3(*q))) {
                                boxes.push((info.motor, b));
                            }
                        }
                    }
                }
                for (i, bp) in &boxes {
                    for bk in &own {
                        let gap = (bp.0 - bk.1).max(bk.0 - bp.1);
                        if gap.max_element() < margin {
                            hits.push(format!("p{p}/c{i}({:.2},{:.2},{:.2})", gap.x, gap.y, gap.z));
                        }
                    }
                }
            }
            println!("  {k:2} {:24} {:5.1} kg {}", c.name, c.mass, hits.join(" "));
        }
    }
}
