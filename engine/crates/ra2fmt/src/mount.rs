//! Component placement: how a `.bot` puts its components together, shared by
//! the renderer, the simulator and the workshop.
//!
//! The rule is the original's, measured on the live game (every scene node of
//! 16 stock robots as built, `tools/oracle/capture_assembly.py`, compared with
//! `tools/oracle/assembly_compare.py`; `docs/29-havok17-backend.md` section 8).
//! With `A_own` a component's own mount point (`own_mount`), `A_parent` the
//! parent's (`parent_mount`), both from the `.gmf` attachment table:
//!
//! ```text
//! chassis child:  T(t) · Q · T(0, 0, -height) · Rz(π - angle) · A_own⁻¹
//! component link: P · A_parent · Ry(π) · Rz(-angle) · A_own⁻¹
//! ```
//!
//! `t` and `Q` are the stored translation and quaternion (every stock chassis
//! child stores the quarter turn about X, so `Q`'s Z axis is the chassis' down
//! and `-height` lifts the part); the own point's origin lands exactly on `t`.
//! At a link the child's point is turned half a turn about the parent point's
//! Y (a plug into a socket) and spun by `-angle` about the common Z. `P` is
//! the parent's pose as carried by whatever hosts the parent point: the
//! component itself, or, for a burst motor's axle (`Host = bmotor_axle`), the
//! rotor turned to its start angle ([`burst_rotor`]).
//!
//! This module is the **only** implementation of the rule: `ra2-render`,
//! `ra2sim` and `ra2-ui`'s workshop all call it. The older guesses
//! (`MountMode::FramesFlipped` and friends) stay selectable through `RA2_MOUNT`
//! for comparison only.

use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};

use crate::bot::{Bot, Component};
use crate::gmi::Attachment;

/// The rotation **every chassis child in the shipped corpus stores**:
/// `(x, y, z, w) = (√½, 0, 0, √½)`, a quarter turn about +X.
///
/// All **523** chassis children across the 57 shipped bots carry exactly this
/// quaternion; it is the component's model-up convention on the baseplate, not
/// a player choice. The workshop writes it when it places a new chassis child.
pub const CHASSIS_CHILD_ROTATION: [f32; 4] = [
    std::f32::consts::FRAC_1_SQRT_2,
    0.0,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
];

/// The blueprint's unit scale: its own `Unit Scale AP = .22 M` bar means 100
/// outline units are 0.22 m, so 1000 units span 2.2 m.
///
/// The chassis' `bottom`/`top` outlines are stored in `0..1000`; the embedded
/// mesh's rings are those outlines mapped through this scale (measured on all
/// 57 bots, `x = (u - 500) · 0.0022`, `z = -(v - 500) · 0.0022`).
pub const BASEPLATE_UNITS_PER_METRE: f32 = 1000.0 / 2.2;

/// The half-extent of the blueprint sheet, in metres (`1000` units → ±1.1 m).
pub const BASEPLATE_HALF_EXTENT: f32 = 1.1;

/// An outline coordinate (`0..1000`, the file's own space) as bot-local metres.
///
/// The mesh negates the outline's `v` axis, which is why this is `(u, -v)`:
/// measured on every shipped chassis, the mesh's first ring is the `bottom`
/// outline in file order with `z` mirrored, and the second is `top`.
pub fn baseplate_to_world(point: [f32; 2]) -> [f32; 2] {
    [
        (point[0] - 500.0) / BASEPLATE_UNITS_PER_METRE,
        -(point[1] - 500.0) / BASEPLATE_UNITS_PER_METRE,
    ]
}

/// Bot-local metres as an outline coordinate (`0..1000`), the inverse of
/// [`baseplate_to_world`].
pub fn world_to_baseplate(point: [f32; 2]) -> [f32; 2] {
    [
        500.0 + point[0] * BASEPLATE_UNITS_PER_METRE,
        500.0 - point[1] * BASEPLATE_UNITS_PER_METRE,
    ]
}

/// Clamp a bot-local point to the blueprint sheet the baseplate grid covers.
pub fn clamp_baseplate(point: [f32; 2]) -> [f32; 2] {
    [
        point[0].clamp(-BASEPLATE_HALF_EXTENT, BASEPLATE_HALF_EXTENT),
        point[1].clamp(-BASEPLATE_HALF_EXTENT, BASEPLATE_HALF_EXTENT),
    ]
}

/// The component a baseplate click creates: the stored transform the corpus
/// uses (`y = 0`, the quarter-turn rotation, the player's yaw and height).
pub fn chassis_child(point: [f32; 2], angle: f32, height: f32) -> Component {
    let point = clamp_baseplate(point);
    Component {
        parent: 0,
        parent_mount: 0,
        translation: [point[0], 0.0, point[1]],
        rotation: CHASSIS_CHILD_ROTATION,
        angle,
        height,
        ..Default::default()
    }
}

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
    /// The original's rule (see the module documentation). **The default.**
    Game,
    /// The stored transform alone — `translation * quaternion * yaw * height`.
    /// Kept because it is what reproduces the game's layout for *chassis*
    /// children, but it cannot place a chain: every component-to-component link
    /// in every stock bot stores an identity transform, so the whole chain
    /// collapses onto its root (see F1).
    Stored,
    /// At a component-to-component mount, align the child's attachment frame to
    /// the parent's: `A_parent * A_own⁻¹`.
    Frames,
    /// The same, with a 180° rotation about X between the frames — the earlier
    /// guess at the plug rule (the default before the live measurement; it
    /// ignores a chassis child's own point and turns about the wrong axis).
    FramesFlipped,
    /// The stored transform, resolved *through* the child's own mount frame:
    /// `M * A_own⁻¹`.
    StoredOwn,
    /// Frames *and* the stored transform: `A_parent * M * A_own⁻¹`.
    FramesStored,
}

impl MountMode {
    /// `RA2_MOUNT=game` (default) | `frames-flip` | `stored` | `frames` |
    /// `stored-own` | `frames-stored`.
    pub fn from_env() -> MountMode {
        match std::env::var("RA2_MOUNT").unwrap_or_default().as_str() {
            "stored" => MountMode::Stored,
            "frames" => MountMode::Frames,
            "frames-flip" => MountMode::FramesFlipped,
            "stored-own" => MountMode::StoredOwn,
            "frames-stored" => MountMode::FramesStored,
            _ => MountMode::Game,
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
    /// Per component: the mesh object hosting each point (`Host = axle`),
    /// lower-cased; points on the component's own body are absent.
    hosts: Vec<BTreeMap<i32, String>>,
    /// Per component: its motor hinge, when its `.gmf` declares one.
    hinges: Vec<Option<RotorHinge>>,
}

/// A motor's hinge as its `.gmf` constraint solver declares it, in component
/// space: `Hinge01` joins the housing (`body_a`) and the rotor (`body_b`).
#[derive(Clone, Debug, PartialEq)]
pub struct RotorHinge {
    /// The rotor object (`bmotor_axle`, `axle`), lower-cased.
    pub rotor: String,
    /// `TM_ROW3` of the hinge node: the pivot.
    pub pivot: Vec3,
    /// `TM_ROW1` of the hinge node: the spin axis.
    pub axis: Vec3,
    /// The rotor object's own transform (`TM`).
    pub rotor_tm: Mat4,
}

impl MountFrames {
    pub fn empty(components: usize) -> Self {
        MountFrames {
            components: vec![BTreeMap::new(); components],
            hosts: vec![BTreeMap::new(); components],
            hinges: vec![None; components],
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
            hosts: tables
                .iter()
                .map(|t| {
                    t.iter()
                        .flat_map(|m| m.iter())
                        .filter_map(|(id, a)| a.host.clone().map(|h| (*id, h.to_ascii_lowercase())))
                        .collect()
                })
                .collect(),
            hinges: vec![None; tables.len()],
        }
    }

    /// Frames already in matrix form, index-aligned with the component list.
    pub fn from_maps(maps: Vec<BTreeMap<i32, Mat4>>) -> Self {
        let n = maps.len();
        MountFrames {
            components: maps,
            hosts: vec![BTreeMap::new(); n],
            hinges: vec![None; n],
        }
    }

    /// Add each component's motor hinge (index-aligned with the components).
    pub fn with_hinges(mut self, hinges: Vec<Option<RotorHinge>>) -> Self {
        self.hinges = hinges;
        self
    }

    /// Add each component's point hosts (index-aligned with the components).
    pub fn with_hosts(mut self, hosts: Vec<BTreeMap<i32, String>>) -> Self {
        self.hosts = hosts;
        self
    }

    pub fn get(&self, component: usize, mount: i32) -> Option<Mat4> {
        self.components.get(component)?.get(&mount).copied()
    }

    /// The mesh object hosting `mount` on `component`, if it is not the
    /// component's own body.
    pub fn host(&self, component: usize, mount: i32) -> Option<&str> {
        self.hosts.get(component)?.get(&mount).map(|s| s.as_str())
    }

    /// The motor hinge of `component`, if it has one.
    pub fn hinge(&self, component: usize) -> Option<&RotorHinge> {
        self.hinges.get(component)?.as_ref()
    }
}

/// The hinge of a component's `.gmf`: the first `Hinge01`-style constraint,
/// or `None` when the file declares none.
pub fn rotor_hinge(scene: &crate::gmi::GmiScene) -> Option<RotorHinge> {
    let h = scene.hinges.first()?;
    let rotor = h.body_b.to_ascii_lowercase();
    let rotor_tm = scene
        .find_object(&h.body_b)
        .map(|o| tm_mat4(&o.transform))
        .unwrap_or(Mat4::IDENTITY);
    let axis = Vec3::from(h.frame[1]).normalize_or_zero();
    Some(RotorHinge {
        rotor,
        pivot: Vec3::from(h.frame[3]),
        axis: if axis == Vec3::ZERO { Vec3::Z } else { axis },
        rotor_tm,
    })
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
            mode: MountMode::Game,
            chassis_plane: 0.0,
        }
    }
}

/// Chained bot-local transform for every component (parents before children).
pub fn worlds(bot: &Bot, frames: &MountFrames, options: MountOptions) -> Vec<Mat4> {
    assembly(bot, frames, options).0
}

/// [`worlds`] plus, per component, the pose of the component frame as its
/// rotor carries it: equal to the component's own pose except for a burst
/// motor, whose rotor (and everything mounted on it) the original turns to
/// its start angle when it builds the robot ([`burst_rotor`]).
pub fn assembly(bot: &Bot, frames: &MountFrames, options: MountOptions) -> (Vec<Mat4>, Vec<Mat4>) {
    let n = bot.components.len();
    let mut world = vec![Mat4::IDENTITY; n];
    let mut rotor = vec![Mat4::IDENTITY; n];
    for (i, c) in bot.components.iter().enumerate() {
        let local = component_local(c);
        if i == 0 || c.parent < 0 || c.parent as usize >= i {
            world[i] = local;
            rotor[i] = local;
            continue;
        }
        let parent = c.parent as usize;
        // The stored translation's Y is always 0 in every stock bot: the
        // chassis' mounting plane is bot-space y = 0 (live, a chassis child's
        // own point lands exactly on its stored translation).
        let base = if parent == 0 {
            Mat4::from_translation(Vec3::new(0.0, options.chassis_plane, 0.0))
        } else {
            Mat4::IDENTITY
        };
        world[i] = if options.mode == MountMode::Game {
            game_link(bot, frames, i, &world, &rotor).unwrap_or(world[parent] * local)
        } else {
            let rest = match bot.components[parent].burst_angles {
                Some([start, _]) => Mat4::from_rotation_z(start),
                None => Mat4::IDENTITY,
            };
            match mount_link(bot, frames, i, options.mode) {
                Some(link) => world[parent] * link,
                None => world[parent] * rest * local,
            }
        };
        world[i] = base * world[i];
        rotor[i] = world[i];
        if options.mode == MountMode::Game && c.burst_angles.is_some() {
            if let Some(h) = frames.hinge(i) {
                rotor[i] = burst_rotor(world[i], h, c.burst_angles.map(|a| a[0]).unwrap_or(0.0));
            }
        }
    }
    (world, rotor)
}

/// The original's placement of component `i` (see the module documentation),
/// or `None` when a frame it needs is missing.
fn game_link(
    bot: &Bot,
    frames: &MountFrames,
    i: usize,
    world: &[Mat4],
    rotor: &[Mat4],
) -> Option<Mat4> {
    let c = &bot.components[i];
    let parent = c.parent as usize;
    let own_inv = frames
        .get(i, c.own_mount)
        .map(|m| m.inverse())
        .unwrap_or(Mat4::IDENTITY);
    if parent == 0 {
        let q =
            Quat::from_xyzw(c.rotation[0], c.rotation[1], c.rotation[2], c.rotation[3]).normalize();
        return Some(
            Mat4::from_translation(Vec3::from(c.translation))
                * Mat4::from_quat(q)
                * Mat4::from_translation(Vec3::new(0.0, 0.0, -c.height))
                * Mat4::from_rotation_z(std::f32::consts::PI - c.angle)
                * own_inv,
        );
    }
    let socket = frames.get(parent, c.parent_mount)?;
    // A point on a rotor rides the rotor; any other on the component itself.
    let carrier = match (frames.host(parent, c.parent_mount), frames.hinge(parent)) {
        (Some(host), Some(h)) if host == h.rotor => rotor[parent],
        _ => world[parent],
    };
    Some(
        carrier
            * socket
            * Mat4::from_rotation_y(std::f32::consts::PI)
            * Mat4::from_rotation_z(-c.angle)
            * own_inv,
    )
}

/// The original's start pose of a burst motor's rotor, as the component frame
/// the rotor carries (bot space).
///
/// `BurstMotor` sets its start angle through the hinge controller
/// (`FUN_00446960` → `FUN_00480c60` → `FUN_004808a0`), which does not turn the
/// rotor from where it was built: it gives the rotor body an absolute
/// orientation. With `a` the hinge axis (the housing's `Hinge01` spin axis) in
/// the robot's frame and `p` the reference the hinge builds for it
/// (`FUN_00525830`: `a × ŷ` when `|a.x| ≥ 0.57`, else `a × x̂`, normalised),
/// the rotor's Z is `-a` and its Y is `p` turned by the start angle about `a`.
/// Measured on eight burst motors of six robots (both stock models, all four
/// mounting orientations): the rotor lands within 1° of this, and the burst
/// motor's own model (`burstmotor.gmf`, whose hinge axis is its +Z) comes out
/// flipped end for end relative to its housing — which is what the live robots
/// show. The pivot stays where it was built.
pub fn burst_rotor(housing: Mat4, hinge: &RotorHinge, start: f32) -> Mat4 {
    let a = housing.transform_vector3(hinge.axis).normalize_or_zero();
    if a == Vec3::ZERO {
        return housing;
    }
    let p = if a.x.abs() >= 0.57 {
        a.cross(Vec3::Y)
    } else {
        a.cross(Vec3::X)
    }
    .normalize();
    let y = Quat::from_axis_angle(a, start) * p;
    let z = -a;
    let x = y.cross(z);
    let target = glam::Mat3::from_cols(x, y, z);
    // The rotor object's frame now, and where the original puts it.
    let rest = housing * hinge.rotor_tm;
    let rest_rot = glam::Mat3::from_mat4(rest);
    let turn = target * rest_rot.transpose();
    let pivot = housing.transform_point3(hinge.pivot);
    Mat4::from_translation(pivot) * Mat4::from_mat3(turn) * Mat4::from_translation(-pivot) * housing
}

/// The relative transform of component `i` on its parent for `mode`.
///
/// `Stored` returns `None` (use the stored transform alone). Every other mode
/// folds in the child's own attach frame, and `Frames`/`FramesStored` also the
/// parent's, so the child's named point is snapped onto the parent's.
fn mount_link(bot: &Bot, frames: &MountFrames, i: usize, mode: MountMode) -> Option<Mat4> {
    if matches!(mode, MountMode::Stored | MountMode::Game) {
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
        MountMode::Stored | MountMode::Game => unreachable!(),
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

    /// A chassis child lands with its own mount point on the stored
    /// translation, `height` above the baseplate, turned by `π - angle` about
    /// that point's Z (live: every chassis child of 14 stock robots,
    /// docs/29 section 8). The legacy `Stored` mode keeps the stored transform.
    #[test]
    fn a_chassis_child_puts_its_own_point_on_the_stored_translation() {
        let child = Component {
            parent: 0,
            parent_mount: 0,
            own_mount: 1,
            translation: [1.0, 0.0, 2.0],
            rotation: CHASSIS_CHILD_ROTATION,
            angle: 0.5,
            height: 0.25,
            class: "Component".into(),
            ..Default::default()
        };
        let bot = Bot {
            components: vec![chassis(), child.clone()],
            ..Default::default()
        };
        // A base point 0.1 m below the component's origin, facing down (the
        // stock motors' Point01).
        let point = Mat4::from_cols(
            glam::Vec4::new(-1.0, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, -1.0, 0.0),
            glam::Vec4::new(0.0, -1.0, 0.0, 0.0),
            glam::Vec4::new(0.0, -0.1, 0.0, 1.0),
        );
        let mut frames = MountFrames::empty(2);
        frames.components[1].insert(1, point);
        let world = worlds(&bot, &frames, MountOptions::default());
        let at = (world[1] * point).transform_point3(Vec3::ZERO);
        assert!(at.abs_diff_eq(Vec3::new(1.0, 0.25, 2.0), 1e-5), "{at:?}");
        // The component's own up stays up, turned by the angle about it.
        let up = world[1].transform_vector3(Vec3::Y);
        assert!(up.abs_diff_eq(Vec3::Y, 1e-5), "{up:?}");
        let x = world[1].transform_vector3(Vec3::X);
        let expect = Mat4::from_rotation_y(0.5).transform_vector3(Vec3::X);
        assert!(x.abs_diff_eq(expect, 1e-5), "{x:?} vs {expect:?}");

        let stored = worlds(
            &bot,
            &frames,
            MountOptions {
                mode: MountMode::Stored,
                chassis_plane: 0.0,
            },
        );
        assert_eq!(stored[1], component_local(&child));
    }

    /// The baseplate's own scale: the blueprint says 100 units are 0.22 m, and
    /// the mesh negates `v`. A round trip through both directions is exact.
    #[test]
    fn the_baseplate_maps_outline_units_to_metres() {
        assert!((baseplate_to_world([500.0, 500.0])[0]).abs() < 1e-6);
        assert!((baseplate_to_world([600.0, 500.0])[0] - 0.22).abs() < 1e-6);
        assert!((baseplate_to_world([500.0, 400.0])[1] - 0.22).abs() < 1e-6);
        let world = baseplate_to_world([731.0, 219.0]);
        let back = world_to_baseplate(world);
        assert!((back[0] - 731.0).abs() < 1e-3);
        assert!((back[1] - 219.0).abs() < 1e-3);
        assert_eq!(clamp_baseplate([4.0, -4.0]), [1.1, -1.1]);
    }

    /// A placed chassis child carries the file's own quarter turn, `y = 0`, and
    /// the point the click named.
    #[test]
    fn a_chassis_child_uses_the_stored_transform() {
        let c = chassis_child([0.44, -0.66], 1.25, 0.1);
        assert_eq!(c.parent, 0);
        assert_eq!(c.parent_mount, 0);
        assert_eq!(c.translation, [0.44, 0.0, -0.66]);
        assert_eq!(c.rotation, CHASSIS_CHILD_ROTATION);
        assert_eq!(c.angle, 1.25);
        assert_eq!(c.height, 0.1);
    }

    /// A component-to-component link with a child's point but no parent point
    /// keeps the stored transform ("nothing to align to").
    #[test]
    fn a_link_with_no_parent_frame_falls_back_to_stored() {
        let mut motor = Component {
            parent: 0,
            rotation: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        motor.class = "SpinMotor".into();
        let mut child = Component {
            parent: 1, // the motor, which has no frame for mount 4
            parent_mount: 4,
            own_mount: 1,
            translation: [0.5, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        child.class = "Wheel".into();
        let bot = Bot {
            components: vec![chassis(), motor, child.clone()],
            ..Default::default()
        };
        let mut frames = MountFrames::empty(3);
        frames.components[2].insert(1, Mat4::IDENTITY);
        let world = worlds(&bot, &frames, MountOptions::default());
        assert_eq!(world[2], world[1] * component_local(&child));
    }

    /// A component link: the child's point meets the parent's, turned half a
    /// turn about the parent point's Y and spun by `-angle` about its Z.
    #[test]
    fn a_link_plugs_the_childs_point_into_the_parents() {
        let mut motor = Component {
            parent: 0,
            rotation: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        motor.class = "SpinMotor".into();
        let child = Component {
            parent: 1,
            parent_mount: 4,
            own_mount: 1,
            angle: 0.3,
            rotation: [0.0, 0.0, 0.0, 1.0],
            class: "Wheel".into(),
            ..Default::default()
        };
        let bot = Bot {
            components: vec![chassis(), motor, child],
            ..Default::default()
        };
        let socket = Mat4::from_translation(Vec3::new(0.2, 0.0, -0.15));
        let plug = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.05));
        let mut frames = MountFrames::empty(3);
        frames.components[1].insert(4, socket);
        frames.components[2].insert(1, plug);
        let world = worlds(&bot, &frames, MountOptions::default());
        let want = world[1]
            * socket
            * Mat4::from_rotation_y(std::f32::consts::PI)
            * Mat4::from_rotation_z(-0.3)
            * plug.inverse();
        assert!(world[2].abs_diff_eq(want, 1e-6));
        let a = (world[2] * plug).transform_point3(Vec3::ZERO);
        let b = (world[1] * socket).transform_point3(Vec3::ZERO);
        assert!(a.abs_diff_eq(b, 1e-6));
    }

    /// A burst motor's rotor is built at its start angle by the original's
    /// absolute rule: rotor Z = -axis, rotor Y = the hinge reference turned by
    /// the start angle about the axis (docs/29 section 8). The Scout's own
    /// numbers: housing yawed so its +Z (the hinge axis) is the robot's -X,
    /// start 3.05433 rad; live, the rotor's Y is (0, -0.097, 0.995).
    #[test]
    fn a_burst_rotor_starts_at_its_angle_about_the_hinge() {
        let housing = Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let hinge = RotorHinge {
            rotor: "bmotor_axle".into(),
            pivot: Vec3::new(0.0, 0.0, -0.058),
            axis: Vec3::Z,
            rotor_tm: Mat4::from_translation(Vec3::new(0.0, 0.0, -0.059)),
        };
        let r = burst_rotor(housing, &hinge, 3.05433);
        let y = r.transform_vector3(Vec3::Y);
        let z = r.transform_vector3(Vec3::Z);
        assert!(z.abs_diff_eq(Vec3::X, 1e-5), "{z:?}");
        assert!(
            y.abs_diff_eq(Vec3::new(0.0, -0.0872, 0.9962), 1e-3),
            "{y:?}"
        );
        // The pivot does not move.
        let p = housing.transform_point3(hinge.pivot);
        assert!(r.transform_point3(hinge.pivot).abs_diff_eq(p, 1e-6));
    }
}
