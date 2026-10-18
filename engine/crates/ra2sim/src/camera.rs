//! The match's one scene camera, as `Robot Arena 2.exe` drives it.
//!
//! The original has a single main camera object (the game object's `+0x2a610`;
//! class constructor defaults at `0x5a5c72`) and the match shell reconfigures
//! it rather than switching between camera objects:
//!
//! | key | handler | camera mode |
//! |-----|---------|-------------|
//! | F2 (`0x71`) | `0x439d60` | orbit — mode 2, FOV pi/4, follows robot 0 |
//! | F3 (`0x72`) | `0x439f20` | static — mode 4, then the arena's `NextStaticCamera` |
//! | F4 (`0x73`) | `0x43a020` | watch — mode 4 with robot 0 as the look-at target, then the arena's `NextWatchCamera` |
//!
//! (`0x439c10` is the key dispatcher; F1 is the help key and is not a camera.)
//!
//! A mode-4 camera is placed by `plus.setCameraPosition` / `setCameraRotation`
//! / `setCameraFOV` (the Python `StaticCamera.apply` and `WatchCamera.apply`) and
//! looks along the Euler angles — `D3DXMatrixRotationYawPitchRoll(yaw = y,
//! pitch = x, roll = z)` applied to `(0, 0, 1)` (`0x5a6900`..`0x5a6948`) — via
//! `D3DXMatrixLookAtLH`. With a target it looks at the target's position
//! instead (`0x5a6828`), and with a FOV range set the vertical FOV is
//! interpolated from the distance to the target (`0x5a684f`..`0x5a68d3`):
//! `t = clamp((d - min) / (max - min), 0, 1)`, `fov = near_fov + (far_fov -
//! near_fov) * t`. The projection is `D3DXMatrixPerspectiveFovLH(fov_y, aspect,
//! near, far)` (`0x5a7460`); the constructor defaults are FOV pi/4, aspect 4:3,
//! near 1.0 and far 2000.0, and the video-option code at `0x4e3a9a` then sets
//! near 1.0, far 125.0.

use glam::{Mat4, Vec3};
use ra2script::camera::{CameraAnimation, CameraCommand};

/// `0x4e3a90`: near plane after the option code has run.
pub const NEAR: f32 = 1.0;
/// `0x4e3a95`: far plane after the option code has run.
pub const FAR: f32 = 125.0;
/// `0x5a5c72`: the camera constructor's FOV (vertical, radians).
pub const DEFAULT_FOV: f32 = std::f32::consts::FRAC_PI_4;

/// Which configuration the match shell put the camera in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneCameraKind {
    /// F2, `0x439d60`.
    Orbit,
    /// F3, `0x439f20`; also the arena intro's camera.
    Static,
    /// F4, `0x43a020`; the `ra2.cfg` default.
    Watch,
}

/// The robot a watch or orbit camera follows: robot 0.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub position: Vec3,
    /// The robot's heading: the angle of its forward axis about +Y, with
    /// `forward = (sin yaw, 0, cos yaw)`.
    pub yaw: f32,
}

/// A running `animateCamera`, with its times made absolute.
#[derive(Clone, Copy, Debug)]
struct Track {
    animation: CameraAnimation,
    start: f32,
    end: f32,
    position_done: bool,
    rotation_done: bool,
    fov_done: bool,
}

/// The one main camera.
#[derive(Clone, Debug)]
pub struct SceneCamera {
    pub kind: SceneCameraKind,
    /// Eye, for the scripted (mode 4) cameras.
    pub position: Vec3,
    /// `(pitch, yaw, roll)`, radians.
    pub rotation: [f32; 3],
    /// Vertical field of view, radians.
    pub fov: f32,
    /// `(min_distance, max_distance, fov_near, fov_far)`; only used while the
    /// camera has a target.
    pub fov_range: Option<[f32; 4]>,
    tracks: Vec<Track>,
    clock: f32,
    /// Last known target, so a destroyed robot does not snap the view.
    last_target: Option<Target>,
}

impl Default for SceneCamera {
    fn default() -> Self {
        SceneCamera::new(SceneCameraKind::Static)
    }
}

impl SceneCamera {
    /// The camera a key press leaves behind, before the arena's `Next*Camera`
    /// overrides it.
    pub fn new(kind: SceneCameraKind) -> Self {
        let mut camera = SceneCamera {
            kind,
            position: Vec3::ZERO,
            rotation: [0.0; 3],
            fov: DEFAULT_FOV,
            fov_range: None,
            tracks: Vec::new(),
            clock: 0.0,
            last_target: None,
        };
        camera.select(kind);
        camera
    }

    /// Reconfigure as the key handler for `kind` does (`0x439d60`, `0x439f20`,
    /// `0x43a020`). The camera keeps whatever the constructor of that kind does
    /// not set, as the single camera object does.
    pub fn select(&mut self, kind: SceneCameraKind) {
        self.kind = kind;
        self.tracks.clear();
        match kind {
            SceneCameraKind::Static => {
                self.position = Vec3::new(-12.2, 5.478, 5.162);
                self.rotation = [0.2933, 1.575, 0.0];
                self.fov = 0.9;
            }
            SceneCameraKind::Watch => {
                self.position = Vec3::new(-4.2, 2.5, 9.5);
                self.fov_range = Some([2.0, 20.0, 0.897_597_9, 0.349_065_87]);
            }
            SceneCameraKind::Orbit => {
                self.fov = DEFAULT_FOV;
            }
        }
    }

    /// Apply one script request (`plus.setCamera*`, `plus.animateCamera`).
    pub fn apply(&mut self, command: CameraCommand) {
        match command {
            CameraCommand::Position(p) => self.position = Vec3::from(p),
            CameraCommand::Rotation([pitch, yaw]) => self.rotation = [pitch, yaw, 0.0],
            CameraCommand::Fov(fov) => self.fov = fov,
            CameraCommand::FovRange(range) => self.fov_range = Some(range),
            CameraCommand::Animate(animation) => self.tracks.push(Track {
                animation,
                start: self.clock + animation.start,
                end: self.clock + animation.end,
                position_done: false,
                rotation_done: false,
                fov_done: false,
            }),
        }
    }

    /// Advance the animation clock and the tracks by `dt` seconds.
    ///
    /// Each track is three independent linear events (`0x5a38d0` position,
    /// `0x5a3df0` rotation, `0x5a3ec0` FOV): nothing before `start`, a lerp
    /// while `start <= t <= end`, and one exact write of the end value the
    /// first time `t` passes `end`.
    pub fn advance(&mut self, dt: f32) {
        self.clock += dt;
        let now = self.clock;
        let mut tracks = std::mem::take(&mut self.tracks);
        for track in tracks.iter_mut() {
            let a = track.animation;
            if now < track.start {
                continue;
            }
            let span = track.end - track.start;
            let f = if span > 0.0 {
                ((now - track.start) / span).min(1.0)
            } else {
                1.0
            };
            let finished = now > track.end || span <= 0.0;
            if !track.position_done {
                let from = Vec3::from(a.from_pos);
                let to = Vec3::from(a.to_pos);
                self.position = if finished { to } else { from.lerp(to, f) };
                track.position_done = finished;
            }
            if !track.rotation_done {
                let lerp = |i: usize| a.from_rot[i] + (a.to_rot[i] - a.from_rot[i]) * f;
                self.rotation = if finished {
                    [a.to_rot[0], a.to_rot[1], 0.0]
                } else {
                    [lerp(0), lerp(1), 0.0]
                };
                track.rotation_done = finished;
            }
            if !track.fov_done {
                self.fov = if finished {
                    a.to_fov
                } else {
                    a.from_fov + (a.to_fov - a.from_fov) * f
                };
                track.fov_done = finished;
            }
        }
        tracks.retain(|t| !(t.position_done && t.rotation_done && t.fov_done));
        self.tracks = tracks;
    }

    /// Remember where the followed robot is (for the frame being drawn).
    pub fn follow(&mut self, target: Option<Target>) {
        if target.is_some() {
            self.last_target = target;
        }
    }

    /// Eye, aim point and up vector for this frame.
    pub fn pose(&self) -> (Vec3, Vec3, Vec3) {
        match self.kind {
            SceneCameraKind::Static => {
                let forward = euler_forward(self.rotation);
                (self.position, self.position + forward, Vec3::Y)
            }
            SceneCameraKind::Watch => match self.last_target {
                Some(target) => (self.position, target.position, Vec3::Y),
                None => {
                    let forward = euler_forward(self.rotation);
                    (self.position, self.position + forward, Vec3::Y)
                }
            },
            SceneCameraKind::Orbit => {
                let target = self.last_target.unwrap_or(Target {
                    position: Vec3::ZERO,
                    yaw: 0.0,
                });
                // `0x5a6cdf`: the eye is the aim point (target + (0, 1, 0),
                // `+0x21c/+0x220`) plus `(0, 0, -4)` (`+0x224`) turned by the
                // robot's heading and a 0.25 rad pitch (`+0x19c = -0.25`,
                // negated at `0x5a6ce8`): behind and above the robot.
                let aim = target.position + Vec3::new(0.0, 1.0, 0.0);
                let pitch = 0.25f32;
                let offset = Vec3::new(
                    -4.0 * pitch.cos() * target.yaw.sin(),
                    4.0 * pitch.sin(),
                    -4.0 * pitch.cos() * target.yaw.cos(),
                );
                (aim + offset, aim, Vec3::Y)
            }
        }
    }

    /// The vertical FOV for this frame, after the distance rule.
    pub fn effective_fov(&self) -> f32 {
        if self.kind == SceneCameraKind::Watch {
            if let (Some(target), Some([min, max, near_fov, far_fov])) =
                (self.last_target, self.fov_range)
            {
                let d = (target.position - self.position).length();
                let t = if max > min {
                    ((d - min) / (max - min)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return near_fov + (far_fov - near_fov) * t;
            }
        }
        self.fov
    }

    /// `proj * view` at `aspect` (width / height).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let (eye, aim, up) = self.pose();
        let view = ra2_render::look_at_lh(eye, aim, up);
        let proj = ra2_render::perspective_lh(self.effective_fov(), aspect, NEAR, FAR);
        proj * view
    }
}

/// The camera's forward axis for `(pitch, yaw, roll)`:
/// `D3DXMatrixRotationYawPitchRoll` applied to `(0, 0, 1)` in D3D's
/// left-handed, row-vector convention. The look-at up vector is always world
/// `+Y` (`0x5a6949`), so a view that is exactly overhead (`Overhead` in
/// `compressor.py` is pitch `1.5708`, a hair past pi/2) takes its screen
/// orientation from the sign of the tiny horizontal component, as the
/// original does.
pub fn euler_forward(rotation: [f32; 3]) -> Vec3 {
    let (sp, cp) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    Vec3::new(cp * sy, -sp, cp * cy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_looks_down_and_yaw_turns_right() {
        let f = euler_forward([0.0, 0.0, 0.0]);
        assert!((f - Vec3::Z).length() < 1e-6);
        let f = euler_forward([0.5, 0.0, 0.0]);
        assert!(f.y < 0.0 && f.z > 0.0);
        let f = euler_forward([0.0, 0.5, 0.0]);
        assert!(f.x > 0.0 && f.z > 0.0);
    }

    #[test]
    fn the_box_intro_start_looks_at_the_arena() {
        // `Arenas/box.py:116`: eye (23.7554, 14.3743, -47.141), (0.266, -0.473).
        let f = euler_forward([0.266_024, -0.472_956, 0.0]);
        let eye = Vec3::new(23.7554, 14.3743, -47.141);
        // Aimed at the arena's centre: the ray passes close to the origin.
        let t = -eye.dot(f);
        let nearest = (eye + f * t).length();
        assert!(nearest < 4.0, "{nearest}");
    }

    // The script's literal 1.5708 is the point: it is a hair past pi/2.
    #[allow(clippy::approx_constant)]
    #[test]
    fn an_overhead_camera_past_pi_over_two_keeps_the_originals_orientation() {
        // `compressor.py:19`: pitch 1.5708, yaw 3.1415, eye straight above the
        // origin. The screen's right is +X and its up is +Z, as in the capture.
        let mut camera = SceneCamera::new(SceneCameraKind::Static);
        camera.apply(CameraCommand::Position([0.0, 45.0, 0.0]));
        camera.apply(CameraCommand::Rotation([1.5708, 3.1415]));
        let (eye, at, up) = camera.pose();
        let view = ra2_render::look_at_lh(eye, at, up);
        let ahead = view.transform_point3(Vec3::new(5.0, 0.0, 5.0));
        assert!(ahead.x > 0.0, "{ahead}");
        assert!(ahead.y > 0.0, "{ahead}");
    }

    #[test]
    fn animate_is_linear_between_its_times_and_exact_at_the_end() {
        let mut camera = SceneCamera::new(SceneCameraKind::Static);
        camera.apply(CameraCommand::Animate(CameraAnimation {
            from_pos: [0.0, 0.0, 0.0],
            from_rot: [0.0, 0.0],
            from_fov: 1.0,
            to_pos: [10.0, 0.0, 0.0],
            to_rot: [1.0, 2.0],
            to_fov: 0.5,
            start: 1.0,
            end: 3.0,
        }));
        camera.advance(0.5);
        assert_eq!(camera.position, Vec3::new(-12.2, 5.478, 5.162));
        camera.advance(1.5); // t = 2.0, halfway
        assert!((camera.position.x - 5.0).abs() < 1e-5);
        assert!((camera.fov - 0.75).abs() < 1e-5);
        assert!((camera.rotation[1] - 1.0).abs() < 1e-5);
        camera.advance(1.5); // t = 3.5, past the end
        assert_eq!(camera.position, Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(camera.fov, 0.5);
    }

    #[test]
    fn the_watch_fov_follows_the_distance_to_its_target() {
        let mut camera = SceneCamera::new(SceneCameraKind::Watch);
        camera.apply(CameraCommand::Position([0.0, 0.0, 0.0]));
        camera.apply(CameraCommand::FovRange([2.0, 20.0, 0.9, 0.3]));
        camera.follow(Some(Target {
            position: Vec3::new(0.0, 0.0, 1.0),
            yaw: 0.0,
        }));
        assert!((camera.effective_fov() - 0.9).abs() < 1e-6);
        camera.follow(Some(Target {
            position: Vec3::new(0.0, 0.0, 11.0),
            yaw: 0.0,
        }));
        assert!((camera.effective_fov() - 0.6).abs() < 1e-5);
        camera.follow(Some(Target {
            position: Vec3::new(0.0, 0.0, 50.0),
            yaw: 0.0,
        }));
        assert!((camera.effective_fov() - 0.3).abs() < 1e-6);
    }
}
