//! The RA2 script layer: the `plus`/`Arenas`/`AI` object model on an embedded
//! CPython.
//!
//! *Robot Arena 2*'s game logic is Python 2.2 — the AI strategies, the arena
//! hazards, the match flow — talking to a C++ engine through a module called
//! `plus`. This crate is that engine side, and it runs the **shipped scripts
//! unmodified** (see [`host::ScriptHost`] and `legacy.py`), which is what makes
//! the port faithful: the behaviour under test is not a re-description of the
//! scripts, it *is* the scripts.
//!
//! Layout:
//!
//! * [`host`] — boots the interpreter, points it at the game's own directories
//!   and installs the translating loader for the Python-2 sources.
//! * [`plus`] — the `plus` module: `Arena`, the controllers, and the free
//!   functions the scripts call.
//! * [`arena`] — `plus.Arena`'s data model, including the navigation map the AI
//!   paths over.
//! * [`State`] — the live match the free functions refer to, plus the
//!   [`MatchApi`] seam the physics side implements.
//!
//! Every bound name here — `GetStartPointLocation`, `SetSubMaterialSound`,
//! `ZoneEvent` — is the name the shipped scripts call, so the crate keeps the
//! original's capitalisation.
#![allow(non_snake_case)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use glam::{Quat, Vec3};

pub mod arena;
pub mod camera;
pub mod giscript;
pub mod gooey;
pub mod host;
pub mod plus;
pub mod ui;

pub use arena::{Arena, ArenaData, ArenaDetail, Drive, HingeController, PrismaticController};
pub use host::ScriptHost;
pub use plus::{Ai, InputEvent};

/// The audio backend, re-exported so a caller draining [`SoundQueue`] does not
/// need a direct dependency on `ra2-audio`.
pub use ra2_audio;

/// One robot, as the scripts see it.
///
/// The AI asks the engine these questions every tick — where am I, which way am
/// I facing, how fast, who is the enemy, what did I last hit — so the simulator
/// publishes a snapshot per robot and the script layer answers from it. Keeping
/// it data (rather than a trait the simulator implements) means the two sides
/// never hold each other's locks: the simulator writes snapshots, the scripts
/// read them, and anything the scripts want to *do* goes on [`Request`].
#[derive(Clone, Debug, Default)]
pub struct BotSnapshot {
    pub alive: bool,
    /// Chassis position.
    pub location: Vec3,
    /// Unit vector the robot faces.
    pub direction: Vec3,
    /// Forward speed in m/s (`GetSpeed`).
    pub speed: f32,
    /// Yaw rate in rad/s (`GetTurning`).
    pub turning: f32,
    pub upside_down: bool,
    pub mass: f32,
    /// `(class, health)` per component, in the rig's order.
    pub components: Vec<(String, f32)>,
    /// World-space sphere of each component, for `plus.rayTest`.
    pub spheres: Vec<(Vec3, f32)>,
    /// World-space oriented box of each component — `(centre, half-extents,
    /// rotation)` — which is what `plus.rayTest` answers against: a part is
    /// rarely a ball, and the flame jet is supposed to touch what it hits.
    pub boxes: Vec<(Vec3, Vec3, Quat)>,
    /// The last blow this robot landed (`GetLastDamageDone`, robot `+0xc4`).
    pub last_damage_done: DamageRecord,
    /// …and the last one it took (`GetLastDamageReceived`, robot `+0xd8`).
    pub last_damage_received: DamageRecord,
    /// `plus.isEliminated` — the original's robot state 3, which is a *different*
    /// state from a defeated robot: a robot taken out by falling off a tabletop
    /// is eliminated, while a robot whose chassis is gone is defeated.
    pub eliminated: bool,
}

/// One remembered hit, as `plus.AI.GetLastDamageDone/Received` hand it to the
/// scripts: a 5-tuple `(component, amount, time, player, other component)`.
///
/// The record is five fields at robot `+0xc4` (done) / `+0xd8` (received):
/// `+0` component, `+4` amount, `+8` the other player, `+0xc` the other
/// player's component, `+0x10` the time; `FUN_004b9050`/`FUN_004b9130` build
/// the tuple in the order `+0, +4, +0x10, +8, +0xc`. `AI/Spinner.py` reads
/// `damage[3]` as "by player" ("component damaged, amount, at time, by
/// player, by component"); for a blow *done*, `component` is the robot's own
/// part that landed it — `CheckForWeaponHits` matches it against the weapon
/// list — and the player/component are the victim's.
pub type DamageRecord = (i32, f32, f32, i32, i32);

/// No hit yet: no component, no player.
pub const NO_DAMAGE: DamageRecord = (0, 0.0, 0.0, -1, -1);

/// Something a script asked the engine to do.
///
/// The `plus` functions run inside Python, so they queue; the simulator drains
/// the queue at the top of the next step.
#[derive(Clone, Copy, Debug)]
pub enum Request {
    /// `plus.damage(bot, component, amount, point)`.
    Damage {
        bot: i32,
        component: i32,
        amount: f32,
        point: Vec3,
    },
    /// `plus.force(bot, x, y, z)` — a force in N at the robot's body, for the
    /// step it lands in. The engine turns it into `force * dt` of impulse.
    Force { bot: i32, momentum: Vec3 },
    /// `plus.zap(bot, amount, radius)`.
    Zap { bot: i32, amount: f32, radius: f32 },
    /// `plus.eliminatePlayer(bot)`.
    Eliminate { bot: i32 },
}

/// A stable handle to a sound a script created with `plus.createSound`.
///
/// The number is the value the scripts keep in their own attributes (they do
/// not inspect it), starts at 1 and is never reused, so a stale handle can be
/// told apart from a live one. `removeSound` frees the live entry; the number
/// itself remains a valid *name* for "the handle that used to be this sound".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoundId(pub u64);

impl std::fmt::Display for SoundId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A sound a script created, before it is played.
///
/// The **path is stored exactly as the script wrote it** — game-relative,
/// usually with backslashes (`Sounds\intro_music\smell_glue.wav`) — and is
/// resolved against the game installation only when the queue is drained (see
/// [`SoundQueue::apply`]). Creating a sound therefore never touches the disk,
/// which is what lets an arena script set up its sounds before a device (or a
/// game directory) exists.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub id: SoundId,
    /// The script's own path spelling.
    pub path: String,
    /// `createSound`'s second argument, changed by `plus.loopSound`.
    pub looping: bool,
    /// `createSound`'s third argument, the emitter position.
    pub position: [f32; 3],
}

/// What a script asked the audio system to do, in call order.
///
/// `Play` carries the sound's path and position so a drained request is
/// self-contained; `Stop` and `Loop` name the handle, and [`SoundQueue::apply`]
/// looks the path up in the queue's table (including sounds removed after the
/// request was queued).
#[derive(Clone, Debug, PartialEq)]
pub enum SoundRequest {
    /// `plus.playSound(handle)` (or `playOnce`, with `looping: false`).
    Play {
        id: SoundId,
        /// The script's raw path; resolve at drain time.
        path: String,
        looping: bool,
        position: [f32; 3],
    },
    /// `plus.stopSound(handle)` and the stop half of `plus.removeSound`.
    Stop { id: SoundId },
    /// `plus.loopSound(handle)`: play it as a loop, or switch a sounding one
    /// to looping.
    Loop { id: SoundId },
    /// `plus.setVolume(handle, volume, ..)`: the handle's level in DirectSound
    /// hundredths of a decibel (`0` unity, `-10000` silence), immediate.
    SetVolume { id: SoundId, hundredths_db: i32 },
    /// `plus.fadeInToLoop(handle, volume, ms)`: start it looping from silence
    /// and ramp to `volume` over `ms` milliseconds.
    FadeInToLoop {
        id: SoundId,
        hundredths_db: i32,
        ms: f32,
    },
    /// `plus.fadeOutLoop(handle, ms)`: ramp to silence over `ms`, then stop.
    FadeOutLoop { id: SoundId, ms: f32 },
}

/// The script host's sound handles and the requests they produced.
///
/// The scripts address sounds by handle only; this is the table that makes a
/// handle mean something, and the ordered queue a caller drains into
/// [`ra2_audio::Audio`]. It is data, not a device: constructing sounds and
/// queueing plays works headless, and [`SoundQueue::apply`] is the only part
/// that needs an audio system.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundQueue {
    next_id: u64,
    sounds: BTreeMap<SoundId, Sound>,
    /// Sounds `removeSound` took out of `sounds` since the last drain. They are
    /// kept here so a `Loop` queued before the removal can still find its path
    /// when the queue is applied; [`SoundQueue::apply`] clears the map once the
    /// pending requests are served.
    retired: BTreeMap<SoundId, Sound>,
    requests: Vec<SoundRequest>,
}

impl Default for SoundQueue {
    fn default() -> SoundQueue {
        SoundQueue::new()
    }
}

impl SoundQueue {
    /// An empty queue; handles start at 1.
    pub const fn new() -> SoundQueue {
        SoundQueue {
            next_id: 1,
            sounds: BTreeMap::new(),
            retired: BTreeMap::new(),
            requests: Vec::new(),
        }
    }

    /// `plus.createSound(path, looping, position)` — make a handle.
    ///
    /// `loop=false` still creates: the arena scripts create the intro music
    /// once and call `loopSound` on it later, while `Spikes` creates its fire
    /// sound with `loop=true` and plays it with `playSound`.
    pub fn create(
        &mut self,
        path: impl Into<String>,
        looping: bool,
        position: [f32; 3],
    ) -> SoundId {
        let id = SoundId(self.next_id);
        self.next_id += 1;
        self.sounds.insert(
            id,
            Sound {
                id,
                path: path.into(),
                looping,
                position,
            },
        );
        id
    }

    /// The live sound behind a handle, if it has not been removed.
    pub fn get(&self, id: SoundId) -> Option<&Sound> {
        self.sounds.get(&id)
    }

    /// The live handles and their sounds, in handle order.
    pub fn iter(&self) -> impl Iterator<Item = &Sound> {
        self.sounds.values()
    }

    /// `plus.playSound(handle)` — queue a play. Unknown handles are an error
    /// that names the handle, never a silent no-op.
    pub fn play(&mut self, id: SoundId) -> Result<(), String> {
        let Some(sound) = self.sounds.get(&id) else {
            return Err(unknown("playSound", id));
        };
        let request = SoundRequest::Play {
            id,
            path: sound.path.clone(),
            looping: sound.looping,
            position: sound.position,
        };
        self.requests.push(request);
        Ok(())
    }

    /// `plus.playOnce(handle)` — the same as [`SoundQueue::play`], forced
    /// non-looping. The shipped caller is the end-of-match announcer, which is
    /// a one-shot even if the handle were created looping.
    pub fn play_once(&mut self, id: SoundId) -> Result<(), String> {
        let Some(sound) = self.sounds.get(&id) else {
            return Err(unknown("playOnce", id));
        };
        let request = SoundRequest::Play {
            id,
            path: sound.path.clone(),
            looping: false,
            position: sound.position,
        };
        self.requests.push(request);
        Ok(())
    }

    /// `plus.loopSound(handle)` — mark it looping and queue that.
    pub fn loop_sound(&mut self, id: SoundId) -> Result<(), String> {
        let Some(sound) = self.sounds.get_mut(&id) else {
            return Err(unknown("loopSound", id));
        };
        sound.looping = true;
        self.requests.push(SoundRequest::Loop { id });
        Ok(())
    }

    /// `plus.stopSound(handle)` — queue a stop without freeing the handle, so
    /// the script can play it again. Returns whether the handle was live.
    pub fn stop(&mut self, id: SoundId) -> bool {
        if !self.sounds.contains_key(&id) {
            return false;
        }
        self.requests.push(SoundRequest::Stop { id });
        true
    }

    /// `plus.removeSound(handle)` — free the handle and stop it if it is
    /// sounding. Returns whether the handle was live; the caller turns `false`
    /// into the named warning an arena's `__del__` needs.
    pub fn remove(&mut self, id: SoundId) -> bool {
        let Some(sound) = self.sounds.remove(&id) else {
            return false;
        };
        self.retired.insert(id, sound);
        self.requests.push(SoundRequest::Stop { id });
        true
    }

    /// `plus.setVolume(handle, volume, ..)`.
    pub fn set_volume(&mut self, id: SoundId, hundredths_db: i32) -> Result<(), String> {
        if !self.sounds.contains_key(&id) {
            return Err(unknown("setVolume", id));
        }
        self.requests
            .push(SoundRequest::SetVolume { id, hundredths_db });
        Ok(())
    }

    /// `plus.fadeInToLoop(handle, volume, ms)`: the sound becomes a loop.
    pub fn fade_in_to_loop(
        &mut self,
        id: SoundId,
        hundredths_db: i32,
        ms: f32,
    ) -> Result<(), String> {
        let Some(sound) = self.sounds.get_mut(&id) else {
            return Err(unknown("fadeInToLoop", id));
        };
        sound.looping = true;
        self.requests.push(SoundRequest::FadeInToLoop {
            id,
            hundredths_db,
            ms,
        });
        Ok(())
    }

    /// `plus.fadeOutLoop(handle, ms)`.
    pub fn fade_out_loop(&mut self, id: SoundId, ms: f32) -> Result<(), String> {
        if !self.sounds.contains_key(&id) {
            return Err(unknown("fadeOutLoop", id));
        }
        self.requests.push(SoundRequest::FadeOutLoop { id, ms });
        Ok(())
    }

    /// The queued requests, in call order.
    pub fn requests(&self) -> &[SoundRequest] {
        &self.requests
    }

    /// Take the queued requests, leaving the handle table intact.
    pub fn take_requests(&mut self) -> Vec<SoundRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Drain the queue into an audio system.
    ///
    /// Paths are resolved here, against `game_root`, with
    /// [`ra2_audio::resolve_game_path`] — the first moment the script layer
    /// touches the disk. `Play` starts (or restarts) the sound at its handle,
    /// `Loop` restarts it with the looping decoder, and `Stop` stops it.
    ///
    /// Returns one message per request that could not be applied (a path that
    /// is not in the game, or the backend's own decode error). The requests are
    /// consumed either way; a missing file is not a reason to fail a match.
    pub fn apply(
        &mut self,
        game_root: &std::path::Path,
        listener: ra2_audio::Listener,
        audio: &mut ra2_audio::Audio,
    ) -> Vec<String> {
        let requests = std::mem::take(&mut self.requests);
        self.apply_requests(requests, game_root, listener, audio)
    }

    /// Drain an already-taken request list into an audio system.
    ///
    /// This is [`SoundQueue::apply`] for a caller that used
    /// [`crate::take_sound_requests`] first: `Loop` and `Stop` still consult
    /// this queue's handle table, so the requests must have come from it.
    pub fn apply_requests(
        &mut self,
        requests: Vec<SoundRequest>,
        game_root: &std::path::Path,
        listener: ra2_audio::Listener,
        audio: &mut ra2_audio::Audio,
    ) -> Vec<String> {
        let mut problems = Vec::new();
        for request in requests {
            match request {
                SoundRequest::Play {
                    id,
                    path,
                    looping,
                    position,
                } => play_one(
                    id,
                    &path,
                    looping,
                    position,
                    game_root,
                    listener,
                    audio,
                    &mut problems,
                ),
                SoundRequest::Loop { id } => {
                    let sound = self.sounds.get(&id).or_else(|| self.retired.get(&id));
                    let Some(sound) = sound else {
                        problems.push(format!("plus.loopSound: sound {id} is gone"));
                        continue;
                    };
                    let (path, position) = (sound.path.clone(), sound.position);
                    play_one(
                        id,
                        &path,
                        true,
                        position,
                        game_root,
                        listener,
                        audio,
                        &mut problems,
                    );
                }
                SoundRequest::Stop { id } => {
                    audio.stop_script(id.0);
                    // A removed handle also loses its `setVolume` level; a
                    // merely stopped one keeps it for the next play.
                    if self.retired.remove(&id).is_some() {
                        audio.forget_script(id.0);
                    }
                }
                SoundRequest::SetVolume { id, hundredths_db } => {
                    audio.set_script_volume(id.0, hundredths_db);
                }
                SoundRequest::FadeInToLoop {
                    id,
                    hundredths_db,
                    ms,
                } => {
                    let sound = self.sounds.get(&id).or_else(|| self.retired.get(&id));
                    let Some(sound) = sound else {
                        problems.push(format!("plus.fadeInToLoop: sound {id} is gone"));
                        continue;
                    };
                    let (path, position) = (sound.path.clone(), sound.position);
                    // It starts from silence and rises to the target.
                    audio.set_script_volume(id.0, -10_000);
                    play_one(
                        id,
                        &path,
                        true,
                        position,
                        game_root,
                        listener,
                        audio,
                        &mut problems,
                    );
                    audio.fade_script_to(id.0, hundredths_db, ms);
                }
                SoundRequest::FadeOutLoop { id, ms } => {
                    audio.fade_out_script(id.0, ms);
                }
            }
        }
        // Only a `Loop` needs a removed sound's path, and `loopSound` cannot be
        // called on a removed handle, so every pending use has been served.
        self.retired.clear();
        problems
    }
}

fn unknown(action: &str, id: SoundId) -> String {
    format!("plus.{action}: unknown sound handle {id}")
}

#[allow(clippy::too_many_arguments)]
fn play_one(
    id: SoundId,
    path: &str,
    looping: bool,
    position: [f32; 3],
    game_root: &std::path::Path,
    listener: ra2_audio::Listener,
    audio: &mut ra2_audio::Audio,
    problems: &mut Vec<String>,
) {
    match ra2_audio::resolve_game_path(game_root, path) {
        Some(resolved) => {
            if let Err(error) = audio.play_script(id.0, &resolved, looping, position, listener, 1.0)
            {
                problems.push(format!("plus.playSound: sound {id}: {error}"));
            }
        }
        None => problems.push(format!(
            "plus.playSound: sound {id}: {path} is not in {}",
            game_root.display()
        )),
    }
}

/// How often the game ran its AI scripts, in seconds.
///
/// Not a guess: `AI.SuperAI.Tick` refreshes its sensors when its own counter
/// reaches 8, and its comment says "once per second", and `AimToHeading` scales
/// a turning rate by `self.tickInterval`. So a brain tick is an eighth of a
/// second and `tickInterval` is what the strategies multiply time by.
pub const AI_TICK_INTERVAL: f32 = 0.125;

/// How often the game ran an arena's `Tick`, in seconds.
///
/// `Hazards.Flame.Tick` counts by 0.25 and its comment says the flames last "the
/// end of 1 second" after four of them; `Hazards.Electricity` counts a 5 s
/// charge down by 0.25, i.e. twenty ticks.
pub const ARENA_TICK_INTERVAL: f32 = 0.25;

/// The live match: what the free functions and `plus.Arena` refer to.
pub struct State {
    pub game_root: Option<PathBuf>,
    /// The arena script's data, shared with the live `plus.Arena` object.
    pub arena: Option<Arc<Mutex<ArenaData>>>,
    /// The Python arena instance (the subclass), so the engine can call `Tick`.
    pub arena_object: Option<pyo3::Py<pyo3::PyAny>>,
    /// The running AI instances, one per robot.
    pub ai: Vec<pyo3::Py<pyo3::PyAny>>,
    /// What the scripts can see of each robot.
    pub bots: Vec<BotSnapshot>,
    /// What the scripts asked the engine to do.
    pub requests: Vec<Request>,
    /// Joints the arena scripts created (`AddPrismatic`, `GetHinge`), waiting
    /// for the simulator to build them.
    pub joints: Vec<arena::JointRequest>,
    /// Which robots the engine currently reports inside each registered zone.
    pub zone_contents: BTreeMap<String, Vec<(i32, bool)>>,
    /// Which robots each robot's own SmartZones overlap, keyed by `(robot,
    /// zone name)`, as `(1-based robot, chassis)` (`AI.GetZoneContents`).
    pub robot_zones: BTreeMap<(i32, String), Vec<(i32, bool)>>,
    pub time: f32,
    pub dt: f32,
    pub paused: bool,
    pub over: bool,
    pub players: Vec<i32>,
    pub game_type: String,
    /// The C++ robot's `+0x20` score, one entry per player. `plus.addPoints`
    /// *adds* to it (`fild [robot+0x20]; fadd amount`); the match shell zeroes
    /// it at the start of a match (`0x415a75`).
    pub scores: Vec<i32>,
    /// Network play: which end of a match this process is. The original's
    /// score writes (`FUN_004177e0`) run only on the host or in a local match
    /// and the host sends the result as `giAddPointsMessage`s (docs/38 §6.1).
    pub net_role: NetRole,
    /// Host: the script-origin point awards (`plus.addPoints`) since the match
    /// last drained them, to be sent as scoring messages.
    pub script_points: Vec<(i32, i32)>,
    /// The script layer's sound handles and the play/stop/loop requests they
    /// produced, in call order.
    pub sounds: SoundQueue,
    /// What the script layer asked for that a later milestone owns.
    pub deferred: Vec<String>,
    /// Anything the script layer wants to say to the operator.
    pub log: Vec<String>,
    /// The colour the arena script gave `plus.setBackColor` (RGB 0..1); the
    /// match renderer clears to it.
    pub back_color: Option<[f32; 3]>,
    /// What `plus.setCamera*` / `plus.animateCamera` asked of the main camera
    /// since the simulator last drained them ([`camera::take_commands`]).
    pub camera_commands: Vec<camera::CameraCommand>,
    /// The arena's intro is playing: `DoIntro(True)` ran and
    /// `plus.introComplete()` has not.
    pub intro_running: bool,
    /// The flame emitters the scripts created (`plus.AddParticleEmitter`).
    pub emitters: Vec<arena::EmitterState>,
}

impl State {
    const fn new() -> Self {
        State {
            game_root: None,
            arena: None,
            arena_object: None,
            ai: Vec::new(),
            bots: Vec::new(),
            requests: Vec::new(),
            joints: Vec::new(),
            zone_contents: BTreeMap::new(),
            robot_zones: BTreeMap::new(),
            time: 0.0,
            dt: 1.0 / 30.0,
            paused: false,
            over: false,
            players: Vec::new(),
            game_type: String::new(),
            scores: Vec::new(),
            net_role: NetRole::Local,
            script_points: Vec::new(),
            sounds: SoundQueue::new(),
            deferred: Vec::new(),
            log: Vec::new(),
            back_color: None,
            camera_commands: Vec::new(),
            intro_running: false,
            emitters: Vec::new(),
        }
    }

    pub fn defer(&mut self, what: &str) {
        if !self.deferred.iter().any(|w| w == what) {
            self.deferred.push(what.to_string());
        }
    }

    /// The snapshot for a robot, if the engine has published one.
    pub fn bot(&self, bot: i32) -> Option<&BotSnapshot> {
        if bot < 0 {
            return None;
        }
        self.bots.get(bot as usize)
    }

    /// The score of a player, or `0` for an id the match does not have.
    pub fn points(&self, bot: i32) -> i32 {
        if bot < 0 {
            return 0;
        }
        self.scores.get(bot as usize).copied().unwrap_or(0)
    }

    /// `plus.addPoints` — the write at `0x417918`: take what is there and add.
    ///
    /// A script's award is the host's to make: a joiner ignores its own copy of
    /// the arena script's `addPoints` (`FUN_004177e0` does nothing unless the
    /// process is the host or the match is local) and takes the host's
    /// `giAddPointsMessage` instead ([`State::add_points_remote`]); the host
    /// logs the award so the match can send it.
    pub fn add_points(&mut self, bot: i32, amount: i32) {
        match self.net_role {
            NetRole::Joiner => return,
            NetRole::Host => self.script_points.push((bot, amount)),
            NetRole::Local => {}
        }
        self.add_points_remote(bot, amount);
    }

    /// Add to a score without the network rules: the write itself, and what a
    /// joiner does with a host's script-origin `giAddPointsMessage`.
    pub fn add_points_remote(&mut self, bot: i32, amount: i32) {
        let Ok(bot) = usize::try_from(bot) else {
            return;
        };
        if bot >= self.scores.len() {
            self.scores.resize(bot + 1, 0);
        }
        self.scores[bot] += amount;
    }

    /// A damage hit's score write: `FUN_004178a0`'s
    /// `fild [robot+0x20]; fadd amount; __ftol; store` (`0x417918`–`0x417928`).
    ///
    /// The stored score is an `i32` and the original truncates the sum to it, so
    /// a hit worth 12.7 adds 12 (the fractional part is lost per event, not
    /// carried). Only a positive finite amount scores; `FUN_004178a0`'s guard is
    /// `0.0 < amount`.
    pub fn add_points_float(&mut self, bot: i32, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let Ok(bot) = usize::try_from(bot) else {
            return;
        };
        if bot >= self.scores.len() {
            self.scores.resize(bot + 1, 0);
        }
        self.scores[bot] += amount.trunc() as i32;
    }
}

/// Which end of a network match this process is (docs/38 §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NetRole {
    /// Not a network match: every rule runs here.
    #[default]
    Local,
    /// The authoritative end: prices hits, awards points, sends the results.
    Host,
    /// Follows the host's scoring messages; prices and awards nothing itself.
    Joiner,
}

static STATE: Mutex<State> = Mutex::new(State::new());

/// The script layer is process-global — `plus` is a module and the game's own
/// `Arenas.currentArena` is a module global — so the live match is shared.
/// Anything that drives a scripted match (a test, a tool) takes this first.
static SCRIPT_LOCK: Mutex<()> = Mutex::new(());

/// Take the whole script layer for this thread.
pub fn lock() -> MutexGuard<'static, ()> {
    SCRIPT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lock the live match state.
///
/// Keep the guard short: never call back into Python while holding it.
pub fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Publish what the scripts can see of the live match, and advance its clock.
///
/// The simulator calls this at the top of every step, after it has run the
/// brains (which read the previous snapshot) and before it applies the drive.
pub fn publish(bots: Vec<BotSnapshot>, time: f32, dt: f32) {
    let mut st = state();
    st.bots = bots;
    st.time = time;
    st.dt = dt;
}

/// Put the match state the scripts read into place, before their arena is
/// built.
///
/// `game_type` is one of the five strings the original compares against
/// (`DEATHMATCH`, `BATTLE ROYAL`, `TEAM MATCH`, `TABLETOP`, `KING OF THE HILL`);
/// `players` are the 0-based ids `plus.getPlayers()` reports (`box.py:158-181`
/// uses them for the intro cameras, and King of the Hill's `Activate` reads the
/// type at `king.py:26`). Scores start at zero, as `0x415a75` zeroes them.
pub fn prepare_match(game_type: &str, players: Vec<i32>) {
    let mut st = state();
    st.game_type = game_type.to_string();
    st.players = players;
    st.scores.clear();
    st.net_role = NetRole::Local;
    st.script_points.clear();
    st.paused = false;
    st.over = false;
    st.zone_contents.clear();
    st.robot_zones.clear();
    // A new match inherits nothing of the last one's arena or camera: the
    // arena object (and with it its intro generator and camera iterators) is
    // dropped until the new arena is built, and no camera request or running
    // intro carries over.
    st.arena_object = None;
    st.camera_commands.clear();
    st.intro_running = false;
    st.emitters.clear();
    // Nor does it inherit the last one's queued engine requests (a damage or
    // force call made after the last step, or by an arena never stepped) or its
    // list of calls deferred to a later milestone.
    st.requests.clear();
    st.deferred.clear();
}

/// Set the game-type string the scripts read (`plus.getGameType`).
pub fn set_game_type(game_type: &str) {
    state().game_type = game_type.to_string();
}

/// Set the 0-based player ids `plus.getPlayers` reports.
pub fn set_players(players: Vec<i32>) {
    state().players = players;
}

/// The pause flag `plus.isMatchPaused` reads.
pub fn set_paused(paused: bool) {
    state().paused = paused;
}

/// The end flag `plus.isMatchOver` reads (`GameWindow+0x8f`, `0x416450`).
pub fn set_over(over: bool) {
    state().over = over;
}

/// Publish this step's zone contents, keyed by zone name, values `(1-based
/// robot, chassis)`.
///
/// The simulator fills both the registered SmartZones (`RegisterZone`) and the
/// model's phantom volumes (`GetRobotsInPhantom`).
pub fn publish_zone_contents(contents: BTreeMap<String, Vec<(i32, bool)>>) {
    state().zone_contents = contents;
}

/// Publish what each robot's own SmartZones hold this step, keyed by `(robot,
/// zone name)`, values `(1-based robot, chassis)`.
pub fn publish_robot_zones(contents: BTreeMap<(i32, String), Vec<(i32, bool)>>) {
    state().robot_zones = contents;
}

/// A player's score, for the match's win conditions.
pub fn points(bot: i32) -> i32 {
    state().points(bot)
}

/// Add to a player's score. `plus.addPoints` and the KOTH script's own read
/// both go through here — there is no "set".
pub fn add_points(bot: i32, amount: i32) {
    state().add_points(bot, amount);
}

/// Add a damage hit's effective damage to a player's score, with the original's
/// per-event truncation (`0x417918`; see [`State::add_points_float`]).
pub fn add_points_float(bot: i32, amount: f32) {
    state().add_points_float(bot, amount);
}

/// Network play: which end of the match this process is. Set after
/// [`prepare_match`] (which resets it to [`NetRole::Local`]).
pub fn set_net_role(role: NetRole) {
    state().net_role = role;
}

/// Host: take the script-origin point awards made since the last call.
pub fn take_script_points() -> Vec<(i32, i32)> {
    std::mem::take(&mut state().script_points)
}

/// Joiner: apply a host's script-origin point award.
pub fn add_points_remote(bot: i32, amount: i32) {
    state().add_points_remote(bot, amount);
}

/// Take the requests the scripts queued during the last tick.
pub fn take_requests() -> Vec<Request> {
    std::mem::take(&mut state().requests)
}

/// Take the sound requests the scripts queued since the last call.
///
/// This leaves the handle table intact, so the caller can inspect the queue
/// without an audio system. To play them, use [`SoundQueue::apply`] instead.
pub fn take_sound_requests() -> Vec<SoundRequest> {
    state().sounds.take_requests()
}

/// Run the live arena's own `DoIntro(on)`.
///
/// The C++ match shell starts the arena's intro when `ra2.cfg`'s
/// `ArenaIntros` is on; the Python base class `Arenas.SuperArena.DoIntro`
/// starts `Introduction()` and `Tick` advances the generator, so the intro's
/// music, crowd and announcers arrive as ordinary [`SoundRequest`]s. The app
/// calls this once when a match starts. A match with no script arena is a
/// successful no-op.
pub fn arena_intro(on: bool) -> Result<(), String> {
    use pyo3::types::PyAnyMethods;
    pyo3::Python::with_gil(|py| {
        let arena = {
            let state = state();
            state
                .arena_object
                .as_ref()
                .map(|object| object.clone_ref(py))
        };
        let Some(arena) = arena else {
            return Ok(());
        };
        state().intro_running = on;
        arena
            .bind(py)
            .call_method1("DoIntro", (on,))
            .map(|_| ())
            .map_err(|error| format!("arena DoIntro: {error}"))
    })
}

/// Take the joints the arena scripts have asked for since the last call.
pub fn take_joint_requests() -> Vec<arena::JointRequest> {
    std::mem::take(&mut state().joints)
}

/// Point the script layer at a game installation.
pub fn set_game_root(root: PathBuf) {
    state().game_root = Some(root);
}

/// The game installation the scripts are being run from.
pub fn game_root() -> Option<PathBuf> {
    state().game_root.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::prelude::*;
    use std::sync::Mutex;
    use std::sync::MutexGuard;

    /// The (uncommitted) game directory, when present.
    fn game_dir() -> Option<PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn boot() -> Option<(ScriptHost, MutexGuard<'static, ()>)> {
        let guard = super::lock();
        let host = ScriptHost::boot(game_dir()?).ok()?;
        Some((host, guard))
    }

    /// Run `f` with the GIL held.
    fn with_py<R>(f: impl FnOnce(Python<'_>) -> R) -> R {
        Python::with_gil(f)
    }

    fn arena_data() -> std::sync::Arc<Mutex<ArenaData>> {
        state().arena.clone().expect("an arena is live")
    }

    /// Length of a list attribute on one of the script's own objects.
    fn list_len(obj: &Py<PyAny>, py: Python<'_>, name: &str) -> usize {
        obj.bind(py).getattr(name).unwrap().len().unwrap()
    }

    /// M4: the shipped arena scripts are Python 2 and run unmodified.
    ///
    /// This is the whole point of the layer — every stock arena is a subclass of
    /// an engine class, and they only register themselves when their module
    /// executes, so a successful list means the translation, the binding surface
    /// and the import path all worked on real game code.
    #[test]
    fn the_shipped_arena_scripts_load_and_register() {
        let Some((host, _guard)) = boot() else { return };
        let names = with_py(|py| host.load_arenas(py)).expect("arena scripts load");

        assert!(
            names.len() >= 12,
            "only {} arenas registered: {names:?}",
            names.len()
        );
        assert!(names.iter().any(|n| n == "Combat Arena"), "{names:?}");
        assert!(names.iter().all(|n| !n.is_empty()), "{names:?}");
        // `Arenas/__init__.py` ends with `import Practice`, which is
        // `Scripts/Practice.py` — a second directory, also Python 2.
        assert!(
            names.iter().any(|n| n == "Practice Arena"),
            "the Scripts/ path is missing: {names:?}"
        );
    }

    /// M4: the translation rewrites what Python 3 will not parse, and nothing
    /// else. Pinned against the constructs the survey found in the game.
    #[test]
    fn the_legacy_loader_rewrites_python2_syntax() {
        let Some((_host, _guard)) = boot() else {
            return;
        };
        with_py(|py| {
            let legacy = py.import("ra2_legacy").unwrap();
            let translate = legacy.getattr("translate").unwrap();
            let cases: &[(&str, &[&str], bool)] = &[
                ("a = 1\nprint \"a\", a\n", &["print(", "\"a\""], true),
                // `print >>f` needs a real file, so it is checked as text only.
                ("print >>f, 1\n", &["file=f"], false),
                (
                    "d = {}\nfor k, v in d.iteritems(): pass\n",
                    &[".items()"],
                    true,
                ),
                (
                    "d = {}\nfor v in d.itervalues(): pass\n",
                    &[".values()"],
                    true,
                ),
                ("it = iter([1])\n[it.next()]\n", &["next(it)"], true),
                ("a = 1\nb = 2\n[a <> b]\n", &["!="], true),
                (
                    "if 0:\n    raise ValueError, \"m\"\n",
                    // The translator inserts the paren where the comma was, so
                    // the spacing follows the source.
                    &["raise ValueError("],
                    true,
                ),
                (
                    "a = 1\nimport Hazards\nfrom king import KingOfHill\n",
                    &["from . import Hazards", "from .king import KingOfHill"],
                    // Relative imports need a package, so this is text-only.
                    false,
                ),
            ];
            for (src, wants, runnable) in cases {
                // `siblings` is how the loader hands the containing package to
                // the translator; here `local/game/Arenas` has both names.
                let dir = game_dir().unwrap().join("Arenas");
                let out: String = translate
                    .call1((*src, dir.to_string_lossy().to_string()))
                    .unwrap()
                    .extract()
                    .unwrap();
                for want in *wants {
                    assert!(out.contains(want), "{src:?} -> {out:?} missing {want:?}");
                }
                if !runnable {
                    continue;
                }
                // And the result has to be valid Python 3.
                let c = std::ffi::CString::new(out.clone()).unwrap();
                py.run(&c, None, None)
                    .unwrap_or_else(|e| panic!("{src:?} -> {out:?} does not compile: {e}"));
            }
            // Text inside a string is not code and is left alone.
            let src = "s = \"a.iteritems() and print x\"\n";
            let out: String = translate.call1((src,)).unwrap().extract().unwrap();
            assert_eq!(out, src);
        });
    }

    /// M4/F13: the engine tells the arena the match has begun, and the arena's
    /// own hazards take it from there.
    ///
    /// `Arena.Activate` is the arenas' start-of-match hook and the game calls it.
    /// Some of them cannot work without it: the Octagon only opens its four trap
    /// floors from `Pit1.setactive(True)` inside `Activate` (King of the Hill
    /// reads the game type there). The floors then open on their *own* script's
    /// clock — `Hazards.PitSlider.Tick` counts halves of a second and its delay is
    /// 45, so it lets go on the ninetieth arena tick — which is what the engine's
    /// `Tick` loop is for.
    #[test]
    fn the_engine_tells_the_arena_the_match_has_begun() {
        let Some(game) = game_dir() else { return };
        let _guard = super::lock();
        let script = game.join("Arenas/octagon.py");
        let host = match ScriptHost::boot_with_scripts(&game, Some(&script)) {
            Ok(h) => h,
            Err(e) => panic!("booting the Octagon: {e}"),
        };
        let pits = ["Pit1", "Pit2", "Pit3", "Pit4"];
        with_py(|py| {
            let arena = state()
                .arena_object
                .as_ref()
                .expect("the arena is live")
                .clone_ref(py);
            let a = arena.bind(py);
            let flag = |pit: &str, name: &str| -> bool {
                a.getattr(pit)
                    .unwrap_or_else(|e| panic!("{pit}: {e}"))
                    .getattr(name)
                    .unwrap()
                    .extract()
                    .unwrap()
            };
            // Only `octagon.py`'s `Activate(True)` arms them.
            for pit in pits {
                assert!(flag(pit, "active"), "{pit} was never told to open");
            }
            // And ninety of the arena's quarter-seconds later they let go.
            for _ in 0..90 {
                host.tick_arena(py, 0.25).unwrap();
            }
            for pit in pits {
                assert!(flag(pit, "sliding"), "{pit} never started sliding");
            }
        });
    }

    /// M8.3: the match state the scripts read is real, and `addPoints` adds.
    ///
    /// `king.py` is why each of these matters: its `Activate` reads the game
    /// type (`king.py:26`), its `Tick` reads `tickInterval` (lines 40,47) and
    /// calls `addPoints` (line 44), and the tabletop arenas eliminate players
    /// through the same queue. A setter is not involved: the binary's write is
    /// `fild [robot+0x20]; fadd amount`, so two calls must leave the sum.
    #[test]
    fn plus_writes_the_match_state_the_scripts_read() {
        let Some((host, _guard)) = boot() else { return };
        crate::prepare_match("KING OF THE HILL", vec![0, 1]);
        with_py(|py| {
            let plus = host.import(py, "plus").unwrap();
            plus.call_method1("addPoints", (0, 10)).unwrap();
            plus.call_method1("addPoints", (0, 5)).unwrap();
            assert_eq!(
                plus.call_method1("getPoints", (0,))
                    .unwrap()
                    .extract::<i32>()
                    .unwrap(),
                15,
                "addPoints must add, not replace"
            );
            assert_eq!(
                plus.call_method1("getGameType", ())
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "KING OF THE HILL",
                "the script compares the exact string"
            );
            assert_eq!(
                plus.call_method1("getPlayers", ())
                    .unwrap()
                    .extract::<Vec<i32>>()
                    .unwrap(),
                vec![0, 1]
            );
            assert!(!plus
                .call_method1("isMatchOver", ())
                .unwrap()
                .extract::<bool>()
                .unwrap());
            assert!(!plus
                .call_method1("isMatchPaused", ())
                .unwrap()
                .extract::<bool>()
                .unwrap());
            crate::set_over(true);
            crate::set_paused(true);
            assert!(plus
                .call_method1("isMatchOver", ())
                .unwrap()
                .extract::<bool>()
                .unwrap());
            assert!(plus
                .call_method1("isMatchPaused", ())
                .unwrap()
                .extract::<bool>()
                .unwrap());

            // Every arena reads `self.tickInterval`; it is the game's 0.25 s
            // arena tick, the value the hazards already count in.
            host.load_arenas(py).unwrap();
            let arena = host.create_arena(py, "Combat Arena").unwrap();
            let interval: f32 = arena
                .bind(py)
                .getattr("tickInterval")
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(interval, crate::ARENA_TICK_INTERVAL);
            assert_eq!(interval, 0.25);
        });
        crate::prepare_match("", Vec::new());
    }

    /// A new match starts with empty per-match queues: engine requests and
    /// joint requests an earlier arena left behind are dropped.
    #[test]
    fn a_new_match_drops_the_last_ones_queues() {
        let _guard = super::lock();
        {
            let mut st = crate::state();
            st.requests.push(crate::Request::Zap {
                bot: 1,
                amount: 1.0,
                radius: 1.0,
            });
            st.defer("something");
        }
        crate::prepare_match("DEATHMATCH", vec![0, 1]);
        let st = crate::state();
        assert!(st.requests.is_empty(), "requests carried over");
        assert!(st.deferred.is_empty(), "deferred carried over");
        drop(st);
        crate::prepare_match("", Vec::new());
    }

    /// M8.3: a raising arena `Tick` is returned with its Python traceback.
    ///
    /// The old host turned the error into `true`, so a `Tick` that never ran
    /// looked like a tick that succeeded — which is how KOTH's missing
    /// `tickInterval` stayed invisible. This test makes the failure the whole
    /// point.
    #[test]
    fn a_raising_arena_tick_is_reported() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| {
            py.run(
                c"class BrokenArena(object):\n    def Tick(self):\n        raise ValueError('tick exploded')\n",
                None,
                None,
            )
            .unwrap();
            let object = py.eval(c"BrokenArena()", None, None).unwrap();
            crate::state().arena_object = Some(object.unbind());
        });
        let error = host
            .run_arena_tick(0.25)
            .expect_err("a raising Tick must not report success");
        assert!(error.contains("tick exploded"), "{error}");
        assert!(
            error.contains("Traceback") || error.contains("ValueError"),
            "the traceback should be in the message: {error}"
        );
        crate::state().arena_object = None;
    }

    /// M4: `Arenas/box.py` drives the engine through `plus.Arena` — cameras,
    /// shadow receivers, the navigation map, the hazard zones.
    #[test]
    fn the_box_arena_configures_itself_from_its_script() {
        let Some((host, _guard)) = boot() else { return };
        let (zones, hazards, spikes_type) = with_py(|py| {
            host.load_arenas(py).unwrap();
            let arena = host.create_arena(py, "Combat Arena").expect("box arena");

            {
                let data = arena_data();
                let d = data.lock().unwrap();
                assert_eq!(d.model, "Arenas/box/boxarena.gmf");
                // `AddShadowReceivers` is the engine's call, and it reaches the
                // engine: `Arenas.SuperArena` leaves `SetShadowSource` and
                // `AddShadowTriangle` to `plus.Arena`.
                assert_eq!(d.shadow_source, Some([0.653, 6.894, -0.418]));
                assert_eq!(d.shadow_triangles.len(), 10);
                // `LoadMap` read the shipped .map: 30x30 cells at 1 m.
                assert_eq!((d.map.cols, d.map.rows), (30, 30));
                assert_eq!(d.map.resolution, 1.0);
                assert!(
                    (d.map.altitude + 2.264_98).abs() < 1e-4,
                    "{}",
                    d.map.altitude
                );
                assert_eq!(d.extent, [-15.0, 15.0, 15.0, -15.0]);
                assert!(d.map.cells.iter().filter(|c| c.wall).count() > 100);
                assert_eq!(d.map.cells.iter().filter(|c| c.hazard).count(), 24);
                // Hazards are off until the match says otherwise.
                assert!(d.zones.is_empty());
                assert!(d.start_points.len() >= 2, "the model declares start points");
            }

            // The cameras are the Python layer's: `Arenas.SuperArena` keeps them
            // in `self.StaticCams`/`self.WatchCams` and applies them through
            // `plus.setCameraPosition`, so the script's own list is the evidence.
            assert_eq!(list_len(&arena, py, "StaticCams"), 3);
            assert_eq!(list_len(&arena, py, "WatchCams"), 1);

            // `HazardsOn(True)` builds the hazards, registers their zones and
            // fences them (and the walls) with collision lines.
            let lines_before = arena_data().lock().unwrap().collision_lines.len();
            assert!(host.hazards_on(py, true).unwrap());
            let (zones, lines_after, hz) = {
                let data = arena_data();
                let d = data.lock().unwrap();
                let lines = d.collision_lines.len();
                assert!(d.map.hazards_on);
                assert!(d.sub_materials.len() >= 3, "SetSubMaterialSound");
                (d.zones.clone(), lines, list_len(&arena, py, "hazards"))
            };
            // `box.py`: three hazard boxes of four lines, and the four walls.
            assert_eq!(lines_after - lines_before, 16, "AddCollisionLine");
            let spikes = arena.bind(py).getattr("spikes1").unwrap();
            let ty = spikes.get_type().name().unwrap().to_string();
            (zones, hz, ty)
        });

        assert_eq!(zones.len(), 3, "saw + two spike zones: {zones:?}");
        assert_eq!(zones.get("saw_zone"), Some(&1));
        assert_eq!(zones.get("spike_zone1"), Some(&2));
        assert_eq!(zones.get("spike_zone2"), Some(&3));
        assert_eq!(hazards, 3);
        // The hazards are the script's own classes, with their own state.
        assert_eq!(spikes_type, "Spikes");
    }

    /// The AI's navigation is the original's (`FUN_00405990`, `FUN_00405720`):
    /// the arena's collision lines and vantage points, not a grid search. A
    /// straight run that clears every line is the whole route, a blocked one goes
    /// over the POVs, and a robot pressed against a wall still gets a route —
    /// an empty answer leaves `DriveToLocation` doing nothing, which is how the
    /// robots used to park against the walls.
    #[test]
    fn the_ai_paths_around_collision_lines_and_always_gets_a_route() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| {
            host.load_arenas(py).unwrap();
            let arena = host.create_arena(py, "Combat Arena").unwrap();
            assert!(host.hazards_on(py, true).unwrap());
            let a = arena.bind(py);
            a.call_method1("SetSearchRadius", (1.2f32,)).unwrap();
            let clear = |from: (f32, f32, f32), to: (f32, f32, f32)| -> bool {
                a.call_method1("IsStraightPathClear", (from, to, false))
                    .unwrap()
                    .extract()
                    .unwrap()
            };
            let path = |from: (f32, f32, f32), to: (f32, f32, f32)| -> Vec<(f32, f32)> {
                a.call_method1("GetPath", (from, to, false))
                    .unwrap()
                    .extract()
                    .unwrap()
            };

            // Open floor: the straight run is the route, end to end.
            let (from, to) = ((-9.0, -2.0, 9.0), (-9.0, -2.0, -2.0));
            assert!(clear(from, to));
            assert_eq!(path(from, to), vec![(-9.0, 9.0), (-9.0, -2.0)]);

            // Corner to corner crosses the spikes' box (`box.py`: x 4.2..5.6,
            // z -6.4..-3.9), so the route bends over the vantage points.
            let (from, to) = ((-9.9, -1.9, 9.95), (9.7, -1.9, -9.5));
            assert!(!clear(from, to));
            let route = path(from, to);
            assert!(route.len() > 2, "{route:?}");
            assert_eq!(route[0], (-9.9, 9.95));
            assert_eq!(*route.last().unwrap(), (9.7, -9.5));
            let second = route[1];
            assert!(
                clear(from, (second.0, -1.9, second.1)),
                "the first leg is clear: {route:?}"
            );

            // Pressed against the west wall (`x = -12.7`) a 1.2 m robot's own
            // spot crosses the line; it still gets a way out.
            let route = path((-11.9, -1.9, 7.0), (9.7, -1.9, -9.5));
            assert!(route.len() >= 2, "{route:?}");
        });
    }

    /// `GetLastDamageReceived`/`Done` are the original's 5-tuples
    /// (`FUN_004b9050`: component, amount, time, player, player's component), and
    /// the shipped `Spinner.RobotInRange` reads the player at index 3. A 3-tuple
    /// made every spinner's `Engage` raise once an enemy came within range.
    #[test]
    fn a_spinner_brain_reads_the_five_field_damage_record() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| {
            host.load_arenas(py).unwrap();
            host.create_arena(py, "Combat Arena").unwrap();
            host.load_ai(py).unwrap();
            let brain = host
                .create_ai(py, "Tornado", 0)
                .expect("Tornado has a brain in Bindings.py");
            let me = BotSnapshot {
                alive: true,
                direction: Vec3::Z,
                last_damage_received: (3, 25.0, 9.5, 1, 7),
                last_damage_done: NO_DAMAGE,
                ..Default::default()
            };
            let other = BotSnapshot {
                alive: true,
                location: Vec3::new(1.0, 0.0, 0.0),
                direction: Vec3::Z,
                ..Default::default()
            };
            publish(vec![me, other], 10.0, 1.0 / 60.0);
            let b = brain.bind(py);
            let record: DamageRecord = b
                .call_method0("GetLastDamageReceived")
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(record, (3, 25.0, 9.5, 1, 7));
            // Robot 1 is 1 m away and hit us half a second ago.
            let in_range: (bool, bool) = b
                .call_method1("RobotInRange", (1,))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(in_range, (true, true));
            // Nothing landed yet reads as nobody's (what the simulator
            // publishes before the first hit).
            let done: DamageRecord = b
                .call_method0("GetLastDamageDone")
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(done, NO_DAMAGE);
        });
    }

    /// M4: `plus.Arena` answers the script's geometry and pathing questions.
    #[test]
    fn the_arena_answers_ray_tests_and_pathing() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| {
            host.load_arenas(py).unwrap();
            let arena = host.create_arena(py, "Combat Arena").unwrap();
            let obj = arena.bind(py);

            // `GetStartPointLocation` comes from the model's own start points.
            let start: (f32, f32, f32) = obj
                .call_method1("GetStartPointLocation", (0usize,))
                .unwrap()
                .extract()
                .unwrap();
            assert!(start.1 > -30.0 && start.1 < 30.0, "{start:?}");

            // A ray down from well above the arena must find its floor.
            let hit: (bool, (f32, f32, f32)) = obj
                .call_method1(
                    "RayTest",
                    ((start.0, 20.0, start.2), (start.0, -20.0, start.2)),
                )
                .unwrap()
                .extract()
                .unwrap();
            assert!(hit.0, "the arena floor is under the start point");

            // And the navigation gives a route across the arena, as world
            // `(x, z)` points from the start to the goal.
            let path: Vec<(f32, f32)> = obj
                .call_method1(
                    "GetPath",
                    ((start.0, start.1, start.2), (0.0, -2.0, 0.0), false),
                )
                .unwrap()
                .extract()
                .unwrap();
            assert!(
                path.len() >= 2,
                "a route from the start point to the centre"
            );
            assert_eq!(path[0], (start.0, start.2), "{path:?}");
            assert_eq!(*path.last().unwrap(), (0.0, 0.0), "{path:?}");

            // The scripts' grid is the world plane (`FUN_00406240`,
            // `FUN_00405fa0`): `ToGrid` keeps `(x, z)` and `FromGrid` puts the
            // floor back under it.
            let g = obj
                .call_method1("ToGrid", ((1.5f32, 0.0f32, -2.25f32),))
                .unwrap()
                .extract::<(f32, f32)>()
                .unwrap();
            assert_eq!(g, (1.5, -2.25));
            let back: (f32, f32, f32) = obj
                .call_method1("FromGrid", (g,))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!((back.0, back.2), (1.5, -2.25));
            assert!((back.1 + 2.265).abs() < 0.01, "the box floor: {back:?}");
        });
    }

    /// The queue drains into a real `ra2_audio::Audio` with no device: the
    /// script's paths resolve case-insensitively, the plays reach the play log
    /// with their positions, and a missing file is reported, not fatal.
    ///
    /// This is the seam `ra2-app` will use; nothing here touches the app.
    #[test]
    fn the_sound_queue_drains_into_a_silent_audio() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("ra2script-sounds-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(root.join("Sounds/Intro_Music")).unwrap();
        std::fs::create_dir_all(root.join("Sounds/Crowd")).unwrap();
        std::fs::write(root.join("Sounds/Intro_Music/Smell_Glue.wav"), b"wav").unwrap();
        std::fs::write(root.join("Sounds/Crowd/LoudCheer_Loop.wav"), b"wav").unwrap();

        let mut queue = SoundQueue::new();
        let intro = queue.create(r"Sounds\intro_music\smell_glue.wav", false, [1.0, 2.0, 3.0]);
        let crowd = queue.create("Sounds/crowd/LoudCheer_Loop.wav", false, [4.0, 5.0, 6.0]);
        queue.play(intro).unwrap();
        queue.play(intro).unwrap();
        queue.loop_sound(crowd).unwrap();
        assert!(queue.stop(intro));
        // Removing the crowd sound after queueing its loop must not lose the
        // path the `Loop` still needs at drain time.
        assert!(queue.remove(crowd));

        let mut audio = ra2_audio::Audio::without_device();
        let problems = queue.apply(
            &root,
            ra2_audio::Listener::looking_forward([0.0; 3]),
            &mut audio,
        );
        assert!(problems.is_empty(), "{problems:?}");

        let entries = audio.play_log().entries();
        assert_eq!(entries.len(), 3, "two plays and the loop");
        assert_eq!(
            entries[0].path,
            root.join("Sounds/Intro_Music/Smell_Glue.wav")
        );
        assert_eq!(entries[0].position, Some([1.0, 2.0, 3.0]));
        assert_eq!(entries[1].position, Some([1.0, 2.0, 3.0]));
        assert_eq!(
            entries[2].path,
            root.join("Sounds/Crowd/LoudCheer_Loop.wav"),
            "the loop resolves too, even though removeSound retired the handle"
        );
        assert_eq!(entries[2].position, Some([4.0, 5.0, 6.0]));
        assert!(queue.requests().is_empty(), "the drain consumes the queue");

        // A path that is not in the game is reported by name and skipped.
        let missing = queue.create("Sounds/not_shipped.wav", false, [0.0; 3]);
        queue.play(missing).unwrap();
        let problems = queue.apply(
            &root,
            ra2_audio::Listener::looking_forward([0.0; 3]),
            &mut audio,
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("not_shipped.wav"), "{problems:?}");

        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod shell_tests {
    use super::*;
    use pyo3::prelude::*;

    fn game_dir() -> Option<PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn boot() -> Option<(ScriptHost, std::sync::MutexGuard<'static, ()>)> {
        let guard = super::lock();
        super::giscript::clear();
        super::ui::clear();
        // The interpreter outlives a test, so `Main` would be a no-op the second
        // time. Drop it so each boot really runs the shell's file.
        with_py(|py| {
            if let Ok(modules) = py.import("sys").and_then(|s| s.getattr("modules")) {
                let _ = modules.del_item("Main");
            }
        });
        let host = ScriptHost::boot(game_dir()?).ok()?;
        Some((host, guard))
    }

    fn with_py<R>(f: impl FnOnce(Python<'_>) -> R) -> R {
        Python::with_gil(f)
    }

    /// The shell's own `Scripts/Main.py` runs: it hooks the interpreter's output
    /// into `giScript`, imports every arena and brain, and hands the engine its
    /// event callback. This is the game's boot sequence, not a stand-in for it.
    #[test]
    fn the_shell_boots_its_own_main_py() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| host.boot_shell(py)).expect("Main.py runs");

        let said = super::giscript::output();
        assert!(
            said.contains("Main module loaded"),
            "the shell printed: {said:?}"
        );
        // `print` went through the hook Main.py installed rather than to the
        // interpreter's own stdout, which is the point of the first lines.
        assert!(
            super::giscript::has_callback(),
            "dispatchEvent was registered"
        );
    }

    /// A script's debug window is a real widget tree: the shipped `Gooey.py`
    /// builds it through `plus.addWindow`/`addText`, and the result is the same
    /// `Widget` a `.gib` screen is made of, so the layout resolver places it.
    #[test]
    fn a_scripts_debug_window_is_a_widget_tree() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| -> PyResult<()> {
            let gooey = host.import(py, "Gooey")?;
            let window = gooey.call_method1("Plain", ("watch", 0.0, 75.0, 100.0, 75.0))?;
            let text = window.call_method1("addText", ("line0", 0.0, 0.0, 100.0, 15.0))?;
            text.call_method1("setText", ("Throttle",))?;
            let same = window.call_method1("get", ("line0",))?;
            assert_eq!(
                same.call_method0("GetText")?.extract::<String>()?,
                "Throttle",
                "the shipped Gooey.Plain hands back the same handle"
            );
            Ok(())
        })
        .expect("the shipped Gooey builds a window");

        let windows = super::ui::windows();
        assert_eq!(windows.len(), 1, "one window");
        let window = &windows[0];
        assert_eq!(window.kind, ra2fmt::gib::WidgetKind::Window);
        assert_eq!(window.name(), "watch");
        assert_eq!(window.children.len(), 1);
        let line = &window.children[0];
        assert_eq!(line.kind, ra2fmt::gib::WidgetKind::Text);
        assert_eq!(line.text(), Some("Throttle"));
        assert_eq!(line.font(), Some("fg_mcn14"), "the shell's debug face");
        // `Size 100 0 15 0` at `Position 0 0`, inside the window's 100x75.
        let frame = ra2ui::place(window, ra2ui::Frame::new(0.0, 0.0, 1280.0, 1005.0));
        assert_eq!((frame.frame.x, frame.frame.y), (0.0, 75.0));
        assert_eq!((frame.frame.w, frame.frame.h), (100.0, 75.0));
        let line = &frame.children[0];
        assert_eq!((line.frame.x, line.frame.y), (0.0, 75.0));
        assert_eq!((line.frame.w, line.frame.h), (100.0, 15.0));
    }

    /// `setColor` reaches the widget the renderer reads, and `Remove` takes the
    /// window out of the tree.
    #[test]
    fn a_line_takes_the_colour_it_is_given() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| -> PyResult<()> {
            let gooey = host.import(py, "Gooey")?;
            let window = gooey.call_method1("Plain", ("watch", 0.0, 0.0, 50.0, 50.0))?;
            let text = window.call_method1("addText", ("line0", 0.0, 0.0, 50.0, 10.0))?;
            text.call_method1("setColor", (1.0, 0.5, 0.0))?;
            Ok(())
        })
        .expect("build");
        let windows = super::ui::windows();
        let line = &windows[0].children[0];
        assert_eq!(
            line.field("_ForeColor").map(|f| f.text()),
            Some("1 0.5 0 1")
        );
    }

    /// The engine can call back into the shell: `dispatchEvent` is registered by
    /// `Main.py` and reached by name, which is how the game's C++ does it.
    #[test]
    fn dispatch_event_reaches_the_shell() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| host.boot_shell(py)).expect("Main.py runs");
        with_py(|py| {
            let data = py.None().into_bound(py);
            let _ = data;
            super::giscript::dispatch_event(py, "anything", &py.None().into_bound(py))
        })
        .expect("the shell's dispatchEvent is called");
    }

    #[test]
    fn volume_and_fade_requests_reach_the_audio_clock() {
        let mut queue = SoundQueue::new();
        let id = queue.create("Sounds/crowd/LoudCheer_Loop.wav", false, [0.0; 3]);
        queue.set_volume(id, 0).unwrap();
        queue.fade_in_to_loop(id, -100, 800.0).unwrap();
        queue.fade_out_loop(id, 8000.0).unwrap();
        assert!(queue.get(id).unwrap().looping, "fadeInToLoop makes a loop");
        assert!(matches!(
            queue.requests(),
            [
                SoundRequest::SetVolume {
                    hundredths_db: 0,
                    ..
                },
                SoundRequest::FadeInToLoop {
                    hundredths_db: -100,
                    ..
                },
                SoundRequest::FadeOutLoop { .. }
            ]
        ));
        assert!(queue.set_volume(SoundId(99), 0).is_err());
        assert!(queue.fade_out_loop(SoundId(99), 1.0).is_err());
        assert!(queue.remove(id));
    }
}
