//! Photograph the battle marks of a captured fight (docs/37-effects.md).
//!
//! ```text
//! decalshot <spec.txt> <out_dir> [game_dir]
//! ```
//!
//! `spec.txt` is what `tools/oracle/fx_decal_spec.py` writes from a
//! `capture_fx.py` run with `POSE=1`: the two robots, the decals the original
//! cut on each chassis (centre, normal and width axis in the chassis' frame),
//! the chassis' matrices at the shots and the scripted cameras. The robots are
//! teleported onto those matrices, the decals are cut from *our* chassis meshes
//! and each camera takes a frame, `<name>.png`. Needs a GPU adapter (wgpu) and
//! the game data.

// Each `shot` line carries both chassis matrices at that shot; the robots are
// teleported onto them before every frame.

use std::path::PathBuf;

use glam::{Mat4, Vec3, Vec4};
use ra2_render::decals::{DecalSpec, Projection};
use ra2_render::Renderer;
use ra2sim::{BotSetup, DriveInput, Duel, DuelCamera};

fn floats(f: &[&str]) -> Vec<f32> {
    f.iter().map(|s| s.parse().expect("a number")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: decalshot <spec.txt> <out_dir> [game_dir]");
        std::process::exit(2);
    }
    let out = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out)?;
    let game = PathBuf::from(
        args.get(2)
            .cloned()
            .or_else(|| std::env::var("RA2_GAME").ok())
            .unwrap_or_else(|| "../local/game".into()),
    );
    let text = std::fs::read_to_string(&args[0])?;
    let mut bots: Vec<(usize, String)> = Vec::new();
    let mut marks = Vec::new();
    let mut shots = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first().copied() {
            Some("bot") => bots.push((f[1].parse()?, f[2].to_string())),
            Some("mark") => {
                let n = floats(&f[4..]);
                let spec = DecalSpec::parse(&format!("{} 0 0 1 {} {} {}", f[2], n[0], n[1], f[3]))
                    .expect("a decal");
                marks.push((
                    f[1].parse::<usize>()?,
                    spec,
                    n[2],
                    Projection {
                        center: Vec3::new(n[3], n[4], n[5]),
                        normal: Vec3::new(n[6], n[7], n[8]),
                        axis: Vec3::new(n[9], n[10], n[11]),
                        width: n[0],
                        height: n[1],
                    },
                ));
            }
            Some("shot") => shots.push((f[1].to_string(), floats(&f[3..]))),
            _ => {}
        }
    }
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let setups: Vec<BotSetup> = bots
        .iter()
        .map(|(_, p)| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let mut duel = Duel::new(&arena, &setups)?;
    let idle = vec![DriveInput::default(); bots.len()];
    for _ in 0..60 {
        duel.step_direct(&idle, 1.0 / 60.0);
    }
    let teleport = |duel: &mut Duel, player: usize, m: Mat4| {
        let robot = &duel.robots[player];
        let current = robot.component_worlds(&*duel.physics)[0];
        let delta = m * current.inverse();
        let handles: Vec<_> = robot.parts.iter().map(|p| p.body).collect();
        for h in handles {
            if let Some(s) = duel.physics.state(h) {
                let now = Mat4::from_rotation_translation(s.rotation, s.position);
                let (_, rot, pos) = (delta * now).to_scale_rotation_translation();
                duel.physics.set_transform(h, pos, rot);
                duel.physics.set_velocity(h, Vec3::ZERO, Vec3::ZERO);
            }
        }
    };
    // The original's 4x3: rows are the axes, then the translation.
    let matrix = |m: &[f32]| {
        Mat4::from_cols(
            Vec4::new(m[0], m[1], m[2], 0.0),
            Vec4::new(m[3], m[4], m[5], 0.0),
            Vec4::new(m[6], m[7], m[8], 0.0),
            Vec4::new(m[9], m[10], m[11], 1.0),
        )
    };
    if std::env::var("DECALSHOT_CHASSIS_ONLY").is_ok() {
        // The other parts sit where the rest pose puts them, not where the fight left
        // them: leave only the chassis to compare the marks.
        for r in &mut duel.robots {
            for a in r.attached.iter_mut().skip(1) {
                *a = false;
            }
        }
    }
    // DECALSHOT_ALPHA=1.0 shows every mark at one strength (the live run's ALPHA_FORCE).
    let forced: Option<f32> = std::env::var("DECALSHOT_ALPHA")
        .ok()
        .and_then(|v| v.parse().ok());
    for (player, spec, alpha, projection) in marks {
        let alpha = forced.unwrap_or(alpha);
        duel.push_mark(player, spec, alpha, projection);
    }
    println!(
        "marks: {:?}",
        duel.marks.iter().map(|m| m.len()).collect::<Vec<_>>()
    );
    let renderer = Renderer::new()?;
    let (w, h) = (1024u32, 768u32);
    duel.scene_camera
        .select(ra2sim::camera::SceneCameraKind::Static);
    for (name, v) in shots {
        let eye = [v[0], v[1], v[2]];
        duel.scene_camera
            .apply(ra2script::camera::CameraCommand::Position(eye));
        duel.scene_camera
            .apply(ra2script::camera::CameraCommand::Rotation([v[3], v[4]]));
        duel.scene_camera
            .apply(ra2script::camera::CameraCommand::Fov(v[5]));
        teleport(&mut duel, 0, matrix(&v[6..18]));
        teleport(&mut duel, 1, matrix(&v[18..30]));
        if std::env::var("DECALSHOT_DEBUG").is_ok() {
            for (p, m) in [(0, matrix(&v[6..18])), (1, matrix(&v[18..30]))] {
                let w = duel.robots[p].component_worlds(&*duel.physics)[0];
                println!(
                    "{name} robot {p}: want {:?}\n   got {:?}",
                    m.to_cols_array(),
                    w.to_cols_array()
                );
            }
        }
        let png = duel.render(&renderer, &DuelCamera::Static, w, h)?;
        image::save_buffer(
            out.join(format!("{name}.png")),
            &png,
            w,
            h,
            image::ColorType::Rgba8,
        )?;
        println!("wrote {name}.png");
    }
    Ok(())
}
