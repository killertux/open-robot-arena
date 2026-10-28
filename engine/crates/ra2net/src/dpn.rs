//! DirectPlay 8 host enumeration: how a LAN server list is filled.
//!
//! The original's transport is DirectPlay 8 over its TCP/IP service provider
//! (`CLSID_DP8SP_TCPIP` {EBFE7BA0-628D-11D2-AE0F-006097B01411} at `0x6b8028`),
//! through an `IDirectPlay8Peer` ({286F484D-...} / IID {5102DACF-...},
//! `CoCreateInstance` at `0x623442`). The server list's *Refresh* on the LAN
//! tab (`FUN_00498460` -> `FUN_00625cd0`) calls `EnumHosts` with an application
//! description that names Robot Arena 2's application GUID ([`APP_GUID`],
//! `0x49aa40` copies it from `0x6aaa70`), no host address (the service provider
//! broadcasts to the DirectPlay enumeration port [`ENUM_PORT`]), the default
//! retry count and interval, an infinite timeout and no user data. *IP Address*
//! does the same with `hostname` set on the host address.
//!
//! The host answers in its `ENUM_HOSTS_QUERY` case of the DirectPlay callback
//! (`FUN_00623720`, message `0xffff000a`): the reply's
//! application payload is eight bytes, two words `giNetwork` keeps
//! (`+0x23c`, `+0x240`), which the lobby refreshes (`FUN_0047ba70`) as the count
//! of occupied slots of kinds 2/3 and of kinds 0/1.
//!
//! The datagrams are the service provider's enumeration frames ([MC-DPL8CS]
//! "EnumQuery"/"EnumResponse"); the layouts below follow that specification.
//! They are not yet checked against a capture of the original: the Wine
//! builtin `dpnet` the oracle runs under does not put DirectPlay frames on the
//! wire (docs/38 §7).

use std::fmt;

/// A COM GUID in its in-memory layout (the first three fields little-endian).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    pub const fn new(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
        let a = d1.to_le_bytes();
        let b = d2.to_le_bytes();
        let c = d3.to_le_bytes();
        Guid([
            a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d4[0], d4[1], d4[2], d4[3], d4[4],
            d4[5], d4[6], d4[7],
        ])
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g = &self.0;
        let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
        let d2 = u16::from_le_bytes([g[4], g[5]]);
        let d3 = u16::from_le_bytes([g[6], g[7]]);
        write!(f, "{{{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-", g[8], g[9])?;
        for b in &g[10..] {
            write!(f, "{b:02X}")?;
        }
        write!(f, "}}")
    }
}

/// Robot Arena 2's DirectPlay application GUID, {BBEDEC00-E25F-11D5-9CF6-0001023E5510}
/// (bytes at `0x6aaa70`).
pub const APP_GUID: Guid = Guid::new(
    0xBBED_EC00,
    0xE25F,
    0x11D5,
    [0x9C, 0xF6, 0x00, 0x01, 0x02, 0x3E, 0x55, 0x10],
);

/// The DirectPlay 8 enumeration port (DPNSVR's well-known port), where a
/// broadcast `EnumHosts` without a port goes.
pub const ENUM_PORT: u16 = 6073;

/// The game port: the default `Port` of `[Begin Multiplayer Config]` (pushed
/// from `0x6b17e8` = `0x263f` at `0x49a846`); `ra2.cfg` may override it.
pub const DEFAULT_PORT: u16 = 9791;

/// The game's version string: `0x4e36e0` (startup) prints it into `0x6f3914`
/// with `"%d.%02d"` (`0x6fdc28`) from 1 and `0x14`, over the `01.00` the file
/// holds there; read back live as `31 2e 32 30 00` (docs/38 §14). It is the
/// connect data, the GameSpy `gamever` and the `:1.20:` of the session name.
pub const VERSION: &str = "1.20";

/// The session name the original's host registers: six fields, each ended by
/// `:` (the separator `0x6f36f8`), built by the create screen (`0x48ee..`)
/// and split by the joiner's `ENUM_HOSTS_RESPONSE` case into the server-list
/// columns. Live, from the original's `Host`:
/// `New Exhibition Match:DEATHMATCH:Bridge of Doom:ALLWEIGHT:1.20:512K LAN:`.
/// The joiner's Join compares the version field with its own (`0x49a228`)
/// and shows "Cannot Join Game" / "Incompatible Program Versions" on a
/// mismatch: a plain name (what this port used to register) cannot be joined
/// by the original.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionName {
    pub server: String,
    pub game_type: String,
    /// The arena's display name (`Bridge of Doom`).
    pub arena: String,
    pub weight_class: String,
    pub version: String,
    /// [`crate::sync::ConnectionSpeed::session_tag`].
    pub speed: String,
}

impl SessionName {
    pub fn encode(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:",
            self.server, self.game_type, self.arena, self.weight_class, self.version, self.speed
        )
    }

    /// The six fields, or `None` for a name without them.
    pub fn parse(name: &str) -> Option<SessionName> {
        let f: Vec<&str> = name.split(':').collect();
        if f.len() < 7 {
            return None;
        }
        Some(SessionName {
            server: f[0].to_string(),
            game_type: f[1].to_string(),
            arena: f[2].to_string(),
            weight_class: f[3].to_string(),
            version: f[4].to_string(),
            speed: f[5].to_string(),
        })
    }
}

/// The connect data a joining player sends, which the host's
/// `INDICATE_CONNECT` case (`FUN_00623720`, `0xffff000e`) compares with its own:
/// [`VERSION`] as `0x6f3914` holds it at run time, NUL included (the `Connect`
/// in `FUN_00626260` passes `strlen + 1` bytes). The static `01.00` the file
/// has there is overwritten at startup: a joiner that sent it was refused
/// with 900 by the original under the replacement `dpnet` (live, docs/38 §14).
pub const CONNECT_DATA: &[u8] = b"1.20\0";

/// Reply data of a refused connect (`INDICATE_CONNECT` returns
/// `DPNERR_HOSTREJECTEDCONNECTION` with a four-byte reply): the joiner's
/// `CONNECT_COMPLETE` case (`FUN_00623720`, `0xffff0005`) shows
/// "Connection refused.  Incompatible versions." for 900 and
/// "Connection refused.  Game already in progress." for 901.
pub const REFUSE_INCOMPATIBLE: u32 = 900;
pub const REFUSE_IN_PROGRESS: u32 = 901;

/// Service-provider frame lead byte and commands ([MC-DPL8CS]).
const LEAD: u8 = 0x00;
const ENUM_QUERY: u8 = 0x02;
const ENUM_RESPONSE: u8 = 0x03;
/// `EnumQuery.Type`: an application GUID follows / none does.
const QUERY_WITH_GUID: u8 = 0x01;
const QUERY_NO_GUID: u8 = 0x02;
/// Size of the application description block of a response.
const APP_DESC_SIZE: u32 = 0x50;
/// `DPNSESSION_REQUIREPASSWORD`, set by `Host` when a password is given
/// (`FUN_006268b0` ors `0x80`).
pub const SESSION_REQUIRE_PASSWORD: u32 = 0x80;

/// An `EnumHosts` query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumQuery {
    /// Echoed back by the host; lets the querier match replies and time them.
    pub payload: u16,
    pub app: Option<Guid>,
    pub user_data: Vec<u8>,
}

impl EnumQuery {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![LEAD, ENUM_QUERY];
        out.extend(self.payload.to_le_bytes());
        match self.app {
            Some(g) => {
                out.push(QUERY_WITH_GUID);
                out.extend(g.0);
            }
            None => out.push(QUERY_NO_GUID),
        }
        out.extend(&self.user_data);
        out
    }

    pub fn decode(b: &[u8]) -> Option<EnumQuery> {
        if b.len() < 5 || b[0] != LEAD || b[1] != ENUM_QUERY {
            return None;
        }
        let payload = u16::from_le_bytes([b[2], b[3]]);
        let (app, rest) = match b[4] {
            QUERY_WITH_GUID if b.len() >= 21 => {
                let mut g = [0; 16];
                g.copy_from_slice(&b[5..21]);
                (Some(Guid(g)), &b[21..])
            }
            QUERY_NO_GUID => (None, &b[5..]),
            _ => return None,
        };
        Some(EnumQuery {
            payload,
            app,
            user_data: rest.to_vec(),
        })
    }
}

/// A host's reply: its application description and the application payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumResponse {
    pub payload: u16,
    pub flags: u32,
    pub max_players: u32,
    pub current_players: u32,
    pub instance: Guid,
    pub app: Guid,
    pub session_name: String,
    /// Robot Arena 2's eight bytes (see [`RaHostData`]).
    pub app_reply: Vec<u8>,
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain([0])
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

fn from_utf16z(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

impl EnumResponse {
    /// The response layout: lead/command/payload, then from offset 4 the
    /// fields whose offsets are counted from offset 4.
    pub fn encode(&self) -> Vec<u8> {
        let name = utf16z(&self.session_name);
        // Variable data follows the fixed 0x58 bytes after the 4-byte header.
        let fixed = 0x58u32;
        let name_off = fixed;
        let reply_off = name_off + name.len() as u32;
        let mut out = vec![LEAD, ENUM_RESPONSE];
        out.extend(self.payload.to_le_bytes());
        let mut word = |v: u32| out.extend(v.to_le_bytes());
        word(if self.app_reply.is_empty() {
            0
        } else {
            reply_off
        });
        word(self.app_reply.len() as u32);
        word(APP_DESC_SIZE);
        word(self.flags);
        word(self.max_players);
        word(self.current_players);
        word(name_off);
        word(name.len() as u32);
        word(0); // password offset
        word(0); // password size
        word(0); // reserved data offset
        word(0); // reserved data size
        word(0); // application reserved data offset
        word(0); // application reserved data size
        out.extend(self.instance.0);
        out.extend(self.app.0);
        out.extend(name);
        out.extend(&self.app_reply);
        out
    }

    pub fn decode(b: &[u8]) -> Option<EnumResponse> {
        if b.len() < 0x5c || b[0] != LEAD || b[1] != ENUM_RESPONSE {
            return None;
        }
        let payload = u16::from_le_bytes([b[2], b[3]]);
        let body = &b[4..];
        let word =
            |at: usize| u32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
        let slice = |off: u32, len: u32| -> Option<&[u8]> {
            let (off, len) = (off as usize, len as usize);
            body.get(off..off.checked_add(len)?)
        };
        let reply = if word(4) == 0 {
            &[][..]
        } else {
            slice(word(0), word(4))?
        };
        let name = slice(word(0x18), word(0x1c))?;
        let mut instance = [0; 16];
        instance.copy_from_slice(&body[0x38..0x48]);
        let mut app = [0; 16];
        app.copy_from_slice(&body[0x48..0x58]);
        Some(EnumResponse {
            payload,
            flags: word(0x0c),
            max_players: word(0x10),
            current_players: word(0x14),
            instance: Guid(instance),
            app: Guid(app),
            session_name: from_utf16z(name),
            app_reply: reply.to_vec(),
        })
    }
}

/// The two words of Robot Arena 2's enumeration reply (`FUN_00623720` case
/// `0xffff000a`: response data = `&DAT_0072e0a8`, 8 bytes; filled from `giNetwork+0x23c` and `+0x240`
/// under their locks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaHostData {
    /// `+0x23c`: occupied slots of kind 2 or 3 (`FUN_0047ba70`).
    pub slots_23: u32,
    /// `+0x240`: occupied slots of kind 0 or 1.
    pub slots_01: u32,
}

impl RaHostData {
    pub fn encode(self) -> Vec<u8> {
        [self.slots_23.to_le_bytes(), self.slots_01.to_le_bytes()].concat()
    }

    pub fn decode(b: &[u8]) -> Option<RaHostData> {
        if b.len() < 8 {
            return None;
        }
        Some(RaHostData {
            slots_23: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            slots_01: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        })
    }
}

/// Whether a datagram is a service-provider enumeration frame (lead byte 0
/// and an enumeration command); the session transport never starts with 0.
pub fn is_enum_frame(b: &[u8]) -> bool {
    b.len() >= 2 && b[0] == LEAD && (b[1] == ENUM_QUERY || b[1] == ENUM_RESPONSE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_name_is_the_original_hosts_six_fields() {
        let live = "New Exhibition Match:DEATHMATCH:Bridge of Doom:ALLWEIGHT:1.20:512K LAN:";
        let n = SessionName::parse(live).unwrap();
        assert_eq!(n.server, "New Exhibition Match");
        assert_eq!(n.arena, "Bridge of Doom");
        assert_eq!(n.version, VERSION);
        assert_eq!(n.speed, crate::sync::ConnectionSpeed::Lan.session_tag());
        assert_eq!(n.encode(), live);
        assert_eq!(SessionName::parse("Default Server"), None);
        assert_eq!(CONNECT_DATA, b"1.20\0");
    }

    #[test]
    fn the_app_guid_is_the_exes_bytes() {
        assert_eq!(
            APP_GUID.0,
            [
                0x00, 0xec, 0xed, 0xbb, 0x5f, 0xe2, 0xd5, 0x11, 0x9c, 0xf6, 0x00, 0x01, 0x02, 0x3e,
                0x55, 0x10
            ]
        );
        assert_eq!(
            format!("{APP_GUID:?}"),
            "{BBEDEC00-E25F-11D5-9CF6-0001023E5510}"
        );
    }

    #[test]
    fn an_enum_query_names_the_application() {
        let q = EnumQuery {
            payload: 0x1234,
            app: Some(APP_GUID),
            user_data: vec![],
        };
        let b = q.encode();
        assert_eq!(&b[..5], &[0x00, 0x02, 0x34, 0x12, 0x01]);
        assert_eq!(&b[5..21], &APP_GUID.0);
        assert_eq!(b.len(), 21);
        assert_eq!(EnumQuery::decode(&b), Some(q));
        let any = EnumQuery {
            payload: 1,
            app: None,
            user_data: vec![9],
        };
        assert_eq!(any.encode(), [0, 2, 1, 0, 2, 9]);
        assert_eq!(EnumQuery::decode(&any.encode()), Some(any));
    }

    #[test]
    fn an_enum_response_carries_the_desc_and_ra2_words() {
        let r = EnumResponse {
            payload: 0x1234,
            flags: SESSION_REQUIRE_PASSWORD,
            max_players: 8,
            current_players: 2,
            instance: Guid([7; 16]),
            app: APP_GUID,
            session_name: "Bruno's game".into(),
            app_reply: RaHostData {
                slots_23: 1,
                slots_01: 2,
            }
            .encode(),
        };
        let b = r.encode();
        assert_eq!(&b[..4], &[0x00, 0x03, 0x34, 0x12]);
        // ApplicationDescSize at offset 0x0c.
        assert_eq!(&b[0x0c..0x10], &0x50u32.to_le_bytes());
        // The GUIDs end the fixed block at 0x5c.
        assert_eq!(&b[0x4c..0x5c], &APP_GUID.0);
        // The name follows, UTF-16 with its terminator.
        assert_eq!(&b[0x5c..0x60], &[b'B', 0, b'r', 0]);
        let back = EnumResponse::decode(&b).unwrap();
        assert_eq!(back, r);
        assert_eq!(
            RaHostData::decode(&back.app_reply),
            Some(RaHostData {
                slots_23: 1,
                slots_01: 2
            })
        );
        assert!(is_enum_frame(&b));
        assert!(!is_enum_frame(&[0x88, 0x01]));
    }

    #[test]
    fn truncated_responses_are_dropped() {
        let r = EnumResponse {
            payload: 1,
            flags: 0,
            max_players: 4,
            current_players: 1,
            instance: Guid::default(),
            app: APP_GUID,
            session_name: "x".into(),
            app_reply: vec![],
        };
        let b = r.encode();
        assert!(EnumResponse::decode(&b[..b.len() - 1]).is_none());
        assert!(EnumResponse::decode(&b[..0x40]).is_none());
    }
}
