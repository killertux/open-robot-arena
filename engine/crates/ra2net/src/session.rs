//! A multiplayer session on one UDP socket: the host or a joining player.
//!
//! What it mirrors in the original:
//!
//! * The host (`Create Game`, `FUN_0049a780` -> `FUN_006268b0`) hosts an
//!   `IDirectPlay8Peer` session on its game port ([`crate::dpn::DEFAULT_PORT`])
//!   with an application description: [`crate::dpn::APP_GUID`], a session name,
//!   a player limit and an optional password. It answers enumeration queries
//!   on that port (and, through DPNSVR, on [`crate::dpn::ENUM_PORT`]).
//! * A joiner (`Join` or `IP Address`, `FUN_00626260`) connects with the
//!   connect data `"1.20\0"`. The host's `INDICATE_CONNECT` refuses a
//!   mismatch with reply word 900 and, while the lobby does not take players
//!   (the interface's `+0x38` is false), with 901 ([`crate::dpn`]).
//! * Each side learns the players from DirectPlay's create/destroy-player
//!   messages and exchanges the game's [`Message`]s with `SendTo`.
//! * The joiner gives up after ten one-second ticks ("Connecting to Server
//!   %d", timer `0x6267a0` counting `DAT_00709860` = 10 down).
//!
//! What is ours: DirectPlay 8's peer topology is a full mesh; here every
//! joiner links to the host only and the host relays a message a joiner
//! addresses to someone else. The session-core messages (connect info,
//! accept/refuse, player joined/left) are a small format of our own carried in
//! `USER_1` frames; DirectPlay's own core messages are not reproduced.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::dpn::{self, EnumQuery, EnumResponse, Guid, RaHostData};
use crate::frame::Frame;
use crate::link::{CloseReason, Link, LinkConfig, LinkState};
use crate::message::{DecodeError, Message};

/// A player id (DirectPlay's `DPNID`).
pub type PlayerId = u32;

/// The host's own id. DirectPlay ids are opaque words; ours count up from here.
pub const HOST_ID: PlayerId = 0x0001_0001;

/// Who a message goes to (`SendTo`'s `dpnid`; `0` is `DPNID_ALL_PLAYERS_GROUP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    All,
    Player(PlayerId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    pub id: PlayerId,
    pub name: String,
}

/// Why a connect was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Reply word 900.
    IncompatibleVersion,
    /// Reply word 901.
    InProgress,
    SessionFull,
    WrongPassword,
}

impl Refusal {
    /// The text the original shows (`FUN_00623720`, `CONNECT_COMPLETE`), or
    /// the generic refusal box (`0x4cf2ef`).
    pub fn text(self) -> &'static str {
        match self {
            Refusal::IncompatibleVersion => "Connection refused.  Incompatible versions.",
            Refusal::InProgress => "Connection refused.  Game already in progress.",
            Refusal::SessionFull | Refusal::WrongPassword => {
                "Connection was refused by server.\nThe game may be full or has already started."
            }
        }
    }

    fn code(self) -> u8 {
        match self {
            Refusal::IncompatibleVersion => 1,
            Refusal::InProgress => 2,
            Refusal::SessionFull => 3,
            Refusal::WrongPassword => 4,
        }
    }

    fn from_code(code: u8) -> Option<Refusal> {
        Some(match code {
            1 => Refusal::IncompatibleVersion,
            2 => Refusal::InProgress,
            3 => Refusal::SessionFull,
            4 => Refusal::WrongPassword,
            _ => return None,
        })
    }

    /// The four-byte reply data the original attaches (900/901), if any.
    pub fn reply_word(self) -> Option<u32> {
        match self {
            Refusal::IncompatibleVersion => Some(dpn::REFUSE_INCOMPATIBLE),
            Refusal::InProgress => Some(dpn::REFUSE_IN_PROGRESS),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    /// The player closed its session.
    Left,
    /// Nothing heard from it.
    TimedOut,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Joiner: the host accepted us.
    Connected {
        local: PlayerId,
        host: PlayerId,
        players: Vec<PlayerInfo>,
    },
    /// Joiner: the host refused us.
    Refused(Refusal),
    /// Joiner: no answer within the connect timeout.
    ConnectFailed,
    PlayerJoined(PlayerInfo),
    /// A player changed its name (the original's `0x72` message, from
    /// DirectPlay's `DPN_MSGID_PEER_INFO`).
    PlayerRenamed(PlayerInfo),
    PlayerLeft {
        id: PlayerId,
        reason: LeaveReason,
    },
    /// Joiner: the host ended the session or vanished.
    SessionTerminated,
    Message {
        from: PlayerId,
        message: Message,
    },
    /// A game datagram that did not decode.
    BadMessage {
        from: PlayerId,
        error: DecodeError,
    },
    /// Bridge mode ([`Session::set_bridge`]): a game payload, undecoded.
    Raw {
        from: PlayerId,
        data: Vec<u8>,
    },
    /// Bridge mode, host: a player asks to join; answer with [`Session::admit`].
    ConnectRequest {
        token: u32,
        name: String,
        connect_data: Vec<u8>,
        addr: SocketAddr,
    },
    /// Bridge mode, host: an enumeration query; answer with
    /// [`Session::answer_enum`] (or leave it unanswered).
    EnumQuery {
        token: u32,
        from: SocketAddr,
        user_data: Vec<u8>,
    },
}

/// A game payload as an event: decoded, or raw in bridge mode.
fn game_event(raw: bool, from: PlayerId, payload: &[u8]) -> Event {
    if raw {
        return Event::Raw {
            from,
            data: payload.to_vec(),
        };
    }
    match Message::decode(payload) {
        Ok(message) => Event::Message { from, message },
        Err(error) => Event::BadMessage { from, error },
    }
}

/// Bridge mode state: what the host waits for the application to decide.
#[derive(Default)]
struct Bridge {
    next_token: u32,
    /// Connect requests: token -> (address, name).
    admissions: HashMap<u32, (SocketAddr, String)>,
    /// Admitted or refused by the application, applied on the next tick.
    decided: Vec<(SocketAddr, String, Result<(), Refusal>)>,
    /// Enumeration queries: token -> (from, payload, asked at).
    queries: HashMap<u32, (SocketAddr, u16, Instant)>,
}

/// Host settings (`DPN_APPLICATION_DESC` plus the game's connect rules).
#[derive(Debug, Clone)]
pub struct HostConfig {
    pub bind: SocketAddr,
    pub session_name: String,
    pub player_name: String,
    pub max_players: u32,
    pub password: Option<String>,
    pub link: LinkConfig,
}

/// Joiner settings.
#[derive(Debug, Clone)]
pub struct JoinConfig {
    pub bind: SocketAddr,
    pub host: SocketAddr,
    pub player_name: String,
    pub password: Option<String>,
    /// Defaults to [`dpn::CONNECT_DATA`]; tests change it to see the refusal.
    pub connect_data: Vec<u8>,
    pub link: LinkConfig,
    /// The "Connecting to Server" countdown.
    pub connect_timeout: Duration,
}

impl JoinConfig {
    pub fn new(bind: SocketAddr, host: SocketAddr, player_name: &str) -> JoinConfig {
        JoinConfig {
            bind,
            host,
            player_name: player_name.to_string(),
            password: None,
            connect_data: dpn::CONNECT_DATA.to_vec(),
            link: LinkConfig::default(),
            connect_timeout: Duration::from_secs(10),
        }
    }
}

// Session-core message opcodes (ours; carried in USER_1 frames).
const CORE_CONNECT_INFO: u8 = 0xc1;
const CORE_ACCEPT: u8 = 0xc2;
const CORE_REFUSE: u8 = 0xc3;
const CORE_PLAYER_JOINED: u8 = 0xc4;
const CORE_PLAYER_LEFT: u8 = 0xc5;
const CORE_TERMINATE: u8 = 0xc6;
/// Joiner -> host: forward a game message to `to` (0 = everyone else).
const CORE_RELAY: u8 = 0xc7;
/// Host -> joiner: a game message relayed from `from`.
const CORE_FROM: u8 = 0xc8;
/// Joiner -> host: this player's name is now `name` (`SetPeerInfo`).
const CORE_SET_NAME: u8 = 0xc9;
/// Host -> joiners: player `id` is now called `name` (DirectPlay's PEER_INFO).
const CORE_PEER_INFO: u8 = 0xca;

fn put_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    out.extend((b.len().min(u16::MAX as usize) as u16).to_le_bytes());
    out.extend(&b[..b.len().min(u16::MAX as usize)]);
}

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Option<u8> {
        let (&b, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(b)
    }
    fn u16(&mut self) -> Option<u16> {
        let v = u16::from_le_bytes(self.0.get(..2)?.try_into().ok()?);
        self.0 = &self.0[2..];
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.0.get(..4)?.try_into().ok()?);
        self.0 = &self.0[4..];
        Some(v)
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u16()? as usize;
        let v = self.0.get(..n)?;
        self.0 = &self.0[n..];
        Some(v)
    }
    fn str(&mut self) -> Option<String> {
        Some(String::from_utf8_lossy(self.bytes()?).into_owned())
    }
}

// One `Role` per session, never in a collection: the size difference does
// not matter.
#[allow(clippy::large_enum_variant)]
enum Role {
    Host {
        config: HostConfig,
        instance: Guid,
        next_id: PlayerId,
        /// Linked addresses that have not sent their connect info yet, or were
        /// refused (kept until their link drains).
        pending: HashMap<SocketAddr, Link>,
        peers: BTreeMap<PlayerId, (SocketAddr, Link)>,
        accepting: bool,
        host_data: RaHostData,
    },
    Joiner {
        config: JoinConfig,
        link: Link,
        local: Option<PlayerId>,
        host: Option<PlayerId>,
        started: Instant,
        introduced: bool,
        done: bool,
    },
}

/// One end of a session.
pub struct Session {
    socket: UdpSocket,
    role: Role,
    players: BTreeMap<PlayerId, PlayerInfo>,
    events: Vec<Event>,
    buf: Vec<u8>,
    /// `Some` in bridge mode ([`Session::set_bridge`]).
    bridge: Option<Bridge>,
}

fn session_id(now: Instant, addr: SocketAddr) -> u32 {
    // Any value that differs between sessions will do.
    let t = now.elapsed().as_nanos() as u32;
    t ^ (addr.port() as u32) << 16 ^ std::process::id()
}

impl Session {
    /// Host a session.
    pub fn host(config: HostConfig) -> io::Result<Session> {
        let socket = UdpSocket::bind(config.bind)?;
        socket.set_nonblocking(true)?;
        let now = Instant::now();
        let mut instance = [0u8; 16];
        let seed = session_id(now, socket.local_addr()?).to_le_bytes();
        for (i, b) in instance.iter_mut().enumerate() {
            *b = seed[i % 4] ^ (i as u8).wrapping_mul(37);
        }
        let mut players = BTreeMap::new();
        players.insert(
            HOST_ID,
            PlayerInfo {
                id: HOST_ID,
                name: config.player_name.clone(),
            },
        );
        Ok(Session {
            socket,
            role: Role::Host {
                config,
                instance: Guid(instance),
                next_id: HOST_ID + 1,
                pending: HashMap::new(),
                peers: BTreeMap::new(),
                accepting: true,
                host_data: RaHostData::default(),
            },
            players,
            events: Vec::new(),
            buf: vec![0; 65536],
            bridge: None,
        })
    }

    /// Join a session.
    pub fn join(config: JoinConfig) -> io::Result<Session> {
        let socket = UdpSocket::bind(config.bind)?;
        socket.set_nonblocking(true)?;
        let now = Instant::now();
        let link = Link::connect(config.link, session_id(now, socket.local_addr()?), now);
        Ok(Session {
            socket,
            role: Role::Joiner {
                config,
                link,
                local: None,
                host: None,
                started: now,
                introduced: false,
                done: false,
            },
            players: BTreeMap::new(),
            events: Vec::new(),
            buf: vec![0; 65536],
            bridge: None,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn is_host(&self) -> bool {
        matches!(self.role, Role::Host { .. })
    }

    /// Our player id (`None` until a joiner is accepted).
    pub fn local_id(&self) -> Option<PlayerId> {
        match &self.role {
            Role::Host { .. } => Some(HOST_ID),
            Role::Joiner { local, .. } => *local,
        }
    }

    pub fn players(&self) -> Vec<PlayerInfo> {
        self.players.values().cloned().collect()
    }

    /// Host: whether new players are let in (the lobby's `+0x38`); a refused
    /// joiner hears 901.
    pub fn set_accepting(&mut self, on: bool) {
        if let Role::Host { accepting, .. } = &mut self.role {
            *accepting = on;
        }
    }

    /// Host: the eight bytes of the enumeration reply.
    pub fn set_host_data(&mut self, data: RaHostData) {
        if let Role::Host { host_data, .. } = &mut self.role {
            *host_data = data;
        }
    }

    /// Round-trip time to a player (host) or to the host (joiner).
    pub fn rtt(&self, to: PlayerId) -> Option<Duration> {
        match &self.role {
            Role::Host { peers, .. } => peers.get(&to).and_then(|(_, l)| l.rtt),
            Role::Joiner { link, .. } => link.rtt,
        }
    }

    /// Send a game message with the delivery its class uses.
    pub fn send(&mut self, to: Target, message: &Message) {
        let guaranteed = message.delivery().guaranteed;
        self.send_with(to, message, guaranteed);
    }

    /// Send a game message, choosing the guarantee.
    pub fn send_with(&mut self, to: Target, message: &Message, guaranteed: bool) {
        let bytes = message.encode();
        let now = Instant::now();
        self.queue(to, &bytes, guaranteed, now);
        self.flush(now);
    }

    fn queue(&mut self, to: Target, bytes: &[u8], guaranteed: bool, now: Instant) {
        match &mut self.role {
            Role::Host { peers, .. } => {
                for (id, (_, link)) in peers.iter_mut() {
                    if to == Target::All || to == Target::Player(*id) {
                        link.send(bytes, false, guaranteed, now);
                    }
                }
            }
            Role::Joiner { link, host, .. } => match to {
                Target::Player(id) if Some(id) == *host => link.send(bytes, false, guaranteed, now),
                Target::Player(id) => {
                    let mut out = vec![CORE_RELAY];
                    out.extend(id.to_le_bytes());
                    out.extend(bytes);
                    link.send(&out, true, guaranteed, now);
                }
                Target::All => {
                    // To the host directly, and relayed to the others.
                    link.send(bytes, false, guaranteed, now);
                    let mut out = vec![CORE_RELAY];
                    out.extend(0u32.to_le_bytes());
                    out.extend(bytes);
                    link.send(&out, true, guaranteed, now);
                }
            },
        }
    }

    /// Bridge mode: another program (the `dpnet` replacement for the
    /// original, docs/38 §14) makes the application's decisions. Game payloads
    /// arrive as [`Event::Raw`]; the host asks before admitting a player
    /// ([`Event::ConnectRequest`]) and before answering an enumeration
    /// ([`Event::EnumQuery`]).
    pub fn set_bridge(&mut self, on: bool) {
        self.bridge = on.then(Bridge::default);
    }

    /// Send a game payload as it is (bridge mode's `SendTo`).
    pub fn send_raw(&mut self, to: Target, bytes: &[u8], guaranteed: bool) {
        let now = Instant::now();
        self.queue(to, bytes, guaranteed, now);
        self.flush(now);
    }

    /// Bridge mode, host: the application's answer to a
    /// [`Event::ConnectRequest`].
    pub fn admit(&mut self, token: u32, verdict: Result<(), Refusal>) {
        if let Some(b) = &mut self.bridge {
            if let Some((addr, name)) = b.admissions.remove(&token) {
                b.decided.push((addr, name, verdict));
            }
        }
        let now = Instant::now();
        self.tick(now);
        self.flush(now);
    }

    /// Bridge mode, host: answer an [`Event::EnumQuery`] with the
    /// application's reply data.
    pub fn answer_enum(&mut self, token: u32, app_reply: &[u8]) {
        let Some((from, payload, _)) = self.bridge.as_mut().and_then(|b| b.queries.remove(&token))
        else {
            return;
        };
        if let Some(reply) = self.enum_response(payload, app_reply.to_vec()) {
            let _ = self.socket.send_to(&reply.encode(), from);
        }
    }

    fn enum_response(&self, payload: u16, app_reply: Vec<u8>) -> Option<EnumResponse> {
        let Role::Host {
            config, instance, ..
        } = &self.role
        else {
            return None;
        };
        Some(EnumResponse {
            payload,
            flags: if config.password.is_some() {
                dpn::SESSION_REQUIRE_PASSWORD
            } else {
                0
            },
            max_players: config.max_players,
            current_players: self.players.len() as u32,
            instance: *instance,
            app: dpn::APP_GUID,
            session_name: config.session_name.clone(),
            app_reply,
        })
    }

    /// Read the socket, run the timers, send what is due; returns what
    /// happened.
    pub fn poll(&mut self) -> Vec<Event> {
        let now = Instant::now();
        loop {
            match self.socket.recv_from(&mut self.buf) {
                Ok((n, from)) => {
                    let data = self.buf[..n].to_vec();
                    self.on_datagram(&data, from, now);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                // A port-unreachable from an earlier send (Windows reports
                // it on the next receive): ignore.
                Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            }
        }
        self.tick(now);
        self.flush(now);
        std::mem::take(&mut self.events)
    }

    fn on_datagram(&mut self, data: &[u8], from: SocketAddr, now: Instant) {
        if dpn::is_enum_frame(data) {
            self.on_enum(data, from);
            return;
        }
        match &mut self.role {
            Role::Host {
                config,
                pending,
                peers,
                ..
            } => {
                if let Some((_, link)) = peers.values_mut().find(|(a, _)| *a == from) {
                    link.receive(data, now);
                    return;
                }
                if let Some(link) = pending.get_mut(&from) {
                    link.receive(data, now);
                    return;
                }
                if let Some(frame) = Frame::decode(data) {
                    if let Some(link) = Link::accept(config.link, &frame, now) {
                        pending.insert(from, link);
                    }
                }
            }
            Role::Joiner { config, link, .. } => {
                if from == config.host {
                    link.receive(data, now);
                }
            }
        }
    }

    fn on_enum(&mut self, data: &[u8], from: SocketAddr) {
        let Role::Host {
            config,
            instance,
            host_data,
            ..
        } = &self.role
        else {
            return;
        };
        let Some(query) = EnumQuery::decode(data) else {
            return;
        };
        if query.app.is_some_and(|g| g != dpn::APP_GUID) {
            return;
        }
        if let Some(b) = &mut self.bridge {
            let now = Instant::now();
            b.queries
                .retain(|_, (_, _, at)| now.duration_since(*at) < Duration::from_secs(5));
            b.next_token = b.next_token.wrapping_add(1);
            let token = b.next_token;
            b.queries.insert(token, (from, query.payload, now));
            self.events.push(Event::EnumQuery {
                token,
                from,
                user_data: query.user_data,
            });
            return;
        }
        let reply = EnumResponse {
            payload: query.payload,
            flags: if config.password.is_some() {
                dpn::SESSION_REQUIRE_PASSWORD
            } else {
                0
            },
            max_players: config.max_players,
            current_players: self.players.len() as u32,
            instance: *instance,
            app: dpn::APP_GUID,
            session_name: config.session_name.clone(),
            app_reply: host_data.encode(),
        };
        let _ = self.socket.send_to(&reply.encode(), from);
    }

    fn tick(&mut self, now: Instant) {
        // Collect deliveries and state changes from the links.
        match &mut self.role {
            Role::Host { .. } => self.tick_host(now),
            Role::Joiner { .. } => self.tick_joiner(now),
        }
    }

    fn tick_host(&mut self, now: Instant) {
        let Role::Host {
            config,
            next_id,
            pending,
            peers,
            accepting,
            ..
        } = &mut self.role
        else {
            return;
        };
        // Pending links: wait for the connect info.
        let mut admitted = Vec::new();
        let mut gone = Vec::new();
        for (addr, link) in pending.iter_mut() {
            for d in link.take_delivered() {
                if !d.core || d.payload.first() != Some(&CORE_CONNECT_INFO) {
                    continue;
                }
                let mut c = Cursor(&d.payload[1..]);
                let (Some(name), Some(password), Some(data)) =
                    (c.str(), c.str(), c.bytes().map(<[u8]>::to_vec))
                else {
                    continue;
                };
                let full = self.players.len() as u32 + admitted.len() as u32 >= config.max_players;
                let wrong_password = config.password.as_deref().is_some_and(|p| p != password);
                if let (Some(b), false, false) = (&mut self.bridge, full, wrong_password) {
                    // The application decides (its INDICATE_CONNECT).
                    b.next_token = b.next_token.wrapping_add(1);
                    b.admissions.insert(b.next_token, (*addr, name.clone()));
                    self.events.push(Event::ConnectRequest {
                        token: b.next_token,
                        name,
                        connect_data: data,
                        addr: *addr,
                    });
                    continue;
                }
                let refusal = if data != dpn::CONNECT_DATA {
                    Some(Refusal::IncompatibleVersion)
                } else if !*accepting {
                    Some(Refusal::InProgress)
                } else if full {
                    Some(Refusal::SessionFull)
                } else if wrong_password {
                    Some(Refusal::WrongPassword)
                } else {
                    None
                };
                match refusal {
                    Some(r) => {
                        let mut out = vec![CORE_REFUSE, r.code()];
                        out.extend(r.reply_word().unwrap_or(0).to_le_bytes());
                        link.send(&out, true, true, now);
                    }
                    None => admitted.push((*addr, name)),
                }
            }
            if link.is_closed() {
                gone.push(*addr);
            }
        }
        for (addr, name, verdict) in self
            .bridge
            .as_mut()
            .map(|b| std::mem::take(&mut b.decided))
            .unwrap_or_default()
        {
            match verdict {
                Ok(()) => admitted.push((addr, name)),
                Err(r) => {
                    if let Some(link) = pending.get_mut(&addr) {
                        let mut out = vec![CORE_REFUSE, r.code()];
                        out.extend(r.reply_word().unwrap_or(0).to_le_bytes());
                        link.send(&out, true, true, now);
                    }
                }
            }
        }
        for addr in gone {
            pending.remove(&addr);
        }
        for (addr, name) in admitted {
            let Some(mut link) = pending.remove(&addr) else {
                continue;
            };
            let id = *next_id;
            *next_id += 1;
            let info = PlayerInfo { id, name };
            // Tell the others first, then accept with the full table.
            let mut joined = vec![CORE_PLAYER_JOINED];
            joined.extend(id.to_le_bytes());
            put_str(&mut joined, &info.name);
            for (_, other) in peers.values_mut() {
                other.send(&joined, true, true, now);
            }
            self.players.insert(id, info.clone());
            let mut accept = vec![CORE_ACCEPT];
            accept.extend(id.to_le_bytes());
            accept.extend(HOST_ID.to_le_bytes());
            accept.extend((self.players.len() as u16).to_le_bytes());
            for p in self.players.values() {
                accept.extend(p.id.to_le_bytes());
                put_str(&mut accept, &p.name);
            }
            link.send(&accept, true, true, now);
            peers.insert(id, (addr, link));
            self.events.push(Event::PlayerJoined(info));
        }
        // Connected players: messages, relays, departures.
        let mut relays: Vec<(PlayerId, u32, Vec<u8>, bool)> = Vec::new();
        let mut renames: Vec<(PlayerId, String)> = Vec::new();
        let mut left = Vec::new();
        for (id, (_, link)) in peers.iter_mut() {
            for d in link.take_delivered() {
                if d.core {
                    if d.payload.first() == Some(&CORE_RELAY) && d.payload.len() >= 5 {
                        let to = u32::from_le_bytes(d.payload[1..5].try_into().expect("4 bytes"));
                        relays.push((*id, to, d.payload[5..].to_vec(), true));
                    } else if d.payload.first() == Some(&CORE_SET_NAME) {
                        if let Some(name) = Cursor(&d.payload[1..]).str() {
                            renames.push((*id, name));
                        }
                    }
                    continue;
                }
                self.events
                    .push(game_event(self.bridge.is_some(), *id, &d.payload));
            }
            if let LinkState::Closed(reason) = link.state {
                left.push((
                    *id,
                    if reason == CloseReason::Remote {
                        LeaveReason::Left
                    } else {
                        LeaveReason::TimedOut
                    },
                ));
            }
        }
        for (id, name) in renames {
            let Some(p) = self.players.get_mut(&id) else {
                continue;
            };
            p.name = name;
            let info = p.clone();
            let mut out = vec![CORE_PEER_INFO];
            out.extend(id.to_le_bytes());
            put_str(&mut out, &info.name);
            for (_, link) in peers.values_mut() {
                link.send(&out, true, true, now);
            }
            self.events.push(Event::PlayerRenamed(info));
        }
        for (from, to, payload, guaranteed) in relays {
            let mut out = vec![CORE_FROM];
            out.extend(from.to_le_bytes());
            out.extend(&payload);
            for (id, (_, link)) in peers.iter_mut() {
                if *id != from && (to == 0 || to == *id) {
                    link.send(&out, true, guaranteed, now);
                }
            }
            if to == HOST_ID {
                self.events
                    .push(game_event(self.bridge.is_some(), from, &payload));
            }
        }
        for (id, reason) in left {
            peers.remove(&id);
            self.players.remove(&id);
            let mut out = vec![CORE_PLAYER_LEFT];
            out.extend(id.to_le_bytes());
            for (_, link) in peers.values_mut() {
                link.send(&out, true, true, now);
            }
            self.events.push(Event::PlayerLeft { id, reason });
        }
    }

    fn tick_joiner(&mut self, now: Instant) {
        let Role::Joiner {
            config,
            link,
            local,
            host,
            started,
            introduced,
            done,
        } = &mut self.role
        else {
            return;
        };
        if *done {
            return;
        }
        // The first time the link is up, introduce ourselves.
        if link.is_connected() && !*introduced {
            *introduced = true;
            let mut out = vec![CORE_CONNECT_INFO];
            put_str(&mut out, &config.player_name);
            put_str(&mut out, config.password.as_deref().unwrap_or(""));
            out.extend((config.connect_data.len() as u16).to_le_bytes());
            out.extend(&config.connect_data);
            link.send(&out, true, true, now);
        }
        for d in link.take_delivered() {
            if !d.core {
                self.events.push(game_event(
                    self.bridge.is_some(),
                    host.unwrap_or(HOST_ID),
                    &d.payload,
                ));
                continue;
            }
            let mut c = Cursor(&d.payload);
            match c.u8() {
                Some(CORE_ACCEPT) => {
                    let (Some(me), Some(h), Some(n)) = (c.u32(), c.u32(), c.u16()) else {
                        continue;
                    };
                    let mut players = Vec::new();
                    for _ in 0..n {
                        let (Some(id), Some(name)) = (c.u32(), c.str()) else {
                            break;
                        };
                        players.push(PlayerInfo { id, name });
                    }
                    *local = Some(me);
                    *host = Some(h);
                    for p in &players {
                        self.players.insert(p.id, p.clone());
                    }
                    self.events.push(Event::Connected {
                        local: me,
                        host: h,
                        players,
                    });
                }
                Some(CORE_REFUSE) => {
                    let r = c
                        .u8()
                        .and_then(Refusal::from_code)
                        .unwrap_or(Refusal::SessionFull);
                    self.events.push(Event::Refused(r));
                    let _ = link.close(now);
                    *done = true;
                    return;
                }
                Some(CORE_PLAYER_JOINED) => {
                    if let (Some(id), Some(name)) = (c.u32(), c.str()) {
                        let info = PlayerInfo { id, name };
                        self.players.insert(id, info.clone());
                        self.events.push(Event::PlayerJoined(info));
                    }
                }
                Some(CORE_PEER_INFO) => {
                    if let (Some(id), Some(name)) = (c.u32(), c.str()) {
                        let info = PlayerInfo { id, name };
                        self.players.insert(id, info.clone());
                        self.events.push(Event::PlayerRenamed(info));
                    }
                }
                Some(CORE_PLAYER_LEFT) => {
                    if let Some(id) = c.u32() {
                        self.players.remove(&id);
                        self.events.push(Event::PlayerLeft {
                            id,
                            reason: LeaveReason::Left,
                        });
                    }
                }
                Some(CORE_TERMINATE) => {
                    self.events.push(Event::SessionTerminated);
                    *done = true;
                    return;
                }
                Some(CORE_FROM) => {
                    if let Some(from) = c.u32() {
                        self.events
                            .push(game_event(self.bridge.is_some(), from, c.0));
                    }
                }
                _ => {}
            }
        }
        match link.state {
            LinkState::Closed(CloseReason::ConnectFailed) => {
                self.events.push(Event::ConnectFailed);
                *done = true;
            }
            LinkState::Closed(_) => {
                self.events.push(if local.is_some() {
                    Event::SessionTerminated
                } else {
                    Event::ConnectFailed
                });
                *done = true;
            }
            _ => {
                let waited = now.saturating_duration_since(*started);
                if local.is_none() && !link.is_connected() && waited >= config.connect_timeout {
                    let _ = link.close(now);
                    self.events.push(Event::ConnectFailed);
                    *done = true;
                }
            }
        }
    }

    fn flush(&mut self, now: Instant) {
        match &mut self.role {
            Role::Host { pending, peers, .. } => {
                for (addr, link) in pending.iter_mut() {
                    for d in link.poll(now) {
                        let _ = self.socket.send_to(&d, addr);
                    }
                }
                for (addr, link) in peers.values_mut() {
                    for d in link.poll(now) {
                        let _ = self.socket.send_to(&d, *addr);
                    }
                }
            }
            Role::Joiner { config, link, .. } => {
                for d in link.poll(now) {
                    let _ = self.socket.send_to(&d, config.host);
                }
            }
        }
    }

    /// Change our own name (`FUN_00623580`, `IDirectPlay8Peer::SetPeerInfo`
    /// with a `DPN_PLAYER_INFO` of size 0x18, `DPNINFO_NAME`, synchronous): every
    /// other player hears a PEER_INFO and renames the entry. A joiner tells the
    /// host, which tells everyone; the host tells everyone itself.
    pub fn set_name(&mut self, name: &str) {
        let now = Instant::now();
        match &mut self.role {
            Role::Host { peers, .. } => {
                if let Some(p) = self.players.get_mut(&HOST_ID) {
                    p.name = name.to_string();
                }
                let mut out = vec![CORE_PEER_INFO];
                out.extend(HOST_ID.to_le_bytes());
                put_str(&mut out, name);
                for (_, link) in peers.values_mut() {
                    link.send(&out, true, true, now);
                }
            }
            Role::Joiner {
                link,
                local,
                config,
                ..
            } => {
                config.player_name = name.to_string();
                if let Some(me) = *local {
                    if let Some(p) = self.players.get_mut(&me) {
                        p.name = name.to_string();
                    }
                    let mut out = vec![CORE_SET_NAME];
                    put_str(&mut out, name);
                    link.send(&out, true, true, now);
                }
            }
        }
        self.flush(now);
    }

    /// Host: drop one player (the lobby's eject). Joiner: ignored.
    pub fn eject(&mut self, id: PlayerId) {
        let now = Instant::now();
        if let Role::Host { peers, .. } = &mut self.role {
            if let Some((addr, mut link)) = peers.remove(&id) {
                link.send(&[CORE_TERMINATE], true, true, now);
                for d in link.poll(now) {
                    let _ = self.socket.send_to(&d, addr);
                }
                for d in link.close(now) {
                    let _ = self.socket.send_to(&d, addr);
                }
                self.players.remove(&id);
                let mut out = vec![CORE_PLAYER_LEFT];
                out.extend(id.to_le_bytes());
                for (_, l) in peers.values_mut() {
                    l.send(&out, true, true, now);
                }
                self.events.push(Event::PlayerLeft {
                    id,
                    reason: LeaveReason::Left,
                });
            }
        }
        self.flush(now);
    }

    /// Leave (joiner) or end the session for everyone (host): the host's
    /// `TerminateSession`, a joiner's `Close`.
    pub fn close(&mut self) {
        let now = Instant::now();
        match &mut self.role {
            Role::Host { pending, peers, .. } => {
                for (addr, link) in peers.values_mut() {
                    link.send(&[CORE_TERMINATE], true, false, now);
                    for d in link.poll(now) {
                        let _ = self.socket.send_to(&d, *addr);
                    }
                    for d in link.close(now) {
                        let _ = self.socket.send_to(&d, *addr);
                    }
                }
                for (addr, link) in pending.iter_mut() {
                    for d in link.close(now) {
                        let _ = self.socket.send_to(&d, addr);
                    }
                }
                peers.clear();
                pending.clear();
            }
            Role::Joiner {
                config, link, done, ..
            } => {
                for d in link.close(now) {
                    let _ = self.socket.send_to(&d, config.host);
                }
                *done = true;
            }
        }
        self.players.clear();
    }
}

/// One server-list row (`giAddHostMessage`).
#[derive(Debug, Clone, PartialEq)]
pub struct HostEntry {
    pub addr: SocketAddr,
    pub session_name: String,
    pub current_players: u32,
    pub max_players: u32,
    pub password: bool,
    pub data: RaHostData,
    pub ping: Duration,
    /// The session's instance GUID (the original's server list keys its
    /// rows on it, `FUN_00623720` case `0xffff000b`).
    pub instance: Guid,
    /// The application reply as it came (RA2's eight bytes).
    pub reply: Vec<u8>,
}

/// The LAN tab's Refresh: send enumeration queries and collect the replies.
pub struct Browser {
    socket: UdpSocket,
    sent: HashMap<u16, Instant>,
    next: u16,
    pub hosts: Vec<HostEntry>,
}

impl Browser {
    pub fn new(bind: SocketAddr) -> io::Result<Browser> {
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        // Broadcast is what the LAN query uses; refusing it (a loopback-only
        // test socket) is not an error.
        let _ = socket.set_broadcast(true);
        Ok(Browser {
            socket,
            sent: HashMap::new(),
            next: 1,
            hosts: Vec::new(),
        })
    }

    /// Query one address (a host's game port, or a broadcast address on
    /// [`dpn::ENUM_PORT`]).
    pub fn query(&mut self, to: SocketAddr) -> io::Result<()> {
        let q = EnumQuery {
            payload: self.next,
            app: Some(dpn::APP_GUID),
            user_data: Vec::new(),
        };
        self.sent.insert(self.next, Instant::now());
        self.next = self.next.wrapping_add(1).max(1);
        self.socket.send_to(&q.encode(), to)?;
        Ok(())
    }

    /// Collect replies; returns how many rows changed.
    pub fn poll(&mut self) -> usize {
        let mut buf = [0u8; 2048];
        let mut changed = 0;
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let Some(r) = EnumResponse::decode(&buf[..n]) else {
                        continue;
                    };
                    if r.app != dpn::APP_GUID {
                        continue;
                    }
                    let ping = self
                        .sent
                        .get(&r.payload)
                        .map(|t| t.elapsed())
                        .unwrap_or_default();
                    let entry = HostEntry {
                        addr: from,
                        session_name: r.session_name,
                        current_players: r.current_players,
                        max_players: r.max_players,
                        password: r.flags & dpn::SESSION_REQUIRE_PASSWORD != 0,
                        data: RaHostData::decode(&r.app_reply).unwrap_or_default(),
                        ping,
                        instance: r.instance,
                        reply: r.app_reply,
                    };
                    match self.hosts.iter_mut().find(|h| h.addr == from) {
                        Some(h) => *h = entry,
                        None => self.hosts.push(entry),
                    }
                    changed += 1;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            }
        }
        changed
    }
}
