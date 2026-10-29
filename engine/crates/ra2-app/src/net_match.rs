//! The original's match wire (docs/38 section 15): the ids its snapshots give a robot's
//! components and bodies, the snapshot a host writes from a [`Duel`] and a joiner puts back
//! into one, and the key events of its `giRemoteInputMessage`.
//!
//! Ids (recovered from 412 snapshots of the original's host and the writer `0x4d2b60`): a
//! robot's id is its panel; the objects of panel `p` are numbered from `(p + 1) << 10`, a
//! component `i` being `base + 4 * i`, its first body `base + 4 * i`, a second `+ 1`. The
//! chassis body is the chassis component's id; every other body belongs to the motor that
//! joins it to its parent.

use ra2net::message::BotInput;
use ra2net::sync::{BodySync, DeviceSync, RobotSync, SyncSchema, SyncState};
use ra2sim::controls::KeySet;
use ra2sim::duel::Duel;
use ra2sim::rig::RobotRig;

/// The id the objects of panel `panel` start at.
pub fn robot_base(panel: u32) -> u32 {
    (panel + 1) << 10
}

/// A component's id.
pub fn component_id(panel: u32, component: usize) -> u32 {
    robot_base(panel) + 4 * component as u32
}

/// The id of a part's body in a snapshot, `None` for a part the original does not list.
///
/// A body is its motor's id plus a sub-index (live, 11 robots): a plain spin motor, a burst motor, a servo
/// and an axle mount own their body (`+0`); a steering motor (`anglemotor*.txt`) and a burst piston's rod
/// are the second (`+1`); a `Steering` unit owns its two wheels' bodies, `+0` and `+1` in part order.
pub fn body_id(panel: u32, rig: &RobotRig, part: usize) -> Option<u32> {
    if part == 0 {
        return Some(component_id(panel, 0));
    }
    let joint = rig.parts.get(part)?.joint.as_ref()?;
    let motor = rig.components.get(joint.motor)?;
    let sub = match motor.class.as_str() {
        "Steering" => rig
            .parts
            .iter()
            .take(part)
            .filter(|p| p.joint.as_ref().is_some_and(|j| j.motor == joint.motor))
            .count() as u32,
        "BurstPiston" => 1,
        "SpinMotor" if motor.file.to_ascii_lowercase().contains("anglemotor") => 1,
        _ => 0,
    };
    Some(component_id(panel, joint.motor) + sub)
}

/// The schema of the snapshots of this match: each robot's component count.
pub fn schema(duel: &Duel) -> SyncSchema {
    SyncSchema {
        robot_components: duel.robots.iter().map(|r| r.rig.components.len()).collect(),
    }
}

/// The snapshot of the match: every robot's hitpoints, its batteries and motors, and every
/// body (the chassis in full, the other bodies by position and rotation, as the original's
/// all-bodies snapshot writes them). `panels[i]` is robot `i`'s panel.
pub fn capture(duel: &Duel, panels: &[u32]) -> SyncState {
    let mut state = SyncState {
        partial: true,
        ..SyncState::default()
    };
    for (r, robot) in duel.robots.iter().enumerate() {
        let panel = panels.get(r).copied().unwrap_or(r as u32);
        state.robots.push(RobotSync {
            id: panel,
            components: robot.health.clone(),
        });
        // Batteries and air tanks, in component order: the "controllers".
        let mut sources: Vec<_> = robot.energy.sources.iter().collect();
        sources.sort_by_key(|s| s.component);
        for source in sources {
            state
                .controllers
                .push((component_id(panel, source.component), source.current));
        }
        for (c, comp) in robot.rig.components.iter().enumerate() {
            let id = component_id(panel, c);
            let joint = robot
                .rig
                .parts
                .iter()
                .find_map(|p| p.joint.as_ref().filter(|j| j.motor == c));
            let (rate, torque) = joint.map_or((0.0, 0.0), |j| (j.rate, j.max_torque / 40.0));
            match comp.class.as_str() {
                "SpinMotor" | "BurstMotor" | "ServoMotor" | "AxleMount" => {
                    state.devices.push(DeviceSync::One {
                        id,
                        flag: false,
                        values: [0.0, 0.0, rate, torque],
                    })
                }
                "BurstPiston" => state.devices.push(DeviceSync::Five {
                    id,
                    flag: false,
                    word: 0,
                    values: [0.0, rate, torque],
                }),
                "Steering" => {
                    for k in 0..2 {
                        state.devices.push(DeviceSync::Four {
                            id: id + k,
                            value: 0.0,
                        });
                    }
                }
                _ => {}
            }
        }
        for (p, part) in robot.parts.iter().enumerate() {
            if !part.alive {
                continue;
            }
            let Some(id) = body_id(panel, &robot.rig, p) else {
                continue;
            };
            let Some(body) = duel.physics.state(part.body) else {
                continue;
            };
            let q = body.rotation;
            state.bodies.push(BodySync {
                id,
                awake: p == 0,
                position: body.position.to_array(),
                rotation: [q.w, q.x, q.y, q.z],
                linear: body.linear_velocity.to_array(),
                angular: body.angular_velocity.to_array(),
            });
        }
    }
    state
}

/// Put a snapshot's bodies into the duel. Returns how many were applied; a body the duel
/// does not have is skipped, as the original's reader does ("Bad Rigid Body ID in Sync").
pub fn apply(duel: &mut Duel, panels: &[u32], state: &SyncState) -> usize {
    let mut applied = 0;
    for body in &state.bodies {
        let panel = (body.id >> 10).wrapping_sub(1);
        let Some(r) = panels.iter().position(|p| *p == panel) else {
            continue;
        };
        let Some(robot) = duel.robots.get(r) else {
            continue;
        };
        let Some(p) =
            (0..robot.parts.len()).find(|&p| body_id(panel, &robot.rig, p) == Some(body.id))
        else {
            continue;
        };
        if !robot.parts[p].alive {
            continue;
        }
        let handle = robot.parts[p].body;
        let [w, x, y, z] = body.rotation;
        let rotation = glam::Quat::from_xyzw(x, y, z, w).normalize();
        duel.physics
            .set_transform(handle, glam::Vec3::from_array(body.position), rotation);
        if body.awake {
            duel.physics.set_velocity(
                handle,
                glam::Vec3::from_array(body.linear),
                glam::Vec3::from_array(body.angular),
            );
        }
        applied += 1;
    }
    applied
}

/// The keyboard binding words of a key event: the first three words of a control's binding
/// (`0, 0, DirectInput scan code`, live).
pub fn key_event(code: i32, down: bool) -> BotInput {
    BotInput {
        key: [0, 0, code as u32],
        value: u32::from(down),
    }
}

/// The key events between two held sets: one per key that changed.
pub fn key_events(before: &KeySet, now: &KeySet) -> Vec<BotInput> {
    (0..256)
        .filter(|&code| before.contains(code) != now.contains(code))
        .map(|code| key_event(code, now.contains(code)))
        .collect()
}

/// Fold one key event of a remote player into the keys it holds. Events of another device
/// (a joystick axis) are ignored.
pub fn apply_key(keys: &mut KeySet, input: &BotInput) {
    if input.key[0] == 0 && input.key[1] == 0 {
        keys.set(input.key[2] as i32, input.value != 0);
    }
}
