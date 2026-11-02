//! Diagnostic: what a robot's SmartZone holds of the other robot's chassis
//! under two rules, at chassis poses taken from a live capture (docs/29
//! section 12i): the chassis as its convex hull, and the chassis as the mesh
//! Havok collides (a triangle has to cross the zone, `0x545b50`).
//!
//! Usage: `zoneprobe <arena.py> <botA> <botB> <samples.jsonl>`
//!
//! Each sample line is `t live_ab live_ba` and then, for A and B, the live
//! chassis body's centre of mass (3) and its `+0xcc` rotation (9, its rows
//! are the body's axes in the world); `live_ab` is whether A's zone listed
//! B's chassis (`-1` unknown). Prints one line per
//! sample: `t live hull mesh` for both directions, then the agreement.
use glam::{Mat3, Quat, Vec3};
use ra2sim::{BotSetup, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bots: Vec<BotSetup> = a[1..3]
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let mut duel = Duel::new(&a[0], &bots)?;
    // The chassis body's centre of mass in its own frame.
    let offs: Vec<Vec3> = duel
        .robots
        .iter()
        .map(|r| {
            let s = duel.physics.state(r.body()).unwrap();
            let com = duel.physics.center_of_mass(r.body()).unwrap();
            if std::env::var("ZONEPROBE_SHOW").is_ok() {
                eprintln!("spawn rot {:?} com {com:?}", Mat3::from_quat(s.rotation));
            }
            s.rotation.inverse() * (com - s.position)
        })
        .collect();
    let mut agree = [[0usize; 2]; 2];
    let mut n = [0usize; 2];
    let (mut zz_n, mut zz_ok) = (0usize, 0usize);
    for line in std::fs::read_to_string(&a[3])?.lines() {
        let v: Vec<f32> = line
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if v.len() < 27 {
            continue;
        }
        let pose = |k: usize| -> (Vec3, Quat) {
            let r = &v[k + 3..k + 12];
            let m = Mat3::from_cols(
                Vec3::new(r[0], r[1], r[2]),
                Vec3::new(r[3], r[4], r[5]),
                Vec3::new(r[6], r[7], r[8]),
            );
            (
                Vec3::new(v[k], v[k + 1], v[k + 2]),
                Quat::from_mat3(&m).normalize(),
            )
        };
        let (pa, pb) = (pose(3), pose(15));
        for (i, (com, rot)) in [pa, pb].into_iter().enumerate() {
            let body = duel.robots[i].body();
            duel.physics.set_transform(body, com - rot * offs[i], rot);
        }
        let worlds: Vec<Vec<glam::Mat4>> = duel
            .robots
            .iter()
            .map(|r| r.component_worlds(&*duel.physics))
            .collect();
        let mut out = format!("{:7.3}", v[0]);
        for (dir, (r, o)) in [(0usize, 1usize), (1, 0)].into_iter().enumerate() {
            let live = v[1 + dir] as i64;
            let robot = &duel.robots[r];
            let other = &duel.robots[o];
            let ch = &other.rig.components[0];
            let pts = &ch.havok.collision[0];
            let tris = ch.havok.collision_mesh[0].as_deref();
            let (mut hull, mut mesh) = (false, false);
            for zone in &robot.rig.smart_zones {
                let frame = worlds[r][zone.component] * zone.offset;
                let to = frame.inverse() * worlds[o][0];
                hull |= ra2sim::hazards::box_touches_piece(to, zone.min, zone.max, pts, None);
                mesh |= ra2sim::hazards::box_touches_piece(to, zone.min, zone.max, pts, tris);
            }
            out += &format!("  {live} {} {}", hull as u8, mesh as u8);
            if live >= 0 {
                n[dir] += 1;
                agree[dir][0] += (hull as i64 == live) as usize;
                agree[dir][1] += (mesh as i64 == live) as usize;
            }
        }
        // The two zones against each other (both phantoms, both convex
        // boxes): column 28 of the sample, when present.
        if let (Some(za), Some(zb)) = (
            duel.robots[0].rig.smart_zones.first(),
            duel.robots[1].rig.smart_zones.first(),
        ) {
            let fa = worlds[0][za.component] * za.offset;
            let fb = worlds[1][zb.component] * zb.offset;
            let corners: Vec<Vec3> = (0..8)
                .map(|k| {
                    Vec3::new(
                        if k & 1 == 0 { zb.min.x } else { zb.max.x },
                        if k & 2 == 0 { zb.min.y } else { zb.max.y },
                        if k & 4 == 0 { zb.min.z } else { zb.max.z },
                    )
                })
                .collect();
            let zz = ra2sim::hazards::box_touches_points(
                fa.inverse() * fb,
                za.min,
                za.max,
                &corners,
                0.0,
            );
            let live = v.get(27).map(|x| *x as i64).unwrap_or(-1);
            out += &format!("  zz {live} {}", zz as u8);
            if live >= 0 {
                zz_n += 1;
                zz_ok += (zz as i64 == live) as usize;
            }
        }
        println!("{out}");
    }
    println!("zone v zone agreement {zz_ok}/{zz_n}");
    println!(
        "agreement A->B: hull {}/{} mesh {}/{}; B->A: hull {}/{} mesh {}/{}",
        agree[0][0], n[0], agree[0][1], n[0], agree[1][0], n[1], agree[1][1], n[1]
    );
    Ok(())
}
