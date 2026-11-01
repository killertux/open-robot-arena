//! Replays the distance queries of a live capture (`tools/oracle/gjk_cases.py`)
//! through `narrow::closest_havok_with` and compares the final simplex, the
//! distance and the hull-side point (docs/29 section 12ak).
//!
//! Usage: `gjkreplay <cases.txt> [brute|adj|full] [verbose]`
use std::collections::HashMap;

use glam::{Mat3, Vec3};
use ra2sim::havok17::narrow::closest_havok_with;

struct Shape {
    verts: Vec<Vec3>,
    adj: Option<Vec<Vec<u16>>>,
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let text = std::fs::read_to_string(&a[0]).expect("cases");
    let mode = a.get(1).map_or("adj", |m| m.as_str());
    let verbose = a.get(2).is_some();
    let mut shapes: HashMap<String, Shape> = HashMap::new();
    let (mut n, mut same_set, mut same_n, mut near, mut worst) = (0, 0, 0, 0, 0.0f32);
    let mut by_hull: HashMap<usize, (usize, usize)> = HashMap::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        match t[0] {
            "S" => {
                let count: usize = t[2].parse().unwrap();
                let f = |i: usize| t[3 + i].parse::<f32>().unwrap();
                let verts: Vec<Vec3> = (0..count)
                    .map(|i| Vec3::new(f(3 * i), f(3 * i + 1), f(3 * i + 2)))
                    .collect();
                let mut adj = None;
                let rest = 3 + 3 * count;
                if t.get(rest) == Some(&"adj") {
                    let mut k = rest + 2;
                    let mut lists = Vec::new();
                    for _ in 0..count {
                        let len: usize = t[k].parse().unwrap();
                        lists.push(
                            t[k + 1..k + 1 + len]
                                .iter()
                                .map(|s| s.parse().unwrap())
                                .collect(),
                        );
                        k += 1 + len;
                    }
                    adj = Some(lists);
                }
                shapes.insert(t[1].to_string(), Shape { verts, adj });
            }
            "J" => {
                let Some(shape) = shapes.get(t[1]) else {
                    continue;
                };
                let start: usize = t[3].parse().unwrap();
                let cnt: usize = t[4].parse().unwrap();
                let ia: Vec<usize> = (0..cnt).map(|i| t[5 + i].parse().unwrap()).collect();
                let mut k = 5 + 2 * cnt;
                let dist: f32 = t[k].parse().unwrap();
                k += 1;
                let nums: Vec<f32> = t[k..]
                    .iter()
                    .take(12 + 9 + 3)
                    .map(|s| s.parse().unwrap())
                    .collect();
                let rot = Mat3::from_cols(
                    Vec3::new(nums[0], nums[1], nums[2]),
                    Vec3::new(nums[3], nums[4], nums[5]),
                    Vec3::new(nums[6], nums[7], nums[8]),
                );
                let tr = Vec3::new(nums[9], nums[10], nums[11]);
                let tri: Vec<Vec3> = (0..3)
                    .map(|i| Vec3::new(nums[12 + 3 * i], nums[13 + 3 * i], nums[14 + 3 * i]))
                    .collect();
                let on_a = Vec3::new(nums[21], nums[22], nums[23]);
                let world: Vec<Vec3> = shape.verts.iter().map(|v| rot * *v + tr).collect();
                let mut cur = start;
                let full: Vec<Vec<u16>> = (0..shape.verts.len())
                    .map(|i| {
                        (0..shape.verts.len() as u16)
                            .filter(|&j| j as usize != i)
                            .collect()
                    })
                    .collect();
                let adj = match mode {
                    "brute" => None,
                    "full" => Some(full.as_slice()),
                    _ => shape.adj.as_deref(),
                };
                let c = closest_havok_with(&world, &tri, tr, adj, &mut cur);
                n += 1;
                let mine: Vec<usize> = c.simplex_a[..c.size].iter().map(|&i| i as usize).collect();
                let mut sa = ia.clone();
                sa.sort();
                sa.dedup();
                let mut sm = mine.clone();
                sm.sort();
                sm.dedup();
                let local = rot.transpose() * (c.on_a - tr);
                let diff = local.distance(on_a);
                let ok_set = sa == sm;
                same_set += ok_set as usize;
                same_n += (c.size == cnt) as usize;
                near += (diff < 2e-3) as usize;
                worst = worst.max(diff);
                let e = by_hull.entry(shape.verts.len()).or_default();
                e.0 += 1;
                e.1 += (ok_set && diff < 2e-3) as usize;
                if verbose && !(ok_set && diff < 2e-3) {
                    println!(
                        "MISS {} seq {} start {start} live ia {ia:?} d {dist:.4} onA {on_a:?} | ours ia {mine:?} d {:.4} onA {local:?}",
                        t[1], t[2], c.distance
                    );
                }
            }
            _ => {}
        }
    }
    println!(
        "{n} queries: same vertex set {same_set}, same size {same_n}, hull point within 2 mm {near}, worst {worst:.4}"
    );
    let mut h: Vec<_> = by_hull.into_iter().collect();
    h.sort();
    for (c, (t, ok)) in h {
        println!("  hull of {c}: {ok} of {t} fully matched");
    }
}
