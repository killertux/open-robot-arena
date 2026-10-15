//! `scriptdump` — what the shipped arena scripts set up when they run.
//!
//! Boots the embedded interpreter against a game installation, imports the
//! game's own `Arenas` package (Python 2, translated on load), and prints what
//! each arena configures: the model, the play field, the navigation map, the
//! cameras, the shadow receivers, the hazard zones.
//!
//! Usage: `scriptdump <game-dir> [arena-name]`

use std::path::PathBuf;

use ra2script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or_else(|| "local/game".to_string());
    let which = args.next();

    let host = ScriptHost::boot(PathBuf::from(&root))?;
    pyo3::Python::with_gil(|py| -> Result<(), Box<dyn std::error::Error>> {
        let names = host.load_arenas(py)?;
        println!("{} arenas registered:", names.len());
        for n in &names {
            println!("  {n}");
        }

        let targets: Vec<String> = match which {
            Some(n) => vec![n],
            None => names.clone(),
        };
        for name in targets {
            let _arena = match host.create_arena(py, &name) {
                Ok(a) => a,
                Err(e) => {
                    println!("\n== {name}: {e}");
                    continue;
                }
            };
            let on = host.hazards_on(py, true).unwrap_or(false);
            println!("\n== {name} (hazards on: {on})");
            let data = ra2script::state().arena.clone().unwrap();
            let d = data.lock().unwrap();
            println!(
                "   model {}  extent {:?}  start points {}",
                d.model,
                d.extent,
                d.start_points.len()
            );
            println!(
                "   cameras: {} static, {} watch   shadow: {:?} + {} triangles   primitive {:?}",
                d.static_cameras.len(),
                d.watch_cameras.len(),
                d.shadow_source,
                d.shadow_triangles.len(),
                d.shadow_primitive
            );
            let walls = d.map.cells.iter().filter(|c| c.wall).count();
            let hazards = d.map.cells.iter().filter(|c| c.hazard).count();
            println!(
                "   map {}x{} @{}  altitude {:.3}  cells: {walls} walls, {hazards} hazards  hazards_on {}",
                d.map.cols,
                d.map.rows,
                d.map.resolution,
                d.map.altitude,
                d.map.hazards_on
            );
            println!(
                "   zones {:?}\n   hazards {}  collision lines {}  POVs {}  xtras {}",
                d.zones,
                d.hazards.len(),
                d.collision_lines.len(),
                d.povs.len(),
                d.xtras.len()
            );
            if !d.deferred.is_empty() {
                println!("   deferred to a later milestone: {:?}", d.deferred);
            }
        }
        Ok(())
    })
}
