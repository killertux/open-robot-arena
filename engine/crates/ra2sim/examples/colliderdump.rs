//! Diagnostic: every collision piece of a bot's rigid bodies, in the part's frame (docs/29 12at).
//!
//! Usage: `colliderdump <bot>`: one JSON line per piece: part, root component, component tag,
//! the component's name, hull vertices (the convex hull of the piece's points), mesh triangle
//! count, the part frame in the chassis frame (16 floats, column-major), and whether the piece
//! is registered with the collision-event filter (`registered`).
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let rig = ra2sim::rig::load(&a[0]).expect("load");
    for (p, tag, points, tris) in ra2sim::robot::Robot::collider_pieces(&rig) {
        let registered = tag & ra2sim::robot::UNREGISTERED == 0;
        let tag = tag & !ra2sim::robot::UNREGISTERED;
        let hull = ra2sim::massprops::hull_vertices(&points);
        let frame = rig.components[rig.parts[p].root].local.to_cols_array();
        let v: Vec<String> = hull
            .iter()
            .map(|q| format!("[{},{},{}]", q.x, q.y, q.z))
            .collect();
        let raw: Vec<String> = points
            .iter()
            .map(|q| format!("[{},{},{}]", q.x, q.y, q.z))
            .collect();
        let f: Vec<String> = frame.iter().map(|x| x.to_string()).collect();
        let ntris = tris
            .as_ref()
            .map_or("null".to_string(), |t| t.len().to_string());
        let idx = tris.as_ref().map_or("[]".to_string(), |t| {
            format!(
                "[{}]",
                t.iter()
                    .map(|q| format!("[{},{},{}]", q[0], q[1], q[2]))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        });
        println!(
            "{{\"part\":{p},\"root\":{},\"tag\":{tag},\"registered\":{registered},\"name\":\"{}\",\"class\":\"{}\",\"hull\":[{}],\"points\":{},\"raw\":[{}],\"tris\":{ntris},\"tri_idx\":{idx},\"frame\":[{}]}}",
            rig.parts[p].root,
            rig.components[tag as usize].name,
            rig.components[tag as usize].class,
            v.join(","),
            points.len(),
            raw.join(","),
            f.join(",")
        );
    }
}
