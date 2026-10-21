//! The game's network messages: every `giNetMessage` subclass the exe sends,
//! with its type word and field layout.
//!
//! A message is `[u32 type][fields...]` in the [`crate::stream`] layout. The
//! type is the constructor's argument to `FUN_00620d20` (`giNetMessage+4`); the
//! fields are what the class's vtable slot 3 writes (slot 2 reads them back in
//! the same order). Each variant names its class, its RTTI vtable and the
//! read/write pair.
//!
//! Types below `0x12c` belong to the engine's `giNetwork` layer (chat text,
//! lobby slot info, remote input, the state sync); `0x12d` and up are Robot
//! Arena 2's own (`RA2NetInterface`, dispatcher `FUN_004ce940`). Some ids in the
//! engine range never travel: the DirectPlay callback (`FUN_00623720`) turns
//! player, connect and enumeration notifications into local events with the
//! same numbering (`0x78` is `giAddHostMessage`, built from an enumeration
//! response in the callback's `0xffff000b` case). Those are listed in [`id`] but have no variant.
//!
//! Field names follow the constructors' parameters where a caller makes the
//! meaning plain; where it does not, the docs (docs/38 §4) mark the name as
//! inferred.

use crate::stream::{Reader, StreamError, Writer};
use crate::sync::SyncMessage;

/// Message type words.
pub mod id {
    /// `giNetwork` chat text (class vtable `0x6b17a8`, ctor `0x6215d0`).
    pub const TEXT: u32 = 0x65;
    /// Local: a player joined (DirectPlay `CREATE_PLAYER`); dispatcher calls the
    /// interface's `+0x30`.
    pub const PLAYER_CREATED: u32 = 0x6e;
    /// Local: a player left (`DESTROY_PLAYER`); dispatcher calls `+0x0c`.
    pub const PLAYER_DESTROYED: u32 = 0x6f;
    /// `giGroupInfoMessage`: one lobby slot.
    pub const GROUP_INFO: u32 = 0x71;
    /// `giRemoteInputMessage`: one `Bot::Input` from a remote human.
    pub const REMOTE_INPUT: u32 = 0x73;
    /// `giSyncMessage`: the host's world snapshot.
    pub const SYNC: u32 = 0x74;
    /// Local: the session ended (`TERMINATE_SESSION`).
    pub const SESSION_TERMINATED: u32 = 0x75;
    /// Text of the second text class (vtable `0x6b17b8`, ctor `0x6218e0`); same
    /// layout as [`TEXT`], the dispatcher routes it to the system line.
    pub const SYSTEM_TEXT: u32 = 0x76;
    /// Local: `giAddHostMessage`, one server-list row from an enumeration
    /// response (`0x4d3b40`).
    pub const ADD_HOST: u32 = 0x78;
    /// Local: the connect finished (refusal box "Connection was refused by
    /// server." at `0x4cf2ef` when it failed).
    pub const CONNECT_COMPLETE: u32 = 0x79;
    /// Local: the connect failed with a reason text (`0x621b20`).
    pub const CONNECT_FAILED: u32 = 0x7a;
    pub const ADD_ARENA_BOT: u32 = 0x12d;
    pub const PREPARE_MATCH: u32 = 0x12e;
    pub const PLAYER_READY: u32 = 0x12f;
    pub const BEGIN_MATCH: u32 = 0x130;
    pub const END_MATCH: u32 = 0x131;
    pub const PLAYER_LOADING: u32 = 0x132;
    pub const ARENA_PREVIEW: u32 = 0x133;
    pub const CHANGE_GAME_TYPE: u32 = 0x134;
    pub const BOT_DAMAGE: u32 = 0x135;
    pub const CHANGE_WEIGHT_CLASS: u32 = 0x136;
    pub const MATCH_WIN: u32 = 0x137;
    pub const TIME_SYNC: u32 = 0x138;
    pub const ADD_POINTS: u32 = 0x139;
    pub const PLAYER_OUT: u32 = 0x13a;
    pub const BOT_IMMOBILE: u32 = 0x13b;
    pub const START_COUNT_DOWN: u32 = 0x13c;
    pub const CLOSE_RESULTS_WINDOW: u32 = 0x13e;
    pub const PEER_DATA_LOADED: u32 = 0x13f;
    pub const FRACTURE: u32 = 0x140;
    pub const CRITICAL_HIT: u32 = 0x141;
}

/// How a message is handed to DirectPlay: the `SendTo` flags and timeout each
/// class's send slot passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    /// `DPNSEND_GUARANTEED` (`0x08`).
    pub guaranteed: bool,
    /// `DPNSEND_NONSEQUENTIAL` (`0x10`).
    pub nonsequential: bool,
    /// The send timeout in ms (0: none).
    pub timeout_ms: u32,
}

impl Delivery {
    /// `FUN_00620ee0`, the base send: `0x20` (`NOLOOPBACK`), or `0x28` with
    /// `GUARANTEED` while `giNetwork`'s guarantee switch (`+4` of the object
    /// `FUN_00625c90` returns) is on. Patch 1.20 made the host's messages
    /// non-guaranteed ("changed to synced, non-guaranteed", ReadMe); the switch
    /// is what that toggles.
    pub const BASE: Delivery = Delivery {
        guaranteed: false,
        nonsequential: false,
        timeout_ms: 0,
    };
    /// `0x4d21b0`: `0x38` (`NOLOOPBACK|NONSEQUENTIAL|GUARANTEED`), no timeout —
    /// `giCloseResultsWindowMessage` and `giAddArenaBotMessage`.
    pub const GUARANTEED: Delivery = Delivery {
        guaranteed: true,
        nonsequential: true,
        timeout_ms: 0,
    };
}

/// One `Bot::Input` (RTTI vtable `0x6a5828`, copy ctor `0x445b30`): a key of
/// three words that a control binding is matched on (`0x508ca0` compares
/// `+4`, `+8`, `+0xc`) and the value delivered to it (`+0x10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BotInput {
    pub key: [u32; 3],
    pub value: u32,
}

/// `giGroupInfoMessage` (`0x71`, vtable `0x6aa77c`, read `0x4d17a0`, write
/// `0x4d18b0`): one slot of the pre-match lobby.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupInfo {
    /// `+0x1c`: the player's DirectPlay id (the prematch window's `FUN_0047d120`
    /// matches its rows on it).
    pub player: u32,
    /// `+0x20`.
    pub group: u32,
    /// `+0x24`: the slot state (`FUN_0047d120` treats 1 and 3 as "has a
    /// robot", 4 as "left").
    pub state: u32,
    /// `+0x28`, `+0x38`, `+0x48`, `+0x58`: the slot's four strings (player,
    /// team, robot, robot file — inferred).
    pub strings: [String; 4],
    /// `+0x68`.
    pub flag: bool,
    /// `+0x6c`.
    pub value: u32,
    /// `+0x69` and `+0x70`: the robot design, written by the design object's
    /// own `+0x4c` writer and read by `FUN_005b52d0`. It is not length
    /// prefixed: it runs to the end of the message.
    pub robot: Option<Vec<u8>>,
}

/// `giBotDamageMessage` (`0x135`, vtable `0x6aa72c`, read `0x4d0fb0`, write
/// `0x4d1050`, ctor `0x4d0f10`). The two vectors are 16-byte members of which
/// the three components travel.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BotDamage {
    /// `+0x1c`, `+0x24`, `+0x20`, `+0x28`, `+0x2c` in wire order.
    pub words: [u32; 5],
    /// `+0x30..+0x38`.
    pub a: [f32; 3],
    /// `+0x40..+0x48`.
    pub b: [f32; 3],
    /// `+0x50`.
    pub last: u32,
}

/// `giFractureMessage` (`0x140`, vtable `0x6aa740`, read `0x4d1240`, write
/// `0x4d13d0`): a robot's broken-off pieces, fourteen words each.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fracture {
    pub robot: u32,
    pub component: u32,
    pub pieces: Vec<[u32; 14]>,
}

/// `giAddPointsMessage` (`0x139`, ctor `0x4d3620`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AddPoints {
    pub player: u32,
    pub points: u32,
    /// `+0x24..+0x2c`.
    pub a: [f32; 3],
    /// `+0x34..+0x3c`.
    pub b: [f32; 3],
    /// `+0x44`, `+0x45`.
    pub flags: [bool; 2],
}

/// `giBotImmobileMessage` (`0x13b`, ctor `0x4d3890`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BotImmobile {
    pub robot: u32,
    /// `+0x20..+0x28`.
    pub at: [f32; 3],
    /// `+0x30`.
    pub immobile: bool,
    /// `+0x34`.
    pub value: u32,
}

/// Every message that travels.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// `0x65`: chat text (`0x621bf0` writes the string; sent with flags 0).
    Text(String),
    /// `0x76`: the second text class.
    SystemText(String),
    /// `0x71`.
    GroupInfo(GroupInfo),
    /// `0x73` (vtable `0x6aa7f4`, write `0x4d2400` -> `0x4819a0`, read `0x4d23d0`
    /// -> `0x481a10`): sent by `HumanLocal` (`0x4b2dc0` -> `0x4cff10`) with
    /// flags `0x20` and a 5000 ms timeout (`0x4d2430`).
    RemoteInput { robot: u32, input: BotInput },
    /// `0x74`.
    Sync(SyncMessage),
    /// `0x12d` (vtable `0x6aa7e0`, read `0x4d2050`, write `0x4d2100`): three
    /// words, then (when the sender has one) an AI robot's design to the end.
    AddArenaBot { words: [u32; 3], design: Vec<u8> },
    /// `0x12e` (read `0x4d1f50`, write `0x4d1f80`): a flag and a string.
    PrepareMatch { flag: bool, arena: String },
    /// `0x12f` (read `0x4d2220`, write `0x4d2270`): the player and a ready word
    /// (non-zero is ready). Flags `0x20`, 1000 ms (`0x4d22b0`).
    PlayerReady { player: u32, ready: bool },
    /// `0x130`, no fields.
    BeginMatch,
    /// `0x131`, no fields (ctor `0x4924f0`).
    EndMatch,
    /// `0x132`.
    PlayerLoading { player: u32 },
    /// `0x133`: the arena the host is showing.
    ArenaPreview(String),
    /// `0x134`.
    ChangeGameType(String),
    /// `0x135`.
    BotDamage(BotDamage),
    /// `0x136`.
    ChangeWeightClass(String),
    /// `0x137` (vtable `0x6a5830`): the two winner ids of `FUN_004172d0`.
    MatchWin { first: u32, second: u32 },
    /// `0x138` (read `0x4d3550`, write `0x4d3590`; built at `0x413300`).
    TimeSync { word: u32, times: [u32; 4] },
    /// `0x139`.
    AddPoints(AddPoints),
    /// `0x13a`.
    PlayerOut { player: u32, reason: u32 },
    /// `0x13b`.
    BotImmobile(BotImmobile),
    /// `0x13c`, no fields.
    StartCountDown,
    /// `0x13e`, no fields (ctor `0x487f40`); guaranteed.
    CloseResultsWindow,
    /// `0x13f` (write `0x4d3a90`): the sender's local player word.
    PeerDataLoaded { player: u32 },
    /// `0x140`.
    Fracture(Fracture),
    /// `0x141`.
    CriticalHit { robot: u32 },
}

impl Message {
    /// The type word.
    pub fn id(&self) -> u32 {
        use id::*;
        match self {
            Message::Text(_) => TEXT,
            Message::SystemText(_) => SYSTEM_TEXT,
            Message::GroupInfo(_) => GROUP_INFO,
            Message::RemoteInput { .. } => REMOTE_INPUT,
            Message::Sync(_) => SYNC,
            Message::AddArenaBot { .. } => ADD_ARENA_BOT,
            Message::PrepareMatch { .. } => PREPARE_MATCH,
            Message::PlayerReady { .. } => PLAYER_READY,
            Message::BeginMatch => BEGIN_MATCH,
            Message::EndMatch => END_MATCH,
            Message::PlayerLoading { .. } => PLAYER_LOADING,
            Message::ArenaPreview(_) => ARENA_PREVIEW,
            Message::ChangeGameType(_) => CHANGE_GAME_TYPE,
            Message::BotDamage(_) => BOT_DAMAGE,
            Message::ChangeWeightClass(_) => CHANGE_WEIGHT_CLASS,
            Message::MatchWin { .. } => MATCH_WIN,
            Message::TimeSync { .. } => TIME_SYNC,
            Message::AddPoints(_) => ADD_POINTS,
            Message::PlayerOut { .. } => PLAYER_OUT,
            Message::BotImmobile(_) => BOT_IMMOBILE,
            Message::StartCountDown => START_COUNT_DOWN,
            Message::CloseResultsWindow => CLOSE_RESULTS_WINDOW,
            Message::PeerDataLoaded { .. } => PEER_DATA_LOADED,
            Message::Fracture(_) => FRACTURE,
            Message::CriticalHit { .. } => CRITICAL_HIT,
        }
    }

    /// The flags and timeout the class's send slot uses.
    pub fn delivery(&self) -> Delivery {
        match self {
            Message::CloseResultsWindow | Message::AddArenaBot { .. } => Delivery::GUARANTEED,
            Message::RemoteInput { .. } => Delivery {
                timeout_ms: 5000,
                ..Delivery::BASE
            },
            Message::PlayerReady { .. } | Message::Sync(_) => Delivery {
                timeout_ms: 1000,
                ..Delivery::BASE
            },
            _ => Delivery::BASE,
        }
    }

    /// The message's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(self.id());
        match self {
            Message::Text(s) | Message::SystemText(s) => {
                w.string(s);
            }
            Message::GroupInfo(g) => {
                w.u32(g.player).u32(g.group).u32(g.state);
                for s in &g.strings {
                    w.string(s);
                }
                w.bool(g.flag).u32(g.value).bool(g.robot.is_some());
                if let Some(robot) = &g.robot {
                    w.raw(robot);
                }
            }
            Message::RemoteInput { robot, input } => {
                // 0x4819a0 writes +4, +8, +0xc, +0x10 (the reader skips +8 for
                // a peer older than 1.11, 0x481a34).
                w.u32(*robot)
                    .u32(input.key[0])
                    .u32(input.key[1])
                    .u32(input.key[2])
                    .u32(input.value);
            }
            Message::Sync(sync) => sync.write(&mut w),
            Message::AddArenaBot { words, design } => {
                w.u32(words[0]).u32(words[1]).u32(words[2]).raw(design);
            }
            Message::PrepareMatch { flag, arena } => {
                w.bool(*flag).string(arena);
            }
            Message::PlayerReady { player, ready } => {
                w.u32(*player).u32(*ready as u32);
            }
            Message::BeginMatch
            | Message::EndMatch
            | Message::StartCountDown
            | Message::CloseResultsWindow => {}
            Message::PlayerLoading { player } | Message::PeerDataLoaded { player } => {
                w.u32(*player);
            }
            Message::CriticalHit { robot } => {
                w.u32(*robot);
            }
            Message::ArenaPreview(s)
            | Message::ChangeGameType(s)
            | Message::ChangeWeightClass(s) => {
                w.string(s);
            }
            Message::BotDamage(d) => {
                for v in d.words {
                    w.u32(v);
                }
                for v in d.a.iter().chain(&d.b) {
                    w.f32(*v);
                }
                w.u32(d.last);
            }
            Message::MatchWin { first, second } => {
                w.u32(*first).u32(*second);
            }
            Message::TimeSync { word, times } => {
                w.u32(*word);
                for t in times {
                    w.u32(*t);
                }
            }
            Message::AddPoints(p) => {
                w.u32(p.player).u32(p.points);
                for v in p.a.iter().chain(&p.b) {
                    w.f32(*v);
                }
                w.bool(p.flags[0]).bool(p.flags[1]);
            }
            Message::PlayerOut { player, reason } => {
                w.u32(*player).u32(*reason);
            }
            Message::BotImmobile(b) => {
                w.u32(b.robot);
                for v in b.at {
                    w.f32(v);
                }
                w.bool(b.immobile).u32(b.value);
            }
            Message::Fracture(f) => {
                w.u32(f.robot).u32(f.component).u32(f.pieces.len() as u32);
                for piece in &f.pieces {
                    for v in piece {
                        w.u32(*v);
                    }
                }
            }
        }
        w.into_bytes()
    }

    /// Read a message. Unknown type words are an error (the original's
    /// dispatcher ignores them; the caller decides).
    pub fn decode(bytes: &[u8]) -> Result<Message, DecodeError> {
        let mut r = Reader::new(bytes);
        let ty = r.u32()?;
        let words3 = |r: &mut Reader| -> Result<[u32; 3], StreamError> {
            Ok([r.u32()?, r.u32()?, r.u32()?])
        };
        let vec3 = |r: &mut Reader| -> Result<[f32; 3], StreamError> {
            Ok([r.f32()?, r.f32()?, r.f32()?])
        };
        use id::*;
        let msg = match ty {
            TEXT => Message::Text(r.string()?),
            SYSTEM_TEXT => Message::SystemText(r.string()?),
            GROUP_INFO => {
                let player = r.u32()?;
                let group = r.u32()?;
                let state = r.u32()?;
                let strings = [r.string()?, r.string()?, r.string()?, r.string()?];
                let flag = r.bool()?;
                let value = r.u32()?;
                let robot = if r.bool()? {
                    Some(r.rest().to_vec())
                } else {
                    None
                };
                Message::GroupInfo(GroupInfo {
                    player,
                    group,
                    state,
                    strings,
                    flag,
                    value,
                    robot,
                })
            }
            REMOTE_INPUT => {
                let robot = r.u32()?;
                let key = words3(&mut r)?;
                let value = r.u32()?;
                Message::RemoteInput {
                    robot,
                    input: BotInput { key, value },
                }
            }
            SYNC => Message::Sync(SyncMessage::read(&mut r)?),
            ADD_ARENA_BOT => {
                let words = words3(&mut r)?;
                Message::AddArenaBot {
                    words,
                    design: r.rest().to_vec(),
                }
            }
            PREPARE_MATCH => Message::PrepareMatch {
                flag: r.bool()?,
                arena: r.string()?,
            },
            PLAYER_READY => Message::PlayerReady {
                player: r.u32()?,
                ready: r.u32()? != 0,
            },
            BEGIN_MATCH => Message::BeginMatch,
            END_MATCH => Message::EndMatch,
            PLAYER_LOADING => Message::PlayerLoading { player: r.u32()? },
            ARENA_PREVIEW => Message::ArenaPreview(r.string()?),
            CHANGE_GAME_TYPE => Message::ChangeGameType(r.string()?),
            BOT_DAMAGE => {
                let words = [r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?];
                let a = vec3(&mut r)?;
                let b = vec3(&mut r)?;
                Message::BotDamage(BotDamage {
                    words,
                    a,
                    b,
                    last: r.u32()?,
                })
            }
            CHANGE_WEIGHT_CLASS => Message::ChangeWeightClass(r.string()?),
            MATCH_WIN => Message::MatchWin {
                first: r.u32()?,
                second: r.u32()?,
            },
            TIME_SYNC => Message::TimeSync {
                word: r.u32()?,
                times: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
            },
            ADD_POINTS => {
                let player = r.u32()?;
                let points = r.u32()?;
                let a = vec3(&mut r)?;
                let b = vec3(&mut r)?;
                Message::AddPoints(AddPoints {
                    player,
                    points,
                    a,
                    b,
                    flags: [r.bool()?, r.bool()?],
                })
            }
            PLAYER_OUT => Message::PlayerOut {
                player: r.u32()?,
                reason: r.u32()?,
            },
            BOT_IMMOBILE => {
                let robot = r.u32()?;
                let at = vec3(&mut r)?;
                Message::BotImmobile(BotImmobile {
                    robot,
                    at,
                    immobile: r.bool()?,
                    value: r.u32()?,
                })
            }
            START_COUNT_DOWN => Message::StartCountDown,
            CLOSE_RESULTS_WINDOW => Message::CloseResultsWindow,
            PEER_DATA_LOADED => Message::PeerDataLoaded { player: r.u32()? },
            FRACTURE => {
                let robot = r.u32()?;
                let component = r.u32()?;
                let n = r.u32()? as usize;
                if n > r.remaining() / 56 {
                    return Err(DecodeError::Stream(StreamError::Invalid("fracture count")));
                }
                let mut pieces = Vec::with_capacity(n);
                for _ in 0..n {
                    let mut piece = [0u32; 14];
                    for v in &mut piece {
                        *v = r.u32()?;
                    }
                    pieces.push(piece);
                }
                Message::Fracture(Fracture {
                    robot,
                    component,
                    pieces,
                })
            }
            CRITICAL_HIT => Message::CriticalHit { robot: r.u32()? },
            other => return Err(DecodeError::Unknown(other)),
        };
        Ok(msg)
    }
}

/// Why a datagram did not decode.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    Unknown(u32),
    Stream(StreamError),
}

impl From<StreamError> for DecodeError {
    fn from(e: StreamError) -> Self {
        DecodeError::Stream(e)
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Unknown(t) => write!(f, "unknown message type {t:#x}"),
            DecodeError::Stream(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn round_trip(msg: Message) -> Vec<u8> {
        let bytes = msg.encode();
        assert_eq!(Message::decode(&bytes).unwrap(), msg, "{msg:?}");
        bytes
    }

    #[test]
    fn empty_messages_are_their_type_word() {
        assert_eq!(round_trip(Message::BeginMatch), le(&[0x130]));
        assert_eq!(round_trip(Message::EndMatch), le(&[0x131]));
        assert_eq!(round_trip(Message::StartCountDown), le(&[0x13c]));
        assert_eq!(round_trip(Message::CloseResultsWindow), le(&[0x13e]));
    }

    #[test]
    fn text_is_a_length_prefixed_string() {
        let bytes = round_trip(Message::Text("gg".into()));
        assert_eq!(bytes, [0x65, 0, 0, 0, 2, 0, 0, 0, b'g', b'g']);
        let bytes = round_trip(Message::SystemText("x".into()));
        assert_eq!(&bytes[..4], &[0x76, 0, 0, 0]);
    }

    #[test]
    fn ready_and_win_layouts() {
        assert_eq!(
            round_trip(Message::PlayerReady {
                player: 7,
                ready: true
            }),
            le(&[0x12f, 7, 1])
        );
        assert_eq!(
            round_trip(Message::MatchWin {
                first: 2,
                second: u32::MAX
            }),
            le(&[0x137, 2, u32::MAX])
        );
        assert_eq!(
            round_trip(Message::PlayerOut {
                player: 3,
                reason: 1
            }),
            le(&[0x13a, 3, 1])
        );
        assert_eq!(
            round_trip(Message::TimeSync {
                word: 1,
                times: [2, 3, 4, 5]
            }),
            le(&[0x138, 1, 2, 3, 4, 5])
        );
        assert_eq!(
            round_trip(Message::CriticalHit { robot: 9 }),
            le(&[0x141, 9])
        );
        assert_eq!(
            round_trip(Message::PlayerLoading { player: 4 }),
            le(&[0x132, 4])
        );
        assert_eq!(
            round_trip(Message::PeerDataLoaded { player: 4 }),
            le(&[0x13f, 4])
        );
    }

    #[test]
    fn remote_input_is_robot_then_the_bot_input_words() {
        let bytes = round_trip(Message::RemoteInput {
            robot: 1,
            input: BotInput {
                key: [10, 11, 12],
                value: 100,
            },
        });
        assert_eq!(bytes, le(&[0x73, 1, 10, 11, 12, 100]));
    }

    #[test]
    fn group_info_ends_with_the_unframed_robot_stream() {
        let slot = GroupInfo {
            player: 0x1234,
            group: 1,
            state: 3,
            strings: [
                "Bruno".into(),
                "Team".into(),
                "Bot".into(),
                "bot.bot".into(),
            ],
            flag: true,
            value: 2,
            robot: Some(vec![0xde, 0xad]),
        };
        let bytes = round_trip(Message::GroupInfo(slot.clone()));
        let mut want = le(&[0x71, 0x1234, 1, 3]);
        for s in &slot.strings {
            want.extend((s.len() as u32).to_le_bytes());
            want.extend(s.as_bytes());
        }
        want.extend([1]);
        want.extend(2u32.to_le_bytes());
        want.extend([1, 0xde, 0xad]);
        assert_eq!(bytes, want);
        let none = GroupInfo {
            robot: None,
            ..slot
        };
        let bytes = round_trip(Message::GroupInfo(none));
        assert_eq!(
            bytes.last(),
            Some(&0),
            "no robot: the has-robot bool is the last byte"
        );
    }

    #[test]
    fn damage_points_immobile_and_fracture_layouts() {
        let damage = BotDamage {
            words: [1, 2, 3, 4, 5],
            a: [1.0, 2.0, 3.0],
            b: [4.0, 5.0, 6.0],
            last: 6,
        };
        let bytes = round_trip(Message::BotDamage(damage));
        assert_eq!(bytes.len(), 4 + 12 * 4);
        assert_eq!(&bytes[24..28], &1.0f32.to_le_bytes());

        let points = AddPoints {
            player: 1,
            points: 50,
            a: [0.0; 3],
            b: [1.0; 3],
            flags: [true, false],
        };
        assert_eq!(round_trip(Message::AddPoints(points)).len(), 4 + 8 * 4 + 2);

        let immobile = BotImmobile {
            robot: 2,
            at: [1.0, 0.0, -1.0],
            immobile: true,
            value: 7,
        };
        assert_eq!(
            round_trip(Message::BotImmobile(immobile)).len(),
            4 + 4 + 12 + 1 + 4
        );

        let fracture = Fracture {
            robot: 1,
            component: 4,
            pieces: vec![[9; 14], [8; 14]],
        };
        let bytes = round_trip(Message::Fracture(fracture));
        assert_eq!(bytes.len(), 16 + 2 * 56);
        assert_eq!(&bytes[12..16], &2u32.to_le_bytes(), "the piece count");
    }

    #[test]
    fn strings_and_arena_bots() {
        round_trip(Message::ArenaPreview("Arenas/box.py".into()));
        round_trip(Message::ChangeGameType("DEATHMATCH".into()));
        round_trip(Message::ChangeWeightClass("Heavyweight".into()));
        let bytes = round_trip(Message::PrepareMatch {
            flag: true,
            arena: "box".into(),
        });
        assert_eq!(&bytes[4..6], &[1, 3]);
        let bytes = round_trip(Message::AddArenaBot {
            words: [1, 2, 3],
            design: vec![5, 6],
        });
        assert_eq!(bytes, [le(&[0x12d, 1, 2, 3]), vec![5, 6]].concat());
    }

    #[test]
    fn unknown_types_and_short_messages_are_errors() {
        assert_eq!(
            Message::decode(&le(&[0x999])),
            Err(DecodeError::Unknown(0x999))
        );
        assert!(matches!(
            Message::decode(&le(&[0x12f, 1])),
            Err(DecodeError::Stream(StreamError::Short { .. }))
        ));
    }

    #[test]
    fn delivery_follows_each_send_slot() {
        assert!(Message::CloseResultsWindow.delivery().guaranteed);
        assert_eq!(
            Message::RemoteInput {
                robot: 0,
                input: BotInput::default()
            }
            .delivery()
            .timeout_ms,
            5000
        );
        assert!(!Message::Text("x".into()).delivery().guaranteed);
    }
}
