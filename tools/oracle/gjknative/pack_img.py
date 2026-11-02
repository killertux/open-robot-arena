#!/usr/bin/env python3
"""pack_img.py <Robot Arena 2.exe> <out img.bin>: the PE image's sections as `gjkn` maps them.

Layout: u32 count, then per section (u32 va, u32 virtual size, u32 offset in this file, u32 raw
size), then the raw bytes. Nothing of the game is kept in the repository; the file is made from the
local installation each time (put it in a scratch directory).
"""
import struct
import sys

d = open(sys.argv[1], "rb").read()
pe = struct.unpack_from("<I", d, 0x3C)[0]
n = struct.unpack_from("<H", d, pe + 6)[0]
opt = struct.unpack_from("<H", d, pe + 20)[0]
base = struct.unpack_from("<I", d, pe + 24 + 28)[0]
o = pe + 24 + opt
secs = []
for i in range(n):
    vs, va, rs, rp = struct.unpack_from("<IIII", d, o + 8 + i * 40)
    secs.append((base + va, vs, rp, rs))
hdr = struct.pack("<I", len(secs))
ents, body = [], bytearray()
off = 4 + 16 * len(secs)
for va, vs, rp, rs in secs:
    raw = d[rp:rp + min(rs, vs)]
    ents.append(struct.pack("<IIII", va, vs, off + len(body), len(raw)))
    body += raw
open(sys.argv[2], "wb").write(hdr + b"".join(ents) + bytes(body))
