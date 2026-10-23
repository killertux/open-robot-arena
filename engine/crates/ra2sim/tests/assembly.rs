//! Robot assembly against the original, measured live.
//!
//! `tests/data/assembly_live.tsv` holds, for 14 stock robots, the position of
//! every component mesh as the original built the robot (its first physics
//! step), in the chassis frame: captured with `tools/oracle/capture_assembly.py`
//! (Frida, the game's own scene nodes) and turned into the table by
//! `tools/oracle/assembly_fixture.py` (`docs/29-havok17-backend.md` section 8).
//!
//! A `fixed` row is pinned to 1 cm. A `spin` row sits on a body a free or
//! spinning hinge carries (a spin motor's rotor, an axle mount, a steering
//! hub), which the original has already turned by its first step; there only
//! the distance along the hinge axis and from it are pinned, also to 1 cm.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glam::Vec3;

const TOLERANCE: f32 = 0.01;

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(PathBuf::from(dir));
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for up in [3usize, 4] {
        let mut p = here.to_path_buf();
        for _ in 0..up {
            p.pop();
        }
        let p = p.join("local/game");
        if p.join("Components").is_dir() {
            return Some(p);
        }
    }
    None
}

struct Row {
    bot: String,
    component: usize,
    object: String,
    spin: bool,
    live: Vec3,
}

fn rows() -> Vec<Row> {
    let text = include_str!("data/assembly_live.tsv");
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let n = |i: usize| f[i].trim().parse::<f32>().expect("number");
            Row {
                bot: f[0].to_string(),
                component: f[1].parse().expect("component"),
                object: f[2].to_ascii_lowercase(),
                spin: f[3] == "spin",
                live: Vec3::new(n(4), n(5), n(6)),
            }
        })
        .collect()
}

/// Distance along the axis from the pivot, and from the axis.
fn invariants(p: Vec3, pivot: Vec3, axis: Vec3) -> (f32, f32) {
    let axis = axis.normalize();
    let d = p - pivot;
    let along = d.dot(axis);
    (along, (d - axis * along).length())
}

#[test]
fn components_sit_where_the_original_builds_them() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return;
    };
    let rows = rows();
    let mut by_bot: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for r in &rows {
        by_bot.entry(r.bot.clone()).or_default().push(r);
    }
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (bot, rows) in &by_bot {
        let path = game.join(bot);
        let path = path.to_str().expect("utf-8 path");
        let rig = ra2sim::rig::load(path).expect("rig");
        let (_bot, _root, loaded) = ra2_render::load_bot_components(path).expect("bot");
        let (mut worst, mut sum) = (0.0f32, 0.0f32);
        for r in rows {
            let c = &rig.components[r.component];
            let scene = &loaded[r.component].as_ref().expect("component mesh").scene;
            let object = scene
                .objects
                .iter()
                .find(|o| o.name.eq_ignore_ascii_case(&r.object))
                .unwrap_or_else(|| panic!("{bot}: no object {} on c{}", r.object, r.component));
            let on_rotor = c
                .hinge
                .as_ref()
                .is_some_and(|h| h.rotor.eq_ignore_ascii_case(&r.object));
            let frame = if on_rotor { c.rotor_local } else { c.local };
            // The original draws a component's own mesh at the component's
            // frame and an object that is a body of its own (a motor's rotor,
            // a steering hub: what an attachment point is hosted on) at that
            // object's transform.
            let own_body = on_rotor
                || scene.attachment_table().values().any(|a| {
                    a.host
                        .as_ref()
                        .is_some_and(|h| h.eq_ignore_ascii_case(&r.object))
                });
            let tm = if own_body {
                ra2_render::object_transform(object)
            } else {
                glam::Mat4::IDENTITY
            };
            let ours = (frame * tm).transform_point3(Vec3::ZERO);
            let err = if r.spin {
                let joint = if on_rotor {
                    rig.parts
                        .iter()
                        .filter_map(|p| p.joint)
                        .find(|j| j.motor == r.component)
                } else {
                    rig.parts[rig.part_of[r.component]].joint
                };
                let Some(j) = joint else {
                    failures.push(format!(
                        "{bot}: c{} {} has no hinge in our rig",
                        r.component, r.object
                    ));
                    continue;
                };
                let (a, d) = invariants(r.live, j.pivot, j.axis);
                let (a2, d2) = invariants(ours, j.pivot, j.axis);
                (a - a2).hypot(d - d2)
            } else {
                (ours - r.live).length()
            };
            worst = worst.max(err);
            sum += err;
            checked += 1;
            if err > TOLERANCE {
                failures.push(format!(
                    "{bot}: c{} {} ({}) off by {err:.3} m: ours {ours:?}, live {:?}",
                    r.component,
                    r.object,
                    if r.spin { "spin" } else { "fixed" },
                    r.live
                ));
            }
        }
        println!(
            "{bot}: {} objects, mean {:.4} m, worst {worst:.4} m",
            rows.len(),
            sum / rows.len() as f32
        );
    }
    assert!(checked > 250, "only {checked} objects checked");
    assert!(
        failures.is_empty(),
        "{} misplaced:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
