#!/usr/bin/env python3
"""Convert an XWD dump (X Window Dump) to a PNG, standard library only.

    tools/capture/xwd2png.py capture.xwd out.png
    tools/capture/xwd2png.py capture.xwd out.png --pixel 640,512 --pixel 10,10

XWD's file is a 100-byte big-endian ``XWDFileHeader`` followed by the window
name, the colour map, and the pixels.  The header's ``header_size`` includes the
name, so the pixel offset is ``header_size + ncolors * 12``.  Each pixel is
``bits_per_pixel`` wide and rows are padded to ``bytes_per_line``; the channel
positions are the header's red/green/blue masks, not an assumed byte order.
That is what makes this work for the 32-bpp BGRX dumps ``xwd`` writes here and
for a 24-bpp dump on a big-endian machine alike.

Only ``ZPixmap`` TrueColor (and DirectColor) dumps are accepted.  PseudoColor
dumps, which need a palette lookup, are rejected with a clear message rather
than written wrong.

The PNG writer is the project's standard-library one (8-bit RGB, filter 0,
zlib), the same shape as ``tools/oracle/compare.py``.  No Pillow, no numpy.
"""

import argparse
import struct
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
XWD_HEADER = struct.Struct(">25I")
HEADER_FIELDS = (
    "header_size",
    "file_version",
    "pixmap_format",
    "pixmap_depth",
    "pixmap_width",
    "pixmap_height",
    "xoffset",
    "byte_order",
    "bitmap_unit",
    "bitmap_bit_order",
    "bitmap_pad",
    "bits_per_pixel",
    "bytes_per_line",
    "visual_class",
    "red_mask",
    "green_mask",
    "blue_mask",
    "bits_per_rgb",
    "colormap_entries",
    "ncolors",
    "window_width",
    "window_height",
    "window_x",
    "window_y",
    "window_bdrwidth",
)
ZPIXMAP = 2
TRUE_COLOR = 4
DIRECT_COLOR = 5


def write_png(path, width, height, rgb):
    """Write an 8-bit RGB PNG with filter-0 rows."""
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


def channel(value, mask):
    """Extract one channel from a pixel word, scaled to 0..255."""
    if mask == 0:
        return 0
    shift = (mask & -mask).bit_length() - 1
    field = (value & mask) >> shift
    top = mask >> shift
    if top == 255:
        return field
    return (field * 255 + top // 2) // top


def parse_xwd(data):
    """(width, height, rgb bytearray, header) for a TrueColor ZPixmap dump."""
    if len(data) < XWD_HEADER.size:
        raise ValueError("shorter than an XWD header")
    values = XWD_HEADER.unpack_from(data, 0)
    h = dict(zip(HEADER_FIELDS, values))
    if h["file_version"] != 7:
        raise ValueError(f"XWD version {h['file_version']}, expected 7")
    if h["pixmap_format"] != ZPIXMAP:
        raise ValueError(
            f"pixmap format {h['pixmap_format']}, expected ZPixmap (2)"
        )
    if h["visual_class"] not in (TRUE_COLOR, DIRECT_COLOR):
        raise ValueError(
            f"visual class {h['visual_class']}, expected TrueColor (4); "
            "a PseudoColor dump needs a palette and is not supported"
        )
    width, height = h["pixmap_width"], h["pixmap_height"]
    bpp = h["bits_per_pixel"]
    if bpp < 8 or bpp % 8:
        raise ValueError(f"unsupported bits_per_pixel {bpp}")
    if width <= 0 or height <= 0:
        raise ValueError(f"empty dump {width}x{height}")
    pixel_bytes = bpp // 8
    stride = h["bytes_per_line"] or width * pixel_bytes
    if stride < width * pixel_bytes:
        raise ValueError(
            f"bytes_per_line {stride} is shorter than {width} x {pixel_bytes}"
        )
    offset = h["header_size"] + h["ncolors"] * 12
    need = offset + stride * height
    if len(data) < need:
        raise ValueError(
            f"truncated dump: {len(data)} bytes, need {need} "
            f"(header {h['header_size']}, {h['ncolors']} colours)"
        )
    big_endian = h["byte_order"] == 1
    order = "big" if big_endian else "little"
    rmask, gmask, bmask = h["red_mask"], h["green_mask"], h["blue_mask"]
    if not (rmask or gmask or bmask):
        raise ValueError("the dump has no red/green/blue masks")

    rgb = bytearray(width * height * 3)
    out = 0
    for y in range(height):
        row = offset + y * stride
        for x in range(width):
            at = row + x * pixel_bytes
            value = int.from_bytes(data[at : at + pixel_bytes], order)
            rgb[out] = channel(value, rmask)
            rgb[out + 1] = channel(value, gmask)
            rgb[out + 2] = channel(value, bmask)
            out += 3
    return width, height, rgb, h


def main(argv):
    parser = argparse.ArgumentParser(
        description="Convert a TrueColor XWD dump to an 8-bit RGB PNG."
    )
    parser.add_argument("xwd", help="the input .xwd dump")
    parser.add_argument("png", help="the output .png")
    parser.add_argument(
        "--pixel",
        action="append",
        default=[],
        metavar="X,Y",
        help="print the RGB value at this pixel (repeatable)",
    )
    parser.add_argument(
        "--quiet", action="store_true", help="only print the output path"
    )
    args = parser.parse_args(argv)

    with open(args.xwd, "rb") as f:
        data = f.read()
    width, height, rgb, h = parse_xwd(data)

    points = []
    for spec in args.pixel:
        try:
            x, y = (int(part) for part in spec.split(","))
        except ValueError:
            parser.error(f"--pixel wants X,Y, got {spec!r}")
        if not (0 <= x < width and 0 <= y < height):
            parser.error(f"pixel {spec} is outside {width}x{height}")
        points.append((x, y))

    write_png(args.png, width, height, rgb)
    if args.quiet:
        print(args.png)
        return 0
    print(
        f"{args.xwd}: {width}x{height}, depth {h['pixmap_depth']}, "
        f"{h['bits_per_pixel']} bpp, {h['bytes_per_line']} bytes/line, "
        f"masks {h['red_mask']:06x}/{h['green_mask']:06x}/{h['blue_mask']:06x}, "
        f"{'MSB' if h['byte_order'] else 'LSB'} first"
    )
    for x, y in points:
        at = (y * width + x) * 3
        print(f"  pixel ({x},{y}) = rgb({rgb[at]},{rgb[at + 1]},{rgb[at + 2]})")
    print(f"wrote {args.png} ({width}x{height})")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, ValueError) as error:
        print(f"xwd2png: {error}", file=sys.stderr)
        sys.exit(2)
