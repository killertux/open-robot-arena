//! Diagnostic: an arena's fixed collision bodies as the original compiles
//! them into MOPPs (docs/29 section 12v), one JSON line per body: name,
//! transform (columns), the body-local vertices and triangles, and the
//! triangles' order in its MOPP (`havok17::mopp::leaf_order`).
//!
//!     moppdump <arena.py>
use ra2sim::havok17::mopp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("usage: moppdump <arena.py>");
    let Some(h) = ra2_render::collect_havok(&path)? else {
        return Ok(());
    };
    for b in h.bodies.iter().filter(|b| b.is_static() && b.collides()) {
        let (verts, tris) = mopp::shared_vertices(&b.points);
        let order = mopp::leaf_order(&verts, &tris);
        println!(
            "{{\"name\":{:?},\"geo_type\":{},\"plane\":{},\"transform\":{:?},\"verts\":{:?},\"tris\":{:?},\"order\":{:?}}}",
            b.name,
            b.geo_type,
            b.is_plane(),
            b.transform.to_cols_array(),
            verts.iter().map(|v| v.to_array()).collect::<Vec<_>>(),
            tris,
            order
        );
    }
    Ok(())
}
