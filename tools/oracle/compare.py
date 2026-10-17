#!/usr/bin/env python3
"""Measure one of our renders against one original frame, and draw the difference.

    tools/oracle/compare.py <ours.png> <reference.png> <diff.png>
                           [--threshold N] [--check] [--delta N]
                           [--pointer-scan auto|off] [--pointer-rect x,y,w,h]

The numbers are the shell's existing standard (`docs/03-engine-design.md` M5,
`docs/12-oracle.md` §6): the mean absolute difference per RGB channel, in units
of 0..255.  This script reports the per-channel means, their overall mean, the
largest single channel difference, and the fraction of *pixels* whose largest
channel difference is above `--delta` (default 8).

The reference is a root-window capture, so it carries things our renderer never
draws.  Two are handled explicitly and reported, never silently cropped:

* the OS pointer.  Auto-scanned by `detect_pointer` (a black left edge beside a
  bright, widening arrow interior); the statistics are reported both over the
  whole frame and with that rectangle excluded.  `--pointer-rect` overrides the
  scan, `--pointer-scan off` disables it.
* a window title bar or black letterbox border.  Detected per edge and reported
  with its rectangle; the statistics with those bands excluded are reported too
  when they are found.  No crop is ever applied to the written image.

The diff image is a 2x2 sheet: reference (top left), ours (top right), the
absolute difference amplified 8x (bottom left), and a 16x16 block heat map
(bottom right, amber value = block mean x 16, clamped).  The pointer rectangle
is outlined in magenta on the reference panel.

Only the Python standard library is used: the PNG reader/writer below handles
8-bit non-interlaced grey, RGB, palette, grey+alpha and RGBA PNGs, which is every
PNG this project and the capture pipeline produce.  No PIL, no numpy.

Exit codes: 0 success (and, with `--check`, below threshold), 1 `--check` above
threshold, 2 a bad pair (usually different dimensions) or an unreadable file.
"""

import argparse
import json
import os
import struct
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
PNG_CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
BLOCK = 16
AMPLIFY = 8
HEAT_SCALE = 16
LABEL_HEIGHT = 16
GAP = 4


# --------------------------------------------------------------------------
# PNG, standard library only
# --------------------------------------------------------------------------

def read_png(path):
    """(width, height, rgb bytearray) for an 8-bit non-interlaced PNG."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != PNG_SIGNATURE:
        raise ValueError(f"{path}: not a PNG")
    at = 8
    width = height = depth = ctype = interlace = None
    palette = None
    idat = []
    while at + 8 <= len(data):
        length = struct.unpack_from(">I", data, at)[0]
        tag = data[at + 4 : at + 8]
        body = data[at + 8 : at + 8 + length]
        at += 12 + length
        if tag == b"IHDR":
            width, height, depth, ctype, _comp, _filt, interlace = struct.unpack(
                ">IIBBBBB", body
            )
        elif tag == b"PLTE":
            palette = body
        elif tag == b"IDAT":
            idat.append(body)
        elif tag == b"IEND":
            break
    if width is None:
        raise ValueError(f"{path}: no IHDR")
    if depth != 8:
        raise ValueError(f"{path}: {depth}-bit PNG is not supported (need 8-bit)")
    if interlace:
        raise ValueError(f"{path}: interlaced PNG is not supported")
    if ctype not in PNG_CHANNELS:
        raise ValueError(f"{path}: PNG colour type {ctype} is not supported")
    if ctype == 3 and not palette:
        raise ValueError(f"{path}: palette PNG without PLTE")

    channels = PNG_CHANNELS[ctype]
    stride = width * channels
    raw = zlib.decompress(b"".join(idat))
    if len(raw) < (stride + 1) * height:
        raise ValueError(f"{path}: truncated image data")

    rows = []
    previous = bytearray(stride)
    at = 0
    for _y in range(height):
        kind = raw[at]
        at += 1
        line = bytearray(raw[at : at + stride])
        at += stride
        if kind == 1:  # Sub
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif kind == 2:  # Up
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif kind == 3:  # Average
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif kind == 4:  # Paeth
            for i in range(stride):
                if i >= channels:
                    a = line[i - channels]
                    b = previous[i]
                    c = previous[i - channels]
                else:
                    a = 0
                    b = previous[i]
                    c = 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
        elif kind != 0:
            raise ValueError(f"{path}: unknown row filter {kind}")
        rows.append(line)
        previous = line

    rgb = bytearray(width * height * 3)
    o = 0
    for line in rows:
        if ctype == 2:
            rgb[o : o + stride] = line
            o += stride
        elif ctype == 6:
            for x in range(width):
                q = x * 4
                rgb[o : o + 3] = line[q : q + 3]
                o += 3
        elif ctype == 0:
            for x in range(width):
                v = line[x]
                rgb[o] = rgb[o + 1] = rgb[o + 2] = v
                o += 3
        elif ctype == 4:
            for x in range(width):
                v = line[x * 2]
                rgb[o] = rgb[o + 1] = rgb[o + 2] = v
                o += 3
        else:  # palette
            for x in range(width):
                p = line[x] * 3
                rgb[o : o + 3] = palette[p : p + 3]
                o += 3
    return width, height, rgb


def write_png(path, width, height, rgb):
    """Write an RGB8 PNG with filter 0 rows."""
    raw = bytearray()
    stride = width * 3
    for y in range(height):
        raw.append(0)
        raw.extend(rgb[y * stride : (y + 1) * stride])

    def chunk(tag, body):
        return (
            struct.pack(">I", len(body))
            + tag
            + body
            + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(
            PNG_SIGNATURE
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
            + chunk(b"IEND", b"")
        )


# --------------------------------------------------------------------------
# Statistics
# --------------------------------------------------------------------------

def luma_plane(rgb, width, height):
    plane = bytearray(width * height)
    for i in range(width * height):
        j = i * 3
        plane[i] = (299 * rgb[j] + 587 * rgb[j + 1] + 114 * rgb[j + 2]) // 1000
    return plane


def inside(x, y, rect):
    rx, ry, rw, rh = rect
    return rx <= x < rx + rw and ry <= y < ry + rh


def stats(a, b, width, height, delta, exclude=()):
    """Mean absolute per-channel difference over pixels not in `exclude`."""
    sums = [0, 0, 0]
    largest = 0
    over = 0
    count = 0
    for y in range(height):
        row = y * width
        for x in range(width):
            if exclude and any(inside(x, y, r) for r in exclude):
                continue
            i = (row + x) * 3
            dr = abs(a[i] - b[i])
            dg = abs(a[i + 1] - b[i + 1])
            db = abs(a[i + 2] - b[i + 2])
            sums[0] += dr
            sums[1] += dg
            sums[2] += db
            m = dr if dr > dg else dg
            if db > m:
                m = db
            if m > largest:
                largest = m
            if m > delta:
                over += 1
            count += 1
    if not count:
        raise ValueError("the exclusion rectangles cover the whole frame")
    return {
        "pixels": count,
        "mae": {
            "r": round(sums[0] / count, 4),
            "g": round(sums[1] / count, 4),
            "b": round(sums[2] / count, 4),
            "all": round(sum(sums) / (count * 3), 4),
        },
        "max": largest,
        "delta": delta,
        "pixels_over_delta": over,
        "fraction_over_delta": round(over / count, 6),
    }


def differences(a, b, width, height):
    """(amplified diff RGB, per-block (sum, count))."""
    diff = bytearray(width * height * 3)
    blocks_x = (width + BLOCK - 1) // BLOCK
    blocks_y = (height + BLOCK - 1) // BLOCK
    blocks = [[0, 0] for _ in range(blocks_x * blocks_y)]
    for y in range(height):
        for x in range(width):
            i = (y * width + x) * 3
            dr = abs(a[i] - b[i])
            dg = abs(a[i + 1] - b[i + 1])
            db = abs(a[i + 2] - b[i + 2])
            diff[i] = 255 if dr * AMPLIFY > 255 else dr * AMPLIFY
            diff[i + 1] = 255 if dg * AMPLIFY > 255 else dg * AMPLIFY
            diff[i + 2] = 255 if db * AMPLIFY > 255 else db * AMPLIFY
            block = blocks[(y // BLOCK) * blocks_x + (x // BLOCK)]
            block[0] += dr + dg + db
            block[1] += 3
    return diff, blocks, blocks_x, blocks_y


def heat_map(blocks, blocks_x, blocks_y):
    values = []
    for by in range(blocks_y):
        row = []
        for bx in range(blocks_x):
            total, n = blocks[by * blocks_x + bx]
            row.append(round(total / n, 3) if n else 0.0)
        values.append(row)
    return values


# --------------------------------------------------------------------------
# Reference artifacts
# --------------------------------------------------------------------------

def detect_pointer(rgb, width, height):
    """Locate the standard black-on-white OS arrow, or return None.

    The Windows/X11 arrow has a long dark left edge and a bright interior that
    widens row by row to the right, then narrows towards the tail.  Cement
    texture has dark runs too, so the shape is tested, not just the darkness:
    thin tip (first three rows <= 4 px wide), a peak of 6..13 px at least three
    rows in, at least four rows with a bright interior before the peak, and a
    tail <= 2 px.  It found the one pointer in `mainmenu__1024x768__000.png` and
    nothing else there; a themed cursor that is all one colour may be missed,
    which is why `--pointer-rect` exists.
    """
    plane = luma_plane(rgb, width, height)
    candidates = []
    for y in range(height):
        row = y * width
        for x in range(width):
            if plane[row + x] >= 140:
                continue
            if y and plane[row - width + x] < 140:
                continue  # not the top of a dark run
            n = 0
            while y + n < height and plane[(y + n) * width + x] < 140 and n < 25:
                n += 1
            if not 10 <= n <= 20:
                continue
            widths = []
            for i in range(n):
                j = x + 1
                w = 0
                base = (y + i) * width
                while j < width and plane[base + j] >= 220 and w < 20:
                    w += 1
                    j += 1
                widths.append(w)
            peak = max(widths)
            at = widths.index(peak)
            if peak < 6 or peak > 13:
                continue
            if sum(widths[:3]) / 3 > 4:
                continue
            if at < 3 or at > n - 3:
                continue
            if widths[-1] > 2:
                continue
            if sum(1 for v in widths[:at] if v >= 1) < 4:
                continue
            if sum(1 for i in range(at) if widths[i + 1] >= widths[i] - 1) < at - 1:
                continue
            candidates.append((x, y, n, peak))
    if not candidates:
        return None
    candidates.sort(key=lambda c: c[2] + c[3], reverse=True)
    x, y, n, peak = candidates[0]
    # Two rows of the same arrow are two candidates; the expansion below covers
    # the soft right/bottom edge that the dark mask does not reach.
    x0 = max(0, x - 2)
    y0 = max(0, y - 2)
    return {
        "rect": [x0, y0, min(width - x0, peak + 6), min(height - y0, n + 7)],
        "edge_dark_run": n,
        "interior_peak": peak,
        "source": "auto",
    }


def _flat_run(edge_pixels, is_flat):
    n = 0
    for pixels in edge_pixels:
        if is_flat(pixels):
            n += 1
        else:
            break
    return n


def detect_decorations(rgb, width, height):
    """Report (never remove) a title bar or a black letterbox border.

    A black border is a band touching an edge in which at least 97% of pixels
    are below luma 20.  A title bar is a top band at least 8 rows high in which
    at least 85% of each row's pixels sit within +-10 of that row's median and
    the median is not black; it ends where the picture becomes textured.  Both
    are reported per edge, and the stats with those bands excluded are printed
    beside the raw stats.
    """
    plane = luma_plane(rgb, width, height)

    def row_pixels(y):
        return plane[y * width : (y + 1) * width]

    def col_pixels(x):
        return [plane[y * width + x] for y in range(height)]

    def is_black(pixels):
        dark = sum(1 for v in pixels if v < 20)
        return dark >= len(pixels) * 0.97

    def is_title_row(pixels):
        ordered = sorted(pixels)
        median = ordered[len(ordered) // 2]
        if median < 24:
            return False
        near = sum(1 for v in ordered if abs(v - median) <= 10)
        return near >= len(pixels) * 0.85

    black = {}
    for side in ("top", "bottom"):
        rows = (
            (row_pixels(y) for y in range(height))
            if side == "top"
            else (row_pixels(y) for y in range(height - 1, -1, -1))
        )
        n = _flat_run(rows, is_black)
        if n and n < height // 3:
            black[side] = n
    for side in ("left", "right"):
        cols = (
            (col_pixels(x) for x in range(width))
            if side == "left"
            else (col_pixels(x) for x in range(width - 1, -1, -1))
        )
        n = _flat_run(cols, is_black)
        if n and n < width // 3:
            black[side] = n

    # A real title bar is a short, near-uniform band with a hard lower edge.
    # Cement/vignette tops also score high on row uniformity, so the 48-row
    # ceiling and the boundary contrast are what reject them: the main menu's
    # textured top scored 62 rows but no row boundary changes most columns.
    title = 0
    limit = min(48, height // 3)
    for y in range(limit):
        if not is_title_row(row_pixels(y)):
            break
        title = y + 1
    if title < 8:
        title = 0
    if title:
        below = min(height - 1, title)
        changed = sum(
            1
            for x in range(width)
            if abs(plane[(title - 1) * width + x] - plane[below * width + x]) > 12
        )
        if changed < width * 0.5:
            title = 0

    rects = []
    if title:
        rects.append([0, 0, width, title])
    for side, n in black.items():
        if side == "top" and title:  # the title bar wins the same pixels
            continue
        if side == "top":
            rects.append([0, 0, width, n])
        elif side == "bottom":
            rects.append([0, height - n, width, n])
        elif side == "left":
            rects.append([0, 0, n, height])
        else:
            rects.append([width - n, 0, n, height])
    return {
        "detected": bool(title or black),
        "title_bar": {"height": title, "rect": [0, 0, width, title]} if title else None,
        "black_border": black or None,
        "rects": rects,
    }


# --------------------------------------------------------------------------
# The 2x2 diff sheet (a 5x7 label font, so the image explains itself)
# --------------------------------------------------------------------------

FONT = {
    " ": "00000 00000 00000 00000 00000 00000 00000",
    "A": "01110 10001 10001 11111 10001 10001 10001",
    "B": "11110 10001 10001 11110 10001 10001 11110",
    "C": "01111 10000 10000 10000 10000 10000 01111",
    "D": "11110 10001 10001 10001 10001 10001 11110",
    "E": "11111 10000 10000 11110 10000 10000 11111",
    "F": "11111 10000 10000 11110 10000 10000 10000",
    "G": "01111 10000 10000 10111 10001 10001 01111",
    "H": "10001 10001 10001 11111 10001 10001 10001",
    "I": "11111 00100 00100 00100 00100 00100 11111",
    "J": "00111 00010 00010 00010 10010 10010 01100",
    "K": "10001 10010 10100 11000 10100 10010 10001",
    "L": "10000 10000 10000 10000 10000 10000 11111",
    "M": "10001 11011 10101 10101 10001 10001 10001",
    "N": "10001 11001 10101 10011 10001 10001 10001",
    "O": "01110 10001 10001 10001 10001 10001 01110",
    "P": "11110 10001 10001 11110 10000 10000 10000",
    "Q": "01110 10001 10001 10001 10101 10010 01101",
    "R": "11110 10001 10001 11110 10100 10010 10001",
    "S": "01111 10000 10000 01110 00001 00001 11110",
    "T": "11111 00100 00100 00100 00100 00100 00100",
    "U": "10001 10001 10001 10001 10001 10001 01110",
    "V": "10001 10001 10001 10001 10001 01010 00100",
    "W": "10001 10001 10001 10101 10101 11011 10001",
    "X": "10001 10001 01010 00100 01010 10001 10001",
    "Y": "10001 10001 01010 00100 00100 00100 00100",
    "Z": "11111 00001 00010 00100 01000 10000 11111",
    "0": "01110 10001 10011 10101 11001 10001 01110",
    "1": "00100 01100 00100 00100 00100 00100 01110",
    "2": "01110 10001 00001 00110 01000 10000 11111",
    "3": "11110 00001 00001 01110 00001 00001 11110",
    "4": "00010 00110 01010 10010 11111 00010 00010",
    "5": "11111 10000 11110 00001 00001 10001 01110",
    "6": "01110 10000 10000 11110 10001 10001 01110",
    "7": "11111 00001 00010 00100 01000 01000 01000",
    "8": "01110 10001 10001 01110 10001 10001 01110",
    "9": "01110 10001 10001 01111 00001 00001 01110",
    "x": "00000 00000 10001 01010 00100 01010 10001",
}


def draw_text(canvas, canvas_w, x, y, text, colour):
    for ch in text.upper():
        glyph = FONT.get(ch) or FONT.get(ch.lower())
        if not glyph:
            x += 6
            continue
        rows = glyph.split()
        for gy, row in enumerate(rows):
            for gx, bit in enumerate(row):
                if bit == "1":
                    px = x + gx
                    py = y + gy
                    q = (py * canvas_w + px) * 3
                    canvas[q : q + 3] = bytes(colour)
        x += 6


def draw_rect_outline(canvas, canvas_w, x, y, w, h, colour):
    for yy in range(y, y + h):
        for xx in (x, x + w - 1):
            q = (yy * canvas_w + xx) * 3
            canvas[q : q + 3] = bytes(colour)
    for xx in range(x, x + w):
        for yy in (y, y + h - 1):
            q = (yy * canvas_w + xx) * 3
            canvas[q : q + 3] = bytes(colour)


def make_sheet(width, height, reference, ours, diff, heat_values, pointer_rect, out_path):
    blocks_x = len(heat_values[0])
    blocks_y = len(heat_values)

    heat = bytearray(width * height * 3)
    for by in range(blocks_y):
        for bx in range(blocks_x):
            v = min(255, int(round(heat_values[by][bx] * HEAT_SCALE)))
            colour = bytes((v, v // 3, 0))
            x0 = bx * BLOCK
            y0 = by * BLOCK
            x1 = min(width, x0 + BLOCK)
            y1 = min(height, y0 + BLOCK)
            for y in range(y0, y1):
                q = (y * width + x0) * 3
                q1 = (y * width + x1) * 3
                heat[q:q1] = colour * (x1 - x0)
            # A one-pixel block boundary, so 16-pixel cells read as a grid.
            if bx:
                for y in range(y0, y1):
                    q = (y * width + x0) * 3
                    heat[q : q + 3] = bytes((90, 90, 90))

    if pointer_rect:
        tmp = bytearray(reference)
        draw_rect_outline(tmp, width, *pointer_rect, (255, 0, 255))
        reference = tmp

    panel_w = width
    panel_h = height + LABEL_HEIGHT
    total_w = panel_w * 2 + GAP * 3
    total_h = panel_h * 2 + GAP * 3
    out = bytearray(bytes((28, 28, 30)) * (total_w * total_h))

    def blit(src, dx, dy):
        for y in range(height):
            s = y * width * 3
            d = ((dy + y) * total_w + dx) * 3
            out[d : d + width * 3] = src[s : s + width * 3]

    panels = ((0, 0, reference, "REFERENCE"),
              (1, 0, ours, "OURS"),
              (0, 1, diff, "DIFF X8"),
              (1, 1, heat, "HEATMAP 16X16"))
    for col, row, image, label in panels:
        x = GAP + col * (panel_w + GAP)
        y = GAP + row * (panel_h + GAP)
        for yy in range(LABEL_HEIGHT):
            q = ((y + yy) * total_w + x) * 3
            out[q : q + panel_w * 3] = bytes((0, 0, 0)) * panel_w
        draw_text(out, total_w, x + 4, y + 5, label, (235, 235, 235))
        blit(image, x, y + LABEL_HEIGHT)

    write_png(out_path, total_w, total_h, out)
    return total_w, total_h


# --------------------------------------------------------------------------

def parse_rect(text):
    parts = text.replace(" ", "").split(",")
    if len(parts) != 4:
        raise argparse.ArgumentTypeError("--pointer-rect wants x,y,w,h")
    try:
        return [int(v) for v in parts]
    except ValueError:
        raise argparse.ArgumentTypeError("--pointer-rect wants four integers")


def main():
    ap = argparse.ArgumentParser(
        description="Compare our screen render with an original ROOT capture",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("ours", help="PNG rendered by us")
    ap.add_argument("reference", help="PNG captured from the original")
    ap.add_argument("diff", help="output diff-sheet PNG")
    ap.add_argument("--threshold", type=float, default=0.5,
                    help="--check ceiling for overall MAE (default 0.5/255)")
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if the overall MAE is above --threshold")
    ap.add_argument("--delta", type=int, default=8,
                    help="per-pixel max-channel delta counted as different (default 8)")
    ap.add_argument("--pointer-scan", choices=("auto", "off"), default="auto",
                    help="auto-detect the OS pointer in the reference (default auto)")
    ap.add_argument("--pointer-rect", type=parse_rect, default=None,
                    metavar="X,Y,W,H", help="mask this rectangle instead of scanning")
    args = ap.parse_args()

    try:
        ow, oh, ours = read_png(args.ours)
        rw, rh, reference = read_png(args.reference)
    except (OSError, ValueError, zlib.error) as e:
        print(f"compare: {e}", file=sys.stderr)
        return 2

    if (ow, oh) != (rw, rh):
        print(
            f"compare: dimensions differ: ours {ow}x{oh}, reference {rw}x{rh}",
            file=sys.stderr,
        )
        return 2
    width, height = ow, oh
    os.makedirs(os.path.dirname(os.path.abspath(args.diff)), exist_ok=True)

    whole = stats(ours, reference, width, height, args.delta)

    pointer = None
    if args.pointer_rect:
        rx, ry, rw_, rh_ = args.pointer_rect
        if rx < 0 or ry < 0 or rw_ <= 0 or rh_ <= 0 or rx + rw_ > width or ry + rh_ > height:
            print("compare: --pointer-rect lies outside the frame", file=sys.stderr)
            return 2
        pointer = {"rect": list(args.pointer_rect), "source": "manual"}
    elif args.pointer_scan == "auto":
        pointer = detect_pointer(reference, width, height)
    if pointer:
        pointer["masked_pixels"] = pointer["rect"][2] * pointer["rect"][3]
        pointer["stats_without"] = stats(
            ours, reference, width, height, args.delta, [pointer["rect"]]
        )

    decorations = detect_decorations(reference, width, height)
    if decorations["rects"]:
        decorations["stats_without"] = stats(
            ours, reference, width, height, args.delta, decorations["rects"]
        )

    diff, blocks, blocks_x, blocks_y = differences(ours, reference, width, height)
    values = heat_map(blocks, blocks_x, blocks_y)
    sheet_w, sheet_h = make_sheet(
        width, height, reference, ours, diff, values,
        pointer["rect"] if pointer else None, args.diff,
    )

    # `--check` uses the pointer-masked number when a pointer was found: the
    # renderer never draws the OS pointer, so including it would make every
    # capture fail for a reason that is not a fidelity defect.
    if pointer:
        checked = pointer["stats_without"]["mae"]["all"]
        metric = "mae.all.pointer_masked"
    else:
        checked = whole["mae"]["all"]
        metric = "mae.all.raw"
    passed = checked <= args.threshold

    report = {
        "ours": os.path.abspath(args.ours),
        "reference": os.path.abspath(args.reference),
        "diff": os.path.abspath(args.diff),
        "size": [width, height],
        "same_size": True,
        "mae": whole["mae"],
        "max": whole["max"],
        "delta": args.delta,
        "pixels_over_delta": whole["pixels_over_delta"],
        "fraction_over_delta": whole["fraction_over_delta"],
        "pixels": whole["pixels"],
        "stats": whole,
        "pointer": pointer,
        "decorations": decorations,
        "heatmap": {
            "block": BLOCK,
            "cols": blocks_x,
            "rows": blocks_y,
            "scale": HEAT_SCALE,
            "mean": "mean absolute RGB difference over the block, 0..255",
            "values": values,
        },
        "diff_image": {
            "width": sheet_w,
            "height": sheet_h,
            "amplify": AMPLIFY,
            "panels": ["reference", "ours", "diff_x8", "heatmap_16x16"],
        },
        "check": {
            "enabled": args.check,
            "threshold": args.threshold,
            "metric": metric,
            "value": checked,
            "passed": passed,
            "reason": (
                f"{metric} {checked:.4f} <= threshold {args.threshold:.4f}"
                if passed
                else f"{metric} {checked:.4f} > threshold {args.threshold:.4f}"
            ),
        },
    }
    print(json.dumps(report, separators=(",", ":")))

    if args.check:
        state = "PASS" if passed else "FAIL"
        print(f"compare: {state}: {report['check']['reason']}", file=sys.stderr)
        if not passed:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
