#!/usr/bin/env python3
"""Unpack a Robot Arena 2 installer into a game folder, without running it.

    extract_install.py [--force] <installer> <out_dir>

<installer> is one of:

  * the "LGU Repack" download (`ra2_repack.zip`, or the folder it unzips to,
    or its `Game Files/Robot Arena 2.exe`): an Advanced Installer
    self-extractor that carries an MSI and one MSZIP cabinet;
  * a bare `.msi` with its `.cab` beside it.

The game folder is written to <out_dir> (the folder that holds
`Robot Arena 2.exe`, `RA2.cfz`, `UI/`, `Scripts/`, ...), ready for
`ra2-app --game <out_dir>`. Pure Python 3 standard library: no Wine, no
msitools, no cabextract, so it runs the same on macOS, Linux and Windows.

How the repack is laid out (read from the file, not from documentation):

  * the exe ends with "ADVINSTSFX"; just before it a table lists the payload
    files as (size u32, offset u32, name length u32, UTF-16 name), here
    RA2.msi, RA21.cab and RA2.ini;
  * the first 0x200 bytes of each payload file are XOR 0xFF, the rest plain;
  * the cabinet stores the files under their MSI `File` keys, flat; the MSI's
    File, Component and Directory tables give each one's real name and folder.

The MSI is an OLE compound file; the reader below handles only what an MSI
needs (one flat storage, FAT/miniFAT streams, the string pool and
column-major tables).
"""
import argparse
import io
import os
import re
import shutil
import struct
import sys
import tempfile
import zipfile
import zlib

SFX_MAGIC = b"ADVINSTSFX"
XOR_SPAN = 0x200


# ----------------------------------------------------------------- the installer

def sfx_payload(f, size):
    """Return {name: (offset, length)} from an Advanced Installer SFX footer."""
    tail_len = min(size, 1 << 16)
    f.seek(size - tail_len)
    tail = f.read(tail_len)
    if not tail.endswith(SFX_MAGIC):
        return None
    files = {}
    # A UTF-16 name of a payload file, preceded by size, offset and length.
    for m in re.finditer(rb"(?:[\x20-\x7e]\x00){1,64}", tail):
        name = m.group().decode("utf-16-le")
        start = m.start()
        if not re.search(r"\.(msi|cab|ini|exe|mst)$", name, re.I) or start < 12:
            continue
        length, offset, nlen = struct.unpack_from("<III", tail, start - 12)
        if nlen == len(name) and 0 < length and offset + length <= size:
            files[name] = (offset, length)
    return files


def copy_payload(f, offset, length, dest):
    """Copy one payload file out of the SFX, undoing the XOR on its head."""
    f.seek(offset)
    with open(dest, "wb") as out:
        head = bytearray(f.read(min(XOR_SPAN, length)))
        for i in range(len(head)):
            head[i] ^= 0xFF
        out.write(head)
        left = length - len(head)
        while left:
            chunk = f.read(min(left, 1 << 22))
            if not chunk:
                raise ValueError("installer is truncated")
            out.write(chunk)
            left -= len(chunk)


# ----------------------------------------------------------------- OLE / MSI

class Cfb:
    """A minimal reader for the streams of an OLE compound file."""

    def __init__(self, data):
        if data[:8] != b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1":
            raise ValueError("not an OLE compound file")
        self.data = data
        self.ssz = 1 << struct.unpack_from("<H", data, 0x1E)[0]
        self.mssz = 1 << struct.unpack_from("<H", data, 0x20)[0]
        (nfat, dir_start, _, self.cutoff, minifat_start, _nminifat,
         difat_start, ndifat) = struct.unpack_from("<IIIIIIII", data, 0x2C)
        difat = list(struct.unpack_from("<109I", data, 0x4C))
        sec = difat_start
        for _ in range(ndifat):
            vals = struct.unpack_from("<%dI" % (self.ssz // 4), self.sector(sec))
            difat.extend(vals[:-1])
            sec = vals[-1]
        self.fat = []
        for s in difat[:nfat]:
            self.fat.extend(struct.unpack_from("<%dI" % (self.ssz // 4), self.sector(s)))
        self.minifat = []
        raw = self.chain(minifat_start)
        self.minifat = list(struct.unpack_from("<%dI" % (len(raw) // 4), raw))
        dirs = self.chain(dir_start)
        self.entries = {}
        root = None
        for i in range(0, len(dirs), 128):
            e = dirs[i:i + 128]
            nlen, kind = struct.unpack_from("<HB", e, 0x40)
            if kind == 0 or nlen < 2:
                continue
            name = e[:nlen - 2].decode("utf-16-le")
            start, size = struct.unpack_from("<II", e, 0x74)
            if kind == 5:
                root = (start, size)
            elif kind == 2:
                self.entries[name] = (start, size)
        self.ministream = self.chain(root[0])[:root[1]] if root else b""

    def sector(self, n):
        o = (n + 1) * self.ssz
        return self.data[o:o + self.ssz]

    def chain(self, start):
        out = []
        seen = set()
        while start < 0xFFFFFFFA and start not in seen:
            seen.add(start)
            out.append(self.sector(start))
            start = self.fat[start]
        return b"".join(out)

    def stream(self, name):
        start, size = self.entries[name]
        if size < self.cutoff:
            out = []
            while start < 0xFFFFFFFA and len(out) * self.mssz < size:
                o = start * self.mssz
                out.append(self.ministream[o:o + self.mssz])
                start = self.minifat[start]
            return b"".join(out)[:size]
        return self.chain(start)[:size]


_B64 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz._"


def msi_stream_name(raw):
    """Decode MSI's packed stream names (two characters per code point)."""
    out = []
    for ch in raw:
        c = ord(ch)
        if 0x3800 <= c < 0x4800:
            c -= 0x3800
            out.append(_B64[c & 0x3F] + _B64[(c >> 6) & 0x3F])
        elif 0x4800 <= c < 0x4840:
            out.append(_B64[c - 0x4800])
        elif c == 0x4840:
            out.append("!")  # marks a table stream
        else:
            out.append(ch)
    return "".join(out)


class Msi:
    STRING, VALID, NULLABLE = 0x0800, 0x0100, 0x1000

    def __init__(self, data):
        cfb = Cfb(data)
        self.streams = {msi_stream_name(n): n for n in cfb.entries}
        self.cfb = cfb
        pool = self.raw("!_StringPool")
        sdata = self.raw("!_StringData")
        codepage = struct.unpack_from("<I", pool, 0)[0]
        self.ref = 3 if codepage & 0x80000000 else 2
        enc = "cp%d" % (codepage & 0xFFFF) if codepage & 0xFFFF else "cp1252"
        self.strings = [""]
        off = 0
        i = 4
        while i + 4 <= len(pool):
            n, refs = struct.unpack_from("<HH", pool, i)
            i += 4
            if n == 0 and refs:  # 64 KiB or more: the next slot holds the length, one id
                lo, hi = struct.unpack_from("<HH", pool, i)
                n = (hi << 16) | lo
                i += 4
            self.strings.append(sdata[off:off + n].decode(enc, "replace"))
            off += n
        self.columns = {}
        cols = self.table_raw("!_Columns", [
            ("Table", self.VALID | self.STRING | 64), ("Number", self.VALID | 2),
            ("Name", self.VALID | self.STRING | 64), ("Type", self.VALID | 2)])
        for row in sorted(cols, key=lambda r: (r["Table"], r["Number"])):
            self.columns.setdefault(row["Table"], []).append((row["Name"], row["Type"]))

    def raw(self, name):
        return self.cfb.stream(self.streams[name])

    def width(self, kind):
        if kind & ~self.NULLABLE == self.STRING | self.VALID:
            return 2  # a binary/stream column holds a 2-byte reference
        if kind & self.STRING:
            return self.ref
        return 2 if (kind & 0xFF) <= 2 else 4

    def table_raw(self, stream, cols):
        data = self.raw(stream) if stream in self.streams else b""
        row = sum(self.width(k) for _, k in cols)
        n = len(data) // row if row else 0
        rows = [dict() for _ in range(n)]
        off = 0
        for name, kind in cols:
            w = self.width(kind)
            for r in rows:
                v = int.from_bytes(data[off:off + w], "little")
                off += w
                if kind & self.STRING:
                    r[name] = self.strings[v] if v < len(self.strings) else ""
                elif w == 2:
                    r[name] = v - 0x8000 if v else None
                elif v:
                    x = v ^ 0x80000000
                    r[name] = x - (1 << 32) if x >= 1 << 31 else x
                else:
                    r[name] = None
        return rows

    def table(self, name):
        return self.table_raw("!" + name, self.columns[name])


def long_name(field):
    """'SHORT~1.TXT|Long Name.txt' -> 'Long Name.txt'."""
    return field.split("|")[-1]


def install_paths(msi):
    """Map each File key to its path, relative to the folder of the game's exe."""
    dirs = {d["Directory"]: d for d in msi.table("Directory")}
    comps = {c["Component"]: c["Directory_"] for c in msi.table("Component")}
    cache = {}

    def dir_path(key):
        if key in cache:
            return cache[key]
        d = dirs[key]
        target = d["DefaultDir"].split(":")[0]
        name = long_name(target)
        parent = d["Directory_Parent"]
        if not parent or parent == key:
            parts = []  # TARGETDIR ("SourceDir")
        else:
            parts = dir_path(parent) + ([] if name in (".", "") else [name])
        cache[key] = parts
        return parts

    files = {}
    for f in msi.table("File"):
        files[f["File"]] = (dir_path(comps[f["Component_"]]), long_name(f["FileName"]), f["FileSize"])
    exe = [p for p, n, _ in files.values() if n.lower() == "robot arena 2.exe"]
    if not exe:
        raise ValueError("the MSI installs no 'Robot Arena 2.exe'")
    root = exe[0]
    out = {}
    for key, (parts, name, size) in files.items():
        if parts[:len(root)] != root:
            print("  skip %s (outside the game folder: %s)" % (name, "/".join(parts)))
            continue
        out[key] = ("/".join(parts[len(root):] + [name]), size)
    return out


# ----------------------------------------------------------------- the cabinet

def extract_cab(path, paths, out_dir):
    """Unpack an MSZIP (or stored) cabinet, naming each file through `paths`."""
    with open(path, "rb") as f:
        hdr = f.read(36)
        if hdr[:4] != b"MSCF":
            raise ValueError("%s is not a cabinet" % path)
        coff_files, = struct.unpack_from("<I", hdr, 16)
        nfolders, nfiles, flags = struct.unpack_from("<HHH", hdr, 26)
        if flags & 0x3:
            raise ValueError("multi-part cabinets are not supported")
        res_hdr = res_folder = res_data = 0
        if flags & 0x4:
            res_hdr, res_folder, res_data = struct.unpack("<HBB", f.read(4))
            f.seek(res_hdr, 1)
        folders = []
        for _ in range(nfolders):
            start, ndata, kind = struct.unpack("<IHH", f.read(8))
            f.seek(res_folder, 1)
            folders.append((start, ndata, kind & 0xF))
        f.seek(coff_files)
        members = []
        for _ in range(nfiles):
            size, foff, ifolder, _d, _t, _a = struct.unpack("<IIHHHH", f.read(16))
            name = bytearray()
            while True:
                c = f.read(1)
                if c in (b"\0", b""):
                    break
                name += c
            members.append((ifolder, foff, size, name.decode("latin-1")))

        written = 0
        missing = []
        for fi, (start, ndata, kind) in enumerate(folders):
            if kind not in (0, 1):
                raise ValueError("cabinet folder %d uses compression %d (only MSZIP)" % (fi, kind))
            todo = sorted((m for m in members if m[0] == fi), key=lambda m: m[1])
            f.seek(start)
            buf = bytearray()
            base = 0  # folder offset of buf[0]
            prev = b""
            k = 0
            for _ in range(ndata):
                _csum, clen, ulen = struct.unpack("<IHH", f.read(8))
                f.seek(res_data, 1)
                block = f.read(clen)
                if kind == 1:
                    if block[:2] != b"CK":
                        raise ValueError("bad MSZIP block")
                    d = zlib.decompressobj(-15, zdict=prev) if prev else zlib.decompressobj(-15)
                    block = d.decompress(block[2:]) + d.flush()
                    prev = block[-32768:]
                if len(block) != ulen:
                    raise ValueError("block size %d != %d" % (len(block), ulen))
                buf += block
                while k < len(todo) and todo[k][1] + todo[k][2] <= base + len(buf):
                    _, foff, size, key = todo[k]
                    k += 1
                    target = paths.get(key)
                    if target is None:
                        missing.append(key)
                        continue
                    rel, want = target
                    if want is not None and want != size:
                        raise ValueError("%s: %d bytes in the cabinet, %d in the MSI" % (rel, size, want))
                    dest = os.path.join(out_dir, *rel.split("/"))
                    os.makedirs(os.path.dirname(dest), exist_ok=True)
                    with open(dest, "wb") as out:
                        out.write(buf[foff - base:foff - base + size])
                    written += 1
                # Keep only what a later file can still need.
                keep = todo[k][1] - base if k < len(todo) else len(buf)
                if keep > 0:
                    del buf[:keep]
                    base += keep
            if k != len(todo):
                raise ValueError("cabinet folder %d ended before its last file" % fi)
    return written, nfiles, missing


# ----------------------------------------------------------------- driver

def find_installer(src, scratch):
    """Return the path of the SFX exe or the MSI named by `src`."""
    if os.path.isdir(src):
        for dirpath, _, names in os.walk(src):
            for n in names:
                if n.lower().endswith((".exe", ".msi")):
                    p = os.path.join(dirpath, n)
                    if n.lower().endswith(".msi") or is_sfx(p):
                        return p
        raise ValueError("no installer found in %s" % src)
    if zipfile.is_zipfile(src):
        with zipfile.ZipFile(src) as z:
            exes = [i for i in z.infolist() if i.filename.lower().endswith(".exe")]
            if not exes:
                raise ValueError("no .exe inside %s" % src)
            big = max(exes, key=lambda i: i.file_size)
            print("unzipping %s (%d MB) ..." % (big.filename, big.file_size >> 20))
            dest = os.path.join(scratch, "installer.exe")
            with z.open(big) as fi, open(dest, "wb") as fo:
                shutil.copyfileobj(fi, fo, 1 << 22)
            return dest
    return src


def is_sfx(path):
    with open(path, "rb") as f:
        f.seek(0, 2)
        size = f.tell()
        f.seek(max(0, size - len(SFX_MAGIC)))
        return f.read() == SFX_MAGIC


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("installer")
    ap.add_argument("out_dir")
    ap.add_argument("--force", action="store_true", help="write into a non-empty out_dir")
    a = ap.parse_args()

    if os.path.isdir(a.out_dir) and os.listdir(a.out_dir) and not a.force:
        sys.exit("%s is not empty (use --force to write into it)" % a.out_dir)
    os.makedirs(a.out_dir, exist_ok=True)
    scratch = tempfile.mkdtemp(prefix="ra2-extract-", dir=os.path.dirname(os.path.abspath(a.out_dir)))
    try:
        inst = find_installer(a.installer, scratch)
        if inst.lower().endswith(".msi"):
            msi_path = inst
            cabs = [os.path.join(os.path.dirname(inst), n) for n in os.listdir(os.path.dirname(inst))
                    if n.lower().endswith(".cab")]
        else:
            with open(inst, "rb") as f:
                f.seek(0, 2)
                payload = sfx_payload(f, f.tell())
                if not payload:
                    sys.exit("%s is not an Advanced Installer self-extractor this tool knows" % inst)
                print("payload: " + ", ".join("%s (%d MB)" % (n, l >> 20) for n, (_, l) in payload.items()))
                for n, (o, l) in payload.items():
                    copy_payload(f, o, l, os.path.join(scratch, n))
            msis = [n for n in payload if n.lower().endswith(".msi")]
            if not msis:
                sys.exit("the installer carries no .msi")
            msi_path = os.path.join(scratch, msis[0])
            cabs = [os.path.join(scratch, n) for n in payload if n.lower().endswith(".cab")]
        with open(msi_path, "rb") as f:
            msi = Msi(f.read())
        paths = install_paths(msi)
        print("MSI: %d files for the game folder" % len(paths))
        total = 0
        for cab in cabs:
            print("unpacking %s ..." % os.path.basename(cab))
            n, count, missing = extract_cab(cab, paths, a.out_dir)
            total += n
            if missing:
                print("  %d cabinet files have no MSI entry: %s" % (len(missing), ", ".join(missing[:5])))
        print("wrote %d files to %s" % (total, a.out_dir))
        if total != len(paths):
            sys.exit("expected %d files, wrote %d" % (len(paths), total))
        exe = os.path.join(a.out_dir, "Robot Arena 2.exe")
        readme = os.path.join(a.out_dir, "ReadMe.txt")
        if os.path.exists(readme):
            with io.open(readme, encoding="latin-1") as r:
                first = r.readline().strip()
            print("ReadMe.txt: %s" % first)
        if not os.path.exists(exe):
            sys.exit("no 'Robot Arena 2.exe' in the result")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
