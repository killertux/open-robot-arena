//! Diagnostic: our assembly of a robot as JSON, for comparison with the
//! original's scene (`tools/oracle/capture_assembly.py`, `tools/oracle/assembly_compare.py`).
//!
//! Usage: cargo run --release -p ra2sim --example asmdump -- <bot>
//!
//! One object per component: its class, `.gmf` file, rest transform in bot space
//! (column-major 4x4), the rigid body (`part`) it belongs to, and every mesh
//! object of its `.gmf` with that object's bot-space transform (the component's
//! transform times the object's own `TM`).

use glam::Mat4;

fn mat(m: &Mat4) -> String {
    let a = m.to_cols_array();
    let v: Vec<String> = a.iter().map(|x| format!("{x:.6}")).collect();
    format!("[{}]", v.join(","))
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: asmdump <bot>");
    let rig = ra2sim::rig::load(&path).expect("load");
    let (bot, _root, loaded) = ra2_render::load_bot_components(&path).expect("bot");
    let rest: Vec<Mat4> = rig.components.iter().map(|c| c.local).collect();
    let mut out = Vec::new();
    for (i, c) in rig.components.iter().enumerate() {
        let mut objs = Vec::new();
        let mut atts = Vec::new();
        if let Some(lc) = loaded.get(i).and_then(|l| l.as_ref()) {
            for (id, a) in lc.scene.attachment_table() {
                atts.push(format!(
                    "{{\"id\":{id},\"name\":{:?},\"types\":{:?},\"host\":{:?},\"m\":{}}}",
                    a.name,
                    a.types,
                    a.host.clone().unwrap_or_default(),
                    mat(&ra2fmt::mount::tm_mat4(&a.transform))
                ));
            }
            let hosts: Vec<String> = lc
                .scene
                .attachment_table()
                .values()
                .filter_map(|a| a.host.clone())
                .map(|h| h.to_ascii_lowercase())
                .collect();
            for o in &lc.scene.objects {
                if o.mesh.as_ref().map(|m| m.points.is_empty()).unwrap_or(true) {
                    continue;
                }
                // A motor's rotor object rides the rotor (a burst motor's is
                // built turned to its start angle).
                let on_rotor = c
                    .hinge
                    .as_ref()
                    .is_some_and(|h| h.rotor.eq_ignore_ascii_case(&o.name));
                let frame = if on_rotor { c.rotor_local } else { rest[i] };
                let tm = ra2_render::object_transform(o);
                let m = frame * tm;
                objs.push(format!(
                    "{{\"rotor\":{},\"host\":{},\"name\":{:?},\"m\":{},\"tm\":{},\"frame\":{}}}",
                    on_rotor,
                    hosts.contains(&o.name.to_ascii_lowercase()),
                    o.name.to_ascii_lowercase(),
                    mat(&m),
                    mat(&tm),
                    mat(&frame)
                ));
            }
        }
        // The joints this component drives (as the motor) and the joint of
        // the body it sits in.
        let joint_json = |j: &ra2sim::rig::JointInfo| {
            format!(
                "{{\"motor\":{},\"motor_class\":{:?},\"pivot\":{:?},\"axis\":{:?},\"slider\":{},\"use\":{:?}}}",
                j.motor,
                rig.components[j.motor].class,
                j.pivot.to_array(),
                j.axis.to_array(),
                j.slider,
                format!("{:?}", j.use_kind)
            )
        };
        let drives: Vec<String> = rig
            .parts
            .iter()
            .filter_map(|p| p.joint.filter(|j| j.motor == i))
            .map(|j| joint_json(&j))
            .collect();
        let own_joint = rig.parts[rig.part_of[i]]
            .joint
            .map(|j| joint_json(&j))
            .unwrap_or_else(|| "null".into());
        out.push(format!(
            "{{\"drives\":[{}],\"joint\":{},\"index\":{},\"class\":{:?},\"file\":{:?},\"parent\":{},\"part\":{},\"m\":{},\"pm\":{},\"om\":{},\"t\":{:?},\"q\":{:?},\"angle\":{},\"height\":{},\"burst\":{:?},\"atts\":[{}],\"objects\":[{}]}}",
            drives.join(","),
            own_joint,
            i,
            c.class,
            bot.components[i].file,
            c.parent.map(|p| p as i64).unwrap_or(-1),
            rig.part_of[i],
            mat(&rest[i]),
            bot.components[i].parent_mount,
            bot.components[i].own_mount,
            bot.components[i].translation,
            bot.components[i].rotation,
            bot.components[i].angle,
            bot.components[i].height,
            bot.components[i].burst_angles.unwrap_or([0.0, 0.0]),
            atts.join(","),
            objs.join(",")
        ));
    }
    println!("[{}]", out.join(",\n"));
}
