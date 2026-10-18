#!/usr/bin/env python3
"""Composite a `.gib` screen into a PNG, from the layout's own data.

This is the independent half of the project's two-renderer check. It reads the
`.gib` file, resolves every `Position`/`Size`/`Alignment` against its parent's
frame, and draws the result with the game's own art under `UI/`. `ra2-render`'s
`uishot` draws the same screen from `ra2ui`; neither reads the other's layout, so
a rule that only one of them implements shows up as a picture rather than as two
programs agreeing with themselves.

    tools/ui_compose.py <screen.gib> <out.png> [--width W] [--height H]
                        [--game DIR] [--dump PATH]

The layout rules are the data's, not a paraphrase of the Rust:

* `Position x xf y yf` and `Size w wf h hf`; a set flag makes the amount a
  fraction of the parent's extent along that axis. A `Size` of zero is the
  editor's automatic size: a label takes its text's extent.
* `Alignment` is a 3x3 anchor of the widget's **box** — except on a `TextBox`.
  A `TextBox`'s `Alignment` is the alignment of the text inside it, so the box
  stays at its authored `Position`. The data settles it: `matchresults.gib`'s
  title is `Position 0 0`, `Size 1 1 28 0`, `Alignment 18`; anchored as a box it
  would start half a box above its panel and be clipped away.
* A `Text` that is a direct child of a `TextBox` or a `Group` has a **baseline**
  in `_Position.y`, not a line-box top. Its frame is shifted up by the face's
  ascent, so drawing at `frame.y + ascent` lands the glyphs on the authored
  baseline (`statusmeters.gib`'s name is at y 8 in a 12-pixel box).
* A hidden widget takes its whole subtree with it. A hidden group's children are
  still `Visible true` in the file, so the walk stops at the hidden widget.
* `ClipMode` 0/1/2: none, the widget's own frame, or its parent's.
* `ChildOrder` ascending, ties by file order; a negative order on a pressed
  control draws last.
* A `Proxy` is replaced by the `.gib` it names, drawn in the proxy's frame.
* Attachments: a `Background` draws behind its widget, every other body in
  front; a `Button`/`CheckBox`'s caption is centred in the button.
* A `SplitFrame` gives its children the two panes; a `ScrollBar` moves its three
  named parts to its ends and the track between them.

`--dump` keeps the older path of reading a saved `uidump --rects` file, which is
handy for diffing the resolver against this one but is not used by the parity
check: there the two renderers must lay the screen out independently.

The glyph page is the game's own `fg_*.giff`, dumped by `ra2 font --page`. The
page is **bottom-up only**: `v` is turned over and `u` is kept, because flipped
vertically the page's row reads `E F G H I J K L M N O P Q R S T U` upright and
in order. Text is blended towards the tint by the page's coverage.

Context: `docs/06-deferred-fidelity.md` F1's method, applied to the shell;
`docs/03-engine-design.md` M5; `docs/10-roadmap.md` rule 3; `docs/14-match-
presentation.md` for the layout correction this follows.
"""

import argparse
import os
import struct
import subprocess
import sys
import tempfile
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


# --------------------------------------------------------------------------
# PNG, TGA and BMP -- standard library only
# --------------------------------------------------------------------------


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
                # A TGA's 24-bit pixels are BGR, like its 32-bit ones; reading
                # them as RGB turned `event_powerbolt.tga`'s blue bar orange.
                b, g, r = pixels[o : o + 3]
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
    (offset,) = struct.unpack_from("<I", data, 10)
    (header_size,) = struct.unpack_from("<I", data, 14)
    if header_size < 40:
        raise ValueError("old BMP header")
    w, h, planes, bpp = struct.unpack_from("<iiHH", data, 18)
    (compression,) = struct.unpack_from("<I", data, 30)
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


# --------------------------------------------------------------------------
# Tools and art
# --------------------------------------------------------------------------


def ra2_tool():
    """The engine CLI, which is what reads the archives and the faces. The debug
    build first: it is the one a working tree has just rebuilt."""
    if os.environ.get("RA2_BIN"):
        return os.environ["RA2_BIN"]
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    for profile in ("debug", "release"):
        path = os.path.join(root, "engine", "target", profile, "ra2")
        if os.path.exists(path):
            return path
    return None


class Font:
    """A `fg_*.giff`, dumped by `ra2 font` so that this stays a picture-maker."""

    FIRST_CHAR = 32  # see `ra2fmt::giff`: 95 records, 0x20..0x7E

    def __init__(self, ra2_binary, cfz, name, tmpdir):
        self.name = name
        pgm = os.path.join(tmpdir, f"{name}.pgm")
        out = subprocess.run(
            [ra2_binary, "font", cfz, name, "--page", pgm],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        self.glyphs = {}
        self.ascent = 0
        self.descent = 0
        for line in out.splitlines():
            parts = line.split()
            if not parts:
                continue
            if parts[0] == "glyph":
                self.glyphs[int(parts[1])] = (
                    int(parts[2]),
                    int(parts[3]),
                    int(parts[4]),
                    *(float(v) for v in parts[5:9]),
                )
            elif parts[0] == "ascent":
                self.ascent = int(parts[1])
            elif parts[0] == "descent":
                self.descent = int(parts[1])
        self.glyphs_only = [g for _, g in sorted(self.glyphs.items())]
        raw = open(pgm, "rb").read()
        head = raw.split(b"\n", 3)[1].split()
        self.width, self.height = int(head[0]), int(head[1])
        self.page = raw.split(b"\n", 3)[3]

    def line_height(self):
        """`ascent - descent`; the descent is stored negative."""
        return self.ascent - self.descent

    def glyph(self, ch):
        i = ord(ch) - self.FIRST_CHAR
        if 0 <= i < len(self.glyphs_only):
            return self.glyphs_only[i]
        return None

    def advance(self, ch):
        g = self.glyph(ch)
        return g[0] if g else self.space_advance()

    def space_advance(self):
        """The space has its own record (cell empty, advance 2-6)."""
        return self.glyphs_only[0][0]

    def extent(self, text):
        """`(sum of advances, line height)`, the game's own extent."""
        return sum(self.advance(c) for c in text), self.line_height()

    def _rect(self, g):
        _adv, bearing_x, bearing_y, u0, v0, u1, v1 = g
        # The page is in texture order (top row first) and `v` is measured on
        # it from the top, as in `ra2fmt::giff`.
        x0, x1 = round(u0 * self.width), round(u1 * self.width)
        y0, y1 = round(v0 * self.height), round(v1 * self.height)
        return bearing_x, bearing_y, x0, y0, x1 - x0, y1 - y0

    def draw_glyph(self, canvas, width, height, x, y, ch, tint, clip):
        """Draw one glyph with its cell's top at `(x, y)`."""
        g = self.glyph(ch)
        if g is None:
            return
        _bx, _by, gx, gy, w, h = self._rect(g)
        for yy in range(h):
            sy = gy + yy
            row = self.page[sy * self.width + gx : sy * self.width + gx + w]
            for xx, v in enumerate(row):
                if not v:
                    continue
                X, Y = int(x + xx), int(y + yy)
                if not (0 <= X < width and 0 <= Y < height):
                    continue
                if clip is not None and not (
                    clip[0] <= x + xx < clip[0] + clip[2] and clip[1] <= y + yy < clip[1] + clip[3]
                ):
                    continue
                q = (Y * width + X) * 3
                # `v` is coverage, so blend towards the tint rather than adding
                # a fraction: white ink over dark cement.
                coverage = v / 255.0 * (tint[3] if len(tint) > 3 else 1.0)
                for c in range(3):
                    want = tint[c] * 255.0
                    canvas[q + c] = max(
                        0, min(255, int(canvas[q + c] + (want - canvas[q + c]) * coverage))
                    )

    def draw(self, canvas, width, height, x, y, text, tint, clip=None):
        """Draw a single line with its baseline at `y` and its pen at `x`."""
        pen = x
        for ch in text:
            g = self.glyph(ch)
            if g is None:
                pen += self.space_advance()
                continue
            bx, by, _gx, _gy, _w, _h = self._rect(g)
            self.draw_glyph(canvas, width, height, pen + bx, y - by, ch, tint, clip)
            pen += g[0]

    def wrap(self, text, width):
        """The authored `\\n` breaks and the space wrap inside `width`."""
        out = []
        for hard in text.split("\n"):
            if width <= 0.0:
                out.append(hard)
                continue
            space = self.advance(" ")
            line = ""
            pen = 0.0
            for word in hard.split(" "):
                w = self.extent(word)[0]
                if not line:
                    line, pen = word, w
                elif pen + space + w <= width:
                    line += " " + word
                    pen += space + w
                else:
                    out.append(line)
                    line, pen = word, w
            out.append(line)
        return out

    def draw_paragraph(self, canvas, width, height, frame, text, tint, clip):
        """A `Glyph`'s paragraph: wrapped lines one line height apart, the first
        baseline one ascent below the box's top."""
        line_height = self.line_height()
        for i, line in enumerate(self.wrap(text, frame[2])):
            if not line:
                continue
            self.draw(
                canvas,
                width,
                height,
                frame[0],
                frame[1] + i * line_height + self.ascent,
                line,
                tint,
                clip,
            )


class Fonts:
    """The faces a screen names, loaded on demand."""

    def __init__(self, ra2_binary, cfz):
        self.ra2 = ra2_binary
        self.cfz = cfz
        self.cache = {}
        self.failed = set()

    def get(self, name):
        if name in self.cache:
            return self.cache[name]
        if name in self.failed or not self.ra2 or not self.cfz:
            return None
        try:
            font = Font(self.ra2, self.cfz, name, tempfile.gettempdir())
        except Exception as e:  # a missing face is not a failed composite
            print(f"   could not load font {name}: {e}")
            self.failed.add(name)
            return None
        self.cache[name] = font
        return font


class Images:
    def __init__(self, root, ra2=None):
        self.root = root
        # `ra2 cfz <archive> cat <entry>` -- the shell's own art lives in
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
                    check=True,
                    capture_output=True,
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


# --------------------------------------------------------------------------
# The `.gib` reader
# --------------------------------------------------------------------------

ATTACHMENT_KINDS = {"Background", "Border", "Caption", "Scroller", "Sound"}


def is_label(token):
    if not token:
        return False
    if not (token[0].isalpha() or token[0] == "_"):
        return False
    return all(c.isalnum() or c == "_" for c in token)


def read_number(token):
    try:
        return float(token)
    except ValueError:
        pass
    if token.startswith("-"):
        sign, body = -1.0, token[1:]
    else:
        sign, body = 1.0, token
    if not body.startswith("1.#"):
        return None
    body = body[3:]
    if body in ("INF", "INFINITY"):
        return sign * float("inf")
    if body in ("IND", "QNAN", "SNAN", "NAN"):
        return float("nan")
    return None


class Field:
    def __init__(self, name, raw):
        self.name = name
        self.raw = raw

    def text(self):
        return self.raw

    def numbers(self):
        out = []
        for token in self.raw.split():
            v = read_number(token)
            if v is not None:
                out.append(v)
        return out

    def number(self):
        first = self.raw.split()[0] if self.raw.split() else None
        return read_number(first) if first is not None else None

    def flag(self):
        value = self.raw.strip()
        if value in ("true", "1"):
            return True
        if value in ("false", "0"):
            return False
        return None

    def four(self):
        v = self.numbers()
        return v if len(v) == 4 else None

    def axes(self):
        v = self.numbers()
        if len(v) != 4:
            return None
        return (v[0], v[1] != 0.0), (v[2], v[3] != 0.0)

    def length(self):
        v = self.numbers()
        if len(v) != 2:
            return None
        return v[0], v[1] != 0.0


class Attachment:
    def __init__(self, kind, fields):
        self.kind = kind
        self.fields = list(fields)
        self.body = None

    def field(self, name):
        return next((f for f in self.fields if f.name == name), None)

    def flag(self, name):
        f = self.field(name)
        return f.flag() if f else None


class Widget:
    def __init__(self, kind, fields, implicit=False):
        self.kind = kind
        self.fields = list(fields)
        self.implicit = implicit
        self.children = []
        self.attachments = []

    def field(self, name):
        return next((f for f in self.fields if f.name == name), None)

    def fields_named(self, name):
        return [f for f in self.fields if f.name == name]

    def has_field(self, name):
        return any(f.name == name for f in self.fields)

    def number(self, name):
        f = self.field(name)
        return f.number() if f else None

    def flag(self, name):
        f = self.field(name)
        return f.flag() if f else None

    def four(self, name):
        f = self.field(name)
        return f.four() if f else None

    def text_field(self, name):
        f = self.field(name)
        return f.text() if f else None

    def text(self):
        text = self.text_field("_Text")
        return text if text is not None else self.text_field("Text")

    def font(self):
        return self.text_field("_Font")

    def fore_color(self):
        return self.four("_ForeColor") or self.four("ForeColor")

    def visible(self):
        v = self.flag("Visible")
        return True if v is None else v

    def child_order(self):
        n = self.number("ChildOrder")
        return int(n) if n is not None else 0

    def clip_mode(self):
        n = self.number("ClipMode")
        return int(n) if n is not None else 0

    def alignment(self):
        n = self.number("Alignment")
        return int(n) if n is not None else 0

    def position(self):
        f = self.field("Position")
        return f.axes() if f else None

    def size(self):
        f = self.field("Size")
        return f.axes() if f else None

    def uv(self):
        unnamed = [f for f in self.fields if f.name is None]
        return unnamed[0].four() if unnamed else None

    def filename(self):
        return self.text_field("_Filename")

    def id(self):
        n = self.number("ID")
        return int(n) if n is not None else -1

    def name(self):
        return self.text_field("Name") or ""

    def attachment(self, kind):
        for a in self.attachments:
            if a.kind == kind:
                return a
        return None


class Gib:
    def __init__(self, version, blocks):
        self.version = version
        self.blocks = blocks


class GibParser:
    def __init__(self, text):
        self.lines = [ln[:-1] if ln.endswith("\r") else ln for ln in text.split("\n")]
        self.at = 0

    def skip_blank(self):
        while self.at < len(self.lines) and not self.lines[self.at].strip():
            self.at += 1

    def header(self):
        if self.at >= len(self.lines):
            return None
        line = self.lines[self.at]
        if line.startswith("[") and line.endswith("]"):
            return line[1:-1]
        return None

    def record(self):
        self.skip_blank()
        if self.at >= len(self.lines):
            return None
        kind = self.header()
        if kind is not None:
            self.at += 1
            implicit = False
        else:
            kind, implicit = "Widget", True
        fields = []
        while self.at < len(self.lines):
            line = self.lines[self.at]
            if not line.strip():
                self.at += 1
                continue
            if line.startswith("["):
                break
            name, sep, raw = line.partition(" ")
            if sep and is_label(name):
                fields.append(Field(name, raw.rstrip()))
                counted = name == "NumChildren"
            else:
                fields.append(Field(None, line.strip()))
                counted = False
            self.at += 1
            if counted:
                break
        return kind, fields, implicit

    def headerless(self):
        self.skip_blank()
        if self.at >= len(self.lines):
            return False
        line = self.lines[self.at]
        return bool(line.strip()) and not line.startswith("[")

    def widget(self, kind, fields, implicit):
        count_field = next((f for f in fields if f.name == "NumChildren"), None)
        count = int(count_field.number() or 0) if count_field else 0
        children = []
        counted = 0
        while counted < count:
            self.skip_blank()
            if self.at >= len(self.lines):
                raise ValueError("unexpected end of file in a widget's children")
            block = self.block()
            if block is None or not isinstance(block, Widget):
                raise ValueError("an attachment with no widget above it")
            if not block.implicit:
                counted += 1
            children.append(block)
        widget = Widget(kind, fields, implicit)
        widget.children = children
        return widget

    def block(self):
        record = self.record()
        if record is None:
            return None
        kind, fields, implicit = record
        if not any(f.name == "NumChildren" for f in fields):
            attachment = Attachment(kind, fields)
            self.skip_blank()
            if self.at >= len(self.lines):
                return attachment
            block = self.block()
            if not isinstance(block, Widget):
                return attachment
            if self.headerless():
                body = self.record()
                if body is not None:
                    bkind, bfields, bimplicit = body
                    attachment.body = self.widget(bkind, bfields, bimplicit)
            block.attachments.insert(0, attachment)
            return block
        return self.widget(kind, fields, implicit)

    def parse(self):
        self.skip_blank()
        header = self.lines[self.at] if self.at < len(self.lines) else ""
        if not header.startswith("T "):
            raise ValueError(f"a .gib starts with 'T <version>', found {header!r}")
        version = header[2:].strip()
        self.at += 1
        blocks = []
        while True:
            self.skip_blank()
            if self.at >= len(self.lines):
                break
            blocks.append(self.block())
        return Gib(version, blocks)


def parse_gib(text):
    return GibParser(text).parse()


def find_ci(root, relative):
    """A case-insensitive path under `root`, the way the game finds its art."""
    parts = [p for p in relative.replace("\\", "/").split("/") if p and p != "."]
    path = root
    for part in parts:
        try:
            entries = os.listdir(path)
        except OSError:
            return None
        match = next((e for e in entries if e.lower() == part.lower()), None)
        if match is None:
            return None
        path = os.path.join(path, match)
    return path


def proxy_path(widget):
    if widget.kind != "Proxy":
        return None
    path = widget.text_field("Filename")
    return path if path else None


def inline_proxies(gib, ui_root):
    """Replace every `Proxy` with the `.gib` it names, in the proxy's frame."""
    loaded = []

    def inline_one(widget, depth):
        for child in widget.children:
            inline_one(child, depth)
        if depth == 0:
            return
        rel = proxy_path(widget)
        if not rel:
            return
        path = find_ci(ui_root, rel)
        if path is None:
            return
        try:
            fragment = parse_gib(open(path, encoding="utf-8", errors="replace").read())
        except (OSError, ValueError):
            return
        loaded.append(rel)
        widgets = [b for b in fragment.blocks if isinstance(b, Widget)]
        for w in widgets:
            inline_one(w, depth - 1)
        widget.children.extend(widgets)

    for block in gib.blocks:
        if isinstance(block, Widget):
            inline_one(block, 4)
    return loaded


# --------------------------------------------------------------------------
# The layout
# --------------------------------------------------------------------------


class Frame:
    __slots__ = ("x", "y", "w", "h")

    def __init__(self, x, y, w, h):
        self.x, self.y, self.w, self.h = x, y, w, h

    def right(self):
        return self.x + self.w

    def bottom(self):
        return self.y + self.h

    def as_tuple(self):
        return (self.x, self.y, self.w, self.h)


def anchor_from_alignment(alignment):
    h = {1: 0.0, 2: 0.5, 4: 1.0}.get(alignment & 0x7, 0.0)
    v = {8: 0.0, 16: 0.5, 32: 1.0}.get(alignment & 0x38, 0.0)
    return h, v


def axis(length, parent):
    amount, fraction = length
    return amount * parent if fraction else amount


def has_alignment(widget):
    """Whether `Alignment` anchors the widget's box. A `TextBox`'s is the text's
    alignment inside the box, so the box keeps its authored `Position`."""
    return widget.has_field("Alignment") and widget.kind != "TextBox"


def intersect(clip, with_):
    if clip is None:
        return with_
    x = max(clip.x, with_.x)
    y = max(clip.y, with_.y)
    return Frame(
        x,
        y,
        max(0.0, min(clip.right(), with_.right()) - x),
        max(0.0, min(clip.bottom(), with_.bottom()) - y),
    )


class Placed:
    def __init__(self, widget, frame, clip, children, attachments):
        self.widget = widget
        self.frame = frame
        self.clip = clip
        self.children = children
        self.attachments = attachments


class PlacedAttachment:
    def __init__(self, attachment, body):
        self.attachment = attachment
        self.body = body


class Layout:
    def __init__(self, fonts):
        self.fonts = fonts

    def measured(self, widget):
        if self.fonts is None:
            return None
        text, face = widget.text(), widget.font()
        if not text or not face:
            return None
        font = self.fonts.get(face)
        if font is None:
            return None
        return font.extent(text)

    def sized(self, widget, parent, baseline):
        fit = self.measured(widget)
        size = widget.size()

        def dim(length, parent_extent, fallback):
            if length is not None and (length[0] != 0.0 or length[1]):
                return axis(length, parent_extent)
            return fallback

        if size is None:
            w, h = parent.w, parent.h
        else:
            w = dim(size[0], parent.w, fit[0] if fit else 0.0)
            h = dim(size[1], parent.h, fit[1] if fit else 0.0)
        position = widget.position()
        px, py = (0.0, 0.0)
        if position is not None:
            px = axis(position[0], parent.w)
            py = axis(position[1], parent.h)
        ah, av = anchor_from_alignment(widget.alignment()) if has_alignment(widget) else (0.0, 0.0)
        ascent = 0.0
        if baseline:
            face = widget.font()
            font = self.fonts.get(face) if (self.fonts and face) else None
            ascent = float(font.ascent) if font else 0.0
        return Frame(
            parent.x + px - ah * w,
            parent.y + py - av * h - ascent,
            w,
            h,
        )

    def place(self, widget, parent, inherited, baseline=False):
        frame = self.sized(widget, parent, baseline)
        mode = widget.clip_mode()
        if mode == 1:
            clip = intersect(inherited, frame)
        elif mode == 2:
            clip = intersect(inherited, parent)
        else:
            clip = inherited
        if widget.kind == "SplitFrame":
            children = self.split_children(widget, frame, clip)
        elif widget.kind == "ScrollBar":
            children = self.scroll_children(widget, frame, clip)
        else:
            baseline_text = widget.kind in ("TextBox", "Group")
            children = [self.place(c, frame, clip, baseline_text) for c in widget.children]
        attachments = []
        for a in widget.attachments:
            if a.body is None:
                continue
            attachments.append(PlacedAttachment(a, self.place(a.body, parent, inherited, False)))
        self.centre_captions(widget, frame, attachments)
        return Placed(widget, frame, clip, children, attachments)

    def split_children(self, widget, frame, clip):
        horizontal = widget.flag("Horizontal")
        if horizontal is None:
            horizontal = True
        master = widget.number("Master")
        master = int(master) if master is not None else 0
        length = widget.field("MasterSize").length() if widget.field("MasterSize") else None
        extent = frame.w if horizontal else frame.h
        master_size = axis(length, extent) if length is not None else extent / 2.0
        master_size = max(0.0, min(extent, master_size))
        if horizontal:
            a = Frame(frame.x, frame.y, master_size, frame.h)
            b = Frame(frame.x + master_size, frame.y, frame.w - master_size, frame.h)
        else:
            a = Frame(frame.x, frame.y, frame.w, master_size)
            b = Frame(frame.x, frame.y + master_size, frame.w, frame.h - master_size)
        panes = (b, a) if master == 1 else (a, b)
        return [
            self.place(child, panes[min(i, 1)], clip, False)
            for i, child in enumerate(widget.children)
        ]

    def scroll_children(self, widget, frame, clip):
        def index(name):
            f = widget.field(name)
            if f is None:
                return 0
            v = f.numbers()
            return int(v[0]) if v else 0

        vertical = widget.number("Orientation")
        vertical = vertical != 2.0 if vertical is not None else True
        placed = [self.place(c, frame, clip, False) for c in widget.children]
        less = placed[index("Less")].frame if index("Less") < len(placed) else Frame(frame.x, frame.y, 0, 0)
        more = placed[index("More")].frame if index("More") < len(placed) else Frame(frame.x, frame.y, 0, 0)
        if vertical:
            parts = [
                Frame(frame.x, frame.y, less.w, less.h),
                Frame(frame.x, frame.bottom() - more.h, more.w, more.h),
                Frame(
                    frame.x,
                    frame.y + less.h,
                    frame.w,
                    max(0.0, frame.h - less.h - more.h),
                ),
            ]
        else:
            parts = [
                Frame(frame.x, frame.y, less.w, less.h),
                Frame(frame.right() - more.w, frame.y, more.w, more.h),
                Frame(
                    frame.x + less.w,
                    frame.y,
                    max(0.0, frame.w - less.w - more.w),
                    frame.h,
                ),
            ]
        for which, target in ((index("Less"), parts[0]), (index("More"), parts[1]), (index("Slider"), parts[2])):
            if which >= len(placed):
                continue
            placement = placed[which]
            self.shift(placement, target.x - placement.frame.x, target.y - placement.frame.y)
            placement.frame.w = target.w
            placement.frame.h = target.h
            self.reclip(placement, target, clip)
        return placed

    def shift(self, placement, dx, dy):
        if dx == 0.0 and dy == 0.0:
            return
        placement.frame.x += dx
        placement.frame.y += dy
        for child in placement.children:
            self.shift(child, dx, dy)
        for a in placement.attachments:
            self.shift(a.body, dx, dy)

    def reclip(self, placement, parent, inherited):
        mode = placement.widget.clip_mode()
        if mode == 1:
            placement.clip = intersect(inherited, placement.frame)
        elif mode == 2:
            placement.clip = intersect(inherited, parent)
        else:
            placement.clip = inherited
        frame, clip = placement.frame, placement.clip
        for child in placement.children:
            self.reclip(child, frame, clip)
        for a in placement.attachments:
            self.reclip(a.body, parent, inherited)

    def centre_captions(self, widget, frame, attachments):
        if widget.kind not in ("Button", "CheckBox") or widget.attachment("Caption") is None:
            return
        for a in attachments:
            if a.attachment.kind != "Caption":
                continue
            for caption in a.body.children:
                if caption.widget.kind != "Text":
                    continue
                text = caption.frame
                caption.frame = Frame(
                    frame.x + (frame.w - text.w) / 2.0,
                    frame.y + (frame.h - text.h) / 2.0,
                    text.w,
                    text.h,
                )


PRESSED_CONTROLS = {"Button", "CheckBox", "TextBox", "Slider", "DropDown"}


def draw_key(widget):
    order = widget.child_order()
    if order < 0 and widget.kind in PRESSED_CONTROLS:
        return 2**31 - 1
    return order


def draw_order(roots):
    """Back to front; a hidden widget takes its subtree with it."""
    out = []

    def visit(placement, parent):
        if not placement.widget.visible():
            return
        for a in placement.attachments:
            if a.attachment.kind == "Background":
                visit(a.body, placement)
        out.append((placement, parent))
        children = list(reversed(placement.children))
        children.sort(key=lambda c: draw_key(c.widget))
        for child in children:
            visit(child, placement)
        for a in placement.attachments:
            if a.attachment.kind != "Background":
                visit(a.body, placement)

    top = list(reversed(roots))
    top.sort(key=lambda p: draw_key(p.widget))
    for root in top:
        visit(root, None)
    return out


def art_for(widget, parent):
    file = widget.filename()
    if file is None:
        return None
    if file:
        return file
    if (
        parent is not None
        and parent.kind == "Rectangle"
        and parent.id() in (100, 101)
        and widget.kind == "Image"
    ):
        return "Point Selection.tga"
    return None


# --------------------------------------------------------------------------
# Drawing
# --------------------------------------------------------------------------


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
            if clip is not None and not (
                clip[0] <= tx < clip[0] + clip[2] and clip[1] <= ty < clip[1] + clip[3]
            ):
                continue
            q = (int(ty) * width + int(tx)) * 3
            for c, want in enumerate((r, g, b)):
                canvas[q + c] = int(canvas[q + c] + (want * 255.0 - canvas[q + c]) * a)


def draw_image(canvas, width, height, images, relative, x, y, w, h, tint, alpha_flag, clip):
    image = images.get(relative)
    if image is None:
        return False
    sw, sh, src = image
    for py in range(int(round(h))):
        ty = y + py + 0.5
        if ty < 0 or ty >= height:
            continue
        sy = min(sh - 1, max(0, int((py + 0.5) / max(h, 1e-6) * sh)))
        for px in range(int(round(w))):
            tx = x + px + 0.5
            if tx < 0 or tx >= width:
                continue
            if clip is not None and not (
                clip[0] <= tx < clip[0] + clip[2] and clip[1] <= ty < clip[1] + clip[3]
            ):
                continue
            sx = min(sw - 1, max(0, int((px + 0.5) / max(w, 1e-6) * sw)))
            o = (sy * sw + sx) * 4
            r, g, b, a = src[o], src[o + 1], src[o + 2], src[o + 3]
            if not alpha_flag:
                a = 255
            # The shader multiplies the texture's alpha by the tint's, and the
            # tint's own alpha is what makes the bar's rounded ends translucent.
            a = int(a * (tint[3] if len(tint) > 3 else 1.0))
            if a == 0:
                continue
            r = min(255, int(r * tint[0]))
            g = min(255, int(g * tint[1]))
            b = min(255, int(b * tint[2]))
            q = (int(ty) * width + int(tx)) * 3
            if a == 255:
                canvas[q], canvas[q + 1], canvas[q + 2] = r, g, b
            else:
                inv = 255 - a
                canvas[q] = (r * a + canvas[q] * inv) // 255
                canvas[q + 1] = (g * a + canvas[q + 1] * inv) // 255
                canvas[q + 2] = (b * a + canvas[q + 2] * inv) // 255
    return True


def draw_tile(canvas, width, height, images, relative, x, y, w, h, tint, alpha_flag, clip):
    image = images.get(relative)
    if image is None:
        return False
    sw, sh, src = image
    for py in range(int(round(h))):
        ty = y + py + 0.5
        if ty < 0 or ty >= height:
            continue
        sy = min(sh - 1, max(0, int(py) % sh))
        for px in range(int(round(w))):
            tx = x + px + 0.5
            if tx < 0 or tx >= width:
                continue
            if clip is not None and not (
                clip[0] <= tx < clip[0] + clip[2] and clip[1] <= ty < clip[1] + clip[3]
            ):
                continue
            sx = min(sw - 1, max(0, int(px) % sw))
            o = (sy * sw + sx) * 4
            r, g, b, a = src[o], src[o + 1], src[o + 2], src[o + 3]
            if not alpha_flag:
                a = 255
            # The shader multiplies the texture's alpha by the tint's, and the
            # tint's own alpha is what makes the bar's rounded ends translucent.
            a = int(a * (tint[3] if len(tint) > 3 else 1.0))
            if a == 0:
                continue
            r = min(255, int(r * tint[0]))
            g = min(255, int(g * tint[1]))
            b = min(255, int(b * tint[2]))
            q = (int(ty) * width + int(tx)) * 3
            if a == 255:
                canvas[q], canvas[q + 1], canvas[q + 2] = r, g, b
            else:
                inv = 255 - a
                canvas[q] = (r * a + canvas[q] * inv) // 255
                canvas[q + 1] = (g * a + canvas[q + 1] * inv) // 255
                canvas[q + 2] = (b * a + canvas[q + 2] * inv) // 255
    return True


def draw_lines(canvas, width, height, frame, parent, fields, colour, clip, line_width):
    """The `Lines` widget's polyline: `Point x xf y yf` pairs are segments."""
    outer = parent.frame if parent is not None else frame
    points = []
    for f in fields:
        n = f.numbers()
        if len(n) < 4:
            continue
        x = frame.x + (n[0] * outer.w if n[1] != 0.0 else n[0])
        y = frame.y + (n[2] * outer.h if n[3] != 0.0 else n[2])
        points.append((x, y))
    if len(points) < 2:
        return
    lw = max(1.0, line_width)
    for i in range(0, len(points) - 1, 2):
        ax, ay = points[i]
        bx, by = points[i + 1]
        dx, dy = bx - ax, by - ay
        length = (dx * dx + dy * dy) ** 0.5
        if length <= 0:
            continue
        nx, ny = -dy / length * lw / 2.0, dx / length * lw / 2.0
        vx = [(ax + nx, ay + ny), (bx + nx, by + ny), (bx - nx, by - ny), (ax - nx, ay - ny)]
        minx = max(0, int(min(p[0] for p in vx)))
        maxx = min(width, int(max(p[0] for p in vx)) + 1)
        miny = max(0, int(min(p[1] for p in vx)))
        maxy = min(height, int(max(p[1] for p in vx)) + 1)
        r, g, b, a = colour

        def sign(px, py, qx, qy, rx, ry):
            return (px - rx) * (qy - ry) - (qx - rx) * (py - ry)

        def inside(px, py):
            d1 = sign(px, py, *vx[0], *vx[1])
            d2 = sign(px, py, *vx[1], *vx[2])
            d3 = sign(px, py, *vx[2], *vx[3])
            d4 = sign(px, py, *vx[3], *vx[0])
            neg = d1 < 0 or d2 < 0 or d3 < 0 or d4 < 0
            pos = d1 > 0 or d2 > 0 or d3 > 0 or d4 > 0
            return not (neg and pos)

        for py in range(miny, maxy):
            ty = py + 0.5
            for px in range(minx, maxx):
                tx = px + 0.5
                if not inside(tx, ty):
                    continue
                if clip is not None and not (
                    clip[0] <= tx < clip[0] + clip[2] and clip[1] <= ty < clip[1] + clip[3]
                ):
                    continue
                q = (py * width + px) * 3
                canvas[q] = int(canvas[q] + (r * 255.0 - canvas[q]) * a)
                canvas[q + 1] = int(canvas[q + 1] + (g * 255.0 - canvas[q + 1]) * a)
                canvas[q + 2] = int(canvas[q + 2] + (b * 255.0 - canvas[q + 2]) * a)


def stated_size(widget):
    """`_Size` when it is a real drawn size, not the editor's 1280x1005 canvas."""
    f = widget.field("_Size")
    axes = f.axes() if f else None
    if axes is None:
        return None
    (w, wf), (h, hf) = axes
    if wf or hf:
        return None
    if (w, h) == (1280.0, 1005.0):
        return None
    return w, h


def composite_scene(placed, images, fonts, width, height):
    canvas = bytearray(width * height * 3)  # black, opaque
    drawn = 0
    labels = 0
    tiles = 0
    for placement, parent in draw_order(placed):
        widget = placement.widget
        if placement.clip is not None and (placement.clip.w <= 0.0 or placement.clip.h <= 0.0):
            continue
        clip = placement.clip.as_tuple() if placement.clip is not None else None
        x, y, w, h = placement.frame.as_tuple()
        if widget.kind == "Text":
            text, face = widget.text(), widget.font()
            if text and face:
                font = fonts.get(face)
                if font is not None:
                    tint = widget.fore_color() or [1.0, 1.0, 1.0, 1.0]
                    font.draw(canvas, width, height, x, y + font.ascent, text, tint, clip)
                    drawn += 1
                    labels += 1
            continue
        if widget.kind == "Glyph":
            text, face = widget.text(), widget.font()
            margin = widget.field("_Margin")
            m = margin.numbers() if margin else []
            if text and face and len(m) >= 3:
                font = fonts.get(face)
                if font is not None:
                    tint = widget.fore_color() or [1.0, 1.0, 1.0, 1.0]
                    frame = (x + m[0], y, m[1], m[2])
                    font.draw_paragraph(canvas, width, height, frame, text, tint, clip)
                    drawn += 1
                    labels += 1
            continue
        if widget.kind == "Lines":
            colour = widget.four("Color") or [1.0, 1.0, 1.0, 1.0]
            if colour[3] > 0.0:
                draw_lines(
                    canvas,
                    width,
                    height,
                    placement.frame,
                    parent,
                    widget.fields_named("Point"),
                    colour,
                    clip,
                    widget.number("Width") or 0.0,
                )
                drawn += 1
            continue
        if widget.kind == "Rectangle":
            if widget.flag("Filled"):
                colour = widget.four("FillColor") or [1.0, 1.0, 1.0, 1.0]
                if colour[3] > 0.0:
                    draw_fill(canvas, width, height, x, y, w, h, colour, clip)
                    drawn += 1
            continue
        if widget.kind != "Image":
            continue
        file = art_for(widget, parent.widget if parent is not None else None)
        if file is None:
            # A blank-named `Image` is a solid colour fill.
            colour = widget.fore_color() or [1.0, 1.0, 1.0, 1.0]
            if colour[3] > 0.0:
                draw_fill(canvas, width, height, x, y, w, h, colour, clip)
                drawn += 1
            continue
        uv = widget.uv() or [0.0, 0.0, 1.0, 1.0]
        tiled = uv[2] > 1.0001 or uv[3] > 1.0001 or uv[0] < -0.0001 or uv[1] < -0.0001
        tint = widget.fore_color() or [1.0, 1.0, 1.0, 1.0]
        alpha = widget.flag("_Alpha")
        alpha = True if alpha is None else alpha
        if tiled:
            if draw_tile(canvas, width, height, images, file, x, y, w, h, tint, alpha, clip):
                drawn += 1
                tiles += 1
            continue
        size = stated_size(widget)
        if size is not None:
            w, h = size
        if draw_image(canvas, width, height, images, file, x, y, w, h, tint, alpha, clip):
            drawn += 1
    return canvas, drawn, labels, tiles


# --------------------------------------------------------------------------
# The older path: rectangles from `uidump --rects`
# --------------------------------------------------------------------------


def composite_rects(lines, images, width, height, fonts):
    canvas = bytearray(width * height * 3)
    drawn = 0
    texts = []
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
        clip = None if cw <= 0.0 or ch <= 0.0 else (cx, cy, cw, ch)
        relative = " ".join(parts[16:])
        if relative.startswith("fill:"):
            r, g, b, a = (float(v) for v in relative[5:].split())
            draw_fill(canvas, width, height, x, y, w, h, (r, g, b, a), clip)
            drawn += 1
            continue
        if relative.startswith("text:"):
            _, font_name, colour, body = relative.split(":", 3)
            texts.append((x, y, font_name, tuple(float(c) for c in colour.split()), body, clip))
            continue
        image = images.get(relative)
        if image is None:
            continue
        sw, sh, src = image
        if u1 > 1.0001 or v1 > 1.0001 or u0 < -0.0001 or v0 < -0.0001:
            tiled = True
        elif abs((u1 - u0) * 1024.0 - sw) > 1.0 or abs((v1 - v0) * 1024.0 - sh) > 1.0:
            tiled = False
            u0, v0, u1, v1 = 0.0, 0.0, 1.0, 1.0
        else:
            tiled = False
        drawn += 1
        if tiled:
            tiles_drawn += 1
            draw_tile(canvas, width, height, images, relative, x, y, w, h, tint, alpha_flag, clip)
        else:
            draw_image(canvas, width, height, images, relative, x, y, w, h, tint, alpha_flag, clip)
    for x, y, font_name, colour, body, clip in texts:
        font = fonts.get(font_name)
        if font is None:
            continue
        font.draw(canvas, width, height, x, y + font.ascent, body, colour, clip)
    return canvas, drawn, len(texts), tiles_drawn


# --------------------------------------------------------------------------
# The rule's own test, with no game data
# --------------------------------------------------------------------------


class _FakeFace:
    """Enough of a face to resolve the layout: an ascent and a text extent."""

    ascent = 10
    descent = -4

    def extent(self, text):
        return len(text) * 6, self.ascent - self.descent


class _FakeFonts:
    def get(self, _name):
        return _FakeFace()


SELF_TEST_GIB = """T 1.09
[Group]
Alignment 9
Name root
Position 0 0 0 0
Size 100 0 50 0
Visible true
ChildOrder 0
ClipMode 0
NumChildren 2
[TextBox]
Alignment 18
Name title box
Position 10 0 5 0
Size 40 0 12 0
Visible true
ChildOrder 0
ClipMode 1
NumChildren 1
[Text]
_Font fake
_Text hi
_ForeColor 1 1 1 1
_Position 3 0 8 0
Name title
Position 3 0 8 0
Size 0 0 0 0
Visible true
ChildOrder 0
ClipMode 0
NumChildren 0
[Group]
Alignment 9
Name hidden group
Position 0 0 20 0
Size 10 0 10 0
Visible false
ChildOrder 1
ClipMode 1
NumChildren 1
[Text]
_Font fake
_Text gone
_ForeColor 1 1 1 1
_Position 0 0 0 0
Name hidden text
Position 0 0 0 0
Size 0 0 0 0
Visible true
ChildOrder 0
ClipMode 0
NumChildren 0
"""


def self_test():
    """The three rules, from synthetic data, with no game install.

    * a `TextBox` keeps its authored position (`Alignment 18` does not move it);
    * its `Text` child's `_Position.y` is a baseline (the frame is shifted up by
      the face's ascent so `frame.y + ascent` is the authored y);
    * a hidden group takes its subtree with it.
    """
    gib = parse_gib(SELF_TEST_GIB)
    roots = [
        Layout(_FakeFonts()).place(block, Frame(0.0, 0.0, 640.0, 480.0), None)
        for block in gib.blocks
        if isinstance(block, Widget)
    ]
    box = roots[0].children[0]
    assert box.widget.kind == "TextBox", box.widget.kind
    assert box.frame.as_tuple() == (10.0, 5.0, 40.0, 12.0), box.frame.as_tuple()
    text = box.children[0]
    assert text.widget.kind == "Text", text.widget.kind
    assert text.frame.y == 3.0, text.frame.y
    assert text.frame.y + _FakeFace.ascent == 13.0, "the authored baseline"
    assert text.frame.x == 13.0, text.frame.x
    names = [p.widget.name() for p, _parent in draw_order(roots)]
    assert "title" in names, names
    assert "hidden group" not in names and "hidden text" not in names, names
    # A `Text` whose parent is neither a `TextBox` nor a `Group` keeps its
    # authored y as the top of its line box.
    lone = Widget("Text", [Field("_Font", "fake"), Field("_Text", "x"),
                           Field("_Position", "0 0 8 0"), Field("Position", "0 0 8 0"),
                           Field("Size", "0 0 0 0"), Field("Visible", "true"),
                           Field("NumChildren", "0")])
    placed = Layout(_FakeFonts()).place(lone, Frame(0.0, 0.0, 100.0, 100.0), None)
    assert placed.frame.y == 8.0, placed.frame.y
    print("ui_compose: self-test ok (TextBox position, baseline, hidden subtree)")
    return 0


# --------------------------------------------------------------------------
# Entry point
# --------------------------------------------------------------------------


def default_game():
    if os.environ.get("RA2_GAME"):
        return os.environ["RA2_GAME"]
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.dirname(here)
    candidate = os.path.join(root, "local", "game")
    if os.path.isdir(candidate):
        return candidate
    # The main worktree holds the ignored local/ directory even when this runs
    # from a linked worktree.
    try:
        common = subprocess.run(
            ["git", "-C", root, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        candidate = os.path.join(os.path.dirname(common), "local", "game")
        if os.path.isdir(candidate):
            return candidate
    except (OSError, subprocess.CalledProcessError):
        pass
    return os.path.join(root, "local", "game")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("screen", nargs="?")
    ap.add_argument("out", nargs="?")
    ap.add_argument("--width", type=int, default=1280)
    ap.add_argument("--height", type=int, default=1005)
    ap.add_argument("--game", default=None, help="the game root (default: guessed)")
    ap.add_argument("--dump", default=None, help="a saved `uidump --rects` file")
    ap.add_argument(
        "--no-proxies",
        action="store_true",
        help="leave `Proxy` widgets unloaded (the data path loads them by default)",
    )
    ap.add_argument(
        "--self-test",
        action="store_true",
        help="check the TextBox/baseline/hidden rules against synthetic data and exit",
    )
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.screen or not args.out:
        ap.error("screen and out are required unless --self-test")

    game = args.game or default_game()
    screen = args.screen
    if not os.path.exists(screen) and os.path.exists(os.path.join(game, screen)):
        screen = os.path.join(game, screen)
    tool = ra2_tool()
    cfz = os.path.join(game, "RA2.cfz")
    fonts = Fonts(tool, cfz)
    images = Images(game, ra2=tool)

    if args.dump:
        lines = [ln for ln in open(args.dump).read().splitlines() if ln.strip()]
        canvas, drawn, labels, tiles = composite_rects(
            lines, images, args.width, args.height, fonts
        )
        source = f"dump {args.dump}"
    else:
        text = open(screen, encoding="utf-8", errors="replace").read()
        gib = parse_gib(text)
        loaded = []
        if not args.no_proxies:
            loaded = inline_proxies(gib, os.path.join(game, "UI"))
        roots = [Layout(fonts).place(b, Frame(0.0, 0.0, args.width, args.height), None)
                 for b in gib.blocks if isinstance(b, Widget)]
        canvas, drawn, labels, tiles = composite_scene(
            roots, images, fonts, args.width, args.height
        )
        source = f"data {os.path.basename(screen)}"
        if loaded:
            print(f"  {len(loaded)} proxies loaded: {', '.join(loaded)}")

    write_png(args.out, args.width, args.height, canvas)
    print(
        f"wrote {args.out} ({args.width}x{args.height}) from {source}, "
        f"{drawn} drawables, {labels} labels"
    )
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
