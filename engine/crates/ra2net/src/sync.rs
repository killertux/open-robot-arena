//! The in-match synchroniser: the host's world snapshots (`giSyncMessage`,
//! type `0x74`) and the connection-speed table that paces them.
//!
//! # The model
//!
//! Robot Arena 2 multiplayer is **host-authoritative state sync**, not
//! lockstep. Only the host steps the physics that matters: clients send their
//! key events as `giRemoteInputMessage` (`0x73`, one `Bot::Input` each), the
//! host applies them to the remote humans' robots (`HumanRemote`), and every
//! `interval` seconds the `Synchronizer` (global at `0x718404`, update
//! `0x4f77b0`) builds a `giSyncMessage` (`0x4d2470`) of every rigid body's
//! position, orientation and velocities and sends it to all players. Scoring
//! events (damage, points, immobility, fractures, critical hits, the win)
//! travel as their own messages. A client that is "not in game yet" drops a
//! sync ("ERROR: Sync ignored - not in game yet").
//!
//! # The message
//!
//! `giSyncMessage` write (`0x4d2b60`) builds the snapshot in a scratch stream,
//! compresses it with zlib 1.1.3's `compress` (`FUN_0056bde0`, level default)
//! and writes
//!
//! ```text
//! u32  0x74
//! u32  the snapshot's byte count
//! u32  the compressed byte count
//! ...  the zlib stream
//! ```
//!
//! The snapshot itself (all words little-endian):
//!
//! ```text
//! u32  match clock word            (FUN_00469b90)
//! u32  second clock word           (FUN_00609060)
//! u32  number of bodies below
//! bool full: 0 = every body in full; 1 = sleeping bodies send position+rotation only
//! f32  robot count (a float)
//! per robot present:  f32, then one f32 per component     (component count from the design)
//! per controller:     u32 id, f32 value                    (vtable +0x70; no count on the wire)
//! per device:         u32 id, u8 kind, then by kind        (no count on the wire)
//!                       1: bool, 4 x f32
//!                       4: f32
//!                       5: bool, u32, 3 x f32
//! per body (count above):
//!     u32 id
//!     bool awake            (full mode: always 1)
//!     3 x f32 position      (body +0x30)
//!     4 x f32 rotation      (body +0x40: the scalar, then the vector)
//!     if awake: 3 x f32 linear velocity (+0x54), 3 x f32 angular velocity (+0x64)
//! ```
//!
//! The controller and device lists carry no count: both ends walk the same
//! lists built from the same designs. Decoding therefore needs a
//! [`SyncSchema`] that says how many of each there are.

use std::io::{Read, Write as _};

use crate::stream::{Reader, StreamError, Writer};

/// The connection-speed choices of `mpc_settings.gib` (`FUN_0048e4a0` builds
/// the dropdown; each row is a `ConnectionSetting` of the sync interval in
/// seconds and a byte budget). The host's choice is the `Synchronizer`'s
/// interval; "hostspeed" in the GameSpy info reply names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSpeed {
    Lan,
    T1,
    Cable,
    Dsl256,
    Dsl128,
}

impl ConnectionSpeed {
    pub const ALL: [ConnectionSpeed; 5] = [
        ConnectionSpeed::Lan,
        ConnectionSpeed::T1,
        ConnectionSpeed::Cable,
        ConnectionSpeed::Dsl256,
        ConnectionSpeed::Dsl128,
    ];

    /// The dropdown text.
    pub fn label(self) -> &'static str {
        match self {
            ConnectionSpeed::Lan => "LAN",
            ConnectionSpeed::T1 => "T1 (512K/s)",
            ConnectionSpeed::Cable => "Cable (384K/s)",
            ConnectionSpeed::Dsl256 => "DSL (256K/s)",
            ConnectionSpeed::Dsl128 => "DSL (128K/s)",
        }
    }

    /// The tag the host puts in its session name (`FUN_0048ed..`: the
    /// dropdown index picks one of `0x6f447c..0x6f44b4` through the jump table
    /// `0x48f7c8`; index 5, ` 56K Dialup`, has no row here). The original
    /// host on LAN sent `512K LAN` (live, docs/38 §14).
    pub fn session_tag(self) -> &'static str {
        match self {
            ConnectionSpeed::Lan => "512K LAN",
            ConnectionSpeed::T1 => "512K T1",
            ConnectionSpeed::Cable => "384K Cable",
            ConnectionSpeed::Dsl256 => "256K DSL",
            ConnectionSpeed::Dsl128 => "128K DSL",
        }
    }

    /// Seconds between snapshots: the first float of the row
    /// (`0x48e891` `0x3c99999a`, `0x48e8f4` `0x3d000000`, `0x48e957`
    /// `0x3d2a64c3`, `0x48e9ba` `0x3d800000`, `0x48ea1d` `0x3e000000`).
    pub fn interval(self) -> f32 {
        f32::from_bits(match self {
            ConnectionSpeed::Lan => 0x3c99_999a,
            ConnectionSpeed::T1 => 0x3d00_0000,
            ConnectionSpeed::Cable => 0x3d2a_64c3,
            ConnectionSpeed::Dsl256 => 0x3d80_0000,
            ConnectionSpeed::Dsl128 => 0x3e00_0000,
        })
    }

    /// The row's second float (`0x46ffd400` = 32746 for LAN down to
    /// `0x44fa0000` = 2000): a byte budget per second.
    pub fn budget(self) -> f32 {
        f32::from_bits(match self {
            ConnectionSpeed::Lan => 0x46ff_d400,
            ConnectionSpeed::T1 => 0x45fa_0000,
            ConnectionSpeed::Cable => 0x45bb_8000,
            ConnectionSpeed::Dsl256 => 0x457a_0000,
            ConnectionSpeed::Dsl128 => 0x44fa_0000,
        })
    }

    pub fn from_label(label: &str) -> Option<ConnectionSpeed> {
        ConnectionSpeed::ALL
            .into_iter()
            .find(|s| s.label() == label)
    }
}

/// The `Synchronizer`'s pacing (`0x4f77b0`): it adds the frame time to an
/// accumulator and sends a snapshot when the accumulator reaches the interval,
/// then zeroes it (no carry).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pacer {
    pub interval: f32,
    accumulator: f32,
}

impl Pacer {
    pub fn new(speed: ConnectionSpeed) -> Pacer {
        Pacer {
            interval: speed.interval(),
            accumulator: 0.0,
        }
    }

    /// Advance by `dt`; true when a snapshot is due.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.accumulator += dt;
        if self.interval <= self.accumulator {
            self.accumulator = 0.0;
            true
        } else {
            false
        }
    }
}

/// One rigid body in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodySync {
    pub id: u32,
    pub awake: bool,
    pub position: [f32; 3],
    /// Scalar first, then the vector part (the order `0x45e9b0` copies).
    pub rotation: [f32; 4],
    pub linear: [f32; 3],
    pub angular: [f32; 3],
}

/// A robot's per-component floats.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RobotSync {
    pub value: f32,
    pub components: Vec<f32>,
}

/// One device entry (12-byte list of the sync writer).
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceSync {
    /// Kind 1.
    One {
        id: u32,
        flag: bool,
        values: [f32; 4],
    },
    /// Kind 4.
    Four { id: u32, value: f32 },
    /// Kind 5.
    Five {
        id: u32,
        flag: bool,
        word: u32,
        values: [f32; 3],
    },
    /// Any other kind: the id and kind byte only (the writer writes nothing
    /// more for them).
    Other { id: u32, kind: u8 },
}

/// The lists both ends share, which the snapshot does not count.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncSchema {
    /// Component count of each robot the snapshot lists.
    pub robot_components: Vec<usize>,
    pub controllers: usize,
    pub devices: usize,
}

/// A decoded snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncState {
    pub clock: u32,
    pub clock2: u32,
    /// `false`: every body full (the writer's `+0x40 == 0`); `true`: sleeping
    /// bodies are cut to position and rotation.
    pub partial: bool,
    pub robots: Vec<RobotSync>,
    pub controllers: Vec<(u32, f32)>,
    pub devices: Vec<DeviceSync>,
    pub bodies: Vec<BodySync>,
}

impl SyncState {
    /// The schema this state was written against.
    pub fn schema(&self) -> SyncSchema {
        SyncSchema {
            robot_components: self.robots.iter().map(|r| r.components.len()).collect(),
            controllers: self.controllers.len(),
            devices: self.devices.len(),
        }
    }

    /// The uncompressed snapshot.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(self.clock)
            .u32(self.clock2)
            .u32(self.bodies.len() as u32);
        w.bool(self.partial).f32(self.robots.len() as f32);
        for robot in &self.robots {
            w.f32(robot.value);
            for c in &robot.components {
                w.f32(*c);
            }
        }
        for (id, value) in &self.controllers {
            w.u32(*id).f32(*value);
        }
        for device in &self.devices {
            match device {
                DeviceSync::One { id, flag, values } => {
                    w.u32(*id).u8(1).bool(*flag);
                    for v in values {
                        w.f32(*v);
                    }
                }
                DeviceSync::Four { id, value } => {
                    w.u32(*id).u8(4).f32(*value);
                }
                DeviceSync::Five {
                    id,
                    flag,
                    word,
                    values,
                } => {
                    w.u32(*id).u8(5).bool(*flag).u32(*word);
                    for v in values {
                        w.f32(*v);
                    }
                }
                DeviceSync::Other { id, kind } => {
                    w.u32(*id).u8(*kind);
                }
            }
        }
        for body in &self.bodies {
            let awake = body.awake || !self.partial;
            w.u32(body.id).bool(awake);
            for v in body.position.iter().chain(&body.rotation) {
                w.f32(*v);
            }
            if awake {
                for v in body.linear.iter().chain(&body.angular) {
                    w.f32(*v);
                }
            }
        }
        w.into_bytes()
    }

    pub fn from_bytes(bytes: &[u8], schema: &SyncSchema) -> Result<SyncState, StreamError> {
        let mut r = Reader::new(bytes);
        let clock = r.u32()?;
        let clock2 = r.u32()?;
        let body_count = r.u32()? as usize;
        let partial = r.bool()?;
        let robot_count = r.f32()?;
        if robot_count < 0.0 || robot_count as usize != schema.robot_components.len() {
            return Err(StreamError::Invalid(
                "robot count does not match the schema",
            ));
        }
        let mut robots = Vec::new();
        for &n in &schema.robot_components {
            let value = r.f32()?;
            let mut components = Vec::with_capacity(n);
            for _ in 0..n {
                components.push(r.f32()?);
            }
            robots.push(RobotSync { value, components });
        }
        let mut controllers = Vec::with_capacity(schema.controllers);
        for _ in 0..schema.controllers {
            controllers.push((r.u32()?, r.f32()?));
        }
        let mut devices = Vec::with_capacity(schema.devices);
        for _ in 0..schema.devices {
            let id = r.u32()?;
            let kind = r.u8()?;
            devices.push(match kind {
                1 => DeviceSync::One {
                    id,
                    flag: r.bool()?,
                    values: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
                },
                4 => DeviceSync::Four {
                    id,
                    value: r.f32()?,
                },
                5 => DeviceSync::Five {
                    id,
                    flag: r.bool()?,
                    word: r.u32()?,
                    values: [r.f32()?, r.f32()?, r.f32()?],
                },
                kind => DeviceSync::Other { id, kind },
            });
        }
        if body_count > r.remaining() / 33 + 1 {
            return Err(StreamError::Invalid("body count"));
        }
        let mut bodies = Vec::with_capacity(body_count);
        for _ in 0..body_count {
            let id = r.u32()?;
            let awake = r.bool()?;
            let mut body = BodySync {
                id,
                awake,
                position: [r.f32()?, r.f32()?, r.f32()?],
                rotation: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
                ..BodySync::default()
            };
            if awake {
                body.linear = [r.f32()?, r.f32()?, r.f32()?];
                body.angular = [r.f32()?, r.f32()?, r.f32()?];
            }
            bodies.push(body);
        }
        Ok(SyncState {
            clock,
            clock2,
            partial,
            robots,
            controllers,
            devices,
            bodies,
        })
    }
}

/// `giSyncMessage` on the wire: the snapshot size and the zlib stream.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncMessage {
    /// The snapshot's byte count. A word: the reader (`0x4d25d0`) takes it with
    /// the word reader `0x58a1d0`, and the original sent `11 00 00 00` for its
    /// 17-byte snapshots (live, docs/38 §14), not the float 17.0.
    pub raw_size: u32,
    /// The zlib stream.
    pub packed: Vec<u8>,
}

/// Largest snapshot a reader inflates (a guard against a hostile size).
pub const MAX_SNAPSHOT: usize = 1 << 20;

impl SyncMessage {
    pub fn pack(state: &SyncState) -> SyncMessage {
        let raw = state.to_bytes();
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&raw).expect("writing to a Vec");
        SyncMessage {
            raw_size: raw.len() as u32,
            packed: z.finish().expect("writing to a Vec"),
        }
    }

    pub fn unpack(&self, schema: &SyncSchema) -> Result<SyncState, StreamError> {
        let want = self.raw_size as usize;
        if want > MAX_SNAPSHOT {
            return Err(StreamError::Invalid("snapshot size"));
        }
        let mut raw = Vec::with_capacity(want);
        flate2::read::ZlibDecoder::new(&self.packed[..])
            .take(MAX_SNAPSHOT as u64)
            .read_to_end(&mut raw)
            .map_err(|_| StreamError::Invalid("zlib stream"))?;
        if raw.len() != want {
            return Err(StreamError::Invalid("snapshot size does not match"));
        }
        SyncState::from_bytes(&raw, schema)
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u32(self.raw_size)
            .u32(self.packed.len() as u32)
            .raw(&self.packed);
    }

    pub(crate) fn read(r: &mut Reader) -> Result<SyncMessage, StreamError> {
        let raw_size = r.u32()?;
        let n = r.u32()? as usize;
        Ok(SyncMessage {
            raw_size,
            packed: r.raw(n)?.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;

    fn sample() -> SyncState {
        SyncState {
            clock: 1234,
            clock2: 100,
            partial: true,
            robots: vec![
                RobotSync {
                    value: 1.0,
                    components: vec![0.5, 0.25],
                },
                RobotSync {
                    value: 2.0,
                    components: vec![],
                },
            ],
            controllers: vec![(7, 0.5)],
            devices: vec![
                DeviceSync::One {
                    id: 1,
                    flag: true,
                    values: [1.0, 2.0, 3.0, 4.0],
                },
                DeviceSync::Four { id: 2, value: 9.0 },
                DeviceSync::Five {
                    id: 3,
                    flag: false,
                    word: 5,
                    values: [0.0, 1.0, 2.0],
                },
            ],
            bodies: vec![
                BodySync {
                    id: 10,
                    awake: true,
                    position: [1.0, 2.0, 3.0],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                    linear: [0.1, 0.2, 0.3],
                    angular: [0.0, 1.0, 0.0],
                },
                BodySync {
                    id: 11,
                    awake: false,
                    position: [4.0, 5.0, 6.0],
                    rotation: [0.0, 0.0, 1.0, 0.0],
                    ..BodySync::default()
                },
            ],
        }
    }

    /// The writer's own size bookkeeping (`0x4d2b60`): 17 header bytes, 4 per
    /// robot plus 4 per component, 8 per controller, 22/9/22 per device kind
    /// 1/4/5, and 33 per body plus 24 more when it is awake (57 in full mode).
    #[test]
    fn the_snapshot_size_is_the_writers_running_count() {
        let state = sample();
        let bytes = state.to_bytes();
        let want = 17 + (4 + 8) + 4 + 8 + (22 + 9 + 22) + (33 + 24) + 33;
        assert_eq!(bytes.len(), want);
        let full = SyncState {
            partial: false,
            ..state.clone()
        };
        assert_eq!(
            full.to_bytes().len(),
            want + 24,
            "full mode: every body 57 bytes"
        );
    }

    #[test]
    fn a_sync_message_round_trips_through_zlib() {
        let state = sample();
        let msg = SyncMessage::pack(&state);
        assert_eq!(msg.raw_size, state.to_bytes().len() as u32);
        let bytes = Message::Sync(msg.clone()).encode();
        assert_eq!(&bytes[..4], &0x74u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &msg.raw_size.to_le_bytes());
        assert_eq!(&bytes[8..12], &(msg.packed.len() as u32).to_le_bytes());
        assert_eq!(
            &bytes[12..14],
            &[0x78, 0x9c],
            "a zlib header, default level"
        );
        let Message::Sync(back) = Message::decode(&bytes).unwrap() else {
            panic!("not a sync");
        };
        assert_eq!(back.unpack(&state.schema()).unwrap(), state);
    }

    #[test]
    fn a_wrong_schema_is_refused() {
        let state = sample();
        let msg = SyncMessage::pack(&state);
        let wrong = SyncSchema {
            robot_components: vec![1],
            ..state.schema()
        };
        assert!(msg.unpack(&wrong).is_err());
    }

    #[test]
    fn the_speed_table_is_the_dropdowns() {
        assert_eq!(ConnectionSpeed::Lan.interval(), 0.01875);
        assert_eq!(ConnectionSpeed::T1.interval(), 1.0 / 32.0);
        assert_eq!(ConnectionSpeed::Dsl256.interval(), 1.0 / 16.0);
        assert_eq!(ConnectionSpeed::Dsl128.interval(), 1.0 / 8.0);
        assert_eq!(ConnectionSpeed::Lan.budget(), 32746.0);
        assert_eq!(ConnectionSpeed::Dsl128.budget(), 2000.0);
        assert_eq!(
            ConnectionSpeed::from_label("Cable (384K/s)"),
            Some(ConnectionSpeed::Cable)
        );
        let mut pacer = Pacer::new(ConnectionSpeed::Dsl128);
        let due = (0..60).filter(|_| pacer.tick(1.0 / 60.0)).count();
        // 0.125 s at 60 Hz: due on the 8th frame, the accumulator zeroed (no
        // carry), so every 8 frames.
        assert_eq!(due, 7);
    }
}
