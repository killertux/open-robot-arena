//! The scripted camera: what `plus.setCamera*` and `plus.animateCamera` ask of
//! the engine's one main camera.
//!
//! The original has a single scene camera (`+0x2a610` of the game object) and
//! every `plus` camera function edits it directly. This crate records the
//! requests in call order and the simulator, which owns the camera and its
//! clock, drains them with [`take_commands`] once per step.
//!
//! Recovered from `Robot Arena 2.exe` (see `docs/27-cameras.md`):
//!
//! * `plus.setCameraPosition(x, y, z)` (`0x4bb670` → `0x58fbd0`) sets the eye;
//! * `plus.setCameraRotation(pitch, yaw)` (`0x4bb690` → `0x5a6180`) sets the
//!   Euler angles in radians, roll 0;
//! * `plus.setCameraFOV(f)` (`0x4bb6b0` → `0x58fcc0`) sets the **vertical**
//!   field of view in radians (it feeds `D3DXMatrixPerspectiveFovLH`);
//! * `plus.setCameraFOVRange(min_d, max_d, fov_near, fov_far)` (`0x4bb6d0` →
//!   `0x58fce0`) enables the distance-driven FOV a camera that is looking at a
//!   robot uses;
//! * `plus.animateCamera(pos0, rot0, fov0, pos1, rot1, fov1, t0, t1)`
//!   (`0x4bb700`) queues three linear tracks (position `0x5a38d0`, rotation
//!   `0x5a3df0`, FOV `0x5a3ec0`) that run from `t0` to `t1` seconds after the
//!   call.

use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// One `plus.animateCamera` call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAnimation {
    pub from_pos: [f32; 3],
    pub from_rot: [f32; 2],
    pub from_fov: f32,
    pub to_pos: [f32; 3],
    pub to_rot: [f32; 2],
    pub to_fov: f32,
    /// Seconds after the call that the move starts.
    pub start: f32,
    /// Seconds after the call that the move ends.
    pub end: f32,
}

/// One request on the main camera, in call order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraCommand {
    Position([f32; 3]),
    /// `(pitch, yaw)`, radians.
    Rotation([f32; 2]),
    /// Vertical FOV, radians.
    Fov(f32),
    /// `(min_distance, max_distance, fov_at_min, fov_at_max)`.
    FovRange([f32; 4]),
    Animate(CameraAnimation),
}

/// Drain the camera requests the scripts have made since the last call.
pub fn take_commands() -> Vec<CameraCommand> {
    std::mem::take(&mut crate::state().camera_commands)
}

/// Whether the arena's `DoIntro(True)` has started and `plus.introComplete()`
/// has not yet been called.
pub fn intro_running() -> bool {
    crate::state().intro_running
}

/// Which of the arena's `Next*Camera` methods to call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextCamera {
    Static,
    Watch,
}

/// Call the live arena's `NextStaticCamera()` / `NextWatchCamera()`: the
/// Python base class advances its camera list and calls `plus.setCamera*`,
/// which lands in [`take_commands`]. Returns the camera's name (`"Static 1"`),
/// or `None` when there is no arena or it has no camera of that kind.
pub fn next_camera(kind: NextCamera) -> Option<String> {
    use pyo3::types::PyAnyMethods;
    let method = match kind {
        NextCamera::Static => "NextStaticCamera",
        NextCamera::Watch => "NextWatchCamera",
    };
    pyo3::Python::with_gil(|py| {
        let arena = crate::state()
            .arena_object
            .as_ref()
            .map(|object| object.clone_ref(py))?;
        match arena.bind(py).call_method0(method) {
            Ok(name) => name.extract::<String>().ok().filter(|n| !n.is_empty()),
            Err(error) => {
                crate::state().log.push(format!("arena {method}: {error}"));
                None
            }
        }
    })
}

fn floats(item: &Bound<'_, PyAny>, n: usize) -> PyResult<Vec<f32>> {
    let v: Vec<f32> = item.extract()?;
    if v.len() < n {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "expected {n} numbers, got {}",
            v.len()
        )));
    }
    Ok(v)
}

/// Parse `animateCamera`'s eight arguments.
pub fn parse_animation(args: &Bound<'_, PyTuple>) -> PyResult<CameraAnimation> {
    if args.len() < 8 {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "animateCamera takes 8 arguments",
        ));
    }
    let pos = |i: usize| -> PyResult<[f32; 3]> {
        let v = floats(&args.get_item(i)?, 3)?;
        Ok([v[0], v[1], v[2]])
    };
    let rot = |i: usize| -> PyResult<[f32; 2]> {
        let v = floats(&args.get_item(i)?, 2)?;
        Ok([v[0], v[1]])
    };
    let num = |i: usize| -> PyResult<f32> { args.get_item(i)?.extract::<f32>() };
    Ok(CameraAnimation {
        from_pos: pos(0)?,
        from_rot: rot(1)?,
        from_fov: num(2)?,
        to_pos: pos(3)?,
        to_rot: rot(4)?,
        to_fov: num(5)?,
        start: num(6)?,
        end: num(7)?,
    })
}
