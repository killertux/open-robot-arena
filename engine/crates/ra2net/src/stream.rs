//! The byte stream every `giNetMessage` is written to and read from.
//!
//! The original serialises a message into a binary `std::strstream` and hands
//! the bytes to DirectPlay (`FUN_00620ee0`, `FUN_00621190`). The primitives are
//! thin wrappers over the stream buffer's `sputn`/`sgetn` (vtable `+0x1c` /
//! `+0x18`), so the layout is the in-memory one, little-endian:
//!
//! | primitive | writer | reader | bytes |
//! |---|---|---|---|
//! | 32-bit word (`int`, `DWORD`, `float`) | `0x58a0b0` | `0x58a1d0` | 4 |
//! | byte | `0x58a0d0` | `0x58a1f0` | 1 |
//! | `bool` | `0x58a050` (writes `v != 0`) | `0x58a160` (reads `b == 1`) | 1 |
//! | string | `0x58a0f0`: length word, then the bytes, no terminator | `0x58a210` | 4 + n |
//! | raw block | `0x58a140` | `0x58a360` | n |
//!
//! There is no framing of its own: the message type is the first word
//! (`giNetMessage+4`, written by `0x621530`, read by `0x621510`), and the rest is
//! whatever the subclass writes.

use std::fmt;

/// A stream that ran out, or carried something the message cannot hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    /// The message ended `needed` bytes early.
    Short { needed: usize },
    /// A field held a value the layout does not allow.
    Invalid(&'static str),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::Short { needed } => write!(f, "message ended {needed} bytes early"),
            StreamError::Invalid(what) => write!(f, "invalid field: {what}"),
        }
    }
}

impl std::error::Error for StreamError {}

/// Longest string a reader accepts. The original allocates whatever the length
/// word says; a cap keeps a corrupt datagram from asking for gigabytes.
pub const MAX_STRING: usize = 64 * 1024;

/// Writes the primitives in the original's layout.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer::default()
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.u32(v as u32)
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.u32(v.to_bits())
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    /// `0x58a050`: one byte, `1` for true.
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }

    /// `0x58a0f0`: the length word, then the bytes (no terminator).
    pub fn string(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self
    }

    /// `0x58a140`: bytes as they are.
    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Reads the primitives back.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], StreamError> {
        let left = self.buf.len() - self.at;
        if left < n {
            return Err(StreamError::Short { needed: n - left });
        }
        let out = &self.buf[self.at..self.at + n];
        self.at += n;
        Ok(out)
    }

    pub fn u32(&mut self) -> Result<u32, StreamError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32, StreamError> {
        Ok(self.u32()? as i32)
    }

    pub fn f32(&mut self) -> Result<f32, StreamError> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn u8(&mut self) -> Result<u8, StreamError> {
        Ok(self.take(1)?[0])
    }

    /// `0x58a160`: true only for a stored `1`.
    pub fn bool(&mut self) -> Result<bool, StreamError> {
        Ok(self.u8()? == 1)
    }

    /// `0x58a210`. The bytes are the game's ANSI text; anything that is not
    /// UTF-8 is kept lossily.
    pub fn string(&mut self) -> Result<String, StreamError> {
        let n = self.u32()? as usize;
        if n > MAX_STRING {
            return Err(StreamError::Invalid("string length"));
        }
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    pub fn raw(&mut self, n: usize) -> Result<&'a [u8], StreamError> {
        self.take(n)
    }

    /// Whatever is left (the robot design stream at the end of a lobby slot).
    pub fn rest(&mut self) -> &'a [u8] {
        let out = &self.buf[self.at..];
        self.at = self.buf.len();
        out
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_little_endian_and_unframed() {
        let mut w = Writer::new();
        w.u32(0x0000_0137).bool(true).f32(1.0).string("box");
        assert_eq!(
            w.into_bytes(),
            [
                0x37, 0x01, 0, 0, // the word
                1, // bool
                0, 0, 0x80, 0x3f, // 1.0f
                3, 0, 0, 0, b'b', b'o', b'x', // length word + bytes, no NUL
            ]
        );
    }

    #[test]
    fn a_reader_stops_at_the_end() {
        let mut r = Reader::new(&[1, 0]);
        assert_eq!(r.u32(), Err(StreamError::Short { needed: 2 }));
        let mut r = Reader::new(&[2, 1]);
        assert!(!r.bool().unwrap(), "only a stored 1 is true (0x58a160)");
        assert!(r.bool().unwrap());
    }
}
