//! Diagnostic: a Steering unit's wheel constraint, call by call, on the live
//! bodies (docs/29 section 12y). `tools/oracle/wheelcmp.py` turns a
//! `wheel-*` capture (`frida/ra2_wheel.js`) into one block per solver call:
//!
//! ```text
//! C <id> <dt> <A: com3 quat4(w x y z) v3 w3 invmass invI9> <B: same>
//!   <anchorA3> <anchorB3> <axleA3> <axleB3> <suspensionA3> <passes>
//! P <tau> <damping> <vA3> <wA3> <vB3> <wB3>      (one per pass, the live entry)
//! ```
//!
//! and this prints `O <id> <k> vA3 wA3 vB3 wB3` after the set-up (k = 0) and
//! after each pass (k = 1..), from `havok17::joints::replay_wheel`.
//!
//! Usage: `wheelreplay < calls.txt`

use std::io::{BufRead, Write};

use glam::{Mat3, Quat, Vec3};
use ra2sim::havok17::joints::{replay_wheel, ReplayBody};

fn v(x: &[f32]) -> Vec3 {
    Vec3::new(x[0], x[1], x[2])
}

fn body(x: &[f32]) -> ReplayBody {
    ReplayBody {
        com: v(&x[0..3]),
        rotation: Quat::from_xyzw(x[4], x[5], x[6], x[3]).normalize(),
        velocity: v(&x[7..10]),
        angular_velocity: v(&x[10..13]),
        inv_mass: x[13],
        // Rows of the world inverse inertia (`+0x10c`, stride 16).
        inv_inertia_world: Mat3::from_cols(v(&x[14..17]), v(&x[17..20]), v(&x[20..23])).transpose(),
    }
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    let mut pending: Option<(String, Vec<f32>, usize)> = None;
    let mut passes: Vec<(f32, f32, [Vec3; 4])> = Vec::new();
    let flush = |pending: &mut Option<(String, Vec<f32>, usize)>,
                 passes: &mut Vec<(f32, f32, [Vec3; 4])>,
                 out: &mut dyn Write| {
        if let Some((id, x, _)) = pending.take() {
            let a = body(&x[1..24]);
            let b = body(&x[24..47]);
            let r = replay_wheel(
                [a, b],
                [v(&x[47..50]), v(&x[50..53])],
                [v(&x[53..56]), v(&x[56..59])],
                v(&x[59..62]),
                x[0],
                passes,
            );
            for (k, s) in r.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "O {id} {k} {} {} {} {} {} {} {} {} {} {} {} {}",
                    s[0].x,
                    s[0].y,
                    s[0].z,
                    s[1].x,
                    s[1].y,
                    s[1].z,
                    s[2].x,
                    s[2].y,
                    s[2].z,
                    s[3].x,
                    s[3].y,
                    s[3].z
                );
            }
            passes.clear();
        }
    };
    for line in stdin.lock().lines().map_while(Result::ok) {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("C") => {
                flush(&mut pending, &mut passes, &mut out);
                let id = it.next().unwrap_or("?").to_string();
                let x: Vec<f32> = it.filter_map(|t| t.parse().ok()).collect();
                if x.len() >= 63 {
                    let n = x[62] as usize;
                    pending = Some((id, x, n));
                }
            }
            Some("P") => {
                let x: Vec<f32> = it.filter_map(|t| t.parse().ok()).collect();
                if x.len() >= 14 {
                    passes.push((
                        x[0],
                        x[1],
                        [v(&x[2..5]), v(&x[5..8]), v(&x[8..11]), v(&x[11..14])],
                    ));
                }
            }
            _ => {}
        }
    }
    flush(&mut pending, &mut passes, &mut out);
}
