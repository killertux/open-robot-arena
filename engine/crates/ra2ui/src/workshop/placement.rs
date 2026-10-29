//! The Components tab's placement drag: pick a part, press Attach, drag it to
//! where it should go, click.
//!
//! This is the model half of the original's `BotLabComponents` flow
//! (`docs/36-playthrough.md` §9.18). Everything with an address is read from
//! the exe; the pieces the exe leaves to its physics or 3D engine are stated as
//! ours.
//!
//! ## The flow
//!
//! 1. **Pick.** A palette click only selects the part (`FUN_0042e650`): the
//!    `comp_preview` group shows its name, description and weight, its
//!    `Component option` styles, and a green box (`AP_box_*.gmf`) on each of
//!    its attachment points that can be the part's own mount ([`mount_mask`],
//!    [`selectable_mounts`]). The tip under the preview is [`TIP_ATTACH`] when
//!    the part has fewer than two attachment points, else [`TIP_BOXES`].
//! 2. **Attach** (`FUN_0042c3a0`). The editor's mass check first ("Total mass of
//!    robot must not exceed 800 kg.", `FUN_0042f790`: the robot's mass plus the
//!    part's weight must not pass the limit); then the drag starts, tip
//!    [`TIP_DRAG`].
//! 3. **Drag** (`FUN_0042c850`, every mouse move). The part follows the mouse.
//!    Over the chassis it lands on the baseplate; over a placed part it snaps
//!    onto a free attachment point that accepts its own. Each move is judged
//!    ([`Build::judge`]) and the part is drawn red while it is refused, with the
//!    refusal as the tip ([`Refusal::message`]).
//! 4. **SHIFT** (`FUN_0042d360`) turns the part: the dominant axis of the mouse
//!    movement times 0.2 is added to a counter that wraps at 36, and the part's
//!    angle is `trunc(counter) * 10°` ([`Drag::rotate`]). **CTRL**
//!    (`FUN_0042d3e0`) raises it: the same movement times 0.2 added to a
//!    counter, clamped at 0 below and at the chassis height above, in 0.05 m
//!    steps ([`Drag::raise`]).
//! 5. **Click** (`FUN_0042d4b0`, on the mouse release). If the part is over
//!    something and not refused it is added to the robot (`FUN_004dbf40`), the
//!    attach sound plays, and the mass readout updates. A refused part stays in
//!    the drag. Releasing over nothing cancels the drag (`FUN_0042c280`).
//!
//! There is **no move**: a placed part is selected by a click (`FUN_0042d280`)
//! and the remove button deletes it (`FUN_0042f270`); to put it elsewhere the
//! player removes it and attaches it again. The remove button refuses a part
//! with children ("You must delete the components attached to this first.").
//!
//! ## What is ours
//!
//! - The exe's overlap test is its physics world (`FUN_004a8660`); ours is an
//!   oriented-box test on the parts' mesh bounds ([`Build::judge`]), with the
//!   parent skipped (a mounted pair always touches).
//! - The exe's "inside the chassis" test is a chassis virtual call
//!   (`+0x1c`); ours is the box's footprint inside the baseplate outline.
//! - The pick radius around an attachment point is ours ([`PICK_RADIUS`]).

use glam::{Mat4, Vec3};
use ra2fmt::bot::{Bot, Component};
use ra2fmt::gmi::Attachment;
use ra2fmt::mount::{self, MountFrames, MountMode, MountOptions};

use super::build::{Build, BuildError, Placement};
use super::library::ComponentInfo;

/// `0x6f07f4`: the tip when the part has fewer than two attachment points.
pub const TIP_ATTACH: &str = "Click the Attach button to place a component.";
/// `0x6f0828`: the tip when it has several.
pub const TIP_BOXES: &str = "Click a green box to change the component's attachment point.  Then click the Attach button to place a component.";
/// `0x6f04f8`: the tip while dragging.
pub const TIP_DRAG: &str = "Drag the component where you want it to go, then click. Hold shift to rotate the component. Hold control to raise/lower the component.";
/// `0x6f091c`: the remove button's refusal for a part that has children.
pub const REMOVE_REFUSAL: &str = "You must delete the components attached to this first.";

/// `0x6a6080`: movement units to counter steps, shared by SHIFT and CTRL.
pub const INPUT_TO_STEPS: f32 = 0.2;
/// `0x6a5a68`: the rotation counter wraps at 36 steps.
pub const TURN_STEPS: f32 = 36.0;
/// `0x6a5df4` times `0x6a607c`: one rotation step, 10 degrees.
pub const TURN_STEP: f32 = 10.0 * 0.017_453_292;
/// `0x6f035c`: one raise step, in metres.
pub const RAISE_STEP: f32 = 0.05;
/// `0x6a6078`: the farthest a part's origin may be from the robot's origin.
pub const MAX_REACH: f32 = 3.0;
/// `0x6a6074`: the lowest a part's origin may be.
pub const MIN_Y: f32 = -1.5;
/// How far two boxes may overlap before they count as intersecting (ours).
pub const TOUCH: f32 = 0.05;
/// How near the mouse ray must pass an attachment point to pick it (ours).
pub const PICK_RADIUS: f32 = 0.09;

/// Why a dragged part cannot be dropped where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `0x6f05f8`, code 3.
    Intersects,
    /// `0x6f063c`, codes 2 and 4.
    TooLarge,
    /// `0x6f05c0`, code 5.
    OutsideChassis,
}

impl Refusal {
    /// The tip text the exe shows.
    pub fn message(self) -> &'static str {
        match self {
            Refusal::Intersects => {
                "Components should not intersect the chassis or other components."
            }
            Refusal::TooLarge => "Robots cannot exceed a maximum size.",
            Refusal::OutsideChassis => "Baseplate components must fit inside the chassis.",
        }
    }
}

/// The attachment-type bits (`FUN_0045de80`): `base_m` 1, `base_f` 2,
/// `generic_m` 4, `generic_f` 8, `axle_m` 0x10, `axle_f` 0x20.
pub fn mount_mask(attachment: &Attachment) -> u32 {
    attachment.types.iter().fold(0, |mask, t| {
        mask | match t.to_ascii_lowercase().as_str() {
            "base_m" => 1,
            "base_f" => 2,
            "generic_m" => 4,
            "generic_f" => 8,
            "axle_m" => 0x10,
            "axle_f" => 0x20,
            _ => 0,
        }
    })
}

/// The attachment points that get a green box (`FUN_0042e650`): every point
/// except those whose type is exactly `axle_m` (0x10) or exactly `generic_m`
/// (4), the plugs other parts push onto.
pub fn selectable_mounts(info: &ComponentInfo) -> Vec<i32> {
    info.mounts
        .iter()
        .filter(|(_, a)| {
            let mask = mount_mask(a);
            mask != 0x10 && mask != 4
        })
        .map(|(id, _)| *id)
        .collect()
}

/// The tip under the component preview: the plain one for a part with fewer
/// than two attachment points.
pub fn preview_tip(info: &ComponentInfo) -> &'static str {
    if info.mounts.len() < 2 {
        TIP_ATTACH
    } else {
        TIP_BOXES
    }
}

/// The own mount a fresh preview starts on: the first green box.
pub fn default_own_mount(info: &ComponentInfo) -> i32 {
    selectable_mounts(info)
        .first()
        .copied()
        .or_else(|| info.mounts.keys().next().copied())
        .unwrap_or(0)
}

/// A part being dragged.
#[derive(Clone, Debug)]
pub struct Drag {
    pub info: ComponentInfo,
    /// The display name (the preview's name box).
    pub name: String,
    /// The green box chosen in the preview: the part's own attachment point.
    pub own_mount: i32,
    /// SHIFT's counter (`this+0x11c`), `0..=36`.
    spin: f32,
    /// CTRL's counter (`this+0x120`).
    lift: f32,
}

impl Drag {
    pub fn new(info: ComponentInfo, name: impl Into<String>, own_mount: i32) -> Self {
        Drag {
            info,
            name: name.into(),
            own_mount,
            spin: 0.0,
            lift: 0.0,
        }
    }

    /// SHIFT with the mouse moving by `input` (the dominant axis, signed).
    pub fn rotate(&mut self, input: f32) {
        let spin = self.spin + input * INPUT_TO_STEPS;
        self.spin = if spin >= 0.0 {
            if spin > TURN_STEPS {
                spin - TURN_STEPS
            } else {
                spin
            }
        } else {
            spin + TURN_STEPS
        };
    }

    /// CTRL with the mouse moving by `input`. `max_height` is the chassis
    /// height in metres (`this+0xd0`'s `+0x2ec`).
    pub fn raise(&mut self, input: f32, max_height: f32) {
        let lift = self.lift + input * INPUT_TO_STEPS;
        self.lift = if lift < 0.0 {
            0.0
        } else if lift * RAISE_STEP > max_height {
            max_height / RAISE_STEP
        } else {
            lift
        };
    }

    /// The part's turn, radians: `trunc(counter) * 10°`.
    pub fn angle(&self) -> f32 {
        self.spin.trunc() * TURN_STEP
    }

    /// The part's lift, metres: `trunc(counter) * 0.05`.
    pub fn height(&self) -> f32 {
        self.lift.trunc() * RAISE_STEP
    }

    pub fn spin_steps(&self) -> f32 {
        self.spin
    }

    pub fn lift_steps(&self) -> f32 {
        self.lift
    }
}

/// What the mouse is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hover {
    /// The chassis' baseplate, at a bot-local `(x, z)` in metres.
    Baseplate([f32; 2]),
    /// A free attachment point of a placed part.
    Mount { parent: usize, mount: i32 },
}

/// Where a dragged part would end up.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub parent: usize,
    pub parent_mount: i32,
    pub own_mount: i32,
    pub placement: Placement,
    /// The part's bot-local transform if dropped here.
    pub world: Mat4,
}

/// Why a drop failed.
#[derive(Clone, Debug, PartialEq)]
pub enum DropError {
    Build(BuildError),
    Refused(Refusal),
}

impl std::fmt::Display for DropError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DropError::Build(e) => write!(f, "{e}"),
            DropError::Refused(r) => write!(f, "{}", r.message()),
        }
    }
}

impl std::error::Error for DropError {}

impl From<BuildError> for DropError {
    fn from(e: BuildError) -> Self {
        DropError::Build(e)
    }
}

/// The text of the Attach button's mass refusal (`0x6f0590` + limit +
/// `0x6f05b8`).
pub fn mass_message(limit: f32) -> String {
    format!("Total mass of robot must not exceed {limit:.0} kg.")
}

impl Build {
    /// The Attach button's first act (`FUN_0042f790`): the robot's mass plus
    /// the part's must not pass the limit.
    pub fn check_attach_mass(&self, info: &ComponentInfo) -> Result<(), BuildError> {
        let mass = self.stats().mass + if info.has_hull { info.mass } else { 0.0 };
        if mass > self.limits().max_mass + 1e-3 {
            return Err(BuildError::OverWeight {
                mass,
                limit: self.limits().max_mass,
            });
        }
        Ok(())
    }

    /// The chassis height in metres, CTRL's ceiling.
    pub fn chassis_height(&self) -> f32 {
        self.chassis().map(|c| c.height).unwrap_or(0.0)
    }

    /// Whether `mount` of placed part `parent` is free and takes the dragged
    /// part's own mount.
    fn socket_takes(&self, drag: &Drag, parent: usize, mount: i32) -> bool {
        let Some(info) = self.info().get(parent) else {
            return false;
        };
        let Some(socket) = info.mounts.get(&mount) else {
            return false;
        };
        let Some(own) = drag.info.mounts.get(&drag.own_mount) else {
            return false;
        };
        let taken = self
            .components()
            .iter()
            .any(|c| c.parent == parent as i32 && c.parent_mount == mount);
        if taken {
            return false;
        }
        if socket.types.is_empty() || own.types.is_empty() {
            return true;
        }
        own.types.iter().any(|t| socket.accepts(t))
    }

    /// Where the part would go if dropped on `hover`, or `None` when the point
    /// cannot take it (the mount is taken, absent or the wrong kind).
    pub fn plan(&self, drag: &Drag, hover: &Hover) -> Option<Candidate> {
        if !drag.info.has_mount(drag.own_mount) {
            return None;
        }
        let (parent, parent_mount, placement) = match *hover {
            Hover::Baseplate(point) => (
                0,
                0,
                Placement::on_baseplate(point, drag.angle(), drag.height()),
            ),
            Hover::Mount { parent, mount } => {
                if parent == 0 || !self.socket_takes(drag, parent, mount) {
                    return None;
                }
                (
                    parent,
                    mount,
                    Placement {
                        angle: drag.angle(),
                        ..Placement::default()
                    },
                )
            }
        };
        let world = self.candidate_world(drag, parent, parent_mount, &placement);
        Some(Candidate {
            parent,
            parent_mount,
            own_mount: drag.own_mount,
            placement,
            world,
        })
    }

    /// The bot-local transform of the part if it were attached: the model's
    /// own mount rule on a copy of the component list without the chassis mesh.
    fn candidate_world(
        &self,
        drag: &Drag,
        parent: usize,
        parent_mount: i32,
        placement: &Placement,
    ) -> Mat4 {
        let mut components: Vec<Component> = self
            .components()
            .iter()
            .map(|c| Component {
                parent: c.parent,
                parent_mount: c.parent_mount,
                own_mount: c.own_mount,
                translation: c.translation,
                rotation: c.rotation,
                angle: c.angle,
                height: c.height,
                burst_angles: c.burst_angles,
                ..Default::default()
            })
            .collect();
        components.push(Component {
            parent: parent as i32,
            parent_mount,
            own_mount: drag.own_mount,
            translation: placement.translation,
            rotation: placement.rotation,
            angle: placement.angle,
            height: placement.height,
            ..Default::default()
        });
        let bot = Bot {
            components,
            ..Bot::default()
        };
        let mut tables: Vec<Option<_>> = self
            .info()
            .iter()
            .map(|info| Some(info.mounts.clone()))
            .collect();
        tables.push(Some(drag.info.mounts.clone()));
        let mut hinges: Vec<_> = self.info().iter().map(|i| i.hinge.clone()).collect();
        hinges.push(drag.info.hinge.clone());
        let worlds = mount::worlds(
            &bot,
            &MountFrames::from_tables(&tables).with_hinges(hinges),
            MountOptions {
                mode: MountMode::Game,
                chassis_plane: 0.0,
            },
        );
        worlds.last().copied().unwrap_or(Mat4::IDENTITY)
    }

    /// The first refusal for a candidate drop, in the exe's order: collision
    /// (code 3), size (4), then the baseplate fit (5).
    pub fn judge(&self, drag: &Drag, candidate: &Candidate) -> Option<Refusal> {
        let mine = drag.info.bounds.map(|b| Obb::new(candidate.world, b));
        if let Some(mine) = &mine {
            let others = self.transforms();
            for (i, info) in self.info().iter().enumerate().skip(1) {
                if i == candidate.parent {
                    continue;
                }
                let (Some(bounds), Some(world)) = (info.bounds, others.get(i)) else {
                    continue;
                };
                if mine.overlaps(&Obb::new(*world, bounds), TOUCH) {
                    return Some(Refusal::Intersects);
                }
            }
        }
        let origin = candidate.world.w_axis.truncate();
        if origin.length() > MAX_REACH || origin.y < MIN_Y {
            return Some(Refusal::TooLarge);
        }
        if candidate.parent == 0 {
            if let (Some(mine), Some(chassis)) = (&mine, self.chassis()) {
                let points = [[origin.x, origin.z], [mine.centre.x, mine.centre.z]];
                if !fits_chassis(chassis, &points) {
                    return Some(Refusal::OutsideChassis);
                }
            }
        }
        None
    }

    /// Judge and add the part: the click (`FUN_0042d4b0`).
    pub fn drop_part(&mut self, drag: &Drag, hover: &Hover) -> Result<usize, DropError> {
        let candidate =
            self.plan(drag, hover)
                .ok_or(DropError::Build(BuildError::UnknownMount {
                    component: 0,
                    mount: drag.own_mount,
                    available: drag.info.mounts.keys().copied().collect(),
                }))?;
        if let Some(refusal) = self.judge(drag, &candidate) {
            return Err(DropError::Refused(refusal));
        }
        Ok(self.attach_info(
            drag.info.clone(),
            candidate.parent,
            candidate.parent_mount,
            candidate.own_mount,
            candidate.placement,
        )?)
    }

    /// What a mouse ray (bot-local) is over, for a dragged part: a free
    /// compatible attachment point of a placed part within [`PICK_RADIUS`] of
    /// the ray, nearest first, else the baseplate where the ray meets `y = 0`.
    pub fn pick(&self, drag: &Drag, origin: Vec3, direction: Vec3) -> Option<Hover> {
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        let worlds = self.transforms();
        let mut best: Option<(f32, Hover)> = None;
        for (i, info) in self.info().iter().enumerate().skip(1) {
            let Some(world) = worlds.get(i) else { continue };
            for (id, attachment) in &info.mounts {
                if !self.socket_takes(drag, i, *id) {
                    continue;
                }
                let at =
                    (*world * mount::tm_mat4(&attachment.transform)).transform_point3(Vec3::ZERO);
                let t = (at - origin).dot(direction);
                if t <= 0.0 {
                    continue;
                }
                if (at - origin - direction * t).length() > PICK_RADIUS {
                    continue;
                }
                if best.as_ref().is_none_or(|(bt, _)| t < *bt) {
                    best = Some((
                        t,
                        Hover::Mount {
                            parent: i,
                            mount: *id,
                        },
                    ));
                }
            }
        }
        if let Some((_, hover)) = best {
            return Some(hover);
        }
        if direction.y.abs() < 1e-6 {
            return None;
        }
        let t = -origin.y / direction.y;
        if t < 0.0 {
            return None;
        }
        let hit = origin + direction * t;
        let half = mount::BASEPLATE_HALF_EXTENT;
        if hit.x.abs() > half || hit.z.abs() > half {
            return None;
        }
        Some(Hover::Baseplate([hit.x, hit.z]))
    }

    /// The placed part under a mouse ray (bot-local), nearest first
    /// (`FUN_0042d280` asks each part to pick; ours tests the mesh boxes).
    pub fn pick_part(&self, origin: Vec3, direction: Vec3) -> Option<usize> {
        let direction = direction.normalize_or_zero();
        let worlds = self.transforms();
        let mut best: Option<(f32, usize)> = None;
        for (i, info) in self.info().iter().enumerate().skip(1) {
            let (Some(bounds), Some(world)) = (info.bounds, worlds.get(i)) else {
                continue;
            };
            if let Some(t) = Obb::new(*world, bounds).ray(origin, direction) {
                if best.is_none_or(|(bt, _)| t < bt) {
                    best = Some((t, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// The remove button (`FUN_0042f270`): a part with children is refused.
    pub fn remove_leaf(&mut self, index: usize) -> Result<(), BuildError> {
        if index == 0 {
            return Err(BuildError::CannotRemoveChassis);
        }
        if index >= self.components().len() {
            return Err(BuildError::IndexOutOfRange {
                index,
                count: self.components().len(),
            });
        }
        if self.components().iter().any(|c| c.parent == index as i32) {
            return Err(BuildError::HasChildren { index });
        }
        self.remove(index).map(|_| ())
    }
}

/// The bottom outline in bot-local metres, `None` when the chassis has none.
fn baseplate_polygon(chassis: &ra2fmt::bot::Chassis) -> Option<Vec<[f32; 2]>> {
    (chassis.bottom.len() >= 3).then(|| {
        chassis
            .bottom
            .iter()
            .map(|p| mount::baseplate_to_world([p[0] as f32, p[1] as f32]))
            .collect()
    })
}

/// The exe tests two points against the chassis (`+0x1c`): the part's origin
/// and one raised by its extent. Ours tests the origin and the mesh box's
/// centre against the baseplate outline; every stock chassis child passes.
fn fits_chassis(chassis: &ra2fmt::bot::Chassis, points: &[[f32; 2]]) -> bool {
    match baseplate_polygon(chassis) {
        Some(polygon) => points.iter().all(|p| inside(&polygon, *p)),
        None => true,
    }
}

/// Even-odd point-in-polygon.
fn inside(polygon: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut hit = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            hit = !hit;
        }
        j = i;
    }
    hit
}

/// An oriented box.
struct Obb {
    centre: Vec3,
    axes: [Vec3; 3],
    half: [f32; 3],
}

impl Obb {
    fn new(world: Mat4, bounds: ([f32; 3], [f32; 3])) -> Self {
        let (mn, mx) = (Vec3::from(bounds.0), Vec3::from(bounds.1));
        let local_centre = (mn + mx) * 0.5;
        let half = (mx - mn) * 0.5;
        let axes = [
            world.x_axis.truncate(),
            world.y_axis.truncate(),
            world.z_axis.truncate(),
        ];
        // The axes carry the scale (a mount frame has none, but be exact).
        let scale = [axes[0].length(), axes[1].length(), axes[2].length()];
        Obb {
            centre: world.transform_point3(local_centre),
            axes: [
                axes[0].normalize_or_zero(),
                axes[1].normalize_or_zero(),
                axes[2].normalize_or_zero(),
            ],
            half: [half.x * scale[0], half.y * scale[1], half.z * scale[2]],
        }
    }

    /// The distance along a ray to where it enters the box.
    fn ray(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let (mut near, mut far) = (0.0_f32, f32::MAX);
        let o = origin - self.centre;
        for i in 0..3 {
            let (po, pd) = (o.dot(self.axes[i]), direction.dot(self.axes[i]));
            if pd.abs() < 1e-8 {
                if po.abs() > self.half[i] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((-self.half[i] - po) / pd, (self.half[i] - po) / pd);
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
        Some(near)
    }

    fn radius_on(&self, axis: Vec3) -> f32 {
        (0..3)
            .map(|i| self.half[i] * self.axes[i].dot(axis).abs())
            .sum()
    }

    /// Separating-axis test; the boxes intersect when they overlap by more than
    /// `touch` on every axis.
    fn overlaps(&self, other: &Obb, touch: f32) -> bool {
        let d = other.centre - self.centre;
        let mut axes: Vec<Vec3> = Vec::with_capacity(15);
        axes.extend(self.axes);
        axes.extend(other.axes);
        for a in self.axes {
            for b in other.axes {
                let c = a.cross(b);
                if c.length_squared() > 1e-8 {
                    axes.push(c.normalize());
                }
            }
        }
        axes.iter().all(|axis| {
            let reach = self.radius_on(*axis) + other.radius_on(*axis);
            reach - d.dot(*axis).abs() > touch
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_turns_in_ten_degree_steps_and_wraps_at_thirty_six() {
        let mut drag = Drag::new(ComponentInfo::default(), "part", 0);
        assert_eq!(drag.angle(), 0.0);
        // Five movement units are one step (0.2 each).
        drag.rotate(4.0);
        assert_eq!(drag.angle(), 0.0, "0.8 steps truncate to none");
        drag.rotate(1.0);
        assert!((drag.angle() - 10f32.to_radians()).abs() < 1e-4);
        drag.rotate(-10.0);
        // Below zero the counter wraps up by 36: -1.0 + 36 = 35.
        assert!(
            (drag.spin_steps() - 35.0).abs() < 1e-3,
            "{}",
            drag.spin_steps()
        );
        assert!((drag.angle() - 350f32.to_radians()).abs() < 1e-3);
        drag.rotate(10.0);
        drag.rotate(10.0);
        assert!(drag.spin_steps() < 36.5);
    }

    #[test]
    fn ctrl_raises_in_five_centimetre_steps_between_zero_and_the_chassis_height() {
        let mut drag = Drag::new(ComponentInfo::default(), "part", 0);
        drag.raise(-5.0, 0.2);
        assert_eq!(drag.height(), 0.0, "never below the baseplate");
        drag.raise(15.0, 0.2);
        assert!(
            (drag.height() - 0.15).abs() < 1e-5,
            "3 steps: {}",
            drag.height()
        );
        drag.raise(500.0, 0.2);
        assert!(
            (drag.height() - 0.2).abs() < 1e-5,
            "capped at the chassis height"
        );
    }

    #[test]
    fn the_mount_mask_follows_the_exes_type_bits() {
        let att = |types: &[&str]| Attachment {
            types: types.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        };
        assert_eq!(mount_mask(&att(&["base_m"])), 1);
        assert_eq!(mount_mask(&att(&["base_f", "generic_f"])), 10);
        assert_eq!(mount_mask(&att(&["axle_m"])), 0x10);
        assert_eq!(mount_mask(&att(&["axle_f"])), 0x20);
    }

    #[test]
    fn boxes_overlap_only_when_they_penetrate() {
        let unit = ([-0.5; 3], [0.5; 3]);
        let a = Obb::new(Mat4::IDENTITY, unit);
        let touching = Obb::new(Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)), unit);
        let inside = Obb::new(Mat4::from_translation(Vec3::new(0.5, 0.0, 0.0)), unit);
        let turned = Obb::new(
            Mat4::from_translation(Vec3::new(1.1, 0.0, 0.0)) * Mat4::from_rotation_y(0.785),
            unit,
        );
        assert!(!a.overlaps(&touching, TOUCH));
        assert!(a.overlaps(&inside, TOUCH));
        assert!(
            a.overlaps(&turned, TOUCH),
            "a turned box's corner reaches in"
        );
    }

    #[test]
    fn polygon_membership_is_even_odd() {
        let square = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        assert!(inside(&square, [0.0, 0.0]));
        assert!(!inside(&square, [1.5, 0.0]));
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

    /// A 0.2 m cube with a `base_F` face and an `axle_M` plug on top.
    fn motor() -> ComponentInfo {
        ComponentInfo {
            class: "SpinMotor".into(),
            file: "memory://motor".into(),
            name: "Test Motor".into(),
            base: "SpinMotor".into(),
            mounts: [
                attachment(0, &["base_f"], [0.0, 0.0, 0.0]),
                attachment(1, &["axle_m"], [0.0, 0.0, -0.15]),
            ]
            .into_iter()
            .collect(),
            mass: 16.0,
            has_hull: true,
            bounds: Some(([-0.1; 3], [0.1; 3])),
            loaded: true,
            ..Default::default()
        }
    }

    fn wheel() -> ComponentInfo {
        ComponentInfo {
            class: "Wheel".into(),
            file: "memory://wheel".into(),
            name: "Test Wheel".into(),
            base: "Wheel".into(),
            mounts: [attachment(1, &["axle_f"], [0.0, 0.0, 0.0])]
                .into_iter()
                .collect(),
            mass: 8.0,
            has_hull: true,
            bounds: Some(([-0.05; 3], [0.05; 3])),
            loaded: true,
            ..Default::default()
        }
    }

    fn build() -> Build {
        use ra2fmt::bot::{BotMesh, Chassis};
        let mesh = BotMesh {
            points: vec![[-0.5, 0.0, -0.5], [0.5, 0.0, -0.5], [0.5, 0.0, 0.5]],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            uvs: vec![[0.0, 0.0]; 3],
            indices: vec![0, 1, 2],
        };
        Build::new(
            "T",
            Chassis {
                height: 0.3,
                mesh: Some(mesh),
                bottom: vec![[300, 300], [700, 300], [700, 700], [300, 700]],
                top: vec![[300, 300], [700, 300], [700, 700], [300, 700]],
                ..Default::default()
            },
        )
    }

    fn drag(info: ComponentInfo, own: i32) -> Drag {
        Drag::new(info, "part", own)
    }

    #[test]
    fn a_part_dropped_on_the_baseplate_is_added_where_the_hover_put_it() {
        let mut b = build();
        let mut d = drag(motor(), 0);
        d.rotate(10.0);
        d.raise(10.0, b.chassis_height());
        let index = b.drop_part(&d, &Hover::Baseplate([0.1, -0.05])).unwrap();
        let placed = &b.components()[index];
        assert_eq!(placed.parent, 0);
        assert_eq!(placed.translation, [0.1, 0.0, -0.05]);
        assert!((placed.angle - 20f32.to_radians()).abs() < 1e-4, "2 steps");
        assert!((placed.height - 0.10).abs() < 1e-5, "2 raise steps");
    }

    #[test]
    fn a_part_over_another_or_outside_the_outline_is_refused_with_the_exes_text() {
        let mut b = build();
        let d = drag(motor(), 0);
        b.drop_part(&d, &Hover::Baseplate([0.0, 0.0])).unwrap();
        let over = b.drop_part(&d, &Hover::Baseplate([0.05, 0.0]));
        assert_eq!(over, Err(DropError::Refused(Refusal::Intersects)));
        let out = b.drop_part(&d, &Hover::Baseplate([0.9, 0.0]));
        assert_eq!(out, Err(DropError::Refused(Refusal::OutsideChassis)));
        assert_eq!(
            Refusal::Intersects.message(),
            "Components should not intersect the chassis or other components."
        );
        assert_eq!(b.components().len(), 2, "refusals add nothing");
        // Beside the first, inside the outline: fine.
        b.drop_part(&d, &Hover::Baseplate([0.3, 0.0])).unwrap();
    }

    #[test]
    fn a_wheel_snaps_to_the_motors_free_axle_and_the_mount_is_then_taken() {
        let mut b = build();
        let m = b
            .drop_part(&drag(motor(), 0), &Hover::Baseplate([0.0, 0.0]))
            .unwrap();
        let w = drag(wheel(), 1);
        let hover = Hover::Mount {
            parent: m,
            mount: 1,
        };
        let candidate = b.plan(&w, &hover).expect("axle_F takes axle_M");
        assert_eq!(candidate.parent, m);
        let wheel_at = b.drop_part(&w, &hover).unwrap();
        assert!(b.plan(&w, &hover).is_none(), "one part per mount");
        // The motor's base face takes no axle: not a target.
        assert!(b
            .plan(
                &w,
                &Hover::Mount {
                    parent: m,
                    mount: 0
                }
            )
            .is_none());
        // The remove button refuses a part with children, then takes the leaf.
        assert_eq!(b.remove_leaf(m), Err(BuildError::HasChildren { index: m }));
        assert_eq!(
            BuildError::HasChildren { index: m }.to_string(),
            REMOVE_REFUSAL
        );
        b.remove_leaf(wheel_at).unwrap();
        b.remove_leaf(m).unwrap();
        assert_eq!(b.components().len(), 1);
    }

    #[test]
    fn the_mouse_ray_meets_a_free_socket_before_the_baseplate() {
        let mut b = build();
        let m = b
            .drop_part(&drag(motor(), 0), &Hover::Baseplate([0.0, 0.0]))
            .unwrap();
        let w = drag(wheel(), 1);
        // Straight down onto empty plate.
        let plate = b.pick(&w, Vec3::new(0.2, 2.0, 0.1), -Vec3::Y);
        assert_eq!(plate, Some(Hover::Baseplate([0.2, 0.1])));
        // Aimed at the axle plug in the motor's frame.
        let world = b.transforms()[m];
        let plug = world.transform_point3(Vec3::new(0.0, 0.0, -0.15));
        let origin = plug + Vec3::new(0.0, 1.0, 0.5);
        let hit = b.pick(&w, origin, plug - origin);
        assert_eq!(
            hit,
            Some(Hover::Mount {
                parent: m,
                mount: 1
            })
        );
        // A ray that misses the sheet is over nothing.
        assert_eq!(b.pick(&w, Vec3::new(5.0, 2.0, 0.0), -Vec3::Y), None);
        // A click on the motor selects it.
        let select = b.pick_part(plug + Vec3::new(0.0, 1.0, 0.0), -Vec3::Y);
        assert_eq!(select, Some(m));
    }

    #[test]
    fn the_attach_button_refuses_a_part_that_would_pass_the_mass_limit() {
        let mut b = build();
        b.check_attach_mass(&motor()).unwrap();
        b.set_limits(super::super::build::BuildLimits { max_mass: 20.0 });
        let heavy = ComponentInfo {
            mass: 100.0,
            ..motor()
        };
        assert!(matches!(
            b.check_attach_mass(&heavy),
            Err(BuildError::OverWeight { .. })
        ));
        assert_eq!(
            mass_message(800.0),
            "Total mass of robot must not exceed 800 kg."
        );
    }

    #[test]
    fn green_boxes_skip_the_plugs_and_the_tip_follows_the_point_count() {
        let m = motor();
        assert_eq!(selectable_mounts(&m), vec![0], "axle_M alone gets no box");
        assert_eq!(default_own_mount(&m), 0);
        assert_eq!(preview_tip(&m), TIP_BOXES);
        assert_eq!(preview_tip(&wheel()), TIP_ATTACH);
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

    /// The overlap and fit rules against the 57 stock bots: they were built in
    /// the original's editor, so its rules (whatever they are) accept them. Ours
    /// must accept every chassis child's fit, and call fewer than 1% of the
    /// non-adjacent pairs intersecting (mesh boxes are looser than the physics
    /// shapes, so a few stock pairs exceed the slack).
    #[test]
    #[allow(clippy::needless_range_loop)]
    fn the_stock_bots_mostly_pass_the_placement_rules() {
        let Some(root) = game_root() else { return };
        let lib = super::super::library::GameLibrary::open(&root).unwrap();
        let mut bots = Vec::new();
        for team in std::fs::read_dir(root.join("AI")).unwrap().flatten() {
            if let Ok(rd) = std::fs::read_dir(team.path()) {
                for f in rd.flatten() {
                    if f.path().extension().and_then(|e| e.to_str()) == Some("bot") {
                        bots.push(f.path());
                    }
                }
            }
        }
        assert!(bots.len() >= 30, "{} stock bots", bots.len());
        let (mut pairs, mut bad_pairs, mut kids, mut bad_kids, mut far) = (0, 0, 0, 0, 0);
        for path in bots {
            let Ok(build) = Build::load(&lib, &path) else {
                continue;
            };
            let worlds = build.transforms();
            let n = build.components().len();
            for i in 1..n {
                let Some(bi) = build.info()[i].bounds else {
                    continue;
                };
                let a = Obb::new(worlds[i], bi);
                let ci = &build.components()[i];
                let o = worlds[i].w_axis.truncate();
                if ci.parent == 0 {
                    kids += 1;
                    if !fits_chassis(
                        build.chassis().unwrap(),
                        &[[o.x, o.z], [a.centre.x, a.centre.z]],
                    ) {
                        bad_kids += 1;
                    }
                }
                if o.length() > MAX_REACH || o.y < MIN_Y {
                    far += 1;
                }
                for j in (i + 1)..n {
                    let cj = &build.components()[j];
                    if cj.parent == i as i32 || ci.parent == j as i32 {
                        continue;
                    }
                    let Some(bj) = build.info()[j].bounds else {
                        continue;
                    };
                    pairs += 1;
                    if a.overlaps(&Obb::new(worlds[j], bj), TOUCH) {
                        bad_pairs += 1;
                    }
                }
            }
        }
        assert!(kids > 300, "{kids} chassis children");
        assert_eq!(bad_kids, 0, "stock chassis children outside the outline");
        assert_eq!(far, 0, "stock parts beyond the size cap");
        assert!(
            bad_pairs * 100 < pairs,
            "{bad_pairs} of {pairs} stock pairs intersect"
        );
    }
}
