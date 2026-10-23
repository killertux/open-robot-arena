//! Diagnostic: the casts of the original's flame particles (`frida/ra2_flamecast.js`, `c_*` rows of a
//! `capture_flame_look.py` run with `CASTLOG=1`) replayed one by one against ours (docs/37 section 6.2).
//!
//! ALARM's rigid bodies are set to the pose the live run held them at (`bodies` row plus the shift the capture
//! script applied), the flame robot stands at the live pose, and each logged particle step `from -> to` is cast
//! with `Duel::flame_cast_all`. The live answer of a step is `B` (damage: a robot part hit), `F` (the floor, no
//! damage), `s` (its own robot, the particle goes on) or `.` (nothing).
//!
//! Usage: flamecasts <arena.py> <flame.bot> <alarm.bot> <live.jsonl> [gap]
use glam::{Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};
use std::collections::BTreeMap;

struct Step {
    k: i64,
    from: Vec3,
    to: Vec3,
    live: char,
    live_point: Option<Vec3>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let text = std::fs::read_to_string(&a[3])?;
    let nums = |l: &str| -> Vec<f32> {
        l.split_whitespace()
            .skip(1)
            .filter_map(|x| x.parse().ok())
            .collect()
    };
    let v3 = |x: &[f32]| Vec3::new(x[0], x[1], x[2]);
    let (mut gap, mut h, mut cs) = (0.0f32, 0.0f32, Vec3::ZERO);
    let mut bodies: Vec<(f32, Vec3, Quat)> = Vec::new();
    let mut phase: Vec<f32> = Vec::new();
    let mut frames: Vec<(f32, Vec3, Vec3)> = Vec::new();
    let mut steps: Vec<Step> = Vec::new();
    for l in text.lines() {
        let n = nums(l);
        match l.split_whitespace().next() {
            Some("gap") => gap = n[0],
            Some("heading") => h = n[0],
            Some("scout") => cs = v3(&n),
            Some("body") => {
                bodies.push((n[0], v3(&n[1..4]), Quat::from_xyzw(n[5], n[6], n[7], n[4])))
            }
            Some("phase") => phase = n,
            Some("frame") => frames.push((n[1], v3(&n[2..5]), v3(&n[5..8]))),
            Some("step") => {
                let c = l.split_whitespace().nth(2).unwrap().chars().next().unwrap();
                let m: Vec<f32> = l
                    .split_whitespace()
                    .skip(3)
                    .filter_map(|x| x.parse().ok())
                    .collect();
                steps.push(Step {
                    k: l.split_whitespace().nth(1).unwrap().parse().unwrap(),
                    from: v3(&m[0..3]),
                    to: v3(&m[3..6]),
                    live: c,
                    live_point: (c != '.').then(|| v3(&m[6..9])),
                });
            }
            _ => {}
        }
    }
    // the shift the capture script gave ALARM's bodies: its chassis to `gap` metres ahead of the flame robot's
    // chassis along the heading read before the first pin
    let alarm0 = bodies[0].1;
    let want = Vec3::new(cs.x + gap * h.sin(), alarm0.y, cs.z + gap * h.cos());
    let shift = Vec3::new(want.x - alarm0.x, 0.0, want.z - alarm0.z);
    let live: Vec<(f32, Vec3, Quat)> = bodies.iter().map(|b| (b.0, b.1 + shift, b.2)).collect();

    // ours at the live poses (as flamerange does), then ALARM's bodies moved onto the pinned ones
    let (fpos, fh, apos, ah) = (v3(&phase[0..3]), phase[3], v3(&phase[4..7]), phase[7]);
    let mut spawn = [[fpos.x, fpos.z], [apos.x, apos.z]];
    let yaws = [fh.to_degrees() + 180.0, ah.to_degrees() + 180.0];
    let mut duel = None;
    for _ in 0..2 {
        let mut d = Duel::new(
            &a[0],
            &[
                BotSetup {
                    path: a[1].clone(),
                    yaw_deg: Some(yaws[0]),
                    spawn: Some(spawn[0]),
                },
                BotSetup {
                    path: a[2].clone(),
                    yaw_deg: Some(yaws[1]),
                    spawn: Some(spawn[1]),
                },
            ],
        )?;
        let still = [DriveInput::default(), DriveInput::default()];
        for _ in 0..(3.0 / DEFAULT_STEP) as usize {
            d.step_direct(&still, DEFAULT_STEP);
        }
        for (i, sp) in spawn.iter_mut().enumerate() {
            let c = d
                .physics
                .center_of_mass(d.robots[i].body())
                .unwrap_or(Vec3::ZERO);
            let want = if i == 0 {
                [fpos.x, fpos.z]
            } else {
                [apos.x, apos.z]
            };
            sp[0] += want[0] - c.x;
            sp[1] += want[1] - c.z;
        }
        duel = Some(d);
    }
    let mut duel = duel.unwrap();
    // map every ALARM part of ours onto the live body of the same mass nearest to where the chassis map puts it
    let chassis = duel.robots[1].body();
    let cs0 = duel.physics.state(chassis).unwrap();
    let ccom = duel.physics.center_of_mass(chassis).unwrap();
    let (lm, lc, lq) = live[0];
    let _ = lm;
    let local_com = cs0.rotation.inverse() * (ccom - cs0.position);
    let map_rot = lq * cs0.rotation.inverse();
    let map = |p: Vec3| lc + map_rot * (p - ccom);
    let mut used = vec![false; live.len()];
    let n_parts = duel.robots[1].parts.len();
    for i in 0..n_parts {
        let b = duel.robots[1].parts[i].body;
        let Some(s) = duel.physics.state(b) else {
            continue;
        };
        let Some((mass, lcom, _)) = duel.physics.mass_properties(b) else {
            continue;
        };
        let com = s.position + s.rotation * lcom;
        let target = map(com);
        let (k, _) = live
            .iter()
            .enumerate()
            .filter(|(k, l)| !used[*k] && (l.0 - mass).abs() < 0.6)
            .map(|(k, l)| (k, (l.1 - target).length()))
            .fold(
                (usize::MAX, f32::MAX),
                |best, x| if x.1 < best.1 { x } else { best },
            );
        if k == usize::MAX {
            println!("part {i}: mass {mass}: no live body");
            continue;
        }
        used[k] = true;
        let q = live[k].2;
        println!(
            "  ours (mapped) vs live orientation: {:.1} deg",
            (map_rot * s.rotation).angle_between(q).to_degrees()
        );
        // the live bodies' frames are not ours (the hammer arm's differs by its start angle): the whole robot is
        // carried by the chassis' pose only
        let (pos, rot) = if i == 0 {
            (live[k].1 - q * local_com, q)
        } else {
            (map(s.position), map_rot * s.rotation)
        };
        println!(
            "part {i} mass {mass:.1} -> live body {k} (error of the chassis map {:.3} m)",
            (live[k].1 - target).length()
        );
        duel.physics.set_transform(b, pos, rot);
    }

    if std::env::var("FLAMECASTS_DUMP").is_ok() {
        let worlds = duel.robots[1].component_worlds(&*duel.physics);
        for (i, c) in duel.robots[1].rig.components.iter().enumerate() {
            let w = worlds[i];
            let mut lo = Vec3::splat(1e9);
            let mut hi = Vec3::splat(-1e9);
            for p in &c.hull_points {
                let q = w.transform_point3(*p);
                lo = lo.min(q);
                hi = hi.max(q);
            }
            println!(
                "ALARM component {i} {} attached {} at {:?} hull world {:?}..{:?} pieces {}",
                c.name,
                duel.robots[1].attached[i],
                w.transform_point3(Vec3::ZERO),
                lo,
                hi,
                c.havok.collision.len()
            );
        }
    }
    // how far the live hit points lie from ours: the nearest collision triangle of each ALARM piece, in world
    {
        let targets = duel.flame_targets();
        let mut near: BTreeMap<String, Vec<f32>> = BTreeMap::new();
        for (comp, _, _, _, live, world, sh) in &targets[1] {
            let (mut lo, mut hi) = (Vec3::splat(1e9), Vec3::splat(-1e9));
            for (pts, ..) in sh.iter() {
                for p in pts {
                    let q = world.transform_point3(*p);
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
            }
            println!(
                "TARGET {} {} live {live} pieces {} world {lo:?}..{hi:?}",
                comp,
                duel.robots[1].rig.components[*comp].name,
                sh.len()
            );
        }
        for s in steps.iter().filter(|s| s.live == 'B') {
            let p = s.live_point.unwrap();
            let mut best = (f32::MAX, 0usize);
            for (comp, _, _, _, _, world, sh) in &targets[1] {
                let lp = world.inverse().transform_point3(p);
                for (pts, tris, _) in sh.iter() {
                    for t in tris {
                        let d = tri_distance(
                            lp,
                            pts[t[0] as usize],
                            pts[t[1] as usize],
                            pts[t[2] as usize],
                        );
                        if d < best.0 {
                            best = (d, *comp);
                        }
                    }
                }
            }
            near.entry(duel.robots[1].rig.components[best.1].name.clone())
                .or_default()
                .push(best.0);
        }
        for (n, v) in &near {
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            let (lo, hi) = v
                .iter()
                .fold((f32::MAX, 0f32), |a, x| (a.0.min(*x), a.1.max(*x)));
            println!(
                "live B hits nearest to {n}: {} (distance mean {mean:.3} min {lo:.3} max {hi:.3})",
                v.len()
            );
        }
    }
    {
        let worlds = duel.robots[1].component_worlds(&*duel.physics);
        for z in &duel.robots[1].rig.smart_zones {
            let w = worlds[z.component] * z.offset;
            let inv = w.inverse();
            println!(
                "ALARM SmartZone {:?} component {} box {:?}..{:?}",
                z.name, z.component, z.min, z.max
            );
            let (mut n_from_in, mut n_to_in, mut n_b, mut surf) = (0, 0, 0, Vec::new());
            let inside = |p: Vec3| {
                let q = inv.transform_point3(p);
                q.cmpge(z.min).all() && q.cmple(z.max).all()
            };
            for s in steps.iter().filter(|s| s.live == 'B') {
                n_b += 1;
                n_from_in += inside(s.from) as usize;
                n_to_in += inside(s.to) as usize;
                let q = inv.transform_point3(s.live_point.unwrap());
                let d = (q - q.clamp(z.min, z.max)).length();
                let inner = (q.min(z.max) - z.max)
                    .abs()
                    .min_element()
                    .min((q.max(z.min) - z.min).abs().min_element());
                surf.push(if d > 0.0 { d } else { inner });
            }
            let mean = surf.iter().sum::<f32>() / surf.len().max(1) as f32;
            let worst = surf.iter().fold(0f32, |a, b| a.max(*b));
            println!("  live B steps {n_b}: from inside {n_from_in}, to inside {n_to_in}, hit point distance to the box surface mean {mean:.4} max {worst:.4}");
        }
    }
    if std::env::var("FLAMECASTS_LOCAL").is_ok() {
        let worlds = duel.robots[1].component_worlds(&*duel.physics);
        let hh = duel.robots[1]
            .rig
            .components
            .iter()
            .position(|c| c.name == "Hammer Head")
            .unwrap();
        let inv = worlds[hh].inverse();
        let c = &duel.robots[1].rig.components[hh];
        let (mut lo, mut hi) = (Vec3::splat(1e9), Vec3::splat(-1e9));
        for p in &c.hull_points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        println!("Hammer Head hull local {lo:?}..{hi:?}");
        for p in &c.hull_points {
            println!(
                "HULLW local {p:?} world {:?}",
                worlds[hh].transform_point3(*p)
            );
        }
        println!("HH world matrix {:?}", worlds[hh]);
        let (mut plo, mut phi) = (Vec3::splat(1e9), Vec3::splat(-1e9));
        for s in steps.iter().filter(|s| s.live == 'B') {
            let q = inv.transform_point3(s.live_point.unwrap());
            println!("LOCAL {:.3} {:.3} {:.3}", q.x, q.y, q.z);
            plo = plo.min(q);
            phi = phi.max(q);
        }
        println!("live B hit points in Hammer Head local {plo:?}..{phi:?}");
    }
    if !frames.is_empty() {
        // the original's emitter frames (nozzle, velocity, dt) through our particle law and cast
        let total: f32 = frames.iter().map(|f| f.0).sum();
        let live_b = steps.iter().filter(|s| s.live == 'B').count();
        let reps: usize = std::env::var("FLAMECASTS_REPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let mut sum = 0usize;
        for rep in 0..reps {
            let dy: f32 = std::env::var("FLAMECASTS_DY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let shifted: Vec<_> = frames
                .iter()
                .map(|f| (f.0, f.1 + Vec3::Y * dy, f.2))
                .collect();
            let hits = duel.replay_jet(0, &shifted);
            let mut by_c: BTreeMap<String, usize> = BTreeMap::new();
            for (_, v, c, _) in &hits {
                *by_c
                    .entry(duel.robots[*v].rig.components[*c].name.clone())
                    .or_default() += 1;
            }
            sum += hits.len();
            println!("replay {rep}: {} frames, {total:.2} s of dt: ours {} hits ({by_c:?}), live {live_b} body hits", frames.len(), hits.len());
        }
        let _ = sum;
    }
    let segs: Vec<(Vec3, Vec3)> = steps.iter().map(|s| (s.from, s.to)).collect();
    let ours = duel.flame_cast_all(0, &segs);
    let mut table = BTreeMap::new();
    let mut shown = 0;
    for (s, o) in steps.iter().zip(&ours) {
        // ours: a part of ALARM (robot 1) is a B; a part of the flame robot is not cast (owner)
        let oc = if o.is_some() { 'B' } else { '.' };
        *table.entry((s.live, oc)).or_insert(0usize) += 1;
        let interesting = (s.live == 'B') != (oc == 'B');
        if interesting && shown < 40 && std::env::var("FLAMECASTS_LIST").is_ok() {
            shown += 1;
            let name = o
                .map(|(v, c, ..)| duel.robots[v].rig.components[c].name.clone())
                .unwrap_or_default();
            println!(
                "k {} live {} ours {} {name} len {:.3} from {:?} to {:?} live hit {:?} ours {:?}",
                s.k,
                s.live,
                oc,
                (s.to - s.from).length(),
                s.from,
                s.to,
                s.live_point,
                o.map(|x| x.2)
            );
        }
    }
    println!("live x ours over {} logged steps:", steps.len());
    for ((l, o), n) in &table {
        println!("  live {l} ours {o}: {n}");
    }
    // by component for the live B steps and for ours
    let mut comp_live: BTreeMap<String, usize> = BTreeMap::new();
    let mut comp_ours: BTreeMap<String, usize> = BTreeMap::new();
    for (s, o) in steps.iter().zip(&ours) {
        if let Some((v, c, ..)) = o {
            *comp_ours
                .entry(duel.robots[*v].rig.components[*c].name.clone())
                .or_default() += 1;
        }
        let _ = s;
    }
    let _ = &mut comp_live;
    println!("ours by component (every step): {comp_ours:?}");
    Ok(())
}

/// Distance from `p` to triangle `abc`.
fn tri_distance(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (p - a).length();
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (p - b).length();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return (p - (a + ab * (d1 / (d1 - d3)))).length();
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (p - c).length();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return (p - (a + ac * (d2 / (d2 - d6)))).length();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).length();
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    (p - (a + ab * v + ac * w)).length()
}
