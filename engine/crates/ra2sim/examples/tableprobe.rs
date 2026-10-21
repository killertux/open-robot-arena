//! Diagnostic: FlexTop's table on its own (docs/29 section 12j). Places the
//! table at a live state, lets it swing with no robot on it and prints, every
//! step, its rotation vector and angular velocity, in the form of the live
//! capture's `T` rows (`frida/ra2_dashpot.js`).
//!
//! Usage: `tableprobe <flextop.py> <steps> <qw,qx,qy,qz> <wx,wy,wz> [vx,vy,vz]`
use glam::{Quat, Vec3};
use ra2sim::duel::Duel;

fn vec(s: &str) -> Vec<f32> {
    s.split(',').map(|v| v.trim().parse().unwrap()).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let steps: usize = a[1].parse()?;
    let q = vec(&a[2]);
    let w = vec(&a[3]);
    let v = a.get(4).map(|s| vec(s)).unwrap_or(vec![0.0; 3]);
    let mut duel = Duel::new(&a[0], &[])?;
    let table = duel
        .props
        .iter()
        .find(|p| p.prop.name == "floor_collision")
        .map(|p| p.body)
        .expect("FlexTop's table");
    let rot = Quat::from_xyzw(q[1], q[2], q[3], q[0]).normalize();
    let (_, com_local, _) = duel.physics.mass_properties(table).unwrap();
    // The pivot holds: put the centre where the rotation puts it about it.
    let pivot = Vec3::new(0.0, 0.202_458_58, 0.0);
    let object_pivot = Vec3::new(0.0, 0.119_571_24, 0.0);
    let origin = pivot - rot * object_pivot;
    let _ = com_local;
    duel.physics.set_transform(table, origin, rot);
    duel.physics.set_velocity(
        table,
        Vec3::new(v[0], v[1], v[2]),
        Vec3::new(w[0], w[1], w[2]),
    );
    for i in 0..=steps {
        let s = duel.physics.state(table).unwrap();
        let r = s.rotation.to_scaled_axis();
        let c = duel.physics.center_of_mass(table).unwrap_or(s.position);
        println!(
            "{i} rv {:.4} {:.4} {:.4} w {:.4} {:.4} {:.4} com {:.4} {:.4} {:.4}",
            r.x,
            r.y,
            r.z,
            s.angular_velocity.x,
            s.angular_velocity.y,
            s.angular_velocity.z,
            c.x,
            c.y,
            c.z
        );
        duel.step_direct(&[], ra2sim::DEFAULT_STEP);
    }
    Ok(())
}
