#!/usr/bin/env python3
"""Composite a `.gib` screen into a PNG, from the same rectangles the engine uses.

This is the visual check for the layout model (`ra2ui`): the rectangles come from
`uidump --rects`, the pixels from the game's own art under `UI/`. If the anchor
reading is right the screen comes out the way the game's own artwork says it
should — the logo centred at the top, the backdrop centred, the blades around it,
the gear strip along the bottom.

    tools/ui_compose.py <screen.gib> <out.png> [--width W] [--height H]
                        [--game DIR] [--dump PATH]

`--dump` reuses a saved `uidump --rects` file instead of running the binary.
Text is not drawn: a `Text` widget has no image and its extent needs the
`fg_*.giff` font metrics, which are not decoded yet. What such a widget would
draw is reported instead.

Context: `docs/06-deferred-fidelity.md` F1's method, applied to the shell;
`docs/03-engine-design.md` M5.
"""

import argparse
import os
import struct
import subprocess
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def write_png(path, w, h, rgb):
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
            PNG_SIGNATURE
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(raw, 6))
            + chunk(b"IEND", b"")
        )


def load_tga(data):
    """(w, h, rgba bytearray) for a plain or RLE truecolour/grayscale TGA."""
    if len(data) < 18:
        raise ValueError("short TGA")
    id_len, cmap, image_type = data[0], data[1], data[2]
    w, h = struct.unpack_from("<HH", data, 12)
    bpp, descr = data[16], data[17]
    if cmap:
        raise ValueError("colour-mapped TGA")
    if image_type not in (2, 3, 10, 11):
        raise ValueError(f"TGA image type {image_type}")
    step = bpp // 8
    pixels = bytearray(w * h * step)
    at = 18 + id_len
    if image_type in (2, 3):
        pixels[:] = data[at : at + len(pixels)]
    else:  # RLE
        i = 0
        while i < len(pixels):
            if at >= len(data):
                break
            header = data[at]
            at += 1
            run = (header & 0x7F) + 1
            if header & 0x80:
                px = data[at : at + step]
                at += step
                for _ in range(run):
                    pixels[i : i + step] = px
                    i += step
            else:
                take = min(run * step, len(pixels) - i)
                pixels[i : i + take] = data[at : at + take]
                at += run * step
                i += take
    out = bytearray(w * h * 4)
    top_down = bool(descr & 0x20)
    for y in range(h):
        src = y if top_down else h - 1 - y
        for x in range(w):
            o = (src * w + x) * step
            q = (y * w + x) * 4
            if step == 1:
                out[q : q + 3] = bytes([pixels[o]] * 3)
                out[q + 3] = 255
            elif step == 3:
                r, g, b = pixels[o : o + 3]
                out[q : q + 3] = bytes([r, g, b])
                out[q + 3] = 255
            else:
                b, g, r, a = pixels[o : o + 4]
                out[q : q + 3] = bytes([r, g, b])
                out[q + 3] = a
    return w, h, out


def load_bmp(data):
    """(w, h, rgba bytearray) for an uncompressed BMP."""
    if data[:2] != b"BM":
        raise ValueError("not a BMP")
    offset, = struct.unpack_from("<I", data, 10)
    header_size, = struct.unpack_from("<I", data, 14)
    if header_size < 40:
        raise ValueError("old BMP header")
    w, h, planes, bpp = struct.unpack_from("<iiHH", data, 18)
    compression, = struct.unpack_from("<I", data, 30)
    if compression not in (0, 3):
        raise ValueError(f"BMP compression {compression}")
    top_down = h < 0
    h = abs(h)
    step = bpp // 8
    stride = ((w * step + 3) // 4) * 4
    out = bytearray(w * h * 4)
    for y in range(h):
        src = y if top_down else h - 1 - y
        row = offset + src * stride
        for x in range(w):
            o = row + x * step
            q = (y * w + x) * 4
            b, g, r = data[o], data[o + 1], data[o + 2]
            a = data[o + 3] if step == 4 else 255
            out[q : q + 4] = bytes([r, g, b, a])
    return w, h, out


class Images:
    def __init__(self, root):
        self.root = root
        self.cache = {}
        self.missing = []
        self.unsupported = []

    def get(self, relative):
        relative = relative.lstrip("./\\").replace("\\", "/")
        if relative in self.cache:
            return self.cache[relative]
        path = os.path.join(self.root, "UI", relative)
        image = None
        if not os.path.exists(path):
            self.missing.append(relative)
        else:
            data = open(path, "rb").read()
            try:
                if relative.lower().endswith(".bmp"):
                    image = load_bmp(data)
                else:
                    image = load_tga(data)
            except ValueError as e:
                self.unsupported.append((relative, str(e)))
        self.cache[relative] = image
        return image


def composite(lines, images, width, height):
    canvas = bytearray(width * height * 3)  # black, opaque
    drawn = 0
    for line in lines:
        parts = line.split()
        if len(parts) < 13:
            continue
        x, y, w, h = (float(v) for v in parts[0:4])
        alpha_flag = parts[4] == "1"
        tint = tuple(float(v) for v in parts[5:8])
        u0, v0, u1, v1 = (float(v) for v in parts[8:12])
        relative = " ".join(parts[12:])
        image = images.get(relative)
        if image is None:
            continue
        sw, sh, src = image
        # The four numbers are the image's rect in a 1024x1024 *page*, and a
        # shipped file is already cropped to its own rect — `(u1-u0)*1024` equals
        # the file's width for every *cell* in the corpus (130 of the 155 rects
        # that lie inside a page; another 104 are the whole page, where the file
        # is the whole image). The 25 that differ are *tiles*: their rect runs
        # past the page (`metaldots_pattern.bmp`, a 64x64 texture with rects up
        # to 9.7 pages) and the repeats are not drawn here yet.
        if abs((u1 - u0) * 1024.0 - sw) > 1.0 or abs((v1 - v0) * 1024.0 - sh) > 1.0:
            u0, v0, u1, v1 = 0.0, 0.0, 1.0, 1.0
        drawn += 1
        for py in range(int(round(h))):
            ty = y + py + 0.5
            if ty < 0 or ty >= height:
                continue
            v = v0 + (v1 - v0) * ((py + 0.5) / max(h, 1e-6))
            sy = min(sh - 1, max(0, int(v * sh)))
            for px in range(int(round(w))):
                tx = x + px + 0.5
                if tx < 0 or tx >= width:
                    continue
                u = u0 + (u1 - u0) * ((px + 0.5) / max(w, 1e-6))
                sx = min(sw - 1, max(0, int(u * sw)))
                o = (sy * sw + sx) * 4
                r, g, b, a = src[o], src[o + 1], src[o + 2], src[o + 3]
                if not alpha_flag:
                    a = 255
                if a == 0:
                    continue
                # The layout's tint is a multiplicative `_ForeColor`.
                r = min(255, int(r * tint[0]))
                g = min(255, int(g * tint[1]))
                b = min(255, int(tint[2] * b))
                q = (int(ty) * width + int(tx)) * 3
                if a == 255:
                    canvas[q], canvas[q + 1], canvas[q + 2] = r, g, b
                else:
                    inv = 255 - a
                    canvas[q] = (r * a + canvas[q] * inv) // 255
                    canvas[q + 1] = (g * a + canvas[q + 1] * inv) // 255
                    canvas[q + 2] = (b * a + canvas[q + 2] * inv) // 255
    return canvas, drawn


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("screen")
    ap.add_argument("out")
    ap.add_argument("--width", type=int, default=1280)
    ap.add_argument("--height", type=int, default=1005)
    ap.add_argument("--game", default=None, help="the game root (default: guessed)")
    ap.add_argument("--dump", default=None, help="a saved `uidump --rects` file")
    args = ap.parse_args()

    game = args.game
    if game is None:
        here = os.path.dirname(os.path.abspath(__file__))
        game = os.path.join(os.path.dirname(here), "local", "game")

    if args.dump:
        text = open(args.dump).read()
    else:
        engine = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "engine",
            "target",
            "debug",
            "examples",
            "uidump",
        )
        if not os.path.exists(engine):
            engine = engine.replace("/debug/", "/release/")
        cmd = [engine, args.screen, str(args.width), str(args.height), "--rects"]
        text = subprocess.run(cmd, check=True, capture_output=True, text=True).stdout

    lines = [ln for ln in text.splitlines() if ln.strip()]
    images = Images(game)
    canvas, drawn = composite(lines, images, args.width, args.height)
    write_png(args.out, args.width, args.height, canvas)
    print(f"wrote {args.out} ({args.width}x{args.height}), {drawn} of {len(lines)} drawables")
    if images.missing:
        print(f"  {len(images.missing)} referenced images are not in the tree:")
        for m in images.missing[:6]:
            print(f"     {m}")
    if images.unsupported:
        print(f"  {len(images.unsupported)} images this loader cannot read:")
        for m, why in images.unsupported[:6]:
            print(f"     {m}: {why}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
