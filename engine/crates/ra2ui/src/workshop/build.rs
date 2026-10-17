//! Building a robot: the component tree over `ra2fmt::bot::Bot`.
//!
//! The model keeps the bot file's own component list and adds what the workshop
//! needs to edit it: the game's placement rule (through [`ra2fmt::mount`]), the
//! one-component-per-mount constraint, parent-before-child order, the editor's
//! 800 kg limit, and undo/redo.
//!
//! ## The rules, and where they come from
//!
//! - **Placement is F1.** A chassis child is placed by its stored transform; a
//!   component-to-component link is `A_parent · R_z(start) · R_x(π) ·
//!   R_z(angle) · A_own⁻¹`. [`Build::transforms`] calls the same
//!   `ra2fmt::mount::worlds` the renderer and the simulator call.
//! - **Parent before child.** The file stores components in a topological order
//!   and the reader/simulator only accept a parent index below the child's
//!   (`ra2sim::rig::load`), so [`Build::attach`] only ever appends.
//! - **One component per mount.** A `.gmf` attachment is a socket; two children
//!   cannot share one. The **chassis mount is the exception**: the chassis has no
//!   `.gmf` and every stock chassis child names mount `0`, placing itself by its
//!   stored transform. So the rule applies to component-to-component mounts only
//!   (the 423 stock mesh links), not to the baseplate.
//! - **800 kg.** The editor's own check is *"Total mass of robot must not exceed
//!   800"* (`FUN_0043ca70`, quoted in `docs/06-deferred-fidelity.md` F10). An
//!   attach that would cross it is refused.
//!
//! Nothing here checks the size or overlap of a part on the baseplate: the game
//! lets the player put a component anywhere inside the chassis outline, and the
//! file records only the stored transform, so there is no recovered rule to
//! enforce. That is named in `docs/15-workshop-model.md`.

use std::collections::BTreeMap;

use glam::Mat4;
use ra2fmt::bot::{Bot, Chassis, Component};
use ra2fmt::gmi::Attachment;
use ra2fmt::mount::{self, MountFrames, MountMode, MountOptions};

use super::library::{ComponentInfo, GameLibrary, LibraryError};
use super::paint::Paint;
use super::stats::BotStats;
use crate::workshop::wiring::{ControlBoard, WiringError};

/// How a newly attached component is placed on its parent, before any mount
/// frame is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub translation: [f32; 3],
    /// `(x, y, z, w)`, the file's order.
    pub rotation: [f32; 4],
    /// The player's free spin, about the mount's Z.
    pub angle: f32,
    /// Vertical offset along the component's +Y.
    pub height: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            angle: 0.0,
            height: 0.0,
        }
    }
}

impl Placement {
    /// A chassis child at `translation`, with the file's usual identity
    /// quaternion.
    pub fn at(translation: [f32; 3]) -> Self {
        Placement {
            translation,
            ..Default::default()
        }
    }
}

/// The editor's limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildLimits {
    /// `FUN_0043ca70`'s "must not exceed 800".
    pub max_mass: f32,
}

impl Default for BuildLimits {
    fn default() -> Self {
        BuildLimits { max_mass: 800.0 }
    }
}

/// A whole build in progress.
#[derive(Debug)]
pub struct Build {
    bot: Bot,
    /// Resolved component data, index-aligned with `bot.components`.
    info: Vec<ComponentInfo>,
    board: ControlBoard,
    paint: Paint,
    limits: BuildLimits,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    bot: Bot,
    info: Vec<ComponentInfo>,
    board: ControlBoard,
}

impl Build {
    /// A new bot with just a chassis and an empty control board — the state the
    /// game's *new robot* starts from (the wiring tutorial's "blank robot
    /// controller").
    pub fn new(name: impl Into<String>, mut chassis: Chassis) -> Self {
        normalize_chassis(&mut chassis);
        let bot = Bot {
            version: "1.12".into(),
            name: name.into(),
            class: 0,
            default_texture: Some(true),
            heading: Some(0.0),
            components: vec![Component {
                parent: -1,
                rotation: [0.0, 0.0, 0.0, 1.0],
                class: "Chassis".into(),
                chassis: Some(chassis.clone()),
                ..Default::default()
            }],
            ..Default::default()
        };
        Build {
            info: vec![chassis_info(&chassis)],
            paint: Paint::from_chassis(&chassis),
            bot,
            board: ControlBoard::default(),
            limits: BuildLimits::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Start with the four stock controls (`Forward`, `LeftRight`, `Fire`,
    /// `Spin`) already on the board.
    pub fn with_default_board(mut self) -> Self {
        self.board = ControlBoard::default_board();
        self
    }

    /// Read a `.bot` without a game directory: the component tree and the board
    /// are exact, but the resolved masses/mounts are not (use [`Build::load`] for
    /// those).
    pub fn from_bot(bot: Bot) -> Result<Self, BuildError> {
        let info = bot
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == 0 {
                    chassis_info(c.chassis.as_ref().unwrap_or(&Chassis::default()))
                } else {
                    ComponentInfo::unloaded(&c.class, &c.file)
                }
            })
            .collect();
        Self::from_bot_with_info(bot, info)
    }

    /// Read a `.bot` with pre-resolved component info (from [`Build::load`] or a
    /// caller that already loaded the components).
    pub fn from_bot_with_info(bot: Bot, mut info: Vec<ComponentInfo>) -> Result<Self, BuildError> {
        let needed = bot.components.len();
        info.resize_with(needed, ComponentInfo::default);
        let mut bot = bot;
        if let Some(chassis) = bot.components.first_mut().and_then(|c| c.chassis.as_mut()) {
            normalize_chassis(chassis);
            info[0] = chassis_info(chassis);
        }
        let board = ControlBoard::from_bot(&bot).map_err(BuildError::Wiring)?;
        let paint = Paint::from_chassis(
            bot.components
                .first()
                .and_then(|c| c.chassis.as_ref())
                .unwrap_or(&Chassis::default()),
        );
        Ok(Build {
            bot,
            info,
            board,
            paint,
            limits: BuildLimits::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// Read a bot and resolve every component against the game data.
    pub fn load(library: &GameLibrary, path: &std::path::Path) -> Result<Self, BuildError> {
        let bot = GameLibrary::read_bot(path)?;
        let info = resolve_info(library, &bot);
        Self::from_bot_with_info(bot, info)
    }

    pub fn limits(&self) -> BuildLimits {
        self.limits
    }

    pub fn set_limits(&mut self, limits: BuildLimits) {
        self.limits = limits;
    }

    pub fn name(&self) -> &str {
        &self.bot.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.bot.name = name.into();
    }

    pub fn bot(&self) -> &Bot {
        &self.bot
    }

    pub fn components(&self) -> &[Component] {
        &self.bot.components
    }

    pub fn info(&self) -> &[ComponentInfo] {
        &self.info
    }

    pub fn component_info(&self, index: usize) -> Option<&ComponentInfo> {
        self.info.get(index)
    }

    pub fn chassis(&self) -> Option<&Chassis> {
        self.bot.components.first()?.chassis.as_ref()
    }

    pub fn chassis_mut(&mut self) -> Option<&mut Chassis> {
        let chassis = self.bot.components.first_mut()?.chassis.as_mut()?;
        Some(chassis)
    }

    /// Replace the chassis paint target (the appearance tab's own state).
    pub fn reset_paint(&mut self) {
        if let Some(chassis) = self.chassis() {
            self.paint = Paint::from_chassis(chassis);
        }
    }

    pub fn paint(&self) -> &Paint {
        &self.paint
    }

    pub fn paint_mut(&mut self) -> &mut Paint {
        &mut self.paint
    }

    pub fn board(&self) -> &ControlBoard {
        &self.board
    }

    pub fn board_mut(&mut self) -> &mut ControlBoard {
        &mut self.board
    }

    /// Attach a component from the game's data at `parent_mount` on `parent`,
    /// using `own_mount` on the child.
    pub fn attach(
        &mut self,
        library: &GameLibrary,
        file: &str,
        parent: usize,
        parent_mount: i32,
        own_mount: i32,
        placement: Placement,
    ) -> Result<usize, BuildError> {
        let mut info = library.component_info_by_file(file);
        if !info.loaded {
            return Err(BuildError::UnknownComponent(file.to_string()));
        }
        info.class = info.base.clone();
        self.attach_info(info, parent, parent_mount, own_mount, placement)
    }

    /// Attach a component the caller already resolved. This is also the
    /// synthetic path used by tests: no game directory is needed.
    pub fn attach_info(
        &mut self,
        info: ComponentInfo,
        parent: usize,
        parent_mount: i32,
        own_mount: i32,
        placement: Placement,
    ) -> Result<usize, BuildError> {
        if parent >= self.bot.components.len() {
            return Err(BuildError::ParentOutOfRange {
                parent,
                count: self.bot.components.len(),
            });
        }
        if parent == 0 {
            // The chassis has one mount, the baseplate, and every stock chassis
            // child names it. Its stored `parent_mount` is not a `.gmf` point.
            if parent_mount != 0 {
                return Err(BuildError::UnknownMount {
                    component: 0,
                    mount: parent_mount,
                    available: vec![0],
                });
            }
        }
        if parent > 0 {
            let parent_info = &self.info[parent];
            if !parent_info.has_mount(parent_mount) {
                return Err(BuildError::UnknownMount {
                    component: parent,
                    mount: parent_mount,
                    available: parent_info.mounts.keys().copied().collect(),
                });
            }
            if let Some(other) = self
                .bot
                .components
                .iter()
                .position(|c| c.parent == parent as i32 && c.parent_mount == parent_mount)
            {
                return Err(BuildError::MountTaken {
                    parent,
                    mount: parent_mount,
                    by: other,
                });
            }
            if !accepts(&parent_info.mounts, parent_mount, &info.mounts, own_mount) {
                return Err(BuildError::IncompatibleMount {
                    parent,
                    parent_mount,
                    own_mount,
                });
            }
        }
        if !info.has_mount(own_mount) {
            return Err(BuildError::UnknownMount {
                component: self.bot.components.len(),
                mount: own_mount,
                available: info.mounts.keys().copied().collect(),
            });
        }
        // The limit is checked before the component exists: if the robot is
        // already over it, the first attach is refused too.
        let mass = self.stats().mass + if info.has_hull { info.mass } else { 0.0 };
        if mass > self.limits.max_mass + 1e-3 {
            return Err(BuildError::OverWeight {
                mass,
                limit: self.limits.max_mass,
            });
        }

        self.snapshot();
        let index = self.bot.components.len();
        self.bot.components.push(Component {
            parent: parent as i32,
            parent_mount,
            own_mount,
            translation: placement.translation,
            rotation: placement.rotation,
            angle: placement.angle,
            height: placement.height,
            class: info.class.clone(),
            file: info.file.clone(),
            ..Default::default()
        });
        self.info.push(info);
        Ok(index)
    }

    /// Remove a component and everything attached below it.
    ///
    /// The bindings that named the removed components go with them; the parent
    /// indices and the remaining bindings' component indices are remapped.
    pub fn remove(&mut self, index: usize) -> Result<usize, BuildError> {
        if index == 0 {
            return Err(BuildError::CannotRemoveChassis);
        }
        if index >= self.bot.components.len() {
            return Err(BuildError::IndexOutOfRange {
                index,
                count: self.bot.components.len(),
            });
        }
        let n = self.bot.components.len();
        let mut doomed = vec![false; n];
        doomed[index] = true;
        loop {
            let mut changed = false;
            for i in 1..n {
                if !doomed[i] {
                    let parent = self.bot.components[i].parent;
                    if parent < 0 || parent as usize >= n {
                        continue;
                    }
                    if doomed[parent as usize] {
                        doomed[i] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let removed = doomed.iter().filter(|d| **d).count();
        let mut remap = vec![usize::MAX; n];
        let mut next = 0;
        for (i, doomed) in doomed.iter().enumerate() {
            if !doomed {
                remap[i] = next;
                next += 1;
            }
        }

        self.snapshot();
        let old_components = std::mem::take(&mut self.bot.components);
        self.bot.components = old_components
            .into_iter()
            .zip(doomed.iter())
            .filter(|(_, doomed)| !**doomed)
            .map(|(mut c, _)| {
                if c.parent >= 0 {
                    c.parent = remap[c.parent as usize] as i32;
                }
                c
            })
            .collect();
        let old_info = std::mem::take(&mut self.info);
        self.info = old_info
            .into_iter()
            .zip(doomed.iter())
            .filter(|(_, doomed)| !**doomed)
            .map(|(info, _)| info)
            .collect();

        let mut board = std::mem::take(&mut self.board);
        board.bindings.retain_mut(|binding| {
            let old = binding.component.max(0) as usize;
            if old >= n || doomed[old] {
                return false;
            }
            binding.component = remap[old] as i32;
            true
        });
        self.board = board;
        Ok(removed)
    }

    /// The F1 world transform of every component.
    pub fn transforms(&self) -> Vec<Mat4> {
        let maps: Vec<BTreeMap<i32, Mat4>> = self
            .info
            .iter()
            .map(|info| {
                info.mounts
                    .iter()
                    .map(|(id, a)| (*id, mount::tm_mat4(&a.transform)))
                    .collect()
            })
            .collect();
        mount::worlds(
            &self.bot,
            &MountFrames::from_maps(maps),
            MountOptions {
                mode: MountMode::FramesFlipped,
                chassis_plane: 0.0,
            },
        )
    }

    /// Weight, hitpoints and cost.
    pub fn stats(&self) -> BotStats {
        BotStats::new(&self.bot, &self.info)
    }

    /// Every wiring row, checked against the current component list.
    pub fn wiring_errors(&self) -> Vec<WiringError> {
        self.board.validate(&self.bot.components)
    }

    /// The bot as it would be saved: the board and the painted chassis applied.
    pub fn to_bot(&self) -> Bot {
        let mut bot = self.bot.clone();
        self.board.apply_to(&mut bot);
        if let Some(chassis) = bot.components.first_mut().and_then(|c| c.chassis.as_mut()) {
            self.paint.apply_to(chassis);
        }
        bot
    }

    /// Save in the game's own text form.
    pub fn save(&self) -> Vec<u8> {
        self.to_bot().write()
    }

    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.save())
    }

    /// Undo the last attach or remove (paint has its own undo).
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot_of());
        self.bot = previous.bot;
        self.info = previous.info;
        self.board = previous.board;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot_of());
        self.bot = next.bot;
        self.info = next.info;
        self.board = next.board;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn snapshot_of(&self) -> Snapshot {
        Snapshot {
            bot: self.bot.clone(),
            info: self.info.clone(),
            board: self.board.clone(),
        }
    }

    fn snapshot(&mut self) {
        let snapshot = self.snapshot_of();
        self.undo.push(snapshot);
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

/// Resolve every component of a bot against the game's data, mirroring the
/// renderer's load rules (the chassis and SmartZones carry no definition/mesh).
pub fn resolve_info(library: &GameLibrary, bot: &Bot) -> Vec<ComponentInfo> {
    bot.components
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                chassis_info(c.chassis.as_ref().unwrap_or(&Chassis::default()))
            } else {
                library.component_info(&c.class, &c.file)
            }
        })
        .collect()
}

/// Fill the cosmetic armour row the game's writer emits, so the in-memory
/// chassis and its saved form are byte-equal after a parse. The writer's
/// fallback is the armour type's own material row.
fn normalize_chassis(chassis: &mut Chassis) {
    if chassis.mesh.is_some() && chassis.armour_material.is_none() {
        chassis.armour_material = Some(chassis.armour.unwrap_or_default().material_row());
    }
}

fn chassis_info(chassis: &Chassis) -> ComponentInfo {
    ComponentInfo {
        class: "Chassis".into(),
        file: String::new(),
        name: "Chassis".into(),
        base: "Chassis".into(),
        def: None,
        mounts: BTreeMap::new(),
        mass: chassis.mass().max(5.0),
        has_hull: chassis.mesh.as_ref().is_some_and(|m| !m.points.is_empty()),
        loaded: true,
    }
}

/// Whether the parent's mount accepts the child's, when both carry type lists.
fn accepts(
    parent: &BTreeMap<i32, Attachment>,
    parent_mount: i32,
    own: &BTreeMap<i32, Attachment>,
    own_mount: i32,
) -> bool {
    let (Some(parent), Some(own)) = (parent.get(&parent_mount), own.get(&own_mount)) else {
        return false;
    };
    if parent.types.is_empty() || own.types.is_empty() {
        // No declared class: the file allows the placement (the game's UI may
        // still refuse it, but there is no data to say so).
        return true;
    }
    own.types.iter().any(|t| parent.accepts(t))
}

/// Why a build action was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum BuildError {
    ParentOutOfRange {
        parent: usize,
        count: usize,
    },
    IndexOutOfRange {
        index: usize,
        count: usize,
    },
    UnknownMount {
        component: usize,
        mount: i32,
        available: Vec<i32>,
    },
    MountTaken {
        parent: usize,
        mount: i32,
        by: usize,
    },
    IncompatibleMount {
        parent: usize,
        parent_mount: i32,
        own_mount: i32,
    },
    UnknownComponent(String),
    CannotRemoveChassis,
    OverWeight {
        mass: f32,
        limit: f32,
    },
    Wiring(WiringError),
    Library(LibraryError),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::ParentOutOfRange { parent, count } => {
                write!(f, "parent {parent} does not exist ({count} components)")
            }
            BuildError::IndexOutOfRange { index, count } => {
                write!(f, "component {index} does not exist ({count} components)")
            }
            BuildError::UnknownMount {
                component,
                mount,
                available,
            } => write!(
                f,
                "component {component} has no mount {mount}; it has {available:?}"
            ),
            BuildError::MountTaken { parent, mount, by } => {
                write!(
                    f,
                    "mount {mount} on component {parent} already holds component {by}"
                )
            }
            BuildError::IncompatibleMount {
                parent,
                parent_mount,
                own_mount,
            } => write!(
                f,
                "mount {parent_mount} on component {parent} does not accept own mount {own_mount}"
            ),
            BuildError::UnknownComponent(file) => write!(f, "no component definition for {file:?}"),
            BuildError::CannotRemoveChassis => write!(f, "the chassis cannot be removed"),
            BuildError::OverWeight { mass, limit } => {
                write!(
                    f,
                    "total mass {mass:.1} kg exceeds the editor's {limit:.0} kg limit"
                )
            }
            BuildError::Wiring(e) => write!(f, "{e}"),
            BuildError::Library(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<LibraryError> for BuildError {
    fn from(e: LibraryError) -> Self {
        BuildError::Library(e)
    }
}

impl From<WiringError> for BuildError {
    fn from(e: WiringError) -> Self {
        BuildError::Wiring(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::bot::{Armour, BotMesh, Face, FaceGroup};
    use ra2fmt::mount::tm_mat4;

    fn mesh() -> BotMesh {
        BotMesh {
            points: vec![
                [-0.5, 0.0, -0.5],
                [0.5, 0.0, -0.5],
                [0.5, 0.0, 0.5],
                [-0.5, 0.0, 0.5],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    fn chassis() -> Chassis {
        Chassis {
            height: 0.5,
            face_count: 2,
            mesh: Some(mesh()),
            bottom: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
            top: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
            face_groups: vec![FaceGroup {
                coords: vec![[0.0, 0.0], [32.0, 0.0], [32.0, 32.0]],
                bounds: [0, 0, 32, 32],
            }],
            faces: vec![Face {
                group: 0,
                points: [0, 1, 2],
            }],
            armour: Some(Armour {
                type_id: 0,
                weight: 8.0,
                hardness: 80.0,
            }),
            ..Default::default()
        }
    }

    fn attachment(id: i32, types: &[&str], at: [f32; 3]) -> (i32, Attachment) {
        let transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], at];
        (
            id,
            Attachment {
                id,
                types: types.iter().map(|s| s.to_string()).collect(),
                transform,
                ..Default::default()
            },
        )
    }

    fn motor_info() -> ComponentInfo {
        let mounts = [
            attachment(0, &["base_f"], [0.0, 0.2, 0.0]),
            attachment(1, &["axle_m"], [0.0, 0.3, 0.0]),
        ]
        .into_iter()
        .collect();
        ComponentInfo {
            class: "SpinMotor".into(),
            file: "memory://motor".into(),
            name: "Test Motor".into(),
            base: "SpinMotor".into(),
            def: None,
            mounts,
            mass: 16.0,
            has_hull: true,
            loaded: true,
        }
    }

    fn wheel_info() -> ComponentInfo {
        let mounts = [attachment(1, &["axle_f"], [0.1, 0.0, 0.0])]
            .into_iter()
            .collect();
        ComponentInfo {
            class: "Wheel".into(),
            file: "memory://wheel".into(),
            name: "Test Wheel".into(),
            base: "Wheel".into(),
            def: None,
            mounts,
            mass: 8.0,
            has_hull: true,
            loaded: true,
        }
    }

    /// The M9.2 acceptance: build from nothing with a chassis and two
    /// components, save with the game's writer, read it back, and the tree,
    /// mounts and transforms are identical.
    #[test]
    fn a_built_bot_saves_and_reloads_with_the_same_tree_mounts_and_transforms() {
        let mut build = Build::new("Built", chassis());
        let motor = build
            .attach_info(motor_info(), 0, 0, 0, Placement::at([0.1, 0.0, 0.2]))
            .unwrap();
        assert_eq!(motor, 1);
        let wheel = build
            .attach_info(wheel_info(), motor, 1, 1, Placement::default())
            .unwrap();
        assert_eq!(wheel, 2);
        assert_eq!(build.components().len(), 3);
        assert_eq!(build.info().len(), 3);

        // The chain link snapped the wheel's own point onto the motor's:
        // `world[wheel] * A_own` and `world[motor] * A_parent` coincide.
        let world = build.transforms();
        let motor_frame = tm_mat4(&motor_info().mounts[&1].transform);
        let wheel_frame = tm_mat4(&wheel_info().mounts[&1].transform);
        let wheel_point = (world[2] * wheel_frame).transform_point3(glam::Vec3::ZERO);
        let motor_point = (world[1] * motor_frame).transform_point3(glam::Vec3::ZERO);
        assert!(
            (wheel_point - motor_point).length() < 1e-5,
            "wheel point {wheel_point:?} != motor point {motor_point:?}"
        );
        // The stored components' own transforms are untouched by the model.
        assert_eq!(build.components()[1].translation, [0.1, 0.0, 0.2]);

        // Save and read back, then rebuild the model with the same resolved
        // component data.
        let bytes = build.save();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        assert_eq!(back.components, build.components());
        let info = build.info().to_vec();
        let reloaded = Build::from_bot_with_info(back.clone(), info).unwrap();
        assert_eq!(reloaded.components(), build.components());
        assert_eq!(reloaded.transforms(), world);
        assert_eq!(reloaded.stats(), build.stats());
        assert_eq!(reloaded.board(), build.board());
        // The writer round-trips the file it just wrote.
        assert_eq!(back.write(), bytes);
    }

    #[test]
    fn one_mount_holds_one_component() {
        let mut build = Build::new("Mounts", chassis());
        let motor = build
            .attach_info(motor_info(), 0, 0, 0, Placement::default())
            .unwrap();
        build
            .attach_info(wheel_info(), motor, 1, 1, Placement::default())
            .unwrap();
        let err = build
            .attach_info(wheel_info(), motor, 1, 1, Placement::default())
            .unwrap_err();
        assert_eq!(
            err,
            BuildError::MountTaken {
                parent: motor,
                mount: 1,
                by: 2
            }
        );
    }

    /// The chassis mount is the baseplate: many children may name it, because
    /// each one is placed by its own stored transform (every stock bot does).
    #[test]
    fn the_chassis_baseplate_takes_many_children() {
        let mut build = Build::new("Baseplate", chassis());
        build
            .attach_info(motor_info(), 0, 0, 0, Placement::at([0.2, 0.0, 0.0]))
            .unwrap();
        build
            .attach_info(motor_info(), 0, 0, 0, Placement::at([-0.2, 0.0, 0.0]))
            .unwrap();
        assert_eq!(build.components().len(), 3);
    }

    #[test]
    fn a_mount_that_does_not_exist_is_refused_with_the_list() {
        let mut build = Build::new("Bad mount", chassis());
        let err = build
            .attach_info(motor_info(), 0, 0, 7, Placement::default())
            .unwrap_err();
        assert_eq!(
            err,
            BuildError::UnknownMount {
                component: 1,
                mount: 7,
                available: vec![0, 1]
            }
        );
    }

    #[test]
    fn an_incompatible_mount_is_refused() {
        let mut build = Build::new("Bad mate", chassis());
        let motor = build
            .attach_info(motor_info(), 0, 0, 0, Placement::default())
            .unwrap();
        // The motor's `base_f` (id 0) is a flat face; a wheel's `axle_f` is not
        // its mate.
        let err = build
            .attach_info(wheel_info(), motor, 0, 1, Placement::default())
            .unwrap_err();
        assert!(
            matches!(err, BuildError::IncompatibleMount { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn removing_a_parent_removes_its_subtree_and_undo_restores_it() {
        let mut build = Build::new("Undo", chassis());
        let motor = build
            .attach_info(motor_info(), 0, 0, 0, Placement::default())
            .unwrap();
        build
            .attach_info(wheel_info(), motor, 1, 1, Placement::default())
            .unwrap();
        build
            .board_mut()
            .add(crate::workshop::wiring::Control::new(
                "Forward",
                [1, 0],
                crate::workshop::wiring::ControlKind::Analog,
            ))
            .unwrap();
        build
            .board_mut()
            .bind(1, crate::workshop::wiring::Action::SpinClockwise, [1, 0], 0)
            .unwrap();
        let before = build.components().to_vec();
        assert_eq!(build.remove(motor).unwrap(), 2);
        assert_eq!(build.components().len(), 1);
        assert!(build.undo());
        assert_eq!(build.components(), before.as_slice());
        assert!(build.redo());
        assert_eq!(build.components().len(), 1);
    }

    #[test]
    fn the_editor_weight_limit_is_enforced() {
        let mut build = Build::new("Heavy", chassis());
        let mut heavy = motor_info();
        heavy.mass = 1000.0;
        let err = build
            .attach_info(heavy, 0, 0, 0, Placement::default())
            .unwrap_err();
        assert!(matches!(err, BuildError::OverWeight { .. }), "{err:?}");
        assert_eq!(build.components().len(), 1);
    }

    /// F1 against the game's own data: a real motor and a real wheel, whose
    /// frames are in their meshes. The model's transform must be the renderer's
    /// (same function) and the link must snap.
    #[test]
    fn a_real_motor_and_wheel_keep_the_game_mount_rule() {
        let Some(root) = game_root() else { return };
        let library = GameLibrary::open(&root).unwrap();
        let data = std::fs::read(root.join("AI/Team1/Bot0.bot")).unwrap();
        let stock = ra2fmt::bot::parse(&data).unwrap();
        let mut chassis = stock.components[0].chassis.clone().unwrap();
        chassis.texture_tga = None;
        let mut build = Build::new("Real", chassis);
        let motor_info = library.component_info_by_file("Components\\anglemotor1.txt");
        let base_mount = *motor_info
            .mounts
            .keys()
            .next()
            .expect("the angle motor has a base point");
        let motor = build
            .attach(
                &library,
                "Components\\anglemotor1.txt",
                0,
                0,
                base_mount,
                Placement::default(),
            )
            .unwrap();
        let parent_mount = *motor_info
            .mounts
            .iter()
            .find(|(_, a)| a.types.iter().any(|t| t == "axle_m"))
            .map(|(id, _)| id)
            .expect("the angle motor has an axle_M point");
        let wheel_info = library.component_info_by_file("Components\\miniwheel.txt");
        let own_mount = *wheel_info
            .mounts
            .iter()
            .find(|(_, a)| a.types.iter().any(|t| t == "axle_f"))
            .map(|(id, _)| id)
            .expect("the mini wheel has an axle_F point");
        let wheel = build
            .attach(
                &library,
                "Components\\miniwheel.txt",
                motor,
                parent_mount,
                own_mount,
                Placement::default(),
            )
            .unwrap();
        let world = build.transforms();
        let wheel_point = (world[wheel] * tm_mat4(&wheel_info.mounts[&own_mount].transform))
            .transform_point3(glam::Vec3::ZERO);
        let motor_point = (world[motor] * tm_mat4(&motor_info.mounts[&parent_mount].transform))
            .transform_point3(glam::Vec3::ZERO);
        assert!((wheel_point - motor_point).length() < 1e-4);

        let dir = std::env::temp_dir().join(format!("ra2-build-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Real.bot");
        build.save_to(&path).unwrap();
        let reloaded = Build::load(&library, &path).unwrap();
        assert_eq!(reloaded.components(), build.components());
        assert_eq!(reloaded.transforms(), world);
        // The reloaded model resolves the real masses, so the stats survive too.
        assert!((reloaded.stats().mass - build.stats().mass).abs() < 1e-3);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn game_root() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = std::path::PathBuf::from(dir);
            return dir.join("Components").is_dir().then_some(dir);
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.join("Components").is_dir() {
                return Some(candidate);
            }
        }
        None
    }
}
