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
    /// The orbit camera's smoothed state (`0x5a6970`).
    orbit: Orbit,
}

/// What the original's orbit camera reads for its distance when the match has
/// no human player to follow (docs/30 §5; the constructor stores 4).
pub const ORBIT_DISTANCE_NO_TARGET: f32 = 8.6;

/// The orbit camera's state (mode 2, `0x5a6970`; input `0x5a6260`).
///
/// `0x439d60` sets the aim offset `(0, 1)` (`+0x21c`/`+0x220`), the distance 4
/// (`+0x224`) and the pitch goal -0.25 (`+0x1e0`). Every frame the heading the
/// robot faces (`+0x208`) and the user's pitch/yaw goals (`+0x1e0`) are chased
/// by the spring `0x61bb70` (`+0x1f8` and `+0x19c`), the eye is the aim point
/// plus `R(yaw = heading + user yaw, pitch = -user pitch) * (0, 0, -distance)`
/// and the camera looks along `R`. No stock arena calls `AddCameraBlocker`, so
/// the blocker rays (`0x5a7680`) never run.
#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    /// `+0x1fc`: the smoothed heading, `None` until the first frame.
    pub heading: Option<f32>,
    /// `+0x210` y: last frame's goal heading (the spring's feed-forward term).
    prev_goal: f32,
    /// `+0x19c`: the smoothed `(pitch, yaw)` the user dialled in.
    pub user: [f32; 2],
    /// `+0x1ec`: last frame's user goal.
    prev_user: [f32; 2],
    /// `+0x1e0`: the user's `(pitch, yaw)` goal. The pitch is stored negated:
    /// -0.25 draws the camera looking 0.25 rad down.
    pub user_goal: [f32; 2],
    /// `+0x224`, clamped to `[1, 1000]` (`+0x230`, `+0x23c`).
    pub distance: f32,
    /// `+0x21c`/`+0x220`: the aim point's offset from the robot.
    pub aim_offset: [f32; 2],
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit {
            heading: None,
            prev_goal: 0.0,
            user: [-0.25, 0.0],
            prev_user: [-0.25, 0.0],
            user_goal: [-0.25, 0.0],
            distance: 4.0,
            aim_offset: [0.0, 1.0],
        }
    }
}

/// `0x61bb70`, one component: `cur += d̂ * (k2 * d̂·((prev - goal) * dt) +
/// (rest - |d|) * k1) * dt` with `d = cur - goal`. The constants are the
/// camera constructor's `+0x260` = 8, `+0x264` = 20, `+0x268` = 1e-4.
fn spring(cur: f32, goal: f32, prev_goal: f32, dt: f32) -> f32 {
    const K1: f32 = 8.0;
    const K2: f32 = 20.0;
    const REST: f32 = 0.000_1;
    let d = cur - goal;
    let len = d.abs();
    if len == 0.0 {
        return cur;
    }
    let dir = d.signum();
    let s = K2 * (dir * (prev_goal - goal) * dt) + (REST - len) * K1;
    cur + dir * s * dt
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
            orbit: Orbit::default(),
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
                // `0x439d60` starts the orbit on the robot's heading.
                self.orbit = Orbit::default();
            }
        }
    }

    /// Mouse input in orbit mode (`0x5a6447`): `wheel` notches (positive zooms
    /// in) change the distance, and a drag of `(dx, dy)` pixels turns the user's
    /// yaw/pitch goals by `0.01` rad per pixel times the sensitivities
    /// (`+0x284`/`+0x288`, taken as 1.0: nothing in the constructor sets them).
    /// The pitch goal stays within +-pi/2.
    pub fn orbit_input(&mut self, dx: f32, dy: f32, wheel: f32) {
        let o = &mut self.orbit;
        o.distance = (o.distance - wheel).clamp(1.0, 1000.0);
        o.user_goal[0] = (o.user_goal[0] + dy * 0.01)
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        o.user_goal[1] += dx * 0.01;
    }

    /// The orbit's current state, for tests and the HUD.
    pub fn orbit_state(&self) -> Orbit {
        self.orbit
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
        if self.kind == SceneCameraKind::Orbit {
            self.advance_orbit(dt);
        }
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

    /// One frame of the orbit's smoothing (`0x5a6b74`..`0x5a6c7b`).
    fn advance_orbit(&mut self, dt: f32) {
        let Some(target) = self.last_target else {
            return;
        };
        let two_pi = std::f32::consts::TAU;
        let o = &mut self.orbit;
        match o.heading {
            None => {
                o.heading = Some(target.yaw);
                o.prev_goal = target.yaw;
            }
            Some(cur) => {
                // The goal is the heading nearest the smoothed one, as the
                // `+-pi` unwrap at `0x5a6a5c` arranges.
                let mut goal = target.yaw;
                while goal - cur > std::f32::consts::PI {
                    goal -= two_pi;
                }
                while cur - goal > std::f32::consts::PI {
                    goal += two_pi;
                }
                o.heading = Some(spring(cur, goal, o.prev_goal, dt));
                o.prev_goal = goal;
            }
        }
        for i in 0..2 {
            o.user[i] = spring(o.user[i], o.user_goal[i], o.prev_user[i], dt);
            o.prev_user[i] = o.user_goal[i];
        }
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
                let o = &self.orbit;
                // `0x5a69a9`: the aim point is the robot plus `(0, 1, 0)`. With no
                // target (an AI-only match has no human player) `0x5a69a3` jumps
                // to `0x5a6b74` and the aim is the stored point `+0x1b0` = the
                // origin, with no offset and heading 0 (measured live: the F2
                // eye sits at `(0, 2.128, -8.333)` = `8.6 * (0, sin .25, -cos .25)`).
                let (aim, target_yaw) = match self.last_target {
                    Some(t) => (
                        t.position + Vec3::new(o.aim_offset[0], o.aim_offset[1], 0.0),
                        t.yaw,
                    ),
                    None => (Vec3::ZERO, 0.0),
                };
                let target = Target {
                    position: aim,
                    yaw: target_yaw,
                };
                // Measured on the live original with no human player (both
                // arenas tried): `+0x224` reads 8.6, not the constructor's 4.
                // Where the extra 4.6 comes from was not found, so it is
                // applied only to this case and only until the user zooms.
                let distance =
                    if self.last_target.is_none() && o.distance == Orbit::default().distance {
                        ORBIT_DISTANCE_NO_TARGET
                    } else {
                        o.distance
                    };
                // `0x5a6cf0`: `D3DXMatrixRotationYawPitchRoll(yaw = heading +
                // user yaw, pitch = -user pitch)` turns `(0, 0, -distance)`.
                let yaw = o.heading.unwrap_or(target.yaw) + o.user[1];
                let forward = euler_forward([-o.user[0], yaw, 0.0]);
                (
                    target.position - forward * distance,
                    target.position,
                    Vec3::Y,
                )
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

    #[test]
    fn a_watch_camera_without_a_target_keeps_the_pose_it_inherited() {
        // Measured on the original: the countdown's `Watch Cam 1` sets only the
        // eye; the rotation and FOV stay what the intro flight left (0.675 rad).
        let mut camera = SceneCamera::new(SceneCameraKind::Static);
        camera.apply(CameraCommand::Rotation([0.4376, -3.1628]));
        camera.apply(CameraCommand::Fov(0.675));
        camera.select(SceneCameraKind::Watch);
        camera.apply(CameraCommand::Position([-1.26, 3.0, 4.56]));
        camera.apply(CameraCommand::FovRange([2.0, 8.0, 1.2, 0.9]));
        assert_eq!(camera.effective_fov(), 0.675, "no target: no range rule");
        let (eye, aim, _) = camera.pose();
        assert_eq!(eye, Vec3::new(-1.26, 3.0, 4.56));
        let forward = (aim - eye).normalize();
        let expected = euler_forward([0.4376, -3.1628, 0.0]);
        assert!((forward - expected).length() < 1e-5);
    }

    #[test]
    fn the_orbit_without_a_target_looks_at_the_origin_from_8_6() {
        // Live: eye (0, 2.128, -8.333) with the pitch goal 0.25, yaw 0.
        let camera = SceneCamera::new(SceneCameraKind::Orbit);
        let (eye, aim, _) = camera.pose();
        assert_eq!(aim, Vec3::ZERO);
        assert!(
            (eye - Vec3::new(0.0, 2.1277, -8.3326)).length() < 1e-3,
            "{eye}"
        );
    }

    #[test]
    fn the_orbit_starts_behind_and_above_then_chases_the_heading() {
        let mut camera = SceneCamera::new(SceneCameraKind::Orbit);
        let at = Vec3::new(1.0, 0.5, 2.0);
        camera.follow(Some(Target {
            position: at,
            yaw: 0.0,
        }));
        camera.advance(1.0 / 60.0);
        let (eye, aim, _) = camera.pose();
        assert!((aim - (at + Vec3::Y)).length() < 1e-6);
        // 4 units behind along -Z, 4 sin(0.25) above the aim point.
        assert!((eye.z - (aim.z - 4.0 * 0.25f32.cos())).abs() < 1e-5);
        assert!((eye.y - (aim.y + 4.0 * 0.25f32.sin())).abs() < 1e-5);
        // The robot spins a quarter turn: the view lags and closes at ~8/s.
        camera.follow(Some(Target {
            position: at,
            yaw: std::f32::consts::FRAC_PI_2,
        }));
        for _ in 0..60 {
            camera.advance(1.0 / 60.0);
        }
        let h = camera.orbit_state().heading.unwrap();
        let goal = std::f32::consts::FRAC_PI_2;
        let expected = goal * (1.0 - (-8.0f32).exp());
        assert!(h > 0.5 * goal && h < goal, "{h}");
        assert!((h - expected).abs() < 0.1, "{h} vs {expected}");
    }

    #[test]
    fn the_orbit_wheel_zooms_between_one_and_a_thousand() {
        let mut camera = SceneCamera::new(SceneCameraKind::Orbit);
        camera.orbit_input(0.0, 0.0, 10.0);
        assert_eq!(camera.orbit_state().distance, 1.0);
        camera.orbit_input(0.0, 0.0, -5000.0);
        assert_eq!(camera.orbit_state().distance, 1000.0);
    }
}
