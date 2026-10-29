//! The other half of `tools/dpnet`: a DirectPlay 8 replacement for the
//! original game under Wine (docs/38 §14).
//!
//! Wine's builtin `dpnet.dll` does not implement `IDirectPlay8Peer::Host`,
//! `Connect` or `EnumHosts` (they are `FIXME` stubs that put nothing on the
//! wire), so the original's multiplayer lobby never opens under Wine. The
//! replacement `dpnet.dll` (`tools/dpnet/ora_dpnet.c`) implements the
//! `IDirectPlay8Peer` / `IDirectPlay8Address` API the game calls and forwards
//! the calls over a loopback TCP connection to this bridge, which runs a
//! [`Session`] in bridge mode: the same transport and session core this port
//! uses, so the original and the port meet on the LAN.
//!
//! The DLL keeps everything the game reads back synchronously (the player
//! table for `GetPeerInfo`, the application description, addresses); the
//! bridge owns the sockets, the session and the server-list queries. The
//! game's decisions (`INDICATE_CONNECT`'s verdict, `ENUM_HOSTS_QUERY`'s reply
//! data) travel back as answers to the bridge's questions.
//!
//! ## The pipe
//!
//! Each frame is `u32 length` (of what follows) `u8 op` and the body,
//! little-endian; a string is `u16 length` and UTF-8, a byte block `u32
//! length` and the bytes. Ops below `0x80` go from the DLL to the bridge
//! ([`Request`]), the others back ([`Note`]).

use std::collections::BTreeSet;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::dpn::{self, Guid};
use crate::link::LinkConfig;
use crate::session::{
    Browser, Event, HostConfig, JoinConfig, LeaveReason, PlayerId, Refusal, Session, Target,
};

/// Where the bridge listens by default (`ORA_DPNET_BRIDGE` in the DLL).
pub const DEFAULT_ADDR: &str = "127.0.0.1:27160";

/// `DPNERR_HOSTREJECTEDCONNECTION`: the joiner's `CONNECT_COMPLETE` result when
/// the host refused (the game reads its reply word 900/901).
pub const HR_HOST_REJECTED: u32 = 0x8015_8260;
/// `DPNERR_NORESPONSE`-like failure for a connect nobody answered.
pub const HR_NO_RESPONSE: u32 = 0x8015_8530;
/// `E_FAIL`.
pub const HR_FAIL: u32 = 0x8000_4005;
/// `DPNERR_CONNECTIONLOST`, the result of a session that ended under us.
pub const HR_CONNECTION_LOST: u32 = 0x8015_00c8;

/// `DPNPLAYER_LOCAL` / `DPNPLAYER_HOST` (the game's `CREATE_PLAYER` case tests
/// `dwPlayerFlags & 2` and `& 4`).
pub const PLAYER_LOCAL: u32 = 0x2;
pub const PLAYER_HOST: u32 = 0x4;

/// `DPNSEND_GUARANTEED` / `DPNSEND_NOLOOPBACK`.
pub const SEND_GUARANTEED: u32 = 0x08;
pub const SEND_NOLOOPBACK: u32 = 0x20;

/// `DPNDESTROYPLAYERREASON_NORMAL` / `_CONNECTIONLOST`.
pub const DESTROY_NORMAL: u32 = 1;
pub const DESTROY_CONNECTION_LOST: u32 = 2;

/// DLL -> bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `Host`: the device port, `dwMaxPlayers`, the session name, the password
    /// (empty: none) and the local player's name (`SetPeerInfo`).
    Host {
        port: u16,
        max_players: u32,
        session: String,
        password: String,
        player: String,
    },
    /// `Connect`: the host address (`hostname`, `port`), the device port.
    Connect {
        host: String,
        port: u16,
        local_port: u16,
        player: String,
        password: String,
        connect_data: Vec<u8>,
    },
    /// `EnumHosts`: one host, or (empty) the LAN broadcast.
    Enum { host: String, port: u16 },
    /// `CancelAsyncOperation` of the enumeration, or `Close`.
    EnumStop,
    /// `SendTo`.
    Send {
        handle: u32,
        to: PlayerId,
        flags: u32,
        data: Vec<u8>,
    },
    /// `SetPeerInfo` while in a session.
    SetName(String),
    /// `Close` (a host's ends the session for everyone).
    Close,
    /// The game's `INDICATE_CONNECT` verdict on [`Note::AdmitAsk`].
    Admit {
        token: u32,
        accept: bool,
        reply: Vec<u8>,
    },
    /// The game's `ENUM_HOSTS_QUERY` answer on [`Note::EnumAsk`].
    EnumAnswer {
        token: u32,
        answer: bool,
        data: Vec<u8>,
    },
}

/// Bridge -> DLL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Hosted {
        hr: u32,
        local: PlayerId,
    },
    Connected {
        hr: u32,
        local: PlayerId,
        host: PlayerId,
        reply: Vec<u8>,
    },
    /// A player is in the session (`CREATE_PLAYER`), with its flags.
    Player {
        id: PlayerId,
        flags: u32,
        name: String,
        addr: String,
    },
    /// `DESTROY_PLAYER` with its reason.
    Left {
        id: PlayerId,
        reason: u32,
    },
    /// `PEER_INFO`.
    Renamed {
        id: PlayerId,
        name: String,
    },
    /// `RECEIVE`.
    Receive {
        from: PlayerId,
        data: Vec<u8>,
    },
    /// `SEND_COMPLETE`.
    Sent {
        handle: u32,
        hr: u32,
    },
    /// `TERMINATE_SESSION`.
    Terminated {
        hr: u32,
    },
    /// Host: a player asks to join; the DLL runs `INDICATE_CONNECT`.
    AdmitAsk {
        token: u32,
        name: String,
        addr: String,
        connect_data: Vec<u8>,
    },
    /// Host: an enumeration query; the DLL runs `ENUM_HOSTS_QUERY`.
    EnumAsk {
        token: u32,
        addr: String,
        user_data: Vec<u8>,
    },
    /// `ENUM_HOSTS_RESPONSE`: one server-list row.
    EnumFound {
        ip: String,
        port: u16,
        flags: u32,
        max_players: u32,
        current_players: u32,
        instance: Guid,
        app: Guid,
        session: String,
        reply: Vec<u8>,
        rtt_ms: u32,
    },
}

#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn str(&mut self, s: &str) -> &mut Self {
        let b = &s.as_bytes()[..s.len().min(u16::MAX as usize)];
        self.u16(b.len() as u16);
        self.0.extend(b);
        self
    }
    fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.u32(b.len() as u32);
        self.0.extend(b);
        self
    }
    fn guid(&mut self, g: Guid) -> &mut Self {
        self.0.extend(g.0);
        self
    }
    /// Prefix the length.
    fn frame(&self) -> Vec<u8> {
        let mut f = (self.0.len() as u32).to_le_bytes().to_vec();
        f.extend(&self.0);
        f
    }
}

struct In<'a>(&'a [u8]);

impl<'a> In<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let v = self.0.get(..n)?;
        self.0 = &self.0[n..];
        Some(v)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u16()? as usize;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }
    fn guid(&mut self) -> Option<Guid> {
        Some(Guid(self.take(16)?.try_into().ok()?))
    }
}

impl Request {
    /// One frame, length included.
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Request::Host {
                port,
                max_players,
                session,
                password,
                player,
            } => o
                .u8(0x01)
                .u16(*port)
                .u32(*max_players)
                .str(session)
                .str(password)
                .str(player),
            Request::Connect {
                host,
                port,
                local_port,
                player,
                password,
                connect_data,
            } => o
                .u8(0x02)
                .str(host)
                .u16(*port)
                .u16(*local_port)
                .str(player)
                .str(password)
                .bytes(connect_data),
            Request::Enum { host, port } => o.u8(0x03).str(host).u16(*port),
            Request::EnumStop => o.u8(0x04),
            Request::Send {
                handle,
                to,
                flags,
                data,
            } => o.u8(0x05).u32(*handle).u32(*to).u32(*flags).bytes(data),
            Request::SetName(name) => o.u8(0x06).str(name),
            Request::Close => o.u8(0x07),
            Request::Admit {
                token,
                accept,
                reply,
            } => o.u8(0x08).u32(*token).u8(*accept as u8).bytes(reply),
            Request::EnumAnswer {
                token,
                answer,
                data,
            } => o.u8(0x09).u32(*token).u8(*answer as u8).bytes(data),
        };
        o.frame()
    }

    /// A frame body (op and fields, no length).
    pub fn decode(body: &[u8]) -> Option<Request> {
        let mut i = In(body);
        Some(match i.u8()? {
            0x01 => Request::Host {
                port: i.u16()?,
                max_players: i.u32()?,
                session: i.str()?,
                password: i.str()?,
                player: i.str()?,
            },
            0x02 => Request::Connect {
                host: i.str()?,
                port: i.u16()?,
                local_port: i.u16()?,
                player: i.str()?,
                password: i.str()?,
                connect_data: i.bytes()?,
            },
            0x03 => Request::Enum {
                host: i.str()?,
                port: i.u16()?,
            },
            0x04 => Request::EnumStop,
            0x05 => Request::Send {
                handle: i.u32()?,
                to: i.u32()?,
                flags: i.u32()?,
                data: i.bytes()?,
            },
            0x06 => Request::SetName(i.str()?),
            0x07 => Request::Close,
            0x08 => Request::Admit {
                token: i.u32()?,
                accept: i.u8()? != 0,
                reply: i.bytes()?,
            },
            0x09 => Request::EnumAnswer {
                token: i.u32()?,
                answer: i.u8()? != 0,
                data: i.bytes()?,
            },
            _ => return None,
        })
    }
}

impl Note {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Note::Hosted { hr, local } => o.u8(0x81).u32(*hr).u32(*local),
            Note::Connected {
                hr,
                local,
                host,
                reply,
            } => o.u8(0x82).u32(*hr).u32(*local).u32(*host).bytes(reply),
            Note::Player {
                id,
                flags,
                name,
                addr,
            } => o.u8(0x83).u32(*id).u32(*flags).str(name).str(addr),
            Note::Left { id, reason } => o.u8(0x84).u32(*id).u32(*reason),
            Note::Renamed { id, name } => o.u8(0x85).u32(*id).str(name),
            Note::Receive { from, data } => o.u8(0x86).u32(*from).bytes(data),
            Note::Sent { handle, hr } => o.u8(0x87).u32(*handle).u32(*hr),
            Note::Terminated { hr } => o.u8(0x88).u32(*hr),
            Note::AdmitAsk {
                token,
                name,
                addr,
                connect_data,
            } => o
                .u8(0x89)
                .u32(*token)
                .str(name)
                .str(addr)
                .bytes(connect_data),
            Note::EnumAsk {
                token,
                addr,
                user_data,
            } => o.u8(0x8a).u32(*token).str(addr).bytes(user_data),
            Note::EnumFound {
                ip,
                port,
                flags,
                max_players,
                current_players,
                instance,
                app,
                session,
                reply,
                rtt_ms,
            } => o
                .u8(0x8b)
                .str(ip)
                .u16(*port)
                .u32(*flags)
                .u32(*max_players)
                .u32(*current_players)
                .guid(*instance)
                .guid(*app)
                .str(session)
                .bytes(reply)
                .u32(*rtt_ms),
        };
        o.frame()
    }

    pub fn decode(body: &[u8]) -> Option<Note> {
        let mut i = In(body);
        Some(match i.u8()? {
            0x81 => Note::Hosted {
                hr: i.u32()?,
                local: i.u32()?,
            },
            0x82 => Note::Connected {
                hr: i.u32()?,
                local: i.u32()?,
                host: i.u32()?,
                reply: i.bytes()?,
            },
            0x83 => Note::Player {
                id: i.u32()?,
                flags: i.u32()?,
                name: i.str()?,
                addr: i.str()?,
            },
            0x84 => Note::Left {
                id: i.u32()?,
                reason: i.u32()?,
            },
            0x85 => Note::Renamed {
                id: i.u32()?,
                name: i.str()?,
            },
            0x86 => Note::Receive {
                from: i.u32()?,
                data: i.bytes()?,
            },
            0x87 => Note::Sent {
                handle: i.u32()?,
                hr: i.u32()?,
            },
            0x88 => Note::Terminated { hr: i.u32()? },
            0x89 => Note::AdmitAsk {
                token: i.u32()?,
                name: i.str()?,
                addr: i.str()?,
                connect_data: i.bytes()?,
            },
            0x8a => Note::EnumAsk {
                token: i.u32()?,
                addr: i.str()?,
                user_data: i.bytes()?,
            },
            0x8b => Note::EnumFound {
                ip: i.str()?,
                port: i.u16()?,
                flags: i.u32()?,
                max_players: i.u32()?,
                current_players: i.u32()?,
                instance: i.guid()?,
                app: i.guid()?,
                session: i.str()?,
                reply: i.bytes()?,
                rtt_ms: i.u32()?,
            },
            _ => return None,
        })
    }
}

/// Split complete frames off the front of `buf`; returns their bodies.
pub fn take_frames(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        if buf.len() < 4 {
            break;
        }
        let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        if buf.len() < 4 + n {
            break;
        }
        out.push(buf[4..4 + n].to_vec());
        buf.drain(..4 + n);
    }
    out
}

/// The refusal the original's reply word names (`INDICATE_CONNECT`'s reply
/// data: 900 incompatible, 901 in progress).
fn refusal_of(reply: &[u8]) -> Refusal {
    match reply
        .get(..4)
        .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
    {
        Some(dpn::REFUSE_INCOMPATIBLE) => Refusal::IncompatibleVersion,
        Some(dpn::REFUSE_IN_PROGRESS) => Refusal::InProgress,
        _ => Refusal::SessionFull,
    }
}

/// What the browser targets: one host, or the LAN broadcast (the DirectPlay
/// enumeration port and the game port) plus `extra` hosts.
fn enum_targets(host: &str, port: u16, extra: &[SocketAddr]) -> Vec<SocketAddr> {
    let port = if port == 0 { dpn::DEFAULT_PORT } else { port };
    if !host.is_empty() {
        return (host, port)
            .to_socket_addrs()
            .map(|a| a.filter(SocketAddr::is_ipv4).take(1).collect())
            .unwrap_or_default();
    }
    // A broadcast does not come back to this machine's own sockets: a host on
    // the same machine (the port beside the original) is asked directly.
    let mut v = vec![
        SocketAddr::from(([255, 255, 255, 255], dpn::ENUM_PORT)),
        SocketAddr::from(([255, 255, 255, 255], port)),
        SocketAddr::from(([127, 0, 0, 1], port)),
    ];
    v.extend(extra);
    v
}

/// The state behind one DLL connection: its session, its server list.
pub struct Bridge {
    pub session: Option<Session>,
    browser: Option<(Browser, Vec<SocketAddr>, Instant)>,
    /// Rows already reported (by instance and address), to send each once per
    /// change.
    reported: BTreeSet<(Vec<u8>, SocketAddr, u32)>,
    /// Extra hosts the LAN refresh also asks (`ORA_DPNET_HOSTS`).
    pub extra_hosts: Vec<SocketAddr>,
    /// The address the sessions bind (`0.0.0.0` by default).
    pub bind_ip: std::net::IpAddr,
    pub link: LinkConfig,
    /// Every game payload the game sends and receives, one line each
    /// (`send <to> <flags> <hex>` / `recv <from> <hex>`, ms since start):
    /// the original's own bytes, for checking our encoders against them.
    pub dump: Option<Box<dyn Write + Send>>,
    started: Instant,
}

impl Default for Bridge {
    fn default() -> Self {
        Bridge::new()
    }
}

impl Bridge {
    pub fn new() -> Bridge {
        Bridge {
            session: None,
            browser: None,
            reported: BTreeSet::new(),
            extra_hosts: Vec::new(),
            bind_ip: "0.0.0.0".parse().expect("an address"),
            link: LinkConfig::default(),
            dump: None,
            started: Instant::now(),
        }
    }

    fn dump_line(&mut self, what: &str, who: PlayerId, flags: Option<u32>, data: &[u8]) {
        let Some(d) = &mut self.dump else { return };
        let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
        let ms = self.started.elapsed().as_millis();
        let _ = match flags {
            Some(f) => writeln!(d, "{ms} {what} {who:#x} {f:#x} {hex}"),
            None => writeln!(d, "{ms} {what} {who:#x} {hex}"),
        };
        let _ = d.flush();
    }

    fn local(&self) -> PlayerId {
        self.session
            .as_ref()
            .and_then(Session::local_id)
            .unwrap_or(0)
    }

    /// Carry out one request; returns the notes it produces at once.
    pub fn request(&mut self, req: Request) -> Vec<Note> {
        let mut out = Vec::new();
        match req {
            Request::Host {
                port,
                max_players,
                session,
                password,
                player,
            } => {
                let config = HostConfig {
                    bind: SocketAddr::new(self.bind_ip, port),
                    session_name: session,
                    player_name: player.clone(),
                    max_players: max_players.max(1),
                    password: (!password.is_empty()).then_some(password),
                    link: self.link,
                };
                match Session::host(config) {
                    Ok(mut s) => {
                        s.set_bridge(true);
                        let local = s.local_id().unwrap_or(0);
                        self.session = Some(s);
                        out.push(Note::Player {
                            id: local,
                            flags: PLAYER_LOCAL | PLAYER_HOST,
                            name: player,
                            addr: String::new(),
                        });
                        out.push(Note::Hosted { hr: 0, local });
                    }
                    Err(_) => out.push(Note::Hosted {
                        hr: HR_FAIL,
                        local: 0,
                    }),
                }
            }
            Request::Connect {
                host,
                port,
                local_port,
                player,
                password,
                connect_data,
            } => {
                let port = if port == 0 { dpn::DEFAULT_PORT } else { port };
                let host = (host.as_str(), port)
                    .to_socket_addrs()
                    .ok()
                    .and_then(|mut a| a.find(SocketAddr::is_ipv4));
                let Some(host) = host else {
                    out.push(Note::Connected {
                        hr: HR_NO_RESPONSE,
                        local: 0,
                        host: 0,
                        reply: Vec::new(),
                    });
                    return out;
                };
                let mut config =
                    JoinConfig::new(SocketAddr::new(self.bind_ip, local_port), host, &player);
                config.password = (!password.is_empty()).then_some(password);
                config.connect_data = connect_data;
                config.link = self.link;
                // The device port is the game port; on one machine the host
                // may hold it already: then any port.
                let session = Session::join(config.clone()).or_else(|_| {
                    config.bind.set_port(0);
                    Session::join(config)
                });
                match session {
                    Ok(mut s) => {
                        s.set_bridge(true);
                        self.session = Some(s);
                    }
                    Err(_) => out.push(Note::Connected {
                        hr: HR_FAIL,
                        local: 0,
                        host: 0,
                        reply: Vec::new(),
                    }),
                }
            }
            Request::Enum { host, port } => {
                if let Ok(b) = Browser::new(SocketAddr::new(self.bind_ip, 0)) {
                    let targets = enum_targets(&host, port, &self.extra_hosts);
                    // Asked at once, then every 1.5 s until stopped.
                    let due = Instant::now() - Duration::from_secs(2);
                    self.browser = Some((b, targets, due));
                    self.reported.clear();
                }
            }
            Request::EnumStop => self.browser = None,
            Request::Send {
                handle,
                to,
                flags,
                data,
            } => {
                let local = self.local();
                self.dump_line("send", to, Some(flags), &data);
                let guaranteed = flags & SEND_GUARANTEED != 0;
                if let Some(s) = &mut self.session {
                    if to == 0 {
                        s.send_raw(Target::All, &data, guaranteed);
                    } else if to != local {
                        s.send_raw(Target::Player(to), &data, guaranteed);
                    }
                }
                // DirectPlay loops a send to everyone back to the sender
                // unless NOLOOPBACK; one to the sender itself always.
                if (to == 0 && flags & SEND_NOLOOPBACK == 0) || (to != 0 && to == local) {
                    out.push(Note::Receive { from: local, data });
                }
                out.push(Note::Sent { handle, hr: 0 });
            }
            Request::SetName(name) => {
                if let Some(s) = &mut self.session {
                    s.set_name(&name);
                }
            }
            Request::Close => {
                if let Some(mut s) = self.session.take() {
                    s.close();
                }
                self.browser = None;
            }
            Request::Admit {
                token,
                accept,
                reply,
            } => {
                if let Some(s) = &mut self.session {
                    let verdict = if accept {
                        Ok(())
                    } else {
                        Err(refusal_of(&reply))
                    };
                    s.admit(token, verdict);
                }
            }
            Request::EnumAnswer {
                token,
                answer,
                data,
            } => {
                if let (Some(s), true) = (&mut self.session, answer) {
                    s.answer_enum(token, &data);
                }
            }
        }
        out
    }

    /// Run the session and the server list; returns what happened.
    pub fn poll(&mut self) -> Vec<Note> {
        let mut out = Vec::new();
        if let Some((browser, targets, last)) = &mut self.browser {
            if last.elapsed() >= Duration::from_millis(1500) {
                *last = Instant::now();
                for t in targets.iter() {
                    let _ = browser.query(*t);
                }
            }
            if browser.poll() > 0 {
                for h in &browser.hosts {
                    let key = (h.instance.0.to_vec(), h.addr, h.current_players);
                    if !self.reported.insert(key) {
                        continue;
                    }
                    out.push(Note::EnumFound {
                        ip: h.addr.ip().to_string(),
                        port: h.addr.port(),
                        flags: if h.password {
                            dpn::SESSION_REQUIRE_PASSWORD
                        } else {
                            0
                        },
                        max_players: h.max_players,
                        current_players: h.current_players,
                        instance: h.instance,
                        app: dpn::APP_GUID,
                        session: h.session_name.clone(),
                        reply: h.reply.clone(),
                        rtt_ms: h.ping.as_millis() as u32,
                    });
                }
            }
        }
        let Some(s) = &mut self.session else {
            return out;
        };
        let mut ended = false;
        let mut received = Vec::new();
        for e in s.poll() {
            match e {
                Event::Connected {
                    local,
                    host,
                    players,
                } => {
                    for p in players {
                        let mut flags = 0;
                        if p.id == local {
                            flags |= PLAYER_LOCAL;
                        }
                        if p.id == host {
                            flags |= PLAYER_HOST;
                        }
                        out.push(Note::Player {
                            id: p.id,
                            flags,
                            name: p.name,
                            addr: String::new(),
                        });
                    }
                    out.push(Note::Connected {
                        hr: 0,
                        local,
                        host,
                        reply: Vec::new(),
                    });
                }
                Event::Refused(r) => {
                    out.push(Note::Connected {
                        hr: HR_HOST_REJECTED,
                        local: 0,
                        host: 0,
                        reply: r
                            .reply_word()
                            .map(|w| w.to_le_bytes().to_vec())
                            .unwrap_or_default(),
                    });
                    ended = true;
                }
                Event::ConnectFailed => {
                    out.push(Note::Connected {
                        hr: HR_NO_RESPONSE,
                        local: 0,
                        host: 0,
                        reply: Vec::new(),
                    });
                    ended = true;
                }
                Event::PlayerJoined(p) => out.push(Note::Player {
                    id: p.id,
                    flags: 0,
                    name: p.name,
                    addr: String::new(),
                }),
                Event::PlayerRenamed(p) => out.push(Note::Renamed {
                    id: p.id,
                    name: p.name,
                }),
                Event::PlayerLeft { id, reason } => out.push(Note::Left {
                    id,
                    reason: match reason {
                        LeaveReason::Left => DESTROY_NORMAL,
                        LeaveReason::TimedOut => DESTROY_CONNECTION_LOST,
                    },
                }),
                Event::SessionTerminated => {
                    out.push(Note::Terminated {
                        hr: HR_CONNECTION_LOST,
                    });
                    ended = true;
                }
                Event::Raw { from, data } => {
                    received.push((from, data.clone()));
                    out.push(Note::Receive { from, data })
                }
                Event::ConnectRequest {
                    token,
                    name,
                    connect_data,
                    addr,
                } => out.push(Note::AdmitAsk {
                    token,
                    name,
                    addr: addr.to_string(),
                    connect_data,
                }),
                Event::EnumQuery {
                    token,
                    from,
                    user_data,
                } => out.push(Note::EnumAsk {
                    token,
                    addr: from.to_string(),
                    user_data,
                }),
                // Bridge mode delivers payloads raw.
                Event::Message { .. } | Event::BadMessage { .. } => {}
            }
        }
        if ended {
            self.session = None;
        }
        for (from, data) in received {
            self.dump_line("recv", from, None, &data);
        }
        out
    }
}

/// Serve DLL connections on `listener`, one at a time, until `stop` says so.
/// `log` gets one line per request and note (op names only for payloads).
pub fn serve(
    listener: TcpListener,
    make: impl Fn() -> Bridge,
    mut log: impl FnMut(&str),
    stop: impl Fn() -> bool,
) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    while !stop() {
        let (stream, peer) = match listener.accept() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(e) => return Err(e),
        };
        log(&format!("dll connected from {peer}"));
        let r = serve_one(stream, make(), &mut log, &stop);
        log(&format!("dll gone ({r:?})"));
    }
    Ok(())
}

fn serve_one(
    mut stream: TcpStream,
    mut bridge: Bridge,
    log: &mut impl FnMut(&str),
    stop: &impl Fn() -> bool,
) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    stream.set_nodelay(true)?;
    let mut inbuf = Vec::new();
    let mut chunk = vec![0u8; 65536];
    let write = |stream: &mut TcpStream, notes: Vec<Note>, log: &mut dyn FnMut(&str)| {
        for n in notes {
            if !matches!(n, Note::Receive { .. } | Note::Sent { .. }) {
                log(&format!("-> {}", short(&format!("{n:?}"))));
            }
            let f = n.encode();
            // The pipe is loopback and the DLL drains it on its own thread.
            stream.set_nonblocking(false)?;
            let r = stream.write_all(&f);
            stream.set_nonblocking(true)?;
            r?;
        }
        Ok::<(), io::Error>(())
    };
    loop {
        if stop() {
            return Ok(());
        }
        let mut idle = true;
        match stream.read(&mut chunk) {
            Ok(0) => {
                if let Some(mut s) = bridge.session.take() {
                    s.close();
                }
                return Ok(());
            }
            Ok(n) => {
                idle = false;
                inbuf.extend_from_slice(&chunk[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        for body in take_frames(&mut inbuf) {
            let Some(req) = Request::decode(&body) else {
                log(&format!("bad request op {:?}", body.first()));
                continue;
            };
            if !matches!(req, Request::Send { .. }) {
                log(&format!("<- {}", short(&format!("{req:?}"))));
            }
            let notes = bridge.request(req);
            write(&mut stream, notes, log)?;
        }
        let notes = bridge.poll();
        if !notes.is_empty() {
            idle = false;
        }
        write(&mut stream, notes, log)?;
        if idle {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn short(s: &str) -> String {
    if s.len() > 200 {
        format!("{}...", &s[..s.floor_char_boundary(200)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_and_note_survives_the_pipe() {
        let reqs = vec![
            Request::Host {
                port: 9791,
                max_players: 8,
                session: "Default Server".into(),
                password: String::new(),
                player: "Bruno".into(),
            },
            Request::Connect {
                host: "127.0.0.1".into(),
                port: 9791,
                local_port: 9791,
                player: "P".into(),
                password: "pw".into(),
                connect_data: dpn::CONNECT_DATA.to_vec(),
            },
            Request::Enum {
                host: String::new(),
                port: 0,
            },
            Request::EnumStop,
            Request::Send {
                handle: 7,
                to: 0,
                flags: 0x28,
                data: vec![0x65, 0, 0, 0, 1, 0, 0, 0, b'x'],
            },
            Request::SetName("N".into()),
            Request::Close,
            Request::Admit {
                token: 3,
                accept: false,
                reply: 900u32.to_le_bytes().to_vec(),
            },
            Request::EnumAnswer {
                token: 4,
                answer: true,
                data: vec![1, 0, 0, 0, 2, 0, 0, 0],
            },
        ];
        let mut buf = Vec::new();
        for r in &reqs {
            buf.extend(r.encode());
        }
        // Frames split anywhere still come out whole.
        let mut partial = buf[..buf.len() - 3].to_vec();
        let mut got: Vec<Request> = take_frames(&mut partial)
            .iter()
            .map(|b| Request::decode(b).unwrap())
            .collect();
        partial.extend(&buf[buf.len() - 3..]);
        got.extend(
            take_frames(&mut partial)
                .iter()
                .map(|b| Request::decode(b).unwrap()),
        );
        assert_eq!(got, reqs);
        assert!(partial.is_empty());

        let notes = vec![
            Note::Hosted {
                hr: 0,
                local: 0x10001,
            },
            Note::Connected {
                hr: HR_HOST_REJECTED,
                local: 0,
                host: 0,
                reply: 901u32.to_le_bytes().to_vec(),
            },
            Note::Player {
                id: 2,
                flags: PLAYER_LOCAL,
                name: "a".into(),
                addr: "1.2.3.4:5".into(),
            },
            Note::Left { id: 2, reason: 1 },
            Note::Renamed {
                id: 2,
                name: "b".into(),
            },
            Note::Receive {
                from: 1,
                data: vec![1, 2, 3],
            },
            Note::Sent { handle: 9, hr: 0 },
            Note::Terminated {
                hr: HR_CONNECTION_LOST,
            },
            Note::AdmitAsk {
                token: 1,
                name: "j".into(),
                addr: "127.0.0.1:4000".into(),
                connect_data: dpn::CONNECT_DATA.to_vec(),
            },
            Note::EnumAsk {
                token: 2,
                addr: "127.0.0.1:4001".into(),
                user_data: vec![],
            },
            Note::EnumFound {
                ip: "127.0.0.1".into(),
                port: 9791,
                flags: 0x80,
                max_players: 8,
                current_players: 1,
                instance: Guid([5; 16]),
                app: dpn::APP_GUID,
                session: "S".into(),
                reply: vec![0; 8],
                rtt_ms: 3,
            },
        ];
        for n in notes {
            let mut f = n.encode();
            let b = take_frames(&mut f);
            assert_eq!(Note::decode(&b[0]), Some(n));
        }
    }

    #[test]
    fn the_reply_word_names_the_refusal() {
        assert_eq!(
            refusal_of(&900u32.to_le_bytes()),
            Refusal::IncompatibleVersion
        );
        assert_eq!(refusal_of(&901u32.to_le_bytes()), Refusal::InProgress);
        assert_eq!(refusal_of(&[]), Refusal::SessionFull);
    }
}
