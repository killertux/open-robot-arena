//! The order Havok 1.x's MOPP returns a fixed mesh's triangles in, for every
//! fixed mesh of the eight stock arenas of the Arena list, against the trees
//! the original compiled (`mopp-scout-alarm-arenas__live__000`, the leaf
//! orders in `tests/data/mopp_live.tsv`; docs/29 section 12v). Needs the
//! game's files (`local/game` or `RA2_GAME`); skipped without them.

use std::path::{Path, PathBuf};

use ra2sim::havok17::mopp;

fn game_dir() -> Option<PathBuf> {
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

#[test]
fn every_stock_arena_mesh_gets_the_live_mopp_order() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game files");
        return;
    };
    let table = include_str!("data/mopp_live.tsv");
    let mut checked = 0;
    for line in table
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split('\t').collect();
        let (script, body, n) = (f[0], f[1], f[2].parse::<usize>().unwrap());
        let live: Vec<u32> = f[3].split(',').map(|s| s.parse().unwrap()).collect();
        let path = game.join("Arenas").join(script);
        let arena = ra2_render::collect_havok(path.to_str().unwrap())
            .unwrap()
            .expect("the arena's rigid bodies");
        let b = arena
            .bodies
            .iter()
            .find(|b| b.name == body)
            .unwrap_or_else(|| panic!("{script}: no body {body}"));
        let (verts, tris) = mopp::shared_vertices(&b.points);
        assert_eq!(tris.len(), n, "{script} {body}");
        assert_eq!(mopp::leaf_order(&verts, &tris), live, "{script} {body}");
        checked += 1;
    }
    assert_eq!(checked, 21);
}
