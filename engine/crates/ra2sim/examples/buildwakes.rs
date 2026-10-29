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
    let pts: Vec<Vec3> = points.collect();
    if pts.is_empty() {
        return None;
    }
    // `SPHERE=1`: the broadphase's box of a moving collidable, the cube
    // around its smallest bounding sphere (docs/29 section 12u).
    if std::env::var_os("SPHERE").is_some() {
        let (c, r) = ra2sim::havok17::narrow::bounding_sphere(&pts);
        return Some((c - Vec3::splat(r), c + Vec3::splat(r)));
    }
    pts.into_iter().fold(None, |acc, p| match acc {
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
        if std::env::var_os("LATE").is_some() {
            println!("  late wakes {:?}", r.late_wakes());
            continue;
        }
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
            // `PHANTOM=1`: the phantoms' geometry too (a wheel's traction
            // ring is a collidable of its body).
            if !rotor && std::env::var_os("PHANTOM").is_some() {
                v.extend(h.phantom_pieces.iter().map(|(p, _)| p.clone()));
            }
            v
        };
        if std::env::var_os("PARTS").is_some() {
            for (p, part) in r.parts.iter().enumerate() {
                let pts: Vec<Vec3> = part
                    .members
                    .iter()
                    .flat_map(|&i| {
                        let m = r.components[i].local;
                        pieces(i, false)
                            .into_iter()
                            .flatten()
                            .map(move |q| m.transform_point3(q))
                    })
                    .collect();
                let c = aabb(pts.into_iter()).map(|(a, b)| (a + b) * 0.5);
                println!("  part {p} members {:?} centre {c:?}", part.members);
            }
        }
        if std::env::var_os("COMS").is_some() {
            // Each component's colliding bodies in its own frame: mass and
            // the hull's volume centroid (where its body sits at the origin).
            for (k, c) in r.components.iter().enumerate() {
                let h = &c.havok;
                let cen = |pts: &Vec<Vec3>| {
                    ra2sim::massprops::hull_volume_centroid(pts)
                        .map(|(_, at)| at)
                        .unwrap_or(Vec3::NAN)
                };
                let own: Vec<String> = h
                    .collision
                    .iter()
                    .zip(&h.collision_mass)
                    .map(|(p, m)| {
                        let (sc, sr) = ra2sim::havok17::narrow::bounding_sphere(p);
                        format!(
                            "{m:.1}@{:.3?} sphere {sc:.3?} r {sr:.3} n{}",
                            cen(p),
                            p.len()
                        )
                    })
                    .collect();
                let rot: Vec<String> = h
                    .rotor_collision
                    .iter()
                    .zip(&h.rotor_piece_mass)
                    .map(|(p, m)| format!("{m:.1}@{:.3?}", cen(p)))
                    .collect();
                let mesh: Vec<bool> = h.collision_mesh.iter().map(|m| m.is_some()).collect();
                println!(
                    "  c{k:2} {:24} {:5.1} kg own {own:?} mesh {mesh:?} phantoms {} rotor {rot:?}",
                    c.name,
                    c.mass,
                    h.phantom_pieces.len()
                );
            }
        }
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
            // `DIST=<d>`: the hulls' distance (GJK) in place of the boxes' gap,
            // what the detector's contact list holds (docs/29 section 12z).
            let dist: Option<f32> = std::env::var("DIST").ok().and_then(|d| d.parse().ok());
            let first = if std::env::var_os("CHASSIS").is_some() {
                0
            } else {
                1
            };
            for (p, part) in r.parts.iter().enumerate().skip(first) {
                if let Some(limit) = dist {
                    let mut sets: Vec<(usize, Vec<Vec3>)> = Vec::new();
                    for &i in part.members.iter().filter(|&&i| i < k) {
                        let m = r.components[i].local;
                        for pc in pieces(i, false) {
                            sets.push((i, pc.iter().map(|q| m.transform_point3(*q)).collect()));
                        }
                    }
                    if let Some(info) = part.joint {
                        if info.motor < k {
                            let m = r.components[info.motor].rotor_local;
                            for pc in pieces(info.motor, true) {
                                sets.push((
                                    info.motor,
                                    pc.iter().map(|q| m.transform_point3(*q)).collect(),
                                ));
                            }
                        }
                    }
                    for (i, set) in &sets {
                        for o in pieces(k, false) {
                            if set.is_empty() || o.is_empty() {
                                continue;
                            }
                            let c = ra2sim::havok17::narrow::closest(&o, set);
                            let d = c.distance;
                            // Overlapping shapes make no points (`NOOVERLAP=1`).
                            if c.overlap && std::env::var_os("NOOVERLAP").is_some() {
                                continue;
                            }
                            if d < limit {
                                let ov = if c.overlap { "!" } else { "" };
                                if std::env::var_os("PTS").is_some() {
                                    let (lo, hi) = aabb(set.iter().copied()).unwrap();
                                    println!(
                                        "     k{k} p{p}/c{i}: {d:.3} on O {:.3?} on R {:.3?}; R box {lo:.3?}..{hi:.3?}",
                                        c.on_a, c.on_b
                                    );
                                }
                                hits.push(format!(
                                    "p{p}/c{i}({d:.3}{ov} n{}/{})",
                                    o.len(),
                                    set.len()
                                ));
                            }
                        }
                    }
                    continue;
                }
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
