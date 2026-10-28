# 38 — Multiplayer: what the original sends, and `ra2net`

Robot Arena 2 1.20's network play, recovered from `Robot Arena 2.exe` (Ghidra
12.1.4 headless + rizin 0.9.1 over the shipped exe; no network traffic of the
original was captured, see §7), and how this port implements it
(`engine/crates/ra2net`, wired into the client by
`engine/crates/ra2-app/src/net_play.rs`).

Addresses are the exe's. "Inferred" marks a name or meaning the code does not
spell out.

## 1. Architecture in one page

| Layer | Original | Where | This port |
|---|---|---|---|
| Transport | **DirectPlay 8**, `IDirectPlay8Peer` (CLSID {286F484D-375E-4458-A272-B138E2F80A6A}, IID {5102DACF-…}) on the TCP/IP service provider (CLSID_DP8SP_TCPIP {EBFE7BA0-628D-11D2-AE0F-006097B01411}, `0x6b8028`): UDP | `CoCreateInstance` `0x623442`; wrapper "giNetwork" `0x620c40..0x627000` | `ra2net::frame`/`link`/`session`: a reliable-UDP link shaped after DirectPlay 8's reliable protocol (§3), our own session-core messages |
| LAN discovery | DirectPlay `EnumHosts` with the app GUID, broadcast to the enumeration port 6073 | `FUN_00625cd0` (from Refresh, `FUN_00498460`) | `ra2net::dpn` (§2) + `session::Browser` |
| Internet discovery | **GameSpy**: QR SDK (query & reporting) on UDP 9655.., heartbeats to `master.gamespy.com:27900`; server list over TCP `master.gamespy.com:28900`, encrypted (`enctype 2`) | `FUN_00509290` (`qr_init`), `0x50a940`..`0x50b4a0` (server list) | `ra2net::gamespy` answers the query protocol; the master (defunct since 2014) is never contacted |
| Game messages | `giNetMessage` subclasses serialised into a binary stream | vtables `0x6aa694..0x6aa8b0`, dispatcher `RA2NetInterface` `FUN_004ce940` | `ra2net::message` — byte-for-byte (§4) |
| Match model | **Host-authoritative state sync**: joiners send inputs, the host simulates and sends world snapshots at the connection-speed rate; scoring events are their own messages | `Synchronizer` `0x4f77b0`, `giSyncMessage` `0x4d2b60` | `ra2net::sync`, `game::NetGame`, `net_play::{capture, apply}` (§6) |
| Voice | DirectPlay Voice objects exist (`DV_*` strings, `0x628500..`) | `0x6285e0` | not implemented |

There is **no lockstep**: nothing in the message set carries a frame number or
an input frame; the host's snapshots (positions, rotations, velocities of
every rigid body) overwrite the joiners' bodies, and the ReadMe's 1.20 notes
("removed force and torque data from sync message", "removed guaranteed
messages from host (changed to synced, non-guaranteed)", "decreased bandwidth
and update intervals for internet connections") describe exactly that design.
`Events_2Player.txt` is the 2-player *event* table (`FUN_0046d8b0`), not
network data.

## 2. Sessions, ports and discovery

* **Application GUID** {BBEDEC00-E25F-11D5-9CF6-0001023E5510}, bytes at
  `0x6aaa70`, copied into the config by `0x49aa40`.
* **Game port** 9791 (`0x263f`, pushed from `0x6b17e8` at `0x49a846` into the
  config, `FUN_00621fd0`); `ra2.cfg`'s `[Begin Multiplayer Config]` may carry
  `Port=` and `PlayerName=` (`FUN_006229e0` reads, `FUN_00622a90` writes,
  strings `0x7096c0`/`0x7096b4`).
* **Mode**: the config's `+4` is the role; multiplayer uses 2 = peer
  (`FUN_00621fd0(2, …)` at `0x49a87e`). Mode 0 would be `IDirectPlay8Server`
  (CLSID {DA825E1B-…}, `0x624e4e`); it is not used by the screens.
* **Host** (`Create Game` → `start game button` → `FUN_0049a780` →
  `FUN_006268b0`): device address port = game port, `SetPeerInfo` (name),
  `Host` with a `DPN_APPLICATION_DESC` (size 0x48): the GUID, `dwMaxPlayers`,
  the session name (`server name textbox`, default `Default Server`), the
  password with `DPNSESSION_REQUIREPASSWORD` (0x80).
* **Refresh** (LAN): `EnumHosts(desc{GUID}, host address without hostname,
  device address, no user data, count/interval default, timeout INFINITE)`.
  **IP Address**: the same with `hostname` set.
* **Enumeration reply**: the host's `ENUM_HOSTS_QUERY` case of the callback
  (`FUN_00623720`, message `0xffff000a`) returns 8 bytes, two words kept in
  `giNetwork+0x23c`/`+0x240` and set by `FUN_0047ba70`: the number of
  occupied lobby slots of kind 2|3 and of kind 0|1 (computer and human, by the
  kinds `FUN_0047d120` draws; inferred). The joiner turns a reply into a
  `giAddHostMessage` (`0x78`, `0x4d3b40`) — a server-list row.
* **Connect** (`FUN_00626260`): `Connect` with user connect data `"1.20\0"`
  (`0x6f3914`, 5 bytes: the file holds `01.00` there, but `0x4e36e0` prints the
  version over it at startup; live, §14). A 1 s timer (`0x6267a0`) counts
  `Connecting to Server     %d` from 10 (`DAT_00709860`) and cancels at 0.
* **Admission** (`INDICATE_CONNECT`, callback case `0xffff000e`): connect data
  ≠ `1.20` → refuse with reply word **900**; lobby not taking players (the
  interface's `+0x38`) → refuse with **901**. The joiner's `CONNECT_COMPLETE`
  (case `0xffff0005`) maps them to "Connection refused.  Incompatible
  versions." (`0x709728`) and "Connection refused.  Game already in
  progress." (`0x709754`); any other refusal shows "Connection was refused by
  server.\nThe game may be full or has already started." (`0x6fce70`).
* **Sends**: `IDirectPlay8Peer::SendTo` (vtable `+0x1c`) with flags
  `0x20` (`NOLOOPBACK`) or `0x28` (`+GUARANTEED`, while giNetwork's guarantee
  switch is on) in the base send `FUN_00620ee0`; classes override with
  `FUN_00621190(msg, flags, timeout)`: 0x38 (`+NONSEQUENTIAL+GUARANTEED`) for
  `CloseResultsWindow`/`AddArenaBot`, 0x20 with 5000 ms for `RemoteInput`,
  0x20 with 1000 ms for `PlayerReady` and `Sync`, flags 0 for chat text.
  giNetwork tracks async sends and resends a lost one ("[giNetwork::PLost!
  Resent!]", `0x7097bc`).
* **Timeouts**: no `SetCaps` call; DirectPlay's own keep-alive/timeout
  defaults apply.

### Wire layouts (DirectPlay 8 service provider, [MC-DPL8CS])

`EnumQuery` (port 6073 broadcast, or the host's port):

| off | size | field |
|---|---|---|
| 0 | 1 | lead byte `0x00` |
| 1 | 1 | command `0x02` |
| 2 | 2 | enum payload (echoed) |
| 4 | 1 | type: `0x01` GUID follows, `0x02` none |
| 5 | 16 | application GUID (type 1) |
| … | n | application payload (none from RA2) |

`EnumResponse` (offsets from 4 are relative to byte 4):

| off | size | field |
|---|---|---|
| 0 | 1 | `0x00` |
| 1 | 1 | `0x03` |
| 2 | 2 | enum payload |
| 4 | 4 | reply offset (RA2's 8 bytes) |
| 8 | 4 | reply size (8) |
| 0x0c | 4 | application desc size `0x50` |
| 0x10 | 4 | desc flags (0x80: password) |
| 0x14 | 4 | max players |
| 0x18 | 4 | current players |
| 0x1c | 4 | session name offset |
| 0x20 | 4 | session name size (UTF-16LE with NUL) |
| 0x24..0x3b | 6×4 | password, reserved, app-reserved offset/size (0) |
| 0x3c | 16 | instance GUID |
| 0x4c | 16 | application GUID |
| 0x5c | … | name, then the reply |

These follow the specification; they are not checked against a capture (§7).

## 3. Session transport (ours, DirectPlay-shaped)

DirectPlay 8 delivers the messages above with its reliable protocol
([MC-DPL8R]) and its core layer (connect info, name table, host migration).
`ra2net` reproduces the frame *shapes* and does the same jobs:

| frame | bytes |
|---|---|
| `CFRAME` CONNECT / CONNECTED / HARD_DISCONNECT | `0x80`(+`0x08` poll), opcode `0x01`/`0x02`/`0x04`, msg id, rsp id, u32 version `0x00010006`, u32 session, u32 timestamp — 16 bytes |
| `CFRAME` SACK | `0x80`, `0x06`, flags (`0x01` response, `0x02` mask), retry, next seq, next recv, 2 pad, u32 timestamp, [u32 mask] |
| `DFRAME` | command (`0x01` data, `0x02` reliable, `0x04` sequential, `0x08` poll, `0x10` new msg, `0x20` end msg, `0x40` user-1 = session-core), control (`0x01` retry, `0x02` keep-alive, `0x10` mask), seq, next recv, [u32 mask], payload |

Guaranteed messages are sequenced, acknowledged (cumulative + one 32-frame
selective mask, piggy-backed or a delayed SACK), retransmitted with backoff,
fragmented above 1200 bytes; non-guaranteed ones go once and are delivered on
arrival (DirectPlay also sequences those; we do not). Keep-alives every 1 s
while idle, a link times out after 10 s of silence or 12 failed retries.
A retry counts against the 12 only when the peer was heard from since the
previous one (it is alive and still has not acknowledged): DirectPlay services
its links on its own threads, so a peer busy loading a match still acks, while
ours are pumped by the game loop and a loading peer is silent. Counting those
resends dropped a peer that loaded a few seconds slower than the host (seen
under load: `link retries exhausted ... heard 14.6 s ago`); a silent peer is
left to the silence timeout.

Session-core messages (ours, `USER_1` frames): `0xC1` connect info (name,
password, connect data), `0xC2` accept (your id, host id, player table),
`0xC3` refuse (code + reply word 900/901), `0xC4`/`0xC5` player joined/left,
`0xC6` terminate, `0xC7`/`0xC8` relay. DirectPlay 8 peers form a full mesh;
here joiners link to the host only and the host relays a message a joiner
addresses to another player.

## 4. The game messages

Every message is `u32 type` + fields, little-endian, no framing
(`ra2net::stream`): words are 4 bytes (`0x58a0b0`/`0x58a1d0`), bytes 1
(`0x58a0d0`), bools 1 (`0x58a050` writes `v != 0`; `0x58a160` reads `== 1`),
strings a length word then the bytes without terminator (`0x58a0f0`/`0x58a210`).
The type is the constructor's argument to `FUN_00620d20`.

| type | class (vtable) | read / write | fields after the type |
|---|---|---|---|
| `0x65` | chat text (`0x6b17a8`) | `0x621bc0`/`0x621bf0` | string |
| `0x71` | `giGroupInfoMessage` (`0x6aa77c`) | `0x4d17a0`/`0x4d18b0` | u32 panel, u32 owner id, u32 state, 4 strings (team, team logo, robot, owner name), bool ready, u32 (1 human panel / 0 computer), bool has-robot, then the robot design stream to the end (written by the design's own `+0x4c`, read by `FUN_005b52d0`) — live, §14 |
| `0x73` | `giRemoteInputMessage` (`0x6aa7f4`) | `0x4d23d0`/`0x4d2400` | u32 robot, then `Bot::Input` (`0x4819a0`): three key words (`+4`, `+8`, `+0xc`, matched together by `0x508ca0`) and the value (`+0x10`); a reader for a peer older than 1.11 skips `+8` (`0x481a34`) |
| `0x74` | `giSyncMessage` (`0x6aa808`) | `0x4d25d0`/`0x4d2b60` | u32 snapshot size (a word: live `11 00 00 00`, §14), u32 packed size, zlib 1.1.3 stream (§6) |
| `0x76` | second text class (`0x6b17b8`) | as `0x65` | string |
| `0x12d` | `giAddArenaBotMessage` (`0x6aa7e0`) | `0x4d2050`/`0x4d2100` | 3 u32, then (when the sender holds one) an AI design to the end |
| `0x12e` | `giPrepareMatchMessage` (`0x6aa7cc`) | `0x4d1f50`/`0x4d1f80` | bool, string |
| `0x12f` | `giPlayerReadyMessage` (`0x6a88a0`) | `0x4d2220`/`0x4d2270` | u32 player, u32 ready (≠0) |
| `0x130` | `giBeginMatchMessage` (`0x6aa714`) | base | — |
| `0x131` | `giEndMatchMessage` (`0x6a8888`, ctor `0x4924f0`) | base | — |
| `0x132` | `giPlayerLoadingMessage` (`0x6aa768`) | `0x4d1590`/`0x4d15b0` | u32 |
| `0x133` | `giArenaPreviewMessage` (`0x6aa7b8`) | `0x4d1ac0`/`0x4d1ae0` | string |
| `0x134` | `giChangeGameTypeMessage` (`0x6aa790`) | same | string |
| `0x135` | `giBotDamageMessage` (`0x6aa72c`, ctor `0x4d0f10`) | `0x4d0fb0`/`0x4d1050` | words `+0x1c`, `+0x24`, `+0x20`, `+0x28`, `+0x2c`, vec3 `+0x30`, vec3 `+0x40`, word `+0x50` (order checked in the disassembly of `0x4d1050`) |
| `0x136` | `giChangeWeightClassMessage` (`0x6aa7a4`) | `0x4d1ac0`/`0x4d1ae0` | string |
| `0x137` | `giMatchWinMessage` (`0x6a5830`) | `0x4d3480`/`0x4d34b0` | u32, u32 — the two winner ids of `FUN_004172d0` (docs/24 §4) |
| `0x138` | `giTimeSyncMessage` (`0x6aa820`, built at `0x413300`) | `0x4d3550`/`0x4d3590` | u32, 4 u32 |
| `0x139` | `giAddPointsMessage` (`0x6aa834`, ctor `0x4d3620`) | `0x4d3680`/`0x4d3710` | u32, u32, vec3, vec3, bool, bool |
| `0x13a` | `giPlayerOutMessage` (`0x6aa848`) | `0x4d3480`/`0x4d34b0` | u32, u32 |
| `0x13b` | `giBotImmobileMessage` (`0x6aa85c`, ctor `0x4d3890`) | `0x4d38e0`/`0x4d3940` | u32, vec3, bool, u32 |
| `0x13c` | `giStartCountDownMessage` (`0x6aa870`) | `0x4d39f0`/`0x4d3a00` | — |
| `0x13e` | `giCloseResultsWindowMessage` (`0x6a8398`, ctor `0x487f40`) | base | — |
| `0x13f` | `giPeerDataLoadedMessage` (`0x6aa884`) | `0x4d3a60`/`0x4d3a90` | u32 |
| `0x140` | `giFractureMessage` (`0x6aa740`) | `0x4d1240`/`0x4d13d0` | u32 robot, u32, u32 count, count × 14 words |
| `0x141` | `giCriticalHitMessage` (`0x6aa754`) | `0x4d1590`/`0x4d15b0` | u32 |

Local-only ids of the same numbering (the callback's notifications, never on
the wire): `0x6e` player created, `0x6f` destroyed, `0x75` session
terminated, `0x78` add host (enumeration reply), `0x79` connect complete,
`0x7a` connect failed with a text.

## 5. The lobby (`prematch.gib`)

The host owns the lobby: each slot is a `giGroupInfoMessage`; slot kinds 0/1
human, 2/3 computer, odd kinds with a robot loaded, 4 a player who left
(`FUN_0047d120`; inferred from its branches and `FUN_0047ba70`'s counts).
Game type, weight class and arena travel as `0x134`/`0x136`/`0x133`. Ready is
`0x12f`; chat `0x65` (shown in `chat messages`). The start (dispatcher cases):
`0x12e` prepare → every peer loads (`0x132` loading, `0x13f` loaded) → `0x130`
begin and `0x13c` count-down; `0x13e` closes the results window, `0x131` ends.
The port follows it (`NetPlay::match_begun`): a joiner or spectator sends
`0x13f` once its match is built, the host sends `0x130` + `0x13c` once every
peer in the lobby has, and nobody's countdown runs before that. Without the
gate each peer started its fight when its own load finished; under load the
loads differ by seconds, so the host's scripted blows reached a peer still
loading. On the host a remote human's robot is driven by its inputs only and
stands still until the first one arrives (it had fallen to the AI, whose
blows then scored: `points=42,131,0,18` instead of `42,45,0,0`).
`AllowNonStandardMultiplayer` in `ra2.cfg` exists; its use was not traced.

## 6. In the match

* **Inputs**: `HumanLocal` (vtable `0x6a8ebc`, `+0x10` = `0x4b2dc0`) sends
  every `Bot::Input` it delivers to its own robot as `0x73` to the host
  (`0x4cff10`), non-guaranteed, 5 s timeout. The host's `HumanRemote`
  applies them.
* **Snapshots**: the `Synchronizer` (global `0x718404`, update `0x4f77b0`)
  adds the frame time to an accumulator; at the interval it zeroes it, builds
  `giSyncMessage` (`0x4d2470`) and sends it; it keeps a bandwidth estimate
  ("Bandwidth:(%f,%f,%f)K/sec Updates:(%f,%f,%f)", `0x6fec64`). The interval
  is the create screen's *Connection speed* (`FUN_0048e4a0`):

  | label | interval (s) | rate | 2nd float |
  |---|---|---|---|
  | LAN | `0x3c99999a` 0.01875 | 53.3 Hz | 32746 |
  | T1 (512K/s) | 0.03125 | 32 Hz | 8000 |
  | Cable (384K/s) | `0x3d2a64c3` 0.0416 | 24 Hz | 6000 |
  | DSL (256K/s) | 0.0625 | 16 Hz | 4000 |
  | DSL (128K/s) | 0.125 | 8 Hz | 2000 |

* **Snapshot body** (`0x4d2b60`, uncompressed; the writer counts its size as
  it goes and sends that count as the float): u32 clock (`FUN_00469b90`),
  u32 (`FUN_00609060`), u32 body count, bool partial (`+0x40`), f32 robot
  count; per robot f32 + one f32 per component; per controller u32 id + f32
  (vtable `+0x70`); per device u32 id, u8 kind, then kind 1: bool + 4 f32,
  kind 4: f32, kind 5: bool, u32, 3 f32; per body u32 id, bool awake,
  position (body `+0x30`), rotation (`+0x40`, scalar first, as `0x45e9b0`
  copies it), and when awake linear (`+0x54`) and angular (`+0x64`)
  velocity. Sizes: 17-byte header, 33/57 bytes per body. The controller and
  device lists carry **no count**: both ends walk lists built from the same
  designs (`ra2net::sync::SyncSchema`). Errors the reader prints: "ERROR: Bad
  Rigid Body ID in SyncList", "Bad Controller ID", "Sync ignored - not in
  game yet".
* **Scoring** comes from the host as `0x135`/`0x139`/`0x13b`/`0x13a`/`0x140`/
  `0x141`, the end as `0x137` (winner ids) then `0x131`; how, in §6.1.

This port: `net_play::capture` writes every live robot part body
(id = robot << 16 | part) with empty robot/controller/device lists;
`net_play::apply` sets each body's transform and velocities; the host drives
remote robots through `match_mode::Session::set_remote_drive`. Our
remote-input key words name the drive axes (0 throttle, 1 steer, value in
percent), not a `.bot` control binding — a joiner of this port and one of the
original would not understand each other's inputs.

## 7. Evidence and wire-compatibility status

* Static: everything above is from the exe (Ghidra decompilation of
  `0x4ce5f0..0x4d3d00`, `0x620c40..0x62b000`, `0x509000..0x50c200`, the RTTI
  vtables of the 24 `gi*Message` classes; rizin disassembly where the order of
  chained stream calls mattered). No live capture: the oracle runs the game
  under flatpak Wine 9.0 whose `dpnet.dll` is Wine's builtin ("Wine builtin
  DLL"; its trace strings mark most interface methods `stub` or
  `Semi-stub`), which does not implement the DirectPlay 8 protocol, so the
  original under Wine has no DirectPlay frames to sniff; Microsoft's `dpnet`
  would need a download (not done). §14 replaces that `dpnet` with our own and records what
  the original then sent.
* **Wire-compatible** with the original: the game-message payloads (§4) and
  the snapshot layout, the GameSpy query replies (§8), the enumeration frames
  as far as the specification goes (§2).
* **Not wire-compatible**: the session transport (§3) — the original's
  DirectPlay core and reliable layer are not reproduced byte for byte, so an
  original client cannot join a port host or vice versa; the inputs' key
  words (§6).

## 8. GameSpy (Internet) — the defunct master

`GSIntegration::Initialize` (`FUN_0047efe0`): `qr_init(NULL, 0, 9655,
"robotarena2", "h4Yc9D", basic, info, rules, players)`; the SDK binds the
first free UDP port in 9655..9754 (`FUN_00509290`), heartbeats
`\heartbeat\<port>\gamename\robotarena2` to `master.gamespy.com:27900`
(`FUN_0050a1f0`) every 300 000 ms. The Internet tab asks the master
(TCP 28900) with `\gamename\%s\gamever\%d\location\0\validate\%s\enctype\2\final\\queryid\1.1\`
and `\list\cmp\gamename\robotarena2\where\gamemode='openstaging'\final\`
(`FUN_0050aaf0`), then queries each server's `\status\`. The callbacks'
texts (`0x6f3990`, `0x6f3a50`, `0x6f3a40`, `0x6f3a80`, `0x6f3ab0`):

```
basic   \gamename\robotarena2\gamever\1.20\location\0\final\\queryid\N.1
info    \hostname\%s\hostport\%d\hostip\%s\mapname\%s\gametype\%s\numplayers\%d\maxplayers\%d\gamemode\%s\programversion\%s\hostspeed\%s\final\\queryid\N.1
rules   \cl_weightclass\%s\cl_hazards\%d\cl_pword\%s\password\%d\final\\queryid\N.1
players \player_%d\%s\\ ... \final\\queryid\N.1
```

`N` is the game's own counter (`DAT_006f391c`, 1..100, bumped by basic and
info); the SDK then appends its `\final\` and `\queryid\<query>.<packet>`
(`qr_parse_query` `FUN_005099b0`, `packet_send` `FUN_00509d10`), splitting at
1400 bytes. `gamemode` is `openstaging` in the lobby, `openplaying`
(`0x6f3940`) in a match. `ra2net::gamespy::Responder` produces these replies;
nothing in the port talks to a master server.

## 9. What is implemented and tested

* `ra2net` unit tests: every message's bytes; the snapshot size equals the
  writer's running count; zlib round trip; the speed table and pacing; the
  enumeration frames and the app GUID bytes; frame layouts; the link through
  ~25 % pseudo-random loss (order, once-only, fragmentation), keep-alive,
  timeout, hard disconnect; the lobby's ownership rules; the GameSpy replies.
* `ra2net/tests/loopback.rs` (two or more sessions on 127.0.0.1): discovery
  by enumeration; join + lobby (settings, a 3000-byte robot design, ready,
  chat both ways); refusals 900/901/full; connect timeout; a 2 s synced match
  (inputs to the host, ~53 Hz snapshots back, a late joiner refused 901, the
  win, the joiner leaving); the host ending the session; a silent joiner timing
  out; joiner-to-joiner relay.
* `ra2-app/tests/netplay.rs`: through the shipped screens (Create Game →
  Start Game hosts and opens the prematch; Refresh lists the host; the row +
  Join reaches the prematch on the joiner; ready; disconnect) and a real
  `Duel` on the box arena where the joiner's robot, driven by its remote
  input on the host, ends where the host's snapshot puts it.

Open: the DirectPlay core/reliable byte layouts; voice; host migration
(DirectPlay supports it, RA2's use not traced); the GameSpy `\secure\`
validation reply; the meaning of several fields marked inferred; the
fracture (`0x140`) and critical-hit (`0x141`) messages in the port's match
(the joiner derives both from the same hits); the chat display's retention
timer and the exact spectator admission rule (§10, §11).

## 6.1 Who prices a hit, who scores it, what a joiner shows

Recovered from `FUN_00417330` (the contact-damage entry), `FUN_004148f0`
(`plus.damage`, flames), `FUN_004175b0` (the hit record and the points rule),
`FUN_00417510` (the apply), `FUN_004177e0`/`FUN_004178a0` (the points),
`FUN_00412fc0` (the per-frame immobility rule), `FUN_00418040` (a robot out)
and the dispatcher cases of `FUN_004ce940`. The match object keeps `+0x8c`
"network match", `+0x8d` "host", `+0x8e` "fighting", `+0x8f` "over".

* **A hit** (`FUN_00417330(attacker, atk part, victim, vic part, damage,
  point, normal, ...)`): ignored when the match is over, the damage is 0 or
  either part is flagged off (`FUN_00401860`). From 10.0 up (the apply
  threshold, `10.0 <= param_5`), on the host or in a local match: the host
  builds a **`giBotDamageMessage`** (`0x4d0f10`, fields: attacker robot
  `+0x1c`, victim robot `+0x20`, attacker part `+0x24`, victim part `+0x28`,
  effective damage `+0x2c`, two vec3 at `+0x30`/`+0x40`, a word `+0x50`),
  sends it, and runs the record `FUN_004175b0` itself. `FUN_004148f0`
  (flames, `plus.damage`) skips the threshold and sends the same message with
  attacker -1.
* **The apply** (`FUN_00417510` -> the victim part's vtable `+0x6c`) runs on
  **every** machine from the same call, so in the original each end
  prices and applies its own physics' contacts, and only the *record* is
  the host's. The joiner reaches the record through the message: `0x135` ->
  `FUN_00417ae0` (joiner and network only) -> `FUN_004175b0`, which writes
  the damage list entry (`FUN_004177d0`, merged within a window for the same
  attacker), plays the hit sound for big hits (>50: 5, >100: 10, >300: 20,
  `FUN_004d53e0`) and draws the damage number.
* **Points** (`FUN_004175b0`, not King of the Hill; in Team Match only
  across teams) call `FUN_004177e0(attacker, damage, vec, vec, 1, 0)`, which
  does anything only on the host or in a local match: it runs `FUN_004178a0`
  (floating score text, then `score += (int)damage`) and, on the host,
  sends a **`giAddPointsMessage`** (`0x4d3620`: player, float points, two
  vec3, bool "adds to the score", bool "show for any player"). The joiner's
  `0x139` -> `FUN_004178a0` adds the same number. The arena script's
  `plus.addPoints` is the same function, so it too is the host's.
* **Immobility** (`FUN_00412fc0`, host or local, once the fight is on):
  under 10 s of stillness a flagged robot is cleared (`FUN_00412240(r, 0)`,
  **`giBotImmobileMessage`** `(robot, position, 0, 0)`); from 10 s it is
  flagged and the message carries `(robot, position, 1, seconds)`; from 20 s
  `FUN_00418040(robot, 2)` puts it out. The joiner's `0x13b` -> `FUN_004120e0`
  flags it and shows the icon.
* **Out** (`FUN_00418040(robot, reason)`, 1 destroyed, 2 immobilised, 3 the
  third branch): shows the banner, and on the host sends a
  **`giPlayerOutMessage`** (`0x4d3800`) and, when one robot or team is left,
  `FUN_004170e0` -> `giMatchWinMessage`. The joiner's `0x13a` -> the same
  function; it only follows (`FUN_004170e0` runs only on the host or locally).

**This port** goes one step further than the original on damage, as asked:
the host is authoritative for damage as well as score. A joiner's own contact
and hazard pricing is discarded (`Duel::net_role == Joiner`), and its immobility rule,
its arena script's eliminations and its script's `addPoints` are off;
everything it shows is a host event (`ra2sim::NetEvent`, converted by
`ra2-app/net_play::{event_message, message_event}`): a hit is applied whole to
the same component with the host's number (so the HUD numbers, damage dealt
and component state match), points are added exactly as the host added
them (truncated float add, or the integer script add), out/immobile follow.
The host sends them guaranteed (`broadcast_guaranteed`: a lost
`giBotDamageMessage` would leave a score behind for good), and the end as
`0x137` winner robot ids then `0x131`; a joiner not yet finished finishes
with the host's winners. Inferred and ours: robot index stands for the
player id in `0x13a`; the immobile "seconds" are the 10 s warning point;
`0x139`'s second flag marks a script-origin integer add; the vec3 roles
(point, normal); `0x140`/`0x141` are not sent (a joiner's break-offs follow
from the same hits). Tests: `ra2sim` `the_host_prices_and_the_joiner_shows_what_it_is_told`;
`ra2-app/tests/netplay.rs` `a_scripted_match_across_five_processes_counts_every_hit_once`
(five processes, scripted blows, a joiner's own blow that must count for nothing).

## 10. Spectators

Evidence: `prematch.gib` has `spectators label` and `spectator list` ("list of
the spectators (joined but not in the match)"); the prematch window registers
the event `0x64` (a peer was created) with `FUN_00492ef0`, which plays
`Sounds\spectator_joined.wav` (`0x492f11`) and then, on the host, adds the
peer to the lobby (`FUN_0047f760`); `0x67` (peer gone) is `FUN_004930a0` ->
`FUN_0047f840`. `FUN_0047c560(player)` writes the `spectator [TextBox]` of that
player's cell (`activeplayer.gib`, "Spectator"). The results window has its own
"Spectator" text box (`0x487686`). No message type is specific to spectators.

So a spectator is **a peer without a player panel**: it joins like anyone
(same admission, same `0x64` sound on every end), is listed in `spectator
list`, and takes part in the session's broadcasts. What the exe does not say
(and no live capture is possible) is what makes a peer a spectator and what it
sees in the match; this port's reading:

* The first four peers take `player1..4 group`; later ones are spectators
  (lobby slot kind 5, ours; the session admits 8). A spectator cannot choose a
  robot or be ready and does not count for the start; when a player leaves,
  the host gives the free panel to the longest-waiting spectator.
* At the start every peer gets `0x12e` and loads; spectators get no robot
  (`robot_of` lacks them, the match has one entry per panel), send no
  `0x73`, follow the snapshots and all scoring messages, and end with the
  host's `0x137`.
* Their view is the AI-versus-AI one: no driven robot, no chase camera, the
  config's `DefaultCamera` (`watch`) and `C`/`F2`-`F4` to change it, the
  four status panels, damage numbers only for a human player (so none).
  They can chat (below).

## 11. In-match chat

`gamewindow.gib`: `chat group` (note: *show/hide this group using the "chat"
key (backslash?)*), its `chat entry` (TextBox) and `chat displaybox`
(`Glyph` + `DisplayBox`, `<text font fg_mcn14>`, 200x100 at (10, 110); note:
*display a few lines of chats here and clear after a few seconds*). Recovered:

* `0x475ee0` opens the group on `\` only when `FUN_004145f0()` is 1 (the game
  type "multiplayer"), so chat is a network-match feature.
* The entry's Enter handler is `FUN_00475240`: nothing for an empty text,
  else `"[" + name + "] " + text` (`0x6f3520`, `0x6f351c`), built as a
  **`0x76`** message (`0x621920`), sent with flags 0 to everyone
  (`0x6218c0` -> `FUN_00621190`), the entry cleared and the group hidden.
  The prematch's chat (`0x492bc0`) is the same code with the `0x65` text.
* The receiver (`0x4ce940` case `0x76`) posts the event `0x70`, which the match
  window registers (ctor `0x4749c0`) for the display box; `chat.wav` is one of
  the nine sounds the match preloads (`0x415849`).

Implemented: `\` (multiplayer matches only) opens the group; Enter sends
`[name] text` as `0x76` to all and closes it; Escape closes it unsent (the
original's Escape opens the pause popup whatever has focus; ours lets the open
entry take it); every line that arrives, ours included (the original gets its
copy through loopback), joins the box, plays `chat.wav`, and is drawn in the
box's own font and place. Ours, not recovered: a line lives 8 s and the box
holds 6 lines (the retention timer of the original was not traced). The lobby
chat now uses the same `[name] text` format.

**Rendered and checked (fx/loose-ends).** `ra2-app --example chatshot` renders the
match HUD with chat lines and the open entry (frames `chat_lines.png`,
`chat_open.png`, scratchpad): white `fg_mcn14` text from (10, 110), wrapped at
200 px, the `CHAT:` label and entry below. The box is 100 px tall and the paragraph
renderer does not clip, so with more rows than fit (11 rows after wrapping) the text
ran 100 px past the box; `hud::fit_chat_lines` now wraps the lines itself and keeps
the newest rows that fit (5 rows at fg_mcn14's 18 px pitch; the original's rule for a
full box was not recovered, the file only says `Scrollable` and "clear after a few
seconds"). Test `hud::tests::the_chat_box_wraps_at_its_width_and_keeps_the_newest_rows_that_fit`.
**No original reference could be taken:** no capture in `local/oracle/original` shows
chat text, and the original's lobby is not reachable offline in the Wine oracle:
Multiplayer -> Create Game -> Start Game (Local network only, Deathmatch, Combat Arena)
does nothing (no DirectPlay host under Wine), so the prematch window with its
`chat messages` box never opened. The layout is therefore checked against
`gamewindow.gib` only.

## 12. Who consumes the session's events (fx/net-events)

**The bug.** The windowed app runs, every update (`window.rs about_to_wait`):
`NetPlay::tick` (screens and lobby), then `before_step`, the step, then
`after_step` (the in-match half). Both used to call `NetGame::poll`, which drains
the session. `tick` ran first, handled the lobby and gate events, and threw the
rest away in a `_ => {}`: snapshots, chat lines and the host's scoring messages
(`0x135`/`0x139`/`0x13b`/`0x13a`/`0x137`) that arrived in the idle time between
the last `after_step` and the next `tick` (nearly all of them: the app idles
between 60 Hz updates, and `after_step` only sees what lands during the step).
`after_step` also dropped lobby/session events it polled (a `Terminated` or
`PlayerLeft` that landed during the step). The headless tests never saw it
because their loops call `after_step` without `tick`, with no idle time between.
Seen with the new test: a joiner finished the scripted match with `ended=false
points=42,0,0,0 taken=0.00,42.70,...` (the host: `ended=true points=42,45,0,0`)
and the host missed the joiners' chat.

**The fix.** `NetPlay` owns an `inbox`. `pump()` polls the session once into it
(only the newest snapshot is kept); `tick` takes the screen, lobby and gate events,
`after_step` takes snapshots, chat, host messages and the session-free match state
(`Loaded`, `Begin`, `CountDown`, `RemoteInput`, via the shared `absorb_state`).
Each event is taken by exactly one consumer, in order, whichever runs first.

**Same class, found on the way.**
- *Keys never reached the host.* The window records the held scan codes
  (`DriveInput::keys`); `before_step` sent only the abstract throttle/steer axes, so a
  windowed joiner's robot never moved on the host. The held set (scan codes 0..96) now
  goes out as a third remote-input event, `[AXIS_KEYS, w0, w1]` + value `w2` (ours,
  like the axes: the original's key words name a `.bot` control binding), whole each
  time so a lost update is repaired by the next, and the host folds it into the remote
  robot's `keys`.
- *The multiplayer Start Game button started an exhibition match.* `window.rs` treated
  every click on a widget named `start game button` as the exhibition's; the multiplayer
  create screen has one of the same name, so the host went straight into an AI match
  instead of the lobby. It now requires the clicked screen to be in `exhibition/`.
- *A second match in the same lobby started on stale state.* `begun`, `loaded`,
  `end_sent` and the `in_match` switch were only reset by `leave()`, so a rematch ran
  without waiting for the joiners, never sent its end, and lobby chat went out as match
  chat. `NetPlay::end_match` (called when the results are closed) and `NetGame::end_match`
  (schema off, host accepts joiners again) clear them; a `Prepare` (joiner) and the host's
  begin click also reset the gate, in event order. Left-over match events are dropped
  with them, and a session that ends (`Terminated`, refused) clears the inbox.

**Tests.** `tests/netplay.rs`: `the_windowed_frame_order_delivers_every_event_once`
(five processes, each running tick / before_step / step / after_step with the idle
between frames; failed before the fix, passes after, also with 16 busy loops on 8
cores), `a_rematch_in_the_same_lobby_starts_from_a_clean_gate`, the existing
five-process test; unit test `held_keys_travel_whole_and_a_release_clears_them`.

**Live, two windowed processes** (Xephyr :21/:22, one scratch `ORA_SAVE_DIR` and game copy
each, loopback, input injected with XTEST; screenshots in the session scratchpad
`netev/`): host Create Game -> Start Game, joiner IP Address 127.0.0.1 -> Connect, both in
the prematch lobby, Begin Match. Both drove forward; chat both ways showed on both
(`[Player] hello from joiner`, `[Player] hi from host`); the host's robot points went 13 ->
26 -> 39 on both HUDs (the joiner a step behind) and both ended on the same Match Results
(Scout immobilized, The Boxer winner with 39). With the unfixed build the clicks never
reached the lobby (exhibition match) and the joiner's keys never moved its robot.

**Open (closed in section 13).** A remote human that leaves mid-match leaves its robot standing (no AI takeover,
no `PlayerOut`); `EndMatch` (`0x131`) is sent but a joiner ends on `MatchWin` alone;
chat keeps the sender's lobby name, which defaults to `Player` for both ends.

## 13. Leaving, ending and names (fx/net-leave)

The four items section 12 left open, recovered from the binary (`objdump` listing of the exe, Ghidra via REA; the
original's multiplayer lobby does not open under Wine, so nothing here is live).

**The dispatcher.** The network callback `FUN_004ce940` (registered by `FUN_004ce7e0`, called from the create-game
handler `0x49a81e`) switches on `message id - 0x65` through the byte table `0x4cfce8` and the jump table `0x4cfc60`.
Cases of note: `0x6e` player created (listener vtable `+0x10`/`+0x30`), `0x6f` **player destroyed** (`0x4cfa0e`),
`0x73` remote input (`0x4cedea` -> the game window's `FUN_00416da0`, no state change), `0x75` session terminated
(`0x4cf20f`, posts UI event `0x6c`), `0x131` end match (`0x4cf510`).

### (a) A remote human leaves mid-match

`0x6f` calls the listener's `+0x10` and `+0xc` slots (`0x4d0020`, `0x4d00b0`): they post UI events `0x67` and
`0x64` (`FUN_005882f0`) carrying the player's name and id. The only consumer of `0x67` is the **prematch window**:
its constructor registers `0x4930a0` for it (`0x490fff`-`0x49103a`, next to `0x64` -> `0x492ef0`). `0x4930a0`
removes the player from the window's list (`+0x1f0`, `0x495c90`) and, **on the host, while the prematch window's
"in progress" byte (`+0x1a8`, `0x491d60`) is set**, removes the player's entry from the session's slot list
(`FUN_0047f840`: erases the 0x14-byte entry with that id from the list at `+0xa8` and decrements `+0x9c`). Nothing
reaches the game window: no `FUN_00418040`, no `giPlayerOutMessage`, no AI handed the robot (a robot's driver is
the `HumanRemote` control, and the last keys received stay). So the original's answer is: **the leaver's robot
stands (or keeps rolling on its last keys) until the ordinary rules end it** (immobility after 20 s, a destroyed
chassis, the clock), and the host's lobby loses the slot. That is what this port already did; the new test
`a_remote_human_who_leaves_leaves_the_robot_standing` pins it (host and joiner in two processes, the joiner drops at
fight step 40; 400 steps later the host's lobby has one slot, robot 1 is still driven from outside, moved less than
0.3 m, is neither immobilized nor out).

### (b) `giEndMatchMessage` (`0x131`)

Case `0x131` finds the window `prematch win` (`0x6ef844`) and calls its method **`FUN_00495250`**: if the game
window's `+0x8e` ("fighting", set by `FUN_00416360`, cleared by the end routine around `0x4168ab`) is set, it closes
`in game popup win` when the match is over (`+0x74`) and calls the `Game Window`'s vtable `+0xd0` with 0
(`FUN_00475c30`: `Show(0)`, which brings `prematch win` back); then, always, `FUN_004d0200(1)`: the **game phase**
object (`0x71607c`, phase at `+0x18`, protected by the critical section `0x716040`) goes back to phase 1, the lobby's
(`0` idle, `1` lobby, `2` loading, `3` loaded, `4`/`5` begun, `6` ending; a phase may only advance one step, phase 1
is always allowed) and it resets the lobby's ready flags (`FUN_0047bbb0`). The results window is its own popup
(`0x13e` closes it on every end), so a finished match's results stay up on the joiner after `0x131`.

Port: a joiner's `after_step` takes the host's `EndMatch`: if its match is still running it sets
`NetPlay::take_leave_match` (the window drops the session and returns to the lobby, `end_match`), and in every case
`NetGame::end_match` (the session stops syncing, the lobby phase). A host that quits its match before the end was
sent (`NetPlay::end_match` with a session still running) broadcasts `EndMatch` itself, so the joiners are not left
in a match nobody hosts. Tests: `a_joiner_leaves_the_match_when_the_host_ends_it_early`,
`a_finished_match_keeps_the_joiners_results_after_the_end_message`.

### (c) The player name

The network object's constructor (`0x621e84`-`0x621f7a`) copies the literal **`Default Player`** (`0x709644`) into
its name string (`+0x244`) and `Generic Network Session` (`0x709654`) as the session name; the game port is `0x263f`
(9791). `FUN_006229e0` (called at `0x622941` when the object is built) reads `ra2.cfg`'s
`[Begin Multiplayer Config]` section: `PlayerName=` (a non-empty value goes through `FUN_00622660`, the name setter)
and `Port=`; `FUN_00622a90` writes them back. Neither the registry (`SOFTWARE\Gabriel Interactive\Robot Arena 2` is the
install path) nor the career profile is used. The prematch window then puts the name in the editable `screen name`
box (`0x49105a`-`0x491085`; `prematch.gib`'s own sample text is `Mister Roboto`, never shown) and its edit handler
`FUN_004931a0` sets the name again (`FUN_00622660`), refreshes the peer info (`FUN_00623580`) and updates the lobby
entry (`FUN_0047f8a0`). So in the original two players with no `PlayerName=` are **both** `Default Player` until one
edits the box, which is what this port now does by default (`DEFAULT_PLAYER_NAME`; `NetPlay::load_config` reads
`PlayerName=` and `Port=` from the install's `ra2.cfg`; `show_lobby` fills `screen name`). Test:
`the_default_player_name_is_the_originals_and_ra2_cfg_replaces_it`. Editing the box now renames the player on the network: see 13.1.

### (d) Which robot each end follows and drives

`Session::exhibition` builds the match for robot 0: the chase camera, `human_robot` (which the watch and orbit cameras
follow, `FUN_004014b0(0)`) and the F1 reminder all named robot 0, and `match_session` only changed `player_robot` and
`human_robot` afterwards. So the **joiner's chase camera followed the host's robot** (robot 0, the red one of the two
panels) while its keys drove robot 1: the "host camera shows the red robot" of the two-window loopback match was this
(the host's own camera was right; the joiner's pointed at the host). Fixed with `Session::set_player_robot`
(chase camera, default camera after the intro, human robot, reminder), called by `match_session`. Test
`each_player_drives_and_follows_their_own_robot`: after both hold forward from fight step 20, the host reports
`chase=0 human=0 driven=0`, the joiner `chase=1 human=1 driven=1`, and on the host both robots have moved (the joiner's
keys drive robot 1 there). Before the fix the same test reported `chase=0` on the joiner.

**Live, two windowed processes** (Xephyr :31/:32, own scratch `ORA_SAVE_DIR` and hard-linked game copy each,
loopback, XTEST input; screenshots in the session scratchpad `nl/`): host Create Game -> Start Game, joiner IP Address
127.0.0.1 -> Connect; both lobbies list two `Default Player` panels and a `Chat Alias`/`screen name` of `Default Player`
(the original's default, (c)); Begin Match, Skip Intro; the host's chase camera follows Scout (robot 0, the `RED ZONE`
team: this is the "red robot" of the earlier report, and it is the host's own), the joiner's follows The Boxer
(robot 1); both holding `W` drove their own robot (the scenery slides in both windows). Killing the joiner's process
mid-match: the host's window keeps running, its robot stands (no AI, no banner) and 10 s later the HUD shows
`IMMOBILITY WARNING 7` over it (docs/32 section 6).

### 13.1 The `screen name` edit handler `FUN_004931a0` (fx/shell-polish)

Recovered from the binary (Ghidra via REA, `objdump`); the lobby does not open under Wine, so nothing here is live.

* **Registration.** The prematch constructor binds the `screen name` box's handler with the same box call as the chat
  entry's `0x492b90` (`0x490c40` `mov eax,0x4931a0` ... `0x490c67` `push 3` / `call 0x5ea170`, against `0x490baa` for the chat box):
  event 3 of an edit box, the one that fires when the text is entered (Enter).
* **The handler.** With the box present (`+0x1f8`) it reads the box's text (`FUN_00412500`), then `FUN_00623150` (the network
  object, `DAT_0072e0a4`), `FUN_00622660` (`0x622660`, the name setter: copies the string into the object's name at `+0x248`; an
  empty string clears it), `FUN_00623580` (`0x623580`), and, when the session exists (`FUN_0047ba50`), the lobby is in progress
  (`FUN_00491d60`) and the network object is up (`FUN_004cfdf0`), `FUN_0047f8a0` (walks the lobby's 0x14-byte entries and
  replaces the one whose id matches, i.e. our own entry). The `0x6f4620` / `0x6f4358` of the earlier note are string
  addresses, not message ids (`"create mp game win"`).
* **The message.** `FUN_00623580(name)` builds a DirectPlay 8 `DPN_PLAYER_INFO` on the stack (`dwSize` `0x18`, `dwInfoFlags` `1` =
  `DPNINFO_NAME`, `pwszName` = the name widened with `MultiByteToWideChar(0, 1, ..., 0x80)`) and calls the peer interface's
  vtable `+0x50` (`SetPeerInfo`) with `pvAsyncContext` 0, no handle and flags `0x80000000` (`DPNOP_SYNC`). So the wire
  message is DirectPlay's own peer-info update, not a game message.
* **The receivers.** The network callback `FUN_00623720` has a case `0xffff0010` (`DPN_MSGID_PEER_INFO`): it calls `GetPeerInfo`
  (vtable `+0x54`) for the sender's id, converts the name to ANSI (`WideCharToMultiByte`, 0x80), and queues a record of class
  `0x72` (`FUN_00621a80(0x72)` at `0x6239a1`; the create-player case queues `0x6e`, destroy `0x6f`) carrying the id (`+0xc`) and the
  name (`+0x20`). The game dispatcher `FUN_004ce940` routes `0x72` (byte table `0x4cfce8`, jump `0x4cf99d`) to the listener's
  vtable `+0x28` with the name. (What that listener slot does with it was not followed further; the port renames the lobby
  entry and the player list, the only things a name shows in.)
* **Port.** `ra2net::Session::set_name` (joiner: `CORE_SET_NAME` to the host; the host answers every player with
  `CORE_PEER_INFO`, id and name; the host's own change is sent straight out), `Event::PlayerRenamed`,
  `GameEvent::PlayerRenamed` (the lobby slot is renamed), `NetGame::rename` (our own slot, then the session). `NetPlay::commit_screen_name`
  is called by the window on Enter in the focused `screen name` box (an empty box is ignored; the original would clear the name), and the box
  is not overwritten from the lobby while it has the focus. Test `ra2net/tests/loopback.rs::renaming_a_player_reaches_the_other_end`
  (host and joiner on loopback: the joiner's rename reaches the host's lobby and session table; the host's reaches the joiner).
  **Open:** the original also writes the name back to `ra2.cfg`'s `PlayerName=` (`FUN_00622a90`) at some point (not found to be tied to this handler).

## 14. The original under Wine: a replacement `dpnet`, and what it sent (fx/dplay)

### 14.1 Why the original's lobby does not open under Wine

Live, flatpak Wine 9.0, `WINEDEBUG=+dpnet` and Frida hooks on the `IDirectPlay8Peer`
vtable (capture `mp-dpnet-builtin`): entering Multiplayer creates the peer
(`CoCreateInstance` `0x623442`, `Initialize`, `EnumServiceProviders(TCPIP)`) and
starts `EnumHosts` — Wine: `fixme:dpnet:IDirectPlay8PeerImpl_EnumHosts ... stub`.
Create Game → Start Game (`FUN_006268b0`) builds the device address (`SetSP`
TCP/IP, `AddComponent port 9791`), calls `SetPeerInfo` (Wine: `Semi-stub`) and
`Host` — Wine: `fixme:dpnet:IDirectPlay8PeerImpl_Host ... stub`, **returning
`0x80004005` (E_FAIL)** (Frida, return value). `FUN_006268b0` returns 0 on any
failure and the screen stays: that is "Start Game does nothing". Wine's `Connect`
is a stub too (its trace string `(%p)->(%p,%p,%p,%p,%p,%p,%lx,%p,%p,%p,%lx): stub`).
Nothing in Wine's builtin puts a DirectPlay frame on the wire, so there is no
override to switch on: the builtin is the only `dpnet` in the prefix and the
runtime (no native `dpnet.dll` on this machine), and Microsoft's would be a
download (not allowed here).

### 14.2 The replacement: `tools/dpnet` and `ora-dpnet-bridge`

`tools/dpnet/ora_dpnet.c` is a `dpnet.dll` written from the public DirectPlay 8
interface (method order, structures, message ids, flags, result codes of
`dplay8.h`/`dpaddr.h`): `IDirectPlay8Peer`, `IDirectPlay8Address` and their class
factory, freestanding C built with the host's `gcc -m32` and GNU ld's `i386pe`
(`tools/dpnet/build.sh`, imports from Wine's own `kernel32`/`ws2_32`). It keeps
what the game reads back at once (the player table for `GetPeerInfo`, the
application description, addresses) and forwards the rest over a loopback TCP pipe
to `ora-dpnet-bridge` (`engine/crates/ra2net/src/bridge.rs`, binary
`src/bin/ora-dpnet-bridge.rs`), which runs a `Session` in bridge mode — **this
port's transport and session core (§3)**. The game's own decisions travel back:
`INDICATE_CONNECT`'s verdict and reply word, `ENUM_HOSTS_QUERY`'s 8 bytes.

What the game needs from DirectPlay, as the callback `FUN_00623720` and the call
sites use it (all observed working live):

* `Host` delivers `CREATE_PLAYER` for the local player (flags `LOCAL|HOST`, read
  back through `GetPeerInfo`: size query → `DPNERR_BUFFERTOOSMALL` `0x80158100`,
  then `DPN_PLAYER_INFO` with the name after it) before it returns.
* `Connect` (synchronous in RA2's Join): `CREATE_PLAYER` for every player, then
  `CONNECT_COMPLETE`; a refusal is `DPNERR_HOSTREJECTEDCONNECTION` `0x80158260`
  with the host's 4-byte reply (900/901).
* `RECEIVE` returns `DPNSUCCESS_PENDING` (`0x0015800e`): the buffer stays the
  game's until `ReturnBuffer`.
* `SendTo` is asynchronous with a handle the game files (`FUN_006253e0`);
  `SEND_COMPLETE` with 0 dequeues it, any other result makes giNetwork cancel and
  resend ("PLost! Resent!", `FUN_00625600`). A send to everyone without
  `NOLOOPBACK` (chat, flags 0) comes back to the sender: that is how the
  original shows its own chat line.
* `ENUM_HOSTS_QUERY` / `INDICATE_CONNECT` reply buffers come back through
  `RETURN_BUFFER` (the game frees context 1, keeps `&DAT_0072e0a8`).
* `ENUM_HOSTS_RESPONSE`: the sender address answers `GetComponentByName
  ("hostname")`; the description's instance GUID keys the row.

Use: `tools/dpnet/build.sh OUT.dll`; back up the prefix's
`drive_c/windows/syswow64/dpnet.dll`, copy `OUT.dll` over it, start
`ora-dpnet-bridge [127.0.0.1:27160] [--dump FILE]` and run the game with
`WINEDLLOVERRIDES=dpnet=n` (optional `ORA_DPNET_BRIDGE=host:port`,
`ORA_DPNET_LOG=<windows path>`); restore the file afterwards. Two originals each
with the replacement play each other the same way. **The wire is ours, not
DirectPlay 8's**: an original on Windows with Microsoft's DirectPlay does not
speak to it (§14.5).

Tests: `ra2net/src/bridge.rs` (the pipe codec), `ra2net/tests/bridge.rs` (the
bridge as host and as joiner against port sessions: admission through the
verdict, 901 refusal, enumeration answered with the game's bytes, loopback of a
flags-0 send, the pipe over TCP).

### 14.3 What the original sent (live; captures `mp-payloads-*`)

`ora-dpnet-bridge --dump` records each `SendTo` payload of the original
(`mp-payloads-host__live__000/001`, `mp-payloads-joiner__live__000`, 6467
payloads, 15 message types). **Every one decodes with `ra2net::message` and
re-encodes to the same bytes** (`tests/original_payloads.rs`). What they changed:

| item | was (static reading) | live |
|---|---|---|
| connect data / version | `01.00\0` (the file's bytes at `0x6f3914`) | `1.20\0`: `0x4e36e0` prints `"%d.%02d"` (1, `0x14`) over it at startup; read back at run time `31 2e 32 30 00`. The original refused a joiner sending `01.00` with 900. Also GameSpy `gamever`/`programversion`. `dpn::VERSION`, `dpn::CONNECT_DATA` |
| session name | the server name | six `:`-ended fields: `New Exhibition Match:DEATHMATCH:Bridge of Doom:ALLWEIGHT:1.20:512K LAN:` — server, game type, arena name, weight class, version, speed tag (`0x6f447c..0x6f44b4` by the speed dropdown, jump table `0x48f7c8`). The joiner fills its columns from it and its Join refuses another version ("Cannot Join Game" / "Incompatible Program Versions", `0x49a228`). `dpn::SessionName`, `ConnectionSpeed::session_tag` |
| `GroupInfo` (`0x71`) | player, group, state, name/team/robot/file | **panel index**, **owner id** (the host owns its computer robots' panels), state (1 own robot, 3 computer robot; **0 draws as OPEN**), team, team logo, robot, owner name, **ready**, 1 for a human panel / 0 computer, design. Our panels re-encode byte for byte |
| taking a panel | joiner sends its slot to the host | the joiner sends `PlayerLoading {panel}` (`0x132`) then its `GroupInfo` for the panel **to everyone, flags `0x20`** (not guaranteed); it took the port host's state-0 panel 0 |
| ready | `PlayerReady` (`0x12f`) | the panel's `GroupInfo` flag (re-sent with the flag set, no design); `0x12f {0, 0}` is sent every 10 s by a joiner while "Loading Robot Data..." |
| spectator list | peers without a panel | everyone in the session (the host listed itself and the joiner while both held panels) |
| `ArenaPreview` (`0x133`) | arena | `Arenas/bridge/bridge_preview.bmp:Bridge of Doom` (preview path `:` name), sent to the newcomer; game type and weight class to everyone |
| `PrepareMatch` (`0x12e`) | arena script | `{1, "Bridge of Doom"}` — the arena's name; `BeginMatch` (`0x130`) right after it |
| loading | — | the host sends its own `PeerDataLoaded` (`0x13f`, its id) about every 300 ms until every peer's arrives, then `StartCountDown` (`0x13c`) |
| `TimeSync` (`0x138`) | u32 + 4 words | first word the match time left as a float (`0x4333fb8d` 179.98 s), about once a second; the other four words unexplained |
| `giSyncMessage` size | f32 | **u32** (`11 00 00 00`): the reader `0x4d25d0` takes two words. `SyncMessage::raw_size` is a `u32` now |
| snapshots | bodies of every robot | in both recorded matches (a joiner with no panel) every snapshot is the empty 17-byte one (clocks 0, no robot, no body), at about 22 Hz on "LAN" |
| robot data | the `GroupInfo` design | after `PrepareMatch` the joiner sends `AddArenaBot` (`0x12d`, words `1 0 0`) carrying its robot as **`.bot` text** (`1.12\nName: C.R.U.S.H.\n...`) and waits in "Loading Robot Data..." |

The port follows these: `1.20`, the six-field session name (host) and its columns
(server list), panels by index with team names, a joiner's claim of an open or
free panel and its ready flag (`Lobby::apply`, test
`an_original_joiner_claims_an_open_panel_with_its_group_info`), arenas by name
(`net_play::MatchArena`: `PrepareMatch` names it, `ArenaPreview` is
`Arenas/<preview>:<name>`), the spectator list from the session members, lobby
chat from the `chat textbox` (Enter) into the `chat messages` DisplayBox (test
in `refresh_join_and_the_prematch_lobby_through_the_screens`), computer panels
played by the AI.

### 14.4 What works, live (flatpak Wine 9.0 + replacement `dpnet`, port on Xephyr, loopback)

* **Original hosts, port joins** (IP Address 127.0.0.1): the port is admitted by
  the original's own `INDICATE_CONNECT`, its prematch shows DEATHMATCH / Bridge of
  Doom / ALL CLASSES, the two members and the host's two panels (C.R.U.S.H.,
  Scout); the port's chat line appears in the original's chat box
  (`mp-lobby-host-chat`); the original's chat reaches the port (on the wire); Begin
  Match: the port loads Bridge of Doom, sends its `0x13f`, the original counts down
  and both run the match (`mp-match-host`); End Match on the original returns both
  to the lobby.
* **Port hosts, original joins**: Refresh List on the original lists the port's
  game with its columns (`mp-list-lan`), Join enters the port's lobby
  (`mp-lobby-joiner`), the original takes panel 1 with C.R.U.S.H., readies and chats
  (`mp-lobby-joiner-panel`).

### 14.5 Open

* **DirectPlay 8 on the wire.** Without a DirectPlay 8 that works under Wine, the
  original's real frames can't be captured here, and the specification is not on
  this machine. A transport byte-compatible with Microsoft's DirectPlay (for an
  original on Windows) is not built; the frames of §3 are shaped after it, not
  equal to it.
* **Port host → original joiner stops at "Loading Robot Data..."**
  (`mp-lobby-joiner-loading`): the joiner sent its `.bot` in `AddArenaBot` and
  waits, presumably for the other robots' data from the host; the port does not
  answer (it has no `.bot` for its panels: the design stream is not a `.bot`, and
  the port lobby has no robot picker).
* **The match itself across the two**: the snapshots the original sent were empty
  (no remote player robot), so the full snapshot layout with robots, controllers
  and devices is still the static reading of §6; the remote input key words (§6);
  the port runs its arena intro before the count-down (about 27 s behind the
  original) and does not use `TimeSync`; robots on the port joiner are stand-ins
  (Scout, The Boxer), not the host's designs.
* The server list's Players column (0/8 for a port host with one human panel), the
  other four `TimeSync` words, the `AddArenaBot` words, voice and host migration.
