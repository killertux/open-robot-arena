//! Robot Arena 2 multiplayer, recovered from `Robot Arena 2.exe` (docs/38).
//!
//! The original plays over DirectPlay 8 (an `IDirectPlay8Peer` session on the
//! TCP/IP service provider, UDP) wrapped by the Gabriel engine's `giNetwork`
//! (`0x620c40..0x627000`), lists Internet games through GameSpy (query and
//! reporting SDK, `0x509000..0x50c200`, game name `robotarena2`), and runs a
//! match host-authoritatively: the host simulates, joiners send their inputs
//! and apply the host's world snapshots.
//!
//! * [`stream`] / [`message`]: the game's messages, byte for byte.
//! * [`sync`]: the snapshot message and the connection-speed pacing.
//! * [`dpn`]: DirectPlay 8 host enumeration (the LAN server list).
//! * [`frame`] / [`link`] / [`session`]: the session transport — shaped after
//!   DirectPlay 8's reliable protocol, with our own session-core messages.
//! * [`lobby`]: the pre-match lobby kept in step with `giGroupInfoMessage`s.
//! * [`game`]: host and joiner drivers tying the session, lobby and sync.
//! * [`gamespy`]: the GameSpy query replies an Internet host gives.

pub mod dpn;
pub mod frame;
pub mod game;
pub mod gamespy;
pub mod link;
pub mod lobby;
pub mod message;
pub mod session;
pub mod stream;
pub mod sync;

pub use message::Message;
pub use session::{Event, PlayerId, Session, Target};
