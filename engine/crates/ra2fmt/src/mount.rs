//! Component placement: the F1 mount rule, shared by the renderer, the
//! simulator and the workshop.
//!
//! A `.bot` places a component on its parent in one of two ways. A **chassis
//! child** (parent 0) is placed by its stored transform alone — translation,
//! quaternion, yaw `angle` and vertical `height` — because the chassis has a
//! single mount point and no `.gmf` to carry attachment frames. Every other
//! link can align attachment frames:
//!
//! ```text
//! A_parent · R_z(start) · R_x(π) · R_z(angle) · A_own⁻¹
//! ```
//!
//! `A_parent` is the parent-side mount frame, `A_own` the child's own point, the
//! 180° turn about X is a male plug into a female socket, `start` is a burst
//! motor's rest angle (applied on the parent's side, because it is the axle that
//! turns) and `angle` is the player's free spin (applied on the child's side,
//! because it belongs to the placement). Both rotations pass through the child's
//! named point, so the snap survives.
//!
//! This module is the **only** implementation of that rule: `ra2-render` and
//! `ra2-ui`'s workshop both call it. See `docs/06-deferred-fidelity.md` F1 for
//! the evidence (wheel axles stay horizontal, the rotor clears the chassis, and
//! the bots' own previews agree).

use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};

use crate::bot::{Bot, Component};
use crate::gmi::Attachment;

/// A `.gmf` transform triple (four rows of three) as a `Mat4`.
///
/// The rows are axes, the last one the origin — PyRA2's `TransformMatrix`
/// transposes the same way.
pub fn tm_mat4(tm: &[[f32; 3]; 4]) -> Mat4 {
    if tm == &[[0.0; 3]; 4] {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols(
        glam::Vec4::new(tm[0][0], tm[0][1], tm[0][2], 0.0),
        glam::Vec4::new(tm[1][0], tm[1][1], tm[1][2], 0.0),
        glam::Vec4::new(tm[2][0], tm[2][1], tm[2][2], 0.0),
        glam::Vec4::new(tm[3][0], tm[3][1], tm[3][2], 1.0),
    )
}

/// A component's stored transform: `translation · quaternion · yaw · height`.
///
/// RA2 model space is Y-up: `translation` and `rotation` place the component,
/// then the mount `angle` yaws it about +Y and `height` offsets it along +Y.
/// The stored quaternion is `(x, y, z, w)` — confirmed against PyRA2's
/// `quatToAxisAndAngle` (element 3 is the scalar) and a chassis whose rotation
/// is exactly `[0, 0, 0, 1]`.
pub fn component_local(c: &Component) -> Mat4 {
    let t = Vec3::from(c.translation);
    let q = Quat::from_xyzw(c.rotation[0], c.rotation[1], c.rotation[2], c.rotation[3]).normalize();
    Mat4::from_translation(t)
        * Mat4::from_quat(q)
        * Mat4::from_rotation_y(c.angle)
        * Mat4::from_translation(Vec3::new(0.0, c.height, 0.0))
}

/// How a component is placed on its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountMode {
    /// The stored transform alone — `translation * quaternion * yaw * height`.
    /// Kept because it is what reproduces the game's layout for *chassis*
    /// children, but it cannot place a chain: every component-to-component link
    /// in every stock bot stores an identity transform, so the whole chain
    /// collapses onto its root (see F1).
    Stored,
    /// At a component-to-component mount, align the child's attachment frame to
    /// the parent's: `A_parent * A_own⁻¹`.
    Frames,
    /// The same, with a 180° rotation about X between the frames — a male plug
    /// into a female socket. **The default**: it is the only rule that both
    /// keeps every wheel's axle horizontal and lifts a rotor clear of the
    /// chassis, and it reproduces the game's own botlab preview.
    FramesFlipped,
    /// The stored transform, resolved *through* the child's own mount frame:
    /// `M * A_own⁻¹`.
    StoredOwn,
    /// Frames *and* the stored transform: `A_parent * M * A_own⁻¹`.
    FramesStored,
}

impl MountMode {
    /// `RA2_MOUNT=frames-flip` (default) | `stored` | `frames` | `stored-own`
    /// | `frames-stored`.
    pub fn from_env() -> MountMode {
        match std::env::var("RA2_MOUNT").unwrap_or_default().as_str() {
            "stored" => MountMode::Stored,
            "frames" => MountMode::Frames,
            "stored-own" => MountMode::StoredOwn,
            "frames-stored" => MountMode::FramesStored,
            _ => MountMode::FramesFlipped,
        }
    }
}

/// The mount frames of every component, keyed by the ids a `.bot` names.
///
/// Index 0 (the chassis) has no frames: its mesh is embedded in the `.bot`, and
/// every chassis child in every stock bot names mount `0`, which is the
/// baseplate rather than an attachment point.
#[derive(Clone, Debug, Default)]
pub struct MountFrames {
    components: Vec<BTreeMap<i32, Mat4>>,
}

impl MountFrames {
    pub fn empty(components: usize) -> Self {
        MountFrames {
            components: vec![BTreeMap::new(); components],
        }
    }

    /// Convert per-component attachment tables into frames.
    pub fn from_tables(tables: &[Option<BTreeMap<i32, Attachment>>]) -> Self {
        MountFrames {
            components: tables
                .iter()
                .map(|t| {
                    t.iter()
                        .flat_map(|m| m.iter())
                        .map(|(id, a)| (*id, tm_mat4(&a.transform)))
                        .collect()
                })
                .collect(),
        }
    }

    /// Frames already in matrix form, index-aligned with the component list.
    pub fn from_maps(maps: Vec<BTreeMap<i32, Mat4>>) -> Self {
        MountFrames { components: maps }
    }

    pub fn get(&self, component: usize, mount: i32) -> Option<Mat4> {
        self.components.get(component)?.get(&mount).copied()
    }
}

/// The knobs the renderer exposes on the command line.
#[derive(Clone, Copy, Debug)]
pub struct MountOptions {
    pub mode: MountMode,
    /// Height of the chassis' mounting plane, in metres.
    pub chassis_plane: f32,
}

impl Default for MountOptions {
    fn default() -> Self {
        MountOptions {
            mode: MountMode::FramesFlipped,
            chassis_plane: 0.0,
        }
    }
}

/// Chained bot-local transform for every component (parents before children).
pub fn worlds(bot: &Bot, frames: &MountFrames, options: MountOptions) -> Vec<Mat4> {
    let mut world = vec![Mat4::IDENTITY; bot.components.len()];
    for (i, c) in bot.components.iter().enumerate() {
        let local = component_local(c);
        if i == 0 || c.parent < 0 || c.parent as usize >= i {
            world[i] = local;
            continue;
        }
        let parent = c.parent as usize;
        // A BurstMotor swings its axle about the axle's own axis, so its
        // children ride along. When a mount frame is available the rotation
        // goes *inside* the link (about the point's Z, through the point's
        // origin); see `mount_link`.
        let rest = match bot.components[parent].burst_angles {
            Some([start, _]) => Mat4::from_rotation_z(start),
            None => Mat4::IDENTITY,
        };
        let link = mount_link(bot, frames, i, options.mode);
        // The stored translation's Y is always 0 in every stock bot, so the
        // height of a chassis-mounted part has to come from the chassis'
        // mounting plane.
        let base = if parent == 0 {
            Mat4::from_translation(Vec3::new(0.0, options.chassis_plane, 0.0))
        } else {
            Mat4::IDENTITY
        };
        world[i] = match link {
            Some(link) => world[parent] * base * link,
            None => world[parent] * base * rest * local,
        };
    }
    world
}

/// The relative transform of component `i` on its parent for `mode`.
///
/// `Stored` returns `None` (use the stored transform alone). Every other mode
/// folds in the child's own attach frame, and `Frames`/`FramesStored` also the
/// parent's, so the child's named point is snapped onto the parent's.
fn mount_link(bot: &Bot, frames: &MountFrames, i: usize, mode: MountMode) -> Option<Mat4> {
    if mode == MountMode::Stored {
        return None;
    }
    let c = &bot.components[i];
    let local = component_local(c);
    let own = frames.get(i, c.own_mount)?;
    let own_inv = own.inverse();
    let parent_frame = if c.parent >= 0 {
        frames.get(c.parent as usize, c.parent_mount)
    } else {
        None
    };
    if matches!(mode, MountMode::Frames | MountMode::FramesFlipped) && parent_frame.is_none() {
        // Nothing on the parent to align to (the chassis, whose only mount is
        // the baseplate): keep the stored transform.
        return None;
    }
    let parent_frame = parent_frame.unwrap_or(Mat4::IDENTITY);
    Some(match mode {
        MountMode::Stored => unreachable!(),
        MountMode::StoredOwn => local * own_inv,
        MountMode::Frames | MountMode::FramesFlipped => {
            let mate = match mode {
                MountMode::FramesFlipped => Mat4::from_rotation_x(std::f32::consts::PI),
                _ => Mat4::IDENTITY,
            };
            // Two rotations turn the child about the mount axis, and each sits
            // in the frame it belongs to:
            //
            // - `angle` is the free spin the player dialled in with SHIFT. It is
            //   a property of the *placement*, so like a chassis child's `angle`
            //   (which sits inside the stored transform) it is applied in the
            //   child's own point's frame.
            // - a burst motor's start angle is the *axle* turning, so it is
            //   applied on the parent's side.
            //
            // Both axes pass through the child's named point, so neither moves
            // it: the snap survives.
            let axle = Mat4::from_rotation_z(burst_start(bot, c.parent));
            let aim = Mat4::from_rotation_z(c.angle);
            parent_frame * axle * mate * aim * own_inv
        }
        MountMode::FramesStored => parent_frame * local * own_inv,
    })
}

/// The start (rest) angle of `parent`'s axle, when it is a burst motor.
fn burst_start(bot: &Bot, parent: i32) -> f32 {
    match bot
        .components
        .get(parent.max(0) as usize)
        .and_then(|p| p.burst_angles)
    {
        Some([start, _]) => start,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::Chassis;

    fn chassis() -> Component {
        Component {
            parent: -1,
            rotation: [0.0, 0.0, 0.0, 1.0],
            class: "Chassis".into(),
            chassis: Some(Chassis::default()),
            ..Default::default()
        }
    }

    /// The chassis has no frames, so a chassis child keeps its stored transform
    /// even in the frame modes — every stock chassis child names mount `0`.
    #[test]
    fn a_chassis_child_keeps_its_stored_transform() {
        let mut child = Component {
            parent: 0,
            parent_mount: 0,
            own_mount: 1,
            translation: [1.0, 0.0, 2.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            angle: 0.5,
            height: 0.25,
            ..Default::default()
        };
        child.class = "Component".into();
        let bot = Bot {
            components: vec![chassis(), child.clone()],
            ..Default::default()
        };
        let frames = MountFrames::empty(2);
        let stored = worlds(&bot, &frames, MountOptions::default());
        assert_eq!(stored[1], component_local(&child));
    }

    /// A component-to-component link with a child's point but no parent point
    /// keeps the stored transform ("nothing to align to").
    #[test]
    fn a_link_with_no_parent_frame_falls_back_to_stored() {
        let mut parent = chassis();
        parent.class = "SpinMotor".into();
        let mut child = Component {
            parent: 0, // the parent is index 0, but it has no `.gmf` frame
            parent_mount: 4,
            own_mount: 1,
            translation: [0.5, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        child.class = "Wheel".into();
        let bot = Bot {
            components: vec![parent, child.clone()],
            ..Default::default()
        };
        let mut frames = MountFrames::empty(2);
        frames.components[1].insert(1, Mat4::IDENTITY);
        let world = worlds(&bot, &frames, MountOptions::default());
        assert_eq!(world[1], component_local(&child));
    }
}
