//! Diagnostic: every component's rest rotation (bot frame) as a quaternion
//! (w x y z) with the hinge axis of the parts it roots, to compare with the
//! original's body frames (`spinup-*` `B` rows) and its hinge atoms' axes in
//! body B (`hingectl-*` `L` rows, docs/29 section 12af).
//!
//! Usage: `wheelframes <bot>`
use glam::{Mat3, Quat, Vec3};

fn main() {
    let path = std::env::args().nth(1).expect("usage: wheelframes <bot>");
    let r = ra2sim::rig::load(&path).expect("load");
    for (p, part) in r.parts.iter().enumerate() {
        let c = &r.components[part.root];
        let q = Quat::from_mat3(&Mat3::from_mat4(c.local)).normalize();
        let qr = Quat::from_mat3(&Mat3::from_mat4(c.rotor_local)).normalize();
        let m = Mat3::from_mat4(c.local);
        println!(
            "   cols x=({:.3} {:.3} {:.3}) y=({:.3} {:.3} {:.3}) z=({:.3} {:.3} {:.3}) det {:.3}",
            m.x_axis.x,
            m.x_axis.y,
            m.x_axis.z,
            m.y_axis.x,
            m.y_axis.y,
            m.y_axis.z,
            m.z_axis.x,
            m.z_axis.y,
            m.z_axis.z,
            m.determinant()
        );
        if let Some(par) = c.parent {
            if let Some(a) = r.components[par].attachments.get(&c.parent_mount) {
                let t = a.transform;
                let mm = Mat3::from_cols(Vec3::from(t[0]), Vec3::from(t[1]), Vec3::from(t[2]));
                let f = Mat3::from_mat4(r.components[par].local) * mm;
                let qf = Quat::from_mat3(&f).normalize();
                println!(
                    "   mount frame x={:?} y={:?} z={:?} -> parent*mount q=({:.4} {:.4} {:.4} {:.4})",
                    t[0], t[1], t[2], qf.w, qf.x, qf.y, qf.z
                );
            }
        }
        if let (Some(par), Some(j)) = (c.parent, part.joint) {
            if let Some(h) = r.components[par].hinge.as_ref() {
                let m = Mat3::from_mat4(h.rotor_tm);
                let rb = Mat3::from_mat4(c.local) * m;
                let ab = rb.transpose() * (j.axis * h.axis.length());
                println!(
                    "   rotor_tm {:?} hinge axis {:?}",
                    h.rotor_tm.to_cols_array(),
                    h.axis
                );
                println!(
                    "   predicted axis in B = ({:.2} {:.2} {:.2})",
                    ab.x, ab.y, ab.z
                );
            }
        }
        let pf = c
            .parent
            .map(|i| r.components[i].file.clone())
            .unwrap_or_default();
        println!("   parentfile {pf}");
        println!(
            "part {p} root #{} {:?} local q(w x y z) = ({:.4} {:.4} {:.4} {:.4}) rotor q = ({:.4} {:.4} {:.4} {:.4}) pos=({:.3} {:.3} {:.3})",
            part.root, c.name, q.w, q.x, q.y, q.z, qr.w, qr.x, qr.y, qr.z,
            c.local.w_axis.x, c.local.w_axis.y, c.local.w_axis.z
        );
    }
}
