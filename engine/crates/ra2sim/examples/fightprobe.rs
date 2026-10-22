//! Diagnostic: an AI-v-AI match, printing both chassis' height and up-vector
//! every half second (who tips over, when).
//!
//! Usage: `fightprobe <arena.py> <botA> <botB> <seconds>`
//!
//! `FIGHTPROBE_GAME=tabletop` plays a Tabletop match (FlexTop, ClawTop); every
//! half second the line also gives each suspended prop's tilt (rad) and
//! centre (FlexTop's table, docs/29 section 12j).
use ra2sim::{GameType, Match, MatchConfig, MatchState, RobotEntry};

fn info_parent(rig: &ra2sim::rig::RobotRig, p: usize) -> usize {
    rig.parts[p].parent.unwrap_or(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let secs: f32 = a[3].parse()?;
    let cfg = MatchConfig::new(
        a[0].clone(),
        match std::env::var("FIGHTPROBE_GAME").as_deref() {
            Ok("tabletop") => GameType::Tabletop,
            _ => GameType::Deathmatch,
        },
        vec![
            RobotEntry::ai(0, a[1].clone()),
            RobotEntry::ai(1, a[2].clone()),
        ],
    )
    .with_duration(Some(secs));
    let mut m = Match::new(cfg)?;
    // `FIDTRACE_WARMUP=<n>`: n idle physics steps first, as `fidtrace match`.
    let warmup: usize = std::env::var("FIDTRACE_WARMUP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for _ in 0..warmup {
        let n = m.duel.robots.len();
        m.duel.step_direct(
            &vec![ra2sim::sim::DriveInput::default(); n],
            ra2sim::DEFAULT_STEP,
        );
    }
    let window: Option<(f32, f32)> = a.get(4).and_then(|w| {
        let (x, y) = w.split_once('-')?;
        Some((x.parse().ok()?, y.parse().ok()?))
    });
    for p in m.duel.props.iter().filter(|p| p.prop.mesh.is_some()) {
        if let Some((mass, com, inv)) = m.duel.physics.mass_properties(p.body) {
            let i = inv.inverse();
            println!(
                "{}: {mass:.1} kg, centre {com:.4} (body frame), inertia {:.2} {:.2} {:.2} / {:.2} {:.2} {:.2}",
                p.prop.name, i.x_axis.x, i.y_axis.y, i.z_axis.z, i.y_axis.x, i.z_axis.x, i.z_axis.y
            );
        }
    }
    let mut next = 0.0f32;
    let mut steps = 0usize;
    // Components that came off, as they go.
    let mut attached: Vec<Vec<bool>> = m.duel.robots.iter().map(|r| r.attached.clone()).collect();
    while m.state() != MatchState::Results && m.clock() < secs && steps < (secs * 400.0) as usize {
        m.step();
        steps += 1;
        let t = m.clock();
        for (ri, r) in m.duel.robots.iter().enumerate() {
            for (ci, (&now, was)) in r.attached.iter().zip(attached[ri].iter_mut()).enumerate() {
                if now != *was {
                    println!(
                        "{t:7.3} robot {ri} component {ci} {:?} attached {now}",
                        r.rig.components[ci].name
                    );
                    *was = now;
                }
            }
        }
        let detail = window.is_some_and(|(x, y)| t >= x && t <= y);
        if detail {
            let mut line = format!("{t:7.3}");
            for r in &m.duel.robots {
                let s = m.duel.physics.state(r.parts[0].body).unwrap();
                let up = s.rotation * glam::Vec3::Y;
                let fwd = s.rotation * r.rig.forward();
                line += &format!(
                    " | x {:6.2} z {:6.2} y {:5.2} up {:5.2} fwd {:5.2} v ({:5.1},{:5.1},{:5.1}) w {:5.1}",
                    s.position.x,
                    s.position.z,
                    s.position.y,
                    up.y,
                    fwd.y,
                    s.linear_velocity.x,
                    s.linear_velocity.y,
                    s.linear_velocity.z,
                    s.angular_velocity.length()
                );
                for (p, part) in r.parts.iter().enumerate() {
                    let Some(j) = part.joint else { continue };
                    let info = r.rig.parts[p].joint.unwrap();
                    if info.slider {
                        line += &format!(" s{p} {:.2}", m.duel.physics.slider_offset(j));
                    } else if info.use_kind == ra2sim::rig::JointUse::Burst {
                        line += &format!(" h{p} {:.2}", m.duel.physics.hinge_angle(j));
                        // The part's rotation relative to its parent, as a
                        // rotation vector in the parent's frame (a burst
                        // hinge should turn about its axis only).
                        if let (Some(pa), Some(pb)) = (
                            m.duel.physics.state(r.parts[info_parent(&r.rig, p)].body),
                            m.duel.physics.state(part.body),
                        ) {
                            let q = pa.rotation.inverse() * pb.rotation;
                            let rest = r.rig.part_local[r.rig.parts[p].root];
                            let rest = glam::Quat::from_mat4(&rest);
                            let q = q * rest.inverse();
                            let q = if q.w < 0.0 { -q } else { q };
                            let (ax, an) = q.to_axis_angle();
                            let v = ax * an;
                            line += &format!(" rel({:.2},{:.2},{:.2})", v.x, v.y, v.z);
                            let dw =
                                pa.rotation.inverse() * (pb.angular_velocity - pa.angular_velocity);
                            line += &format!(" dw({:.1},{:.1},{:.1})", dw.x, dw.y, dw.z);
                        }
                    }
                }
            }
            println!("{line}");
        } else if t >= next {
            next += 0.5;
            let mut line = format!("{t:6.2}");
            for r in &m.duel.robots {
                let s = m.duel.physics.state(r.parts[0].body).unwrap();
                let up = s.rotation * glam::Vec3::Y;
                let fwd = s.rotation * r.rig.forward();
                line += &format!(
                    " | ({:6.2},{:6.2},{:6.2}) up.y {:5.2} fwd.y {:5.2}",
                    s.position.x, s.position.y, s.position.z, up.y, fwd.y
                );
            }
            for p in m.duel.props.iter().filter(|p| p.prop.mesh.is_some()) {
                if let Some(s) = m.duel.physics.state(p.body) {
                    let up = s.rotation * glam::Vec3::Y;
                    let c = m.duel.physics.center_of_mass(p.body).unwrap_or(s.position);
                    line += &format!(
                        " | {} tilt {:.3} com ({:.3},{:.3},{:.3})",
                        p.prop.name,
                        up.y.clamp(-1.0, 1.0).acos(),
                        c.x,
                        c.y,
                        c.z
                    );
                    if m.duel
                        .rest_watch
                        .iter()
                        .any(|w| w.body == p.body && w.frozen)
                    {
                        line += " FROZEN";
                    }
                }
            }
            println!("{line}");
        }
    }
    Ok(())
}
