//! GameSpy query and reporting (QR): what an Internet host answers on its
//! query port.
//!
//! An Internet host (`GSIntegration::Initialize`, `FUN_0047efe0`) calls the
//! QR SDK's `qr_init` (`FUN_00509290`) with the game name `robotarena2`, the
//! secret key `h4Yc9D` and the base port `0x25b7` = 9655: the SDK binds the
//! first free UDP port of `9655..9755` and sends a heartbeat
//! `\heartbeat\<port>\gamename\robotarena2` to `master.gamespy.com:27900`
//! (`FUN_0050a1f0`), again every 300 000 ms (`0x493e1` test in `FUN_00509560`
//! and its neighbours). The master then queries the host like any browser
//! does. The Internet tab's list comes from the master's TCP list service
//! (`master.gamespy.com:28900`, `\gamename\%s\gamever\%d\location\0\validate\%s\enctype\2\final\\queryid\1.1\`
//! then `\list\cmp\gamename\robotarena2\where\gamemode='openstaging'\final\`,
//! `FUN_0050aaf0`/`FUN_0050a940`) and each listed server's UDP `\status\`.
//! GameSpy's master servers closed in 2014; this port never contacts them.
//!
//! Query handling is the SDK's `qr_parse_query` (`FUN_005099b0`): a datagram
//! that contains `\queryid\` is ignored; otherwise, for each keyword in the
//! table at `0x6ff88c` — `basic`, `info`, `rules`, `players`, `status`
//! (basic+info+rules+players), `packets`, `echo`, `secure` — the matching
//! callback's text is appended; the reply ends `\final\` and every packet gets
//! `\queryid\<query>.<packet>` (`FUN_00509d10`). The game's callbacks
//! (`0x47f940` basic, `0x47f9b0` -> `0x47f3a0` info, `0x47fa00` -> `0x47f4c0`
//! rules, `0x47fa50` -> `0x47f640` players) append their own
//! `\final\\queryid\%d.%d` with the game's counter (`DAT_006f391c`, 1..100,
//! and `DAT_006f3920` = 1); the duplicates are what goes on the wire.

/// The QR SDK's base query port.
pub const QR_BASE_PORT: u16 = 9655;
/// How many ports above the base `qr_init` tries.
pub const QR_PORT_TRIES: u16 = 100;
/// The (defunct) master server and its ports — documented, never contacted.
pub const MASTER_HOST: &str = "master.gamespy.com";
pub const MASTER_HEARTBEAT_PORT: u16 = 27900;
pub const MASTER_LIST_PORT: u16 = 28900;
/// Seconds between heartbeats.
pub const HEARTBEAT_SECONDS: u32 = 300;
/// The SDK splits replies at this many bytes.
pub const MAX_DATA_SIZE: usize = 1400;
/// `gamemode` while the host takes players (the browser filters on it) and
/// once the match runs (`0x6f3940`).
pub const GAMEMODE_STAGING: &str = "openstaging";
pub const GAMEMODE_PLAYING: &str = "openplaying";

/// What the host reports.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerInfo {
    pub hostname: String,
    pub hostport: u16,
    pub hostip: String,
    pub mapname: String,
    pub gametype: String,
    pub numplayers: u32,
    pub maxplayers: u32,
    pub gamemode: String,
    pub programversion: String,
    pub hostspeed: String,
    pub weightclass: String,
    pub hazards: bool,
    pub pword: String,
    pub password: bool,
    pub players: Vec<String>,
}

/// The query port's state: the SDK's query and packet numbers and the game's
/// own counter.
#[derive(Debug, Clone, PartialEq)]
pub struct Responder {
    pub info: ServerInfo,
    query: u32,
    packet: u32,
    game_counter: u32,
}

impl Responder {
    pub fn new(info: ServerInfo) -> Responder {
        Responder {
            info,
            query: 0,
            packet: 0,
            game_counter: 1,
        }
    }

    fn game_queryid(&mut self, bump: bool) -> String {
        let s = format!("\\final\\\\queryid\\{}.1", self.game_counter);
        if bump {
            self.game_counter = if self.game_counter < 100 {
                self.game_counter + 1
            } else {
                1
            };
        }
        s
    }

    /// `0x47f940`.
    fn basic(&mut self) -> String {
        let tail = self.game_queryid(true);
        format!("\\gamename\\robotarena2\\gamever\\01.00\\location\\0{tail}")
    }

    /// `0x47f3a0`.
    fn info(&mut self) -> String {
        let i = self.info.clone();
        let tail = self.game_queryid(true);
        format!(
            "\\hostname\\{}\\hostport\\{}\\hostip\\{}\\mapname\\{}\\gametype\\{}\\numplayers\\{}\\maxplayers\\{}\\gamemode\\{}\\programversion\\{}\\hostspeed\\{}{tail}",
            i.hostname, i.hostport, i.hostip, i.mapname, i.gametype, i.numplayers, i.maxplayers, i.gamemode, i.programversion, i.hostspeed
        )
    }

    /// `0x47f4c0`.
    fn rules(&mut self) -> String {
        let i = &self.info;
        let mut s = format!(
            "\\cl_weightclass\\{}\\cl_hazards\\{}\\cl_pword\\{}",
            i.weightclass, i.hazards as u32, i.pword
        );
        s += &format!("\\password\\{}", i.password as u32);
        s += &self.game_queryid(false);
        s
    }

    /// `0x47f640`.
    fn players(&mut self) -> String {
        let mut s = String::new();
        for (n, name) in self.info.players.iter().enumerate() {
            s += &format!("\\player_{n}\\{name}\\\\");
        }
        s += &self.game_queryid(false);
        s
    }

    /// Answer one query datagram; returns the reply datagrams (none for a
    /// datagram the SDK ignores).
    pub fn respond(&mut self, query: &str) -> Vec<String> {
        if query.starts_with(';') || query.contains("\\queryid\\") {
            return Vec::new();
        }
        self.query += 1;
        self.packet = 0;
        let mut out = Vec::new();
        let mut buf = String::new();
        let has = |key: &str| query.contains(&format!("\\{key}\\"));
        let mut parts: Vec<String> = Vec::new();
        if has("basic") {
            parts.push(self.basic());
        }
        if has("info") {
            parts.push(self.info());
        }
        if has("rules") {
            parts.push(self.rules());
        }
        if has("players") {
            parts.push(self.players());
        }
        if has("status") {
            parts.push(self.basic());
            parts.push(self.info());
            parts.push(self.rules());
            parts.push(self.players());
        }
        if let Some(at) = query.find("\\echo\\") {
            let value = query[at + 6..].split('\\').next().unwrap_or("");
            parts.push(format!("\\echo\\{value}"));
        }
        parts.push("\\final\\".to_string());
        for part in parts {
            if buf.len() + part.len() > MAX_DATA_SIZE {
                self.packet_send(&mut buf, &mut out);
            }
            buf += &part;
        }
        self.packet_send(&mut buf, &mut out);
        out
    }

    /// `FUN_00509d10`: append the query id and send.
    fn packet_send(&mut self, buf: &mut String, out: &mut Vec<String>) {
        if buf.is_empty() {
            return;
        }
        self.packet += 1;
        *buf += &format!("\\queryid\\{}.{}", self.query, self.packet);
        out.push(std::mem::take(buf));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ServerInfo {
        ServerInfo {
            hostname: "Bruno".into(),
            hostport: 9791,
            hostip: "192.168.1.2".into(),
            mapname: "Combat Arena".into(),
            gametype: "DEATHMATCH".into(),
            numplayers: 2,
            maxplayers: 4,
            gamemode: GAMEMODE_STAGING.into(),
            programversion: "01.00".into(),
            hostspeed: "LAN".into(),
            weightclass: "Heavyweight".into(),
            hazards: true,
            pword: String::new(),
            password: false,
            players: vec!["Bruno".into(), "Guest".into()],
        }
    }

    #[test]
    fn a_basic_query_is_the_games_line_then_the_sdks_tail() {
        let mut r = Responder::new(info());
        let out = r.respond("\\basic\\");
        assert_eq!(
            out,
            ["\\gamename\\robotarena2\\gamever\\01.00\\location\\0\\final\\\\queryid\\1.1\\final\\\\queryid\\1.1"]
        );
        // The next query bumps both counters.
        let out = r.respond("\\basic\\");
        assert!(out[0].contains("queryid\\2.1\\final\\\\queryid\\2.1"));
    }

    #[test]
    fn status_is_basic_info_rules_players() {
        let mut r = Responder::new(info());
        let out = r.respond("\\status\\");
        assert_eq!(out.len(), 1);
        let s = &out[0];
        let order = [
            "\\gamename\\",
            "\\hostname\\Bruno\\hostport\\9791",
            "\\cl_weightclass\\Heavyweight\\cl_hazards\\1",
            "\\player_0\\Bruno\\\\",
        ];
        let mut at = 0;
        for key in order {
            let found = s[at..]
                .find(key)
                .unwrap_or_else(|| panic!("{key} after {at} in {s}"));
            at += found;
        }
        assert!(s.contains("\\gamemode\\openstaging\\programversion\\01.00\\hostspeed\\LAN"));
        assert!(s.contains("\\password\\0"));
        assert!(s.ends_with("\\final\\\\queryid\\1.1"));
    }

    #[test]
    fn replies_split_at_the_sdk_size_and_ignore_answers() {
        let mut big = info();
        big.players = (0..40).map(|i| format!("player number {i}")).collect();
        let mut r = Responder::new(big);
        // basic+info+rules fit one packet; the players' text does not fit
        // after them, so it starts the second (the SDK's buffer_add splits
        // between appended texts, never inside one).
        let out = r.respond("\\basic\\info\\rules\\players\\");
        assert!(out.len() >= 2, "{out:?}");
        assert!(out[0].starts_with("\\gamename\\") && out[0].ends_with("\\queryid\\1.1"));
        assert!(out[1].starts_with("\\player_0\\") && out[1].ends_with("\\queryid\\1.2"));
        assert!(r.respond("\\basic\\\\queryid\\5.1").is_empty());
        let echo = r.respond("\\echo\\hello");
        assert!(echo[0].starts_with("\\echo\\hello\\final\\"));
    }
}
