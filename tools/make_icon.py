#!/usr/bin/env python3
"""Draw Open Robot Arena's package icon, with no third-party modules.

    python3 tools/make_icon.py dist/.../share/icons/hicolor/256x256/apps/ra2-open-robot-arena.png

The shapes are ours and mirror `packaging/ra2-open-robot-arena.svg`: a robot
head on a workshop-blue rounded tile. The game's art is never read. The image
is drawn at 4x and box-downsampled, so the PNG has smooth edges while staying
reproducible byte for byte.
"""

import struct
import sys
import zlib

SS = 4  # supersampling factor


def rounded_rect(x0, y0, x1, y1, r):
    def inside(x, y):
        if not (x0 <= x < x1 and y0 <= y < y1):
            return False
        cx = min(max(x, x0 + r), x1 - r)
        cy = min(max(y, y0 + r), y1 - r)
        return (x - cx) ** 2 + (y - cy) ** 2 <= r * r

    return inside


def circle(cx, cy, r):
    return lambda x, y: (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def segment(x0, y0, x1, y1, half_width):
    dx, dy = x1 - x0, y1 - y0

    def inside(x, y):
        if dx == 0 and dy == 0:
            return (x - x0) ** 2 + (y - y0) ** 2 <= half_width * half_width
        t = ((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy)
        t = min(max(t, 0.0), 1.0)
        px, py = x0 + t * dx, y0 + t * dy
        return (x - px) ** 2 + (y - py) ** 2 <= half_width * half_width

    return inside


def rgba(hex_color):
    hex_color = hex_color.lstrip("#")
    return tuple(int(hex_color[i : i + 2], 16) for i in (0, 2, 4)) + (255,)


def draw(canvas, size, inside, color):
    r, g, b, a = color
    for y in range(size):
        row = y * size * 4
        for x in range(size):
            if inside(x + 0.5, y + 0.5):
                i = row + x * 4
                canvas[i : i + 4] = bytes((r, g, b, a))


def main():
    if len(sys.argv) not in (2, 3):
        print("usage: make_icon.py OUT.png [SIZE]", file=sys.stderr)
        return 2
    out = sys.argv[1]
    size = int(sys.argv[2]) if len(sys.argv) == 3 else 256
    if size < 16 or size > 1024:
        print("make_icon.py: SIZE must be 16..1024", file=sys.stderr)
        return 2
    s = size * SS
    # All coordinates are for a 256-unit art board; scale them.
    def k(v):
        return v * s / 256.0

    canvas = bytearray(s * s * 4)

    # The tile, its border, then the robot from the top down.
    draw(canvas, s, rounded_rect(k(8), k(8), k(248), k(248), k(44)), rgba("#12233a"))
    draw(
        canvas,
        s,
        lambda x, y: rounded_rect(k(8), k(8), k(248), k(248), k(44))(x, y)
        and not rounded_rect(k(13), k(13), k(243), k(243), k(40))(x, y),
        rgba("#2f5a8a"),
    )
    draw(canvas, s, segment(k(128), k(42), k(128), k(76), k(4.5)), rgba("#ff8a3d"))
    draw(canvas, s, circle(k(128), k(38), k(13)), rgba("#ff8a3d"))
    draw(canvas, s, rounded_rect(k(60), k(70), k(196), k(190), k(22)), rgba("#e8edf2"))
    draw(canvas, s, rounded_rect(k(74), k(92), k(182), k(146), k(13)), rgba("#0b1522"))
    draw(canvas, s, circle(k(102), k(119), k(13)), rgba("#ff8a3d"))
    draw(canvas, s, circle(k(154), k(119), k(13)), rgba("#ff8a3d"))
    draw(canvas, s, rounded_rect(k(88), k(160), k(168), k(170), k(5)), rgba("#0b1522"))
    draw(canvas, s, rounded_rect(k(106), k(160), k(114), k(170), k(1)), rgba("#e8edf2"))
    draw(canvas, s, rounded_rect(k(142), k(160), k(150), k(170), k(1)), rgba("#e8edf2"))
    draw(canvas, s, circle(k(57), k(130), k(9)), rgba("#2f5a8a"))
    draw(canvas, s, circle(k(199), k(130), k(9)), rgba("#2f5a8a"))

    # Box filter SS x SS.
    n = SS * SS
    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            r = g = b = a = 0
            for dy in range(SS):
                base = ((y * SS + dy) * s + x * SS) * 4
                for dx in range(SS):
                    i = base + dx * 4
                    r += canvas[i]
                    g += canvas[i + 1]
                    b += canvas[i + 2]
                    a += canvas[i + 3]
            row += bytes((r // n, g // n, b // n, a // n))
        rows.append(bytes(row))

    raw = b"".join(b"\x00" + row for row in rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(out, "wb") as f:
        f.write(png)
    print(f"wrote {out} ({size}x{size}, {len(png)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
