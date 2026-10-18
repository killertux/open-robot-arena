//! Diagnostic: settle one robot and print each component's lowest world point.
use glam::Vec3;
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut duel = Duel::new(
        &a[0],
        &[BotSetup {
            path: a[1].clone(),
            yaw_deg: None,
            spawn: None,
        }],
    )?;
    for _ in 0..(3.0 / DEFAULT_STEP) as usize {
        duel.step_direct(&[DriveInput::default()], DEFAULT_STEP);
    }
    let r = &duel.robots[0];
    let worlds = r.component_worlds(&*duel.physics);
    let tf = r.transform(&*duel.physics);
    let f = tf.transform_vector3(r.rig.forward());
    println!(
        "origin y {:.3} pitch {:.1}",
        tf.w_axis.y,
        f.y.asin().to_degrees()
    );
    for (i, c) in r.rig.components.iter().enumerate() {
        let lo = c
            .hull_points
            .iter()
            .map(|p| worlds[i].transform_point3(*p).y)
            .fold(f32::MAX, f32::min);
        let hi = c
            .hull_points
            .iter()
            .map(|p| worlds[i].transform_point3(*p).y)
            .fold(f32::MIN, f32::max);
        println!("{i:2} {:28} lowest y {lo:7.3} highest {hi:7.3}", c.name);
    }
    let _ = Vec3::ZERO;
    Ok(())
}
