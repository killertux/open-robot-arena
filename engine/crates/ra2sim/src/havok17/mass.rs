//! Mass properties of a body's collision shape: volume, centre of mass and
//! inertia tensor, at unit density.
//!
//! Havok 1.x computes a rigid body's inertia from its geometry ("Invalid vertex
//! index in Inertia Tensor", `0x537000..0x53c000`); the game then sets the mass
//! (`FUN_00525090`) and the tensor scales with it. We do the same with the
//! convex hulls the robot and prop bodies are built from: every child of a
//! compound has one density, so the body's inertia is the hulls' volume
//! distribution scaled to the body's mass.

use glam::{Mat3, Quat, Vec3};

use crate::backend::ShapeDesc;
use crate::massprops;

/// Mass properties at unit density. `inertia` is about `com`, in shape space.
#[derive(Clone, Copy, Debug)]
pub struct MassProps {
    pub volume: f32,
    pub com: Vec3,
    pub inertia: Mat3,
}

impl MassProps {
    const EMPTY: MassProps = MassProps {
        volume: 0.0,
        com: Vec3::ZERO,
        inertia: Mat3::ZERO,
    };

    /// The same distribution rotated by `rotation` and moved by `offset`.
    fn transformed(self, rotation: Quat, offset: Vec3) -> MassProps {
        let r = Mat3::from_quat(rotation);
        MassProps {
            volume: self.volume,
            com: offset + rotation * self.com,
            inertia: r * self.inertia * r.transpose(),
        }
    }
}

/// `m (|d|^2 I - d d^T)`: the parallel-axis term for moving an inertia tensor
/// a distance `d` away from the centre of mass.
pub fn parallel_axis(mass: f32, d: Vec3) -> Mat3 {
    let outer = Mat3::from_cols(d * d.x, d * d.y, d * d.z);
    (Mat3::IDENTITY * d.length_squared() - outer) * mass
}

/// Unit-density mass properties of `shape`.
pub fn of_shape(shape: &ShapeDesc) -> MassProps {
    match shape {
        ShapeDesc::Box { half_extents } => {
            let s = *half_extents * 2.0;
            let v = s.x * s.y * s.z;
            MassProps {
                volume: v,
                com: Vec3::ZERO,
                inertia: Mat3::from_diagonal(
                    Vec3::new(
                        s.y * s.y + s.z * s.z,
                        s.x * s.x + s.z * s.z,
                        s.x * s.x + s.y * s.y,
                    ) * (v / 12.0),
                ),
            }
        }
        ShapeDesc::Sphere { radius } => {
            let v = 4.0 / 3.0 * std::f32::consts::PI * radius.powi(3);
            MassProps {
                volume: v,
                com: Vec3::ZERO,
                inertia: Mat3::from_diagonal(Vec3::splat(0.4 * v * radius * radius)),
            }
        }
        // Jolt's cylinder and capsule stand along Y.
        ShapeDesc::Cylinder {
            half_height,
            radius,
        }
        | ShapeDesc::Capsule {
            half_height,
            radius,
        } => {
            let h = 2.0 * half_height;
            let v = std::f32::consts::PI * radius * radius * h;
            let side = v * (3.0 * radius * radius + h * h) / 12.0;
            MassProps {
                volume: v,
                com: Vec3::ZERO,
                inertia: Mat3::from_diagonal(Vec3::new(side, 0.5 * v * radius * radius, side)),
            }
        }
        ShapeDesc::ConvexHull { points } | ShapeDesc::MeshHull { points, .. } => {
            match massprops::hull_inertia(points) {
                Some((volume, com, inertia)) => MassProps {
                    volume,
                    com,
                    inertia,
                },
                None => bounds_box(points),
            }
        }
        ShapeDesc::TriMesh { vertices, .. } => bounds_box(vertices),
        ShapeDesc::Compound(children) => {
            let parts: Vec<MassProps> = children
                .iter()
                .map(|c| of_shape(&c.shape).transformed(c.local_rotation, c.local_position))
                .filter(|p| p.volume > 0.0)
                .collect();
            let volume: f32 = parts.iter().map(|p| p.volume).sum();
            if volume <= 0.0 {
                return MassProps::EMPTY;
            }
            let com = parts.iter().map(|p| p.com * p.volume).sum::<Vec3>() / volume;
            let inertia = parts.iter().fold(Mat3::ZERO, |acc, p| {
                acc + p.inertia + parallel_axis(p.volume, p.com - com)
            });
            MassProps {
                volume,
                com,
                inertia,
            }
        }
        ShapeDesc::OffsetCenterOfMass { shape, offset } => {
            // Only the centre moves (the surface stays): the tensor is the
            // inner one carried to the new centre by the parallel-axis rule.
            let inner = of_shape(shape);
            MassProps {
                volume: inner.volume,
                com: inner.com + *offset,
                inertia: inner.inertia + parallel_axis(inner.volume, *offset),
            }
        }
    }
}

/// A solid box over the points' bounds, for sets without a hull.
fn bounds_box(points: &[Vec3]) -> MassProps {
    if points.is_empty() {
        return MassProps::EMPTY;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let half = ((hi - lo) * 0.5).max(Vec3::splat(0.01));
    let mut m = of_shape(&ShapeDesc::Box { half_extents: half });
    m.com = (lo + hi) * 0.5;
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::CompoundChild;

    #[test]
    fn two_boxes_side_by_side_are_one_long_box() {
        let half = Vec3::new(0.5, 0.5, 0.5);
        let child = |x: f32| CompoundChild {
            local_position: Vec3::new(x, 0.0, 0.0),
            local_rotation: Quat::IDENTITY,
            shape: ShapeDesc::Box { half_extents: half },
            tag: 0,
        };
        let two = of_shape(&ShapeDesc::Compound(vec![child(-0.5), child(0.5)]));
        let one = of_shape(&ShapeDesc::Box {
            half_extents: Vec3::new(1.0, 0.5, 0.5),
        });
        assert!((two.volume - one.volume).abs() < 1e-4);
        assert!(two.com.length() < 1e-5);
        let d = two.inertia - one.inertia;
        assert!(d.abs_diff_eq(Mat3::ZERO, 1e-4), "{two:?} vs {one:?}");
    }
}
