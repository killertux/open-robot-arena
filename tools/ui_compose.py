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
import tempfile
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


class Font:
    """A `fg_*.giff`, dumped by `ra2 font` so that this stays a picture-maker."""

    FIRST_CHAR = 34  # see `ra2fmt::giff`

    def __init__(self, ra2_binary, cfz, name, tmpdir):
        self.name = name
        pgm = os.path.join(tmpdir, f"{name}.pgm")
        out = subprocess.run(
            [ra2_binary, "font", cfz, name, "--page", pgm],
            check=True, capture_output=True, text=True,
        ).stdout
        self.glyphs = {}
        self.ascent = 0
        for line in out.splitlines():
            parts = line.split()
            if parts[0] == "glyph":
                self.glyphs[int(parts[1])] = (
                    int(parts[2]), int(parts[3]), int(parts[4]),
                    *(float(v) for v in parts[5:9]),
                )
            elif parts[0] == "ascent":
                self.ascent = int(parts[1])
        self.glyphs_only = [g for _, g in sorted(self.glyphs.items())]
        raw = open(pgm, "rb").read().split(b"\n", 3)[3]
        head = open(pgm, "rb").read().split(b"\n", 3)[1].split()
        self.width, self.height = int(head[0]), int(head[1])
        self.page = raw

    def glyph(self, ch):
        i = ord(ch) - self.FIRST_CHAR
        if 0 <= i < len(self.glyphs_only):
            return self.glyphs_only[i]
        return None

    def _rect(self, g):
        adv, bear, third, u0, v0, u1, v1 = g
        # The page's rows are stored **bottom-up**, so `v` is turned over here.
        # `u` is not: the cells run left to right in reading order. What *is*
        # mirrored is the glyph inside its cell (the `draw` below flips the
        # sample in `u`, as the engine's renderer does).
        x0, x1 = round(u0 * self.width), round(u1 * self.width)
        y0, y1 = self.height - round(v1 * self.height), self.height - round(v0 * self.height)
        return adv, third, x0, y0, x1 - x0, y1 - y0

    def space_advance(self):
        """The table starts at `"` — the space and the bang are not in it — so a
        space is given the median advance, which is what it looks like."""
        advances = sorted(g[0] for g in self.glyphs_only if g[0] > 0)
        return advances[len(advances) // 2]

    def measure(self, text):
        pen, space = 0, self.space_advance()
        for ch in text:
            g = self.glyph(ch)
            pen += g[0] if g else space
        return pen

    def draw(self, canvas, width, height, x, y, text, tint, clip=None):
        """Draw with the widget's top-left at `(x, y)`; each glyph's cell hangs
        from its own `third` above the baseline. `clip` crops what the label may
        cover, which is what its container's `ClipMode` says."""
        pen = x
        for ch in text:
            g = self.glyph(ch)
            if g is None:
                pen += self.space_advance()
                continue
            adv, third, gx, gy, w, h = self._rect(g)
            top = y + 0 - third          # the caller passes the baseline
            for yy in range(h):
                # The page is stored **both ways round**: bottom-up in `v` and
                # mirrored in `u`. The last byte row is the one the eye sees first
                # (sampled in byte order a `T` comes out with its bar at the
                # bottom), and the cell's columns run right to left (sampled as
                # stored, an `E` comes out with its bars on the left of the stem).
                # The engine's own renderer swaps both, the same way round.
                sy = gy + h - 1 - yy
                row = self.page[sy * self.width + gx : sy * self.width + gx + w][::-1]
                for xx, v in enumerate(row):
                    if not v:
                        continue
                    X, Y = int(pen + xx), int(top + yy)
                    if not (0 <= X < width and 0 <= Y < height):
                        continue
                    if clip is not None and not (
                        clip[0] <= pen + xx < clip[0] + clip[2]
                        and clip[1] <= top + yy < clip[1] + clip[3]
                    ):
                        continue
                    q = (Y * width + X) * 3
                    # `v` is coverage, so blend towards the tint rather than
                    # adding a fraction: white ink over dark cement.
                    for c in range(3):
                        want = tint[c] * 255.0
                        canvas[q + c] = max(0, min(255, int(canvas[q + c] + (want - canvas[q + c]) * v / 255.0)))
            pen += adv


def ra2_tool():
    """The engine CLI, which is what reads the archives and the faces. The debug
    build first: it is the one a working tree has just rebuilt."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    for profile in ("debug", "release"):
        path = os.path.join(root, "engine", "target", profile, "ra2")
        if os.path.exists(path):
            return path
    return None


class Images:
    def __init__(self, root, ra2=None):
        self.root = root
        # `ra2 cfz <archive> cat <entry>` — the shell's own art lives in
        # `RA2.cfz` as often as it does loose, and a scroll bar's arrows only
        # ever live there.
        self.ra2 = ra2
        self.archive = os.path.join(root, "RA2.cfz")
        self.cache = {}
        self.missing = []
        self.unsupported = []

    def get(self, relative):
        relative = relative.lstrip("./\\").replace("\\", "/")
        if relative in self.cache:
            return self.cache[relative]
        path = os.path.join(self.root, "UI", relative)
        image = None
        stem = os.path.basename(relative)
        if not os.path.exists(path) and self.ra2 and os.path.exists(self.archive):
            try:
                data = subprocess.run(
                    [self.ra2, "cfz", self.archive, "cat", stem],
                    check=True, capture_output=True,
                ).stdout
                image = self.decode(stem, data)
            except Exception:
                image = None
            if image is not None:
                self.cache[relative] = image
                return image
        if not os.path.exists(path):
            self.missing.append(relative)
        else:
            data = open(path, "rb").read()
            try:
                image = self.decode(relative, data)
            except ValueError as e:
                self.unsupported.append((relative, str(e)))
        self.cache[relative] = image
        return image

    @staticmethod
    def decode(name, data):
        return load_bmp(data) if name.lower().endswith(".bmp") else load_tga(data)


def draw_fill(canvas, width, height, x, y, w, h, colour, clip):
    """A solid rectangle: the quad the engine draws for a filled `Rectangle`."""
    r, g, b, a = colour
    for py in range(int(round(h))):
        ty = y + py + 0.5
        if ty < 0 or ty >= height:
            continue
        for px in range(int(round(w))):
            tx = x + px + 0.5
            if tx < 0 or tx >= width:
                continue
            if clip is not None and not (clip[0] <= tx < clip[0] + clip[2] and clip[1] <= ty < clip[1] + clip[3]):
                continue
            q = (int(ty) * width + int(tx)) * 3
            for c, want in enumerate((r, g, b)):
                canvas[q + c] = int(canvas[q + c] + (want * 255.0 - canvas[q + c]) * a)


def composite(lines, images, width, height, game, args):
    canvas = bytearray(width * height * 3)  # black, opaque
    drawn = 0
    fonts = {}
    texts = []
    fills = []
    tiles_drawn = 0
    for line in lines:
        parts = line.split()
        if len(parts) < 13:
            continue
        x, y, w, h = (float(v) for v in parts[0:4])
        alpha_flag = parts[4] == "1"
        tint = tuple(float(v) for v in parts[5:8])
        u0, v0, u1, v1 = (float(v) for v in parts[8:12])
        cx, cy, cw, ch = (float(v) for v in parts[12:16])
        # The resolver's clip: nothing clips when the rectangle is empty. It is
        # what `ClipMode` means, and a widget that overflows its container is
        # cropped by it.
        clip = None if cw <= 0.0 or ch <= 0.0 else (cx, cy, cw, ch)
        relative = " ".join(parts[16:])
        if relative.startswith("fill:"):
            # `fill:<r> <g> <b> <a>` — a filled rectangle, drawn in order.
            r, g, b, a = (float(v) for v in relative[5:].split())
            draw_fill(canvas, width, height, x, y, w, h, (r, g, b, a), clip)
            drawn += 1
            continue
        if relative.startswith("text:"):
            # `text:<font>:<r> <g> <b>:<the string>` — drawn after the images.
            _, font_name, colour, body = relative.split(":", 3)
            texts.append((x, y, font_name, tuple(float(c) for c in colour.split()), body, clip))
            continue
        image = images.get(relative)
        if image is None:
            continue
        sw, sh, src = image
        # The four numbers are the image's rect in a 1024x1024 *page*. A rect
        # that runs *past* the page is a **tile**: a pattern repeated at its own
        # natural size to fill the frame (`metaldots_pattern`'s 341-pixel box is
        # 5.32813 x 64). Anything else whose rect does not match the file is a
        # whole-page image — the file is bigger than a page, the cement
        # background is 1280 wide — and drawing it whole is exactly right.
        if u1 > 1.0001 or v1 > 1.0001 or u0 < -0.0001 or v0 < -0.0001:
            tiled = True
        elif abs((u1 - u0) * 1024.0 - sw) > 1.0 or abs((v1 - v0) * 1024.0 - sh) > 1.0:
            tiled = False
            u0, v0, u1, v1 = 0.0, 0.0, 1.0, 1.0
        else:
            tiled = False
        # The rect is in the 1024² *page*, and a cell's shipped file is already
        # cropped to that cell — so the file is sampled 0..1, which is the frame's
        # own fraction. The rect is only used to tell a cell from a *tile* (a
        # repeat, left out rather than stretched) and from a whole-page image.
        drawn += 1
        if tiled:
            tiles_drawn += 1
        for py in range(int(round(h))):
            ty = y + py + 0.5
            if ty < 0 or ty >= height:
                continue
            if tiled:
                # A pattern repeats at its own size: the source pixel is the
                # destination one, wrapped — which is the same mapping the
                # engine gets by asking a repeating sampler for `frame/texture`.
                sy = min(sh - 1, max(0, int(py) % sh))
            else:
                sy = min(sh - 1, max(0, int((py + 0.5) / max(h, 1e-6) * sh)))
            for px in range(int(round(w))):
                tx = x + px + 0.5
                if tx < 0 or tx >= width:
                    continue
                if clip is not None and not (clip[0] <= tx < clip[0] + clip[2] and clip[1] <= ty < clip[1] + clip[3]):
                    continue
                if tiled:
                    sx = min(sw - 1, max(0, int(px) % sw))
                else:
                    sx = min(sw - 1, max(0, int((px + 0.5) / max(w, 1e-6) * sw)))
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
    cfz = os.path.join(game, "RA2.cfz")
    tool = ra2_tool()
    for x, y, font_name, colour, body, clip in texts:
        font = fonts.get(font_name)
        if font is None:
            try:
                font = Font(tool, cfz, font_name, tempfile.gettempdir())
            except Exception as e:  # a missing face is not a failed composite
                print(f"   could not load font {font_name}: {e}")
                font = False
            fonts[font_name] = font
        if font is False:
            continue
        # The caller passes the box's top; the baseline is one ascent down it.
        before = sum(canvas)
        font.draw(canvas, width, height, x, y + font.ascent, body, colour, clip)
        after = sum(canvas)
        if os.environ.get("UI_DEBUG"):
            print(f"   text {body!r} at ({x},{y}) with {font_name}: {after - before} px")
    return canvas, drawn, len(texts), tiles_drawn


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
        cfz = os.path.join(game, "RA2.cfz")
        cmd = [engine, args.screen, str(args.width), str(args.height), "--rects"]
        if os.path.exists(cfz):
            # Let the resolver size the labels from their own fonts, the way the
            # game does.
            cmd += ["--fonts", cfz]
        text = subprocess.run(cmd, check=True, capture_output=True, text=True).stdout

    lines = [ln for ln in text.splitlines() if ln.strip()]
    images = Images(game, ra2=ra2_tool())
    canvas, drawn, texts, tiles = composite(lines, images, args.width, args.height, game, args)
    write_png(args.out, args.width, args.height, canvas)
    print(f"wrote {args.out} ({args.width}x{args.height}), {drawn} of {len(lines)} drawables")
    if images.missing:
        print(f"  {len(images.missing)} referenced images are not in the tree:")
        for m in images.missing[:6]:
            print(f"     {m}")
    if tiles:
        print(f"  {tiles} tiles drawn as repeats")
    if images.unsupported:
        print(f"  {len(images.unsupported)} images this loader cannot read:")
        for m, why in images.unsupported[:6]:
            print(f"     {m}: {why}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
