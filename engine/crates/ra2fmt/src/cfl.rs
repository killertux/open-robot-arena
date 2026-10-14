//! Reader for **CFL3** archives (`.cfz`) — Jari Komppa's "Compressed File
//! Library 3" (zlib license). Independent reimplementation of the documented
//! on-disk format.
//!
//! Layout:
//! ```text
//! "CFL3" | index_offset:u32 | index_size:u32 | <file data...>
//! @index_offset:  directory_compression:u32 | resource
//! resource:       packed_size:u32 | packed bytes
//! directory (uncompressed, index_size bytes): repeats of
//!                 unpacked_size:u32 | offset:u32 | compression:u32 |
//!                 name_len:u16 | name bytes
//! ```
//! `offset` is relative to the archive start. The zlib filter prefixes the
//! uncompressed size as a `u32` before the zlib stream.

use crate::Error;

/// Compression tag for stored (uncompressed) resources.
pub const COMPRESS_NONE: u32 = 0;
/// Compression tag for zlib-deflated resources.
pub const COMPRESS_ZLIB: u32 = 1;

/// A single archive entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub unpacked_size: u32,
    /// Offset of the entry's resource, relative to the archive start.
    pub offset: u32,
    pub compression: u32,
}

/// An opened CFL3 archive, backed by the archive bytes.
pub struct Cfl {
    data: Vec<u8>,
    entries: Vec<Entry>,
}

impl Cfl {
    /// Parse an archive from its raw bytes.
    pub fn open(data: Vec<u8>) -> Result<Cfl, Error> {
        if data.len() < 12 || &data[0..4] != b"CFL3" {
            if data.len() >= 4 && &data[data.len() - 4..] == b"3CFL" {
                return Err(Error::BadMagic(
                    "CFL3 footer variant (trailing index) unsupported".into(),
                ));
            }
            return Err(Error::BadMagic("not a CFL3 archive".into()));
        }
        let index_off = le32(&data, 4)? as usize;
        let index_size = le32(&data, 8)? as usize;

        let dir_comp = le32(&data, index_off)?;
        let res_len = le32(&data, index_off + 4)? as usize;
        let res = data
            .get(index_off + 8..index_off + 8 + res_len)
            .ok_or(Error::UnexpectedEof {
                at: index_off + 8,
                wanted: res_len,
            })?;
        let dir = unhandle(res, dir_comp)?;
        if dir.len() < index_size {
            return Err(Error::Structure(format!(
                "directory is {} bytes, header claims {index_size}",
                dir.len()
            )));
        }

        let mut entries = Vec::new();
        let mut p = 0usize;
        while p < index_size {
            let unpacked_size = le32(&dir, p)?;
            let offset = le32(&dir, p + 4)?;
            let compression = le32(&dir, p + 8)?;
            let namelen = le16(&dir, p + 12)? as usize;
            p += 14;
            let raw = dir.get(p..p + namelen).ok_or(Error::UnexpectedEof {
                at: p,
                wanted: namelen,
            })?;
            p += namelen;
            entries.push(Entry {
                name: String::from_utf8_lossy(raw).into_owned(),
                unpacked_size,
                offset,
                compression,
            });
        }

        Ok(Cfl { data, entries })
    }

    /// Read an archive from a file path.
    pub fn open_path(path: &std::path::Path) -> Result<Cfl, Error> {
        let data = std::fs::read(path)
            .map_err(|e| Error::Structure(format!("{}: {e}", path.display())))?;
        Cfl::open(data)
    }

    /// All entries, in archive order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Find an entry by name (case-insensitive, `/` and `\` equivalent).
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        let want = normalize(name);
        self.entries.iter().find(|e| normalize(&e.name) == want)
    }

    /// Decompress an entry by name.
    pub fn get(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.entry(name)?;
        let base = e.offset as usize;
        let res_len = le32(&self.data, base).ok()? as usize;
        let res = self.data.get(base + 4..base + 4 + res_len)?;
        unhandle(res, e.compression).ok()
    }

    /// Decompress an entry as UTF-8 text (lossy).
    pub fn get_text(&self, name: &str) -> Option<String> {
        self.get(name)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

fn normalize(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

fn le16(d: &[u8], o: usize) -> Result<u16, Error> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::UnexpectedEof { at: o, wanted: 2 })
}

fn le32(d: &[u8], o: usize) -> Result<u32, Error> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::UnexpectedEof { at: o, wanted: 4 })
}

/// Reverse a CFL resource filter chain (only the main filter is used by RA2).
fn unhandle(data: &[u8], flags: u32) -> Result<Vec<u8>, Error> {
    match flags & 0xff {
        COMPRESS_NONE => Ok(data.to_vec()),
        COMPRESS_ZLIB => zlib_decode(data),
        other => Err(Error::Structure(format!(
            "unsupported CFL3 compression {other:#x} (flags {flags:#x})"
        ))),
    }
}

fn zlib_decode(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() < 4 {
        return Err(Error::UnexpectedEof { at: 0, wanted: 4 });
    }
    // The resource is prefixed with the uncompressed size. RA2's archives omit
    // the final adler32, so we decode with the low-level API and accept the
    // output once we have all `expected` bytes (tolerating the truncated tail).
    let expected = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let body = &data[4..];
    let mut out = Vec::with_capacity(expected.max(64));
    let mut z = flate2::Decompress::new(true);
    let mut pos = 0usize;
    loop {
        let in_before = z.total_in();
        let out_before = z.total_out() as usize;
        let status = z.decompress_vec(&body[pos..], &mut out, flate2::FlushDecompress::None);
        pos += (z.total_in() - in_before) as usize;
        let produced = z.total_out() as usize - out_before;
        match status {
            Ok(flate2::Status::StreamEnd) => break,
            Ok(_) => {
                if out.len() >= expected || (produced == 0 && pos >= body.len()) {
                    break;
                }
            }
            Err(e) => {
                if out.is_empty() {
                    return Err(Error::Structure(format!("zlib inflate failed: {e}")));
                }
                break; // truncated tail: keep what we decoded
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_cfl() {
        assert!(Cfl::open(b"not an archive".to_vec()).is_err());
    }

    #[test]
    fn roundtrip_synthetic_none() {
        // Build a minimal CFL3 with one stored (uncompressed) entry.
        let payload = b"hello world";
        let mut data = Vec::new();
        data.extend_from_slice(b"CFL3");
        let index_off_pos = data.len();
        data.extend_from_slice(&0u32.to_le_bytes()); // index offset (patched later)
        data.extend_from_slice(&0u32.to_le_bytes()); // index size (patched later)

        // file data resource: packed_size + raw bytes
        let file_off = data.len() as u32;
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(payload);

        // index
        let index_off = data.len() as u32;
        let mut dir = Vec::new();
        dir.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // unpacked size
        dir.extend_from_slice(&file_off.to_le_bytes()); // offset
        dir.extend_from_slice(&COMPRESS_NONE.to_le_bytes()); // compression
        dir.extend_from_slice(&5u16.to_le_bytes()); // name len
        dir.extend_from_slice(b"a.txt");
        let index_size = dir.len() as u32;
        data.extend_from_slice(&COMPRESS_NONE.to_le_bytes()); // directory compression
        data.extend_from_slice(&(dir.len() as u32).to_le_bytes()); // resource size
        data.extend_from_slice(&dir);

        data[index_off_pos..index_off_pos + 4].copy_from_slice(&index_off.to_le_bytes());
        data[index_off_pos + 4..index_off_pos + 8].copy_from_slice(&index_size.to_le_bytes());

        let cfl = Cfl::open(data).unwrap();
        assert_eq!(cfl.entries().len(), 1);
        assert_eq!(cfl.get("A.TXT").as_deref(), Some(&payload[..]));
    }
}
