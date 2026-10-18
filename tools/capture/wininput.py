#!/usr/bin/env python3
"""Build and run a minimal Windows PE that injects one mouse click under Wine.

The original *Robot Arena 2* reads its mouse through DirectInput.  On this
machine Wine's DirectInput path does not see XTEST's synthetic pointer events:
keyboard events reach the game (they dismiss its startup dialogs) but the game's
own cursor never moves and no button activates.  `XSendEvent` and
`XWarpPointer` do not help either.  The remaining path is Wine's own input
queue, i.e. a Windows process calling ``SetCursorPos`` + ``mouse_event``.

There is no mingw or clang cross compiler here, and installing one is out of
scope, so this script emits the PE32 itself: DOS + PE headers, one executable
section, an import table for ``kernel32!Sleep``/``ExitProcess`` and
``user32!SetCursorPos``/``mouse_event``.  The code is a fixed sequence of
stdcall calls with the coordinates and button flags patched in.  Relocations
are stripped and DYNAMIC_BASE is clear, so Wine loads the image at 0x400000 and
the absolute IAT addresses are valid.

    tools/capture/wininput.py build --x 514 --y 424 --out click.exe
    tools/capture/wininput.py run --x 514 --y 424
    tools/capture/wininput.py run --x 514 --y 424 --button 2 --hold 0.2

``run`` builds the PE in the Wine prefix's ``windows\\temp`` (game data lives
outside the repository; nothing is committed) and launches it with the installed
flatpak Wine.  The display is taken from the environment exactly as Wine takes
it.

This tool is for the original-game capture only.  Our own client is driven with
``xtest.py``; a capture is never claimed because an injected event was merely
sent -- each step is followed by a screenshot.
"""

import argparse
import os
import struct
import subprocess
import sys
import tempfile

TEXT_RVA = 0x1000
CODE_RVA = 0x1000
IDESC_RVA = 0x1100
K_INT_RVA = 0x1140
K_IAT_RVA = 0x1150
U_INT_RVA = 0x1160
U_IAT_RVA = 0x1180
NAMES_RVA = 0x1200
K_NAME_RVA = 0x1280
U_NAME_RVA = 0x1290
TITLE_RVA = 0x12A0
SLEEP_RVA = 0x1200
EXIT_RVA = 0x120A
SETCURSOR_RVA = 0x1218
MOUSE_RVA = 0x1228
FINDWINDOW_RVA = 0x1236
SETFOREGROUND_RVA = 0x1244
IMAGE_BASE = 0x400000
FILE_ALIGN = 0x200
SECTION_ALIGN = 0x1000


def build_pe(x, y, button=1, hold=0.1, foreground=False):
    """Return the bytes of a PE32 that clicks at x,y and exits."""
    down, up = {
        1: (0x0002, 0x0004),
        2: (0x0008, 0x0010),
        3: (0x0020, 0x0040),
    }[button]
    hold_ms = max(1, int(hold * 1000))

    code = bytearray()

    def push(value):
        code.extend(b"\x68" + struct.pack("<I", value))

    def call(iat_rva):
        code.extend(b"\xff\x15" + struct.pack("<I", IMAGE_BASE + iat_rva))

    if foreground:
        push(IMAGE_BASE + TITLE_RVA)  # lpWindowName
        push(0)  # lpClassName
        call(U_IAT_RVA + 8)  # FindWindowA -> eax
        code.extend(b"\x50")  # push eax: the hwnd argument
        call(U_IAT_RVA + 12)  # SetForegroundWindow
        push(200)
        call(K_IAT_RVA)  # Sleep, let Wine settle the foreground change

    push(y)
    push(x)
    call(U_IAT_RVA)
    for _ in range(4):  # dx, dy, dwData, dwExtraInfo
        code.append(0x6A)
        code.append(0)
    push(down)
    call(U_IAT_RVA + 4)
    push(hold_ms)
    call(K_IAT_RVA)
    for _ in range(4):
        code.append(0x6A)
        code.append(0)
    push(up)
    call(U_IAT_RVA + 4)
    code.append(0x6A)
    code.append(0)
    call(K_IAT_RVA + 4)
    code.extend(b"\xCC")

    section = bytearray(SECTION_ALIGN)
    section[CODE_RVA - TEXT_RVA : CODE_RVA - TEXT_RVA + len(code)] = code

    def put(rva, data):
        at = rva - TEXT_RVA
        section[at : at + len(data)] = data

    # Import descriptors: kernel32 then user32, null terminated.
    put(
        IDESC_RVA,
        struct.pack(
            "<IIIII",
            K_INT_RVA,
            0,
            0,
            K_NAME_RVA,
            K_IAT_RVA,
        )
        + struct.pack(
            "<IIIII",
            U_INT_RVA,
            0,
            0,
            U_NAME_RVA,
            U_IAT_RVA,
        )
        + b"\0" * 20,
    )
    # Thunks: Sleep, ExitProcess; SetCursorPos, mouse_event, FindWindowA,
    # SetForegroundWindow; zero terminated.
    kthunks = struct.pack("<III", SLEEP_RVA, EXIT_RVA, 0)
    uthunks = struct.pack(
        "<IIIII",
        SETCURSOR_RVA,
        MOUSE_RVA,
        FINDWINDOW_RVA,
        SETFOREGROUND_RVA,
        0,
    )
    put(K_INT_RVA, kthunks)
    put(K_IAT_RVA, kthunks)
    put(U_INT_RVA, uthunks)
    put(U_IAT_RVA, uthunks)

    def hint_name(name):
        return struct.pack("<H", 0) + name.encode() + b"\0"

    put(SLEEP_RVA, hint_name("Sleep"))
    put(EXIT_RVA, hint_name("ExitProcess"))
    put(SETCURSOR_RVA, hint_name("SetCursorPos"))
    put(MOUSE_RVA, hint_name("mouse_event"))
    put(FINDWINDOW_RVA, hint_name("FindWindowA"))
    put(SETFOREGROUND_RVA, hint_name("SetForegroundWindow"))
    put(K_NAME_RVA, b"kernel32.dll\0")
    put(U_NAME_RVA, b"user32.dll\0")
    put(TITLE_RVA, b"Robot Arena 2\0")

    dos = bytearray(0x40)
    dos[0:2] = b"MZ"
    dos[0x3C:0x40] = struct.pack("<I", 0x40)

    coff = struct.pack(
        "<HHIIIHH",
        0x014C,  # i386
        1,  # sections
        0,
        0,
        0,
        0xE0,  # SizeOfOptionalHeader
        0x0103,  # executable | 32-bit | relocations stripped
    )

    opt = bytearray(0xE0)
    struct.pack_into("<H", opt, 0x00, 0x10B)  # PE32
    struct.pack_into("<I", opt, 0x04, SECTION_ALIGN)  # SizeOfCode
    struct.pack_into("<I", opt, 0x10, CODE_RVA)  # AddressOfEntryPoint
    struct.pack_into("<I", opt, 0x14, CODE_RVA)  # BaseOfCode
    struct.pack_into("<I", opt, 0x18, 0x2000)  # BaseOfData
    struct.pack_into("<I", opt, 0x1C, IMAGE_BASE)
    struct.pack_into("<I", opt, 0x20, SECTION_ALIGN)
    struct.pack_into("<I", opt, 0x24, FILE_ALIGN)
    struct.pack_into("<H", opt, 0x28, 4)  # MajorOSVersion
    struct.pack_into("<H", opt, 0x30, 4)  # MajorSubsystemVersion
    struct.pack_into("<I", opt, 0x38, 0x2000)  # SizeOfImage
    struct.pack_into("<I", opt, 0x3C, 0x200)  # SizeOfHeaders
    struct.pack_into("<H", opt, 0x44, 2)  # GUI subsystem, no console window
    struct.pack_into("<I", opt, 0x48, 0x100000)  # stack reserve
    struct.pack_into("<I", opt, 0x4C, 0x1000)  # stack commit
    struct.pack_into("<I", opt, 0x50, 0x100000)  # heap reserve
    struct.pack_into("<I", opt, 0x54, 0x1000)  # heap commit
    struct.pack_into("<I", opt, 0x5C, 16)  # NumberOfRvaAndSizes
    # Data directory 1 is the import table.
    struct.pack_into("<II", opt, 0x60 + 1 * 8, IDESC_RVA, 60)

    section_header = struct.pack(
        "<8sIIIIIIHHI",
        b".text\0\0\0",
        SECTION_ALIGN,
        TEXT_RVA,
        SECTION_ALIGN,
        0x200,
        0,
        0,
        0,
        0,
        0xE0000020,
    )

    headers = bytes(dos) + b"PE\0\0" + coff + bytes(opt) + section_header
    headers = headers.ljust(FILE_ALIGN, b"\0")
    return headers + bytes(section)


def wine_prefix():
    return os.path.expanduser(
        "~/.var/app/org.winehq.Wine/data/wine"
    )


def main(argv):
    parser = argparse.ArgumentParser(
        description="Inject one mouse click into Wine with a hand-built PE."
    )
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("build", "run"):
        p = sub.add_parser(command)
        p.add_argument("--x", type=int, required=True)
        p.add_argument("--y", type=int, required=True)
        p.add_argument("--button", type=int, default=1, choices=(1, 2, 3))
        p.add_argument("--hold", type=float, default=0.1)
        p.add_argument(
            "--foreground",
            action="store_true",
            help="call FindWindowA + SetForegroundWindow before injecting",
        )
        p.add_argument("--out", help="the PE path (build; run defaults to the prefix)")
    args = parser.parse_args(argv)

    if args.command == "build":
        out = args.out or os.path.join(
            tempfile.gettempdir(), "ra2-click.exe"
        )
        with open(out, "wb") as f:
            f.write(
                build_pe(args.x, args.y, args.button, args.hold, args.foreground)
            )
        print(out)
        return 0

    prefix = wine_prefix()
    folder = os.path.join(prefix, "drive_c", "windows", "temp")
    os.makedirs(folder, exist_ok=True)
    out = os.path.join(folder, "ra2-click.exe")
    with open(out, "wb") as f:
        f.write(build_pe(args.x, args.y, args.button, args.hold, args.foreground))
    windows_path = r"C:\windows\temp\ra2-click.exe"
    print(f"{out} -> {windows_path}", file=sys.stderr)
    result = subprocess.run(
        [
            "flatpak",
            "run",
            "--command=wine",
            "org.winehq.Wine",
            windows_path,
        ],
        check=False,
    )
    return result.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
