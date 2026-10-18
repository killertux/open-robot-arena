#!/usr/bin/env python3
"""A virtual evdev mouse for the original-game capture, through /dev/uinput.

The original *Robot Arena 2* reads its mouse through DirectInput8.  Wine 9.0
implements that on top of the hid/evdev stack, so **X input does not reach it at
all**: XTEST motion, XWarpPointer, XSendEvent and even ``SetCursorPos`` from a
Wine process leave the game's cursor where it started, while keyboard events
still dismiss its dialogs (those go through the normal Win32 message path).

What Wine does see is a real Linux input device.  ``/dev/uinput`` is writable by
this user (an ACL grants ``bruno:rw-``), so this tool creates a relative mouse
with ``REL_X``/``REL_Y`` and ``BTN_LEFT`` and writes evdev events.  The device
must exist **before** Wine starts, because DirectInput enumerates devices at
startup; ``daemon`` mode therefore creates it once and serves commands from a
FIFO until ``quit``.

    # start it before the game; ready when the FIFO answers "ready"
    tools/capture/uinput_mouse.py daemon --fifo /tmp/ra2-mouse.fifo
    echo "move -4000 -4000" > /tmp/ra2-mouse.fifo   # clamp to the top-left
    echo "move 514 346" > /tmp/ra2-mouse.fifo       # then the target
    echo "click 1" > /tmp/ra2-mouse.fifo
    echo quit > /tmp/ra2-mouse.fifo

Commands, one per line:

    move DX DY       relative motion, in mouse counts (1 count = 1 pixel here)
    click [BUTTON]   button 1 (left), 2 (middle), 3 (right); press+release
    down [BUTTON] / up [BUTTON]
    wheel N          vertical wheel clicks
    quit             destroy the device and exit

``--node`` prints the created ``/dev/input/eventN``.  The tool never writes to
an existing device and never needs root.  No game file is touched.
"""

import argparse
import fcntl
import os
import select
import struct
import sys
import time

UINPUT_IOCTL_BASE = ord("U")
UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_SET_RELBIT = 0x40045566
UI_DEV_SETUP = 0x405C5503
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502

EV_SYN = 0x00
EV_KEY = 0x01
EV_REL = 0x02
SYN_REPORT = 0
REL_X = 0x00
REL_Y = 0x01
REL_WHEEL = 0x08
BTN_LEFT = 0x110
BTN_RIGHT = 0x111
BTN_MIDDLE = 0x112

BUTTONS = {1: BTN_LEFT, 2: BTN_MIDDLE, 3: BTN_RIGHT}


class VirtualMouse:
    def __init__(self, name="ra2-capture-mouse"):
        try:
            self.fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
        except OSError as error:
            raise RuntimeError(f"cannot open /dev/uinput: {error}") from error
        for event, code in ((EV_SYN, 0), (EV_KEY, 0), (EV_REL, 0)):
            fcntl.ioctl(self.fd, UI_SET_EVBIT, event)
        for button in BUTTONS.values():
            fcntl.ioctl(self.fd, UI_SET_KEYBIT, button)
        for axis in (REL_X, REL_Y, REL_WHEEL):
            fcntl.ioctl(self.fd, UI_SET_RELBIT, axis)
        # struct uinput_setup: input_id (4 x u16), name[80], ff_effects_max
        setup = struct.pack("<HHHH80sI", 0x03, 0x1D6B, 0x0001, 1, name.encode(), 0)
        fcntl.ioctl(self.fd, UI_DEV_SETUP, setup)
        fcntl.ioctl(self.fd, UI_DEV_CREATE)
        time.sleep(0.4)

    def event(self, kind, code, value):
        os.write(self.fd, struct.pack("llHHi", 0, 0, kind, code, value))

    def sync(self):
        self.event(EV_SYN, SYN_REPORT, 0)

    def move(self, dx, dy):
        if dx:
            self.event(EV_REL, REL_X, int(dx))
        if dy:
            self.event(EV_REL, REL_Y, int(dy))
        self.sync()

    def button(self, button, pressed):
        self.event(EV_KEY, BUTTONS.get(button, BTN_LEFT), 1 if pressed else 0)
        self.sync()

    def click(self, button=1, hold=0.08):
        self.button(button, True)
        time.sleep(hold)
        self.button(button, False)
        time.sleep(0.05)

    def wheel(self, clicks):
        self.event(EV_REL, REL_WHEEL, int(clicks))
        self.sync()

    def close(self):
        try:
            fcntl.ioctl(self.fd, UI_DEV_DESTROY)
        finally:
            os.close(self.fd)


def node_for(fd):
    """The /dev/input/eventN the kernel gave the new device (best effort)."""
    try:
        name = fcntl.ioctl(fd, 0x80085502, b"\0" * 256)  # UI_GET_SYSNAME(256)
        sysname = name.split(b"\0", 1)[0].decode()
        return f"/dev/input/{sysname}"
    except OSError:
        return None


def daemon(fifo, name):
    if os.path.exists(fifo):
        os.unlink(fifo)
    os.mkfifo(fifo)
    mouse = VirtualMouse(name)
    node = node_for(mouse.fd)
    print(f"ready {node or '(uinput device created)'} fifo {fifo}", flush=True)
    fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
    pending = b""
    try:
        while True:
            ready, _, _ = select.select([fd], [], [], 1.0)
            if not ready:
                continue
            chunk = os.read(fd, 4096)
            if not chunk:
                continue
            pending += chunk
            while b"\n" in pending:
                line, pending = pending.split(b"\n", 1)
                words = line.decode(errors="replace").split()
                if not words:
                    continue
                command = words[0]
                print(f"command {line.decode(errors='replace')}", flush=True)
                if command == "quit":
                    return 0
                if command == "move" and len(words) >= 3:
                    mouse.move(int(words[1]), int(words[2]))
                elif command == "click":
                    mouse.click(int(words[1]) if len(words) > 1 else 1)
                elif command == "down":
                    mouse.button(int(words[1]) if len(words) > 1 else 1, True)
                elif command == "up":
                    mouse.button(int(words[1]) if len(words) > 1 else 1, False)
                elif command == "wheel" and len(words) >= 2:
                    mouse.wheel(int(words[1]))
                else:
                    print(f"unknown command: {line.decode(errors='replace')!r}", flush=True)
    finally:
        os.close(fd)
        mouse.close()
        try:
            os.unlink(fifo)
        except OSError:
            pass
    return 0


def main(argv):
    parser = argparse.ArgumentParser(
        description="A virtual evdev mouse for Wine DirectInput, via /dev/uinput."
    )
    parser.add_argument("mode", choices=("daemon", "once"))
    parser.add_argument("--fifo", default="/tmp/ra2-mouse.fifo")
    parser.add_argument("--name", default="ra2-capture-mouse")
    parser.add_argument(
        "--commands",
        default="",
        help="once mode: semicolon-separated commands (e.g. 'move 10 10;click 1')",
    )
    args = parser.parse_args(argv)
    if args.mode == "daemon":
        return daemon(args.fifo, args.name)
    mouse = VirtualMouse(args.name)
    try:
        for chunk in args.commands.split(";"):
            words = chunk.split()
            if not words:
                continue
            if words[0] == "move":
                mouse.move(int(words[1]), int(words[2]))
            elif words[0] == "click":
                mouse.click(int(words[1]) if len(words) > 1 else 1)
            elif words[0] == "wheel":
                mouse.wheel(int(words[1]))
        time.sleep(0.2)
    finally:
        mouse.close()
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, RuntimeError, ValueError) as error:
        print(f"uinput_mouse: {error}", file=sys.stderr)
        sys.exit(2)
