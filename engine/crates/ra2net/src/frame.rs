//! Session-transport frames, laid out after DirectPlay 8's reliable protocol
//! ([MC-DPL8R]): connection-control frames (`CFRAME`) and data frames
//! (`DFRAME`) with 8-bit sequence numbers, a cumulative "next expected"
//! acknowledgement and selective-acknowledgement masks.
//!
//! This is the shape of the original's wire, not yet the whole of it: the
//! header bits and the control-frame fields follow the specification as
//! recalled, but nothing here has been checked against a capture of a real
//! DirectPlay peer (the oracle's Wine `dpnet` does not emit these frames, see
//! docs/38 §7), and the session-core messages carried inside
//! ([`crate::session`]) are ours. Two simplifications are deliberate:
//!
//! * a non-guaranteed frame does not take a sequence number (it repeats the
//!   next one, with `RELIABLE` clear) and is delivered on arrival; DirectPlay
//!   sequences them too and cancels lost ones with send masks;
//! * acknowledgement masks cover the 32 frames after `next_recv` (one mask).

/// `bCommand` bits of a data frame.
pub const CMD_DATA: u8 = 0x01;
pub const CMD_RELIABLE: u8 = 0x02;
pub const CMD_SEQUENTIAL: u8 = 0x04;
pub const CMD_POLL: u8 = 0x08;
pub const CMD_NEW_MSG: u8 = 0x10;
pub const CMD_END_MSG: u8 = 0x20;
/// Set on a frame whose payload is a session-core message rather than game
/// data.
pub const CMD_USER_1: u8 = 0x40;
/// A connection-control frame.
pub const CMD_CFRAME: u8 = 0x80;

/// `bControl` bits of a data frame.
pub const CTL_RETRY: u8 = 0x01;
pub const CTL_KEEPALIVE: u8 = 0x02;
pub const CTL_SACK_MASK1: u8 = 0x10;

/// Control-frame opcodes (`bExtOpcode`).
pub const OP_CONNECT: u8 = 0x01;
pub const OP_CONNECTED: u8 = 0x02;
pub const OP_HARD_DISCONNECT: u8 = 0x04;
pub const OP_SACK: u8 = 0x06;

/// `bFlags` of a SACK control frame.
pub const SACK_RESPONSE: u8 = 0x01;
pub const SACK_MASK1: u8 = 0x02;

/// The protocol version the connect frames carry.
pub const PROTOCOL_VERSION: u32 = 0x0001_0006;

/// Largest payload of one data frame; bigger messages are fragmented.
pub const MAX_PAYLOAD: usize = 1200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// `CONNECT`, `CONNECTED`, `HARD_DISCONNECT`.
    Control {
        op: u8,
        poll: bool,
        msg_id: u8,
        rsp_id: u8,
        version: u32,
        session: u32,
        timestamp: u32,
    },
    /// A standalone acknowledgement.
    Sack {
        poll: bool,
        flags: u8,
        retry: u8,
        next_seq: u8,
        next_recv: u8,
        timestamp: u32,
        mask: Option<u32>,
    },
    Data {
        command: u8,
        control: u8,
        seq: u8,
        next_recv: u8,
        mask: Option<u32>,
        payload: Vec<u8>,
    },
}

impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Frame::Control {
                op,
                poll,
                msg_id,
                rsp_id,
                version,
                session,
                timestamp,
            } => {
                let mut out = vec![
                    CMD_CFRAME | if *poll { CMD_POLL } else { 0 },
                    *op,
                    *msg_id,
                    *rsp_id,
                ];
                out.extend(version.to_le_bytes());
                out.extend(session.to_le_bytes());
                out.extend(timestamp.to_le_bytes());
                out
            }
            Frame::Sack {
                poll,
                flags,
                retry,
                next_seq,
                next_recv,
                timestamp,
                mask,
            } => {
                let flags = flags & !SACK_MASK1 | if mask.is_some() { SACK_MASK1 } else { 0 };
                let mut out = vec![
                    CMD_CFRAME | if *poll { CMD_POLL } else { 0 },
                    OP_SACK,
                    flags,
                    *retry,
                    *next_seq,
                    *next_recv,
                    0,
                    0,
                ];
                out.extend(timestamp.to_le_bytes());
                if let Some(m) = mask {
                    out.extend(m.to_le_bytes());
                }
                out
            }
            Frame::Data {
                command,
                control,
                seq,
                next_recv,
                mask,
                payload,
            } => {
                let control =
                    control & !CTL_SACK_MASK1 | if mask.is_some() { CTL_SACK_MASK1 } else { 0 };
                let mut out = vec![
                    (command | CMD_DATA) & !CMD_CFRAME,
                    control,
                    *seq,
                    *next_recv,
                ];
                if let Some(m) = mask {
                    out.extend(m.to_le_bytes());
                }
                out.extend(payload);
                out
            }
        }
    }

    pub fn decode(b: &[u8]) -> Option<Frame> {
        let word = |at: usize| -> Option<u32> {
            Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
        };
        let first = *b.first()?;
        if first & CMD_CFRAME != 0 {
            let poll = first & CMD_POLL != 0;
            let op = *b.get(1)?;
            if op == OP_SACK {
                let flags = *b.get(2)?;
                let mask = if flags & SACK_MASK1 != 0 {
                    Some(word(12)?)
                } else {
                    None
                };
                return Some(Frame::Sack {
                    poll,
                    flags,
                    retry: *b.get(3)?,
                    next_seq: *b.get(4)?,
                    next_recv: *b.get(5)?,
                    timestamp: word(8)?,
                    mask,
                });
            }
            if !matches!(op, OP_CONNECT | OP_CONNECTED | OP_HARD_DISCONNECT) {
                return None;
            }
            return Some(Frame::Control {
                op,
                poll,
                msg_id: *b.get(2)?,
                rsp_id: *b.get(3)?,
                version: word(4)?,
                session: word(8)?,
                timestamp: word(12)?,
            });
        }
        if first & CMD_DATA == 0 {
            return None;
        }
        let control = *b.get(1)?;
        let (mask, at) = if control & CTL_SACK_MASK1 != 0 {
            (Some(word(4)?), 8)
        } else {
            (None, 4)
        };
        Some(Frame::Data {
            command: first,
            control,
            seq: *b.get(2)?,
            next_recv: *b.get(3)?,
            mask,
            payload: b.get(at..)?.to_vec(),
        })
    }
}

/// `a` is before `b` in the 8-bit sequence space (within half the space).
pub fn seq_before(a: u8, b: u8) -> bool {
    a != b && b.wrapping_sub(a) < 128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connect_frame_is_sixteen_bytes() {
        let f = Frame::Control {
            op: OP_CONNECT,
            poll: true,
            msg_id: 1,
            rsp_id: 0,
            version: PROTOCOL_VERSION,
            session: 0xdead_beef,
            timestamp: 5,
        };
        let b = f.encode();
        assert_eq!(b.len(), 16);
        assert_eq!(&b[..4], &[0x88, 0x01, 1, 0]);
        assert_eq!(&b[4..8], &[0x06, 0x00, 0x01, 0x00]);
        assert_eq!(Frame::decode(&b), Some(f));
    }

    #[test]
    fn a_sack_carries_its_mask_only_when_flagged() {
        let f = Frame::Sack {
            poll: false,
            flags: SACK_RESPONSE,
            retry: 0,
            next_seq: 3,
            next_recv: 9,
            timestamp: 7,
            mask: None,
        };
        let b = f.encode();
        assert_eq!(b.len(), 12);
        assert_eq!(&b[..6], &[0x80, 0x06, 0x01, 0, 3, 9]);
        assert_eq!(Frame::decode(&b), Some(f));
        let f = Frame::Sack {
            poll: false,
            flags: SACK_RESPONSE | SACK_MASK1,
            retry: 0,
            next_seq: 3,
            next_recv: 9,
            timestamp: 7,
            mask: Some(0b101),
        };
        let b = f.encode();
        assert_eq!(b.len(), 16);
        assert_eq!(b[2], SACK_RESPONSE | SACK_MASK1);
        assert_eq!(Frame::decode(&b), Some(f));
    }

    #[test]
    fn a_data_frame_header_is_four_bytes_plus_its_mask() {
        let f = Frame::Data {
            command: CMD_DATA | CMD_RELIABLE | CMD_SEQUENTIAL | CMD_NEW_MSG | CMD_END_MSG,
            control: 0,
            seq: 4,
            next_recv: 2,
            mask: None,
            payload: vec![0x37, 1, 0, 0],
        };
        let b = f.encode();
        assert_eq!(&b[..4], &[0x37, 0x00, 4, 2]);
        assert_eq!(Frame::decode(&b), Some(f));
        let f = Frame::Data {
            command: CMD_DATA | CMD_USER_1 | CMD_NEW_MSG | CMD_END_MSG,
            control: CTL_SACK_MASK1,
            seq: 0,
            next_recv: 0,
            mask: Some(1),
            payload: vec![],
        };
        let b = f.encode();
        assert_eq!(b.len(), 8);
        assert_eq!(Frame::decode(&b), Some(f));
        // A frame that is neither: dropped.
        assert_eq!(Frame::decode(&[0x00, 0x02]), None);
    }

    #[test]
    fn sequence_order_wraps() {
        assert!(seq_before(250, 3));
        assert!(!seq_before(3, 250));
        assert!(!seq_before(7, 7));
    }
}
