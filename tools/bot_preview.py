#!/usr/bin/env python3
"""Write out the 128x128 snapshot a `.bot` carries inside itself.

Every bot file embeds a preview the *game* rendered of it, as an uncompressed
TGA (the same blob `tools/bot_probe.py` skips). It is not an oracle for fine
placement — that is what F1's searches established, the 128 square being too
small to score a fraction of a component — but it is decisive for the questions
where a part is on the wrong *side* of something, or hanging instead of level:
`AI/Team6/Bot2`'s hammers are buried inside the hull without the mount flip and
clear of it with it, and the snapshot settles which of those the game meant.

    tools/bot_preview.py <bot> <out.png>

No third-party modules: the TGA is plain and the PNG is written here.

Context: `docs/06-deferred-fidelity.md` F1.
"""

import struct
import sys
import zlib


def write_png(path, w, h, rgb):
    """8-bit RGB PNG from a flat bytearray."""
    raw = b"".join(b"\x00" + bytes(rgb[y * w * 3 : (y + 1) * w * 3]) for y in range(h))

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(raw))
            + chunk(b"IEND", b"")
        )


def snapshot(path):
    """The (width, height, rgb) of the bot's embedded preview, or None."""
    data = open(path, "rb").read()
    at = 0

    def line():
        nonlocal at
        end = data.find(b"\n", at)
        out = data[at:end]
        at = end + 1
        return out

    line()  # version
    line()  # name
    line()  # weight class
    flag = line().strip()
    # 1.11 and earlier put the snapshot flag where the default-texture flag is.
    present = flag == b"true" if flag in (b"true", b"false") else line().strip() == b"true"
    if not present:
        return None

    head = data[at : at + 18]
    id_len, image_type = head[0], head[2]
    w, h = struct.unpack_from("<H", head, 12)[0], struct.unpack_from("<H", head, 14)[0]
    bpp, descr = head[16], head[17]
    if image_type not in (2, 3):
        raise ValueError(f"TGA type {image_type} is not plain")
    at += 18 + id_len

    step = bpp // 8
    rgb = bytearray(w * h * 3)
    top_down = bool(descr & 0x20)
    for y in range(h):
        src = y if top_down else h - 1 - y
        for x in range(w):
            o = at + (src * w + x) * step
            q = (y * w + x) * 3
            rgb[q], rgb[q + 1], rgb[q + 2] = data[o + 2], data[o + 1], data[o]
    return w, h, rgb


def main():
    if len(sys.argv) != 3:
        print(__doc__.strip())
        return 1
    got = snapshot(sys.argv[1])
    if got is None:
        print(f"{sys.argv[1]}: no snapshot")
        return 1
    w, h, rgb = got
    write_png(sys.argv[2], w, h, rgb)
    print(f"wrote {sys.argv[2]} ({w}x{h})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
