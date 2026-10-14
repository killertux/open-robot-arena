#!/usr/bin/env python3
"""Extract the RA2 "LGU Repack" self-extracting exe payload.

The repack's inner exe embeds a *streaming* ZIP: a sequence of local file
headers (PK\\x03\\x04) with sizes stored inline and **no central directory /
EOCD**. Standard tools refuse it, so we walk the local headers ourselves.

Usage: extract_sfx.py <sfx.exe> <out_dir>
"""
import os
import struct
import sys
import zlib

SIG = b"PK\x03\x04"


def entries(data):
    """Yield (name, method, csize, usize, crc, data_offset) for each local header."""
    off = 0
    while True:
        h = data.find(SIG, off)
        if h < 0:
            return
        (ver, flags, method, mtime, mdate, crc,
         csize, usize, fnl, exl) = struct.unpack_from("<HHHHHIIIHH", data, h + 4)
        name = data[h + 30:h + 30 + fnl].decode("latin1", "replace")
        doff = h + 30 + fnl + exl
        # Streaming zips may put real sizes in a trailing data descriptor.
        if (flags & 0x08) or csize == 0:
            off = h + 4  # resync; skip this one (unusual here)
            continue
        yield name, method, csize, usize, crc, doff
        off = doff + csize


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    src, outdir = sys.argv[1], sys.argv[2]
    data = open(src, "rb").read()
    print(f"source: {src} ({len(data):,} bytes)")

    n = ok = bad = 0
    total = 0
    for name, method, csize, usize, crc, doff in entries(data):
        n += 1
        raw = data[doff:doff + csize]
        try:
            if method == 8:
                blob = zlib.decompress(raw, -15)
            elif method == 0:
                blob = raw
            else:
                print(f"  skip {name!r}: unsupported method {method}")
                bad += 1
                continue
            if usize and len(blob) != usize:
                raise ValueError(f"size {len(blob)} != {usize}")
            if crc and (zlib.crc32(blob) & 0xFFFFFFFF) != crc:
                raise ValueError("crc mismatch")
        except Exception as e:  # noqa: BLE001
            print(f"  FAIL {name!r}: {e}")
            bad += 1
            continue

        dest = os.path.join(outdir, name.replace("\\", "/"))
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        with open(dest, "wb") as fh:
            fh.write(blob)
        ok += 1
        total += len(blob)

    print(f"entries={n} ok={ok} bad={bad} written={total:,} bytes -> {outdir}")


if __name__ == "__main__":
    main()
