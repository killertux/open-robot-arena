#!/usr/bin/env python3
"""Synthetic X11 input with XTEST, through ctypes and the system libX11/libXtst.

No ``xdotool`` and no Python Xlib are needed -- only the two shared libraries
every X server client already has.  This is the input half of the capture
harness: it finds a window by title, focuses and raises it, moves the pointer,
clicks, and sends keys, all on the display the caller names.

    tools/capture/xtest.py --display :0 windows
    tools/capture/xtest.py --display :0 find --title "Open Robot Arena"
    tools/capture/xtest.py --display :0 focus --title "Open Robot Arena"
    tools/capture/xtest.py --display :0 click 640 476
    tools/capture/xtest.py --display :2 click 640 476 --button 1 --hold 0.10
    tools/capture/xtest.py --display :2 key Return
    tools/capture/xtest.py --display :2 key w down
    tools/capture/xtest.py --display :2 key w up

Coordinates are **root-window** coordinates, the same space ``xwd -root`` and
``XTestFakeMotionEvent`` use.  ``find`` prints the window's absolute origin and
size, which is what a caller adds to a window-relative point.  ``click`` always
sends motion, a short settle, button press, a hold, and release, because a press
without a preceding motion never reaches a client that tracks the pointer from
``CursorMoved`` (our client does; see ``ra2-app/src/window.rs``).

Exit status is 0 on success, 2 for a missing display/library/window.
"""

import argparse
import ctypes
import ctypes.util
import os
import sys
import time

CURRENT_TIME = 0
REVERT_TO_PARENT = 2


class Display(ctypes.Structure):
    pass


class XButtonEvent(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int),
        ("serial", ctypes.c_ulong),
        ("send_event", ctypes.c_int),
        ("display", ctypes.c_void_p),
        ("window", ctypes.c_ulong),
        ("root", ctypes.c_ulong),
        ("subwindow", ctypes.c_ulong),
        ("time", ctypes.c_ulong),
        ("x", ctypes.c_int),
        ("y", ctypes.c_int),
        ("x_root", ctypes.c_int),
        ("y_root", ctypes.c_int),
        ("state", ctypes.c_uint),
        ("button", ctypes.c_uint),
        ("same_screen", ctypes.c_int),
    ]


class XMotionEvent(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int),
        ("serial", ctypes.c_ulong),
        ("send_event", ctypes.c_int),
        ("display", ctypes.c_void_p),
        ("window", ctypes.c_ulong),
        ("root", ctypes.c_ulong),
        ("subwindow", ctypes.c_ulong),
        ("time", ctypes.c_ulong),
        ("x", ctypes.c_int),
        ("y", ctypes.c_int),
        ("x_root", ctypes.c_int),
        ("y_root", ctypes.c_int),
        ("state", ctypes.c_uint),
        ("is_hint", ctypes.c_int),
        ("same_screen", ctypes.c_int),
    ]


class X11:
    def __init__(self):
        self.x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
        self.xtst = ctypes.CDLL(ctypes.util.find_library("Xtst") or "libXtst.so.6")
        d = ctypes.c_void_p
        u = ctypes.c_uint
        i = ctypes.c_int
        ul = ctypes.c_ulong
        self.x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
        self.x11.XOpenDisplay.restype = ctypes.POINTER(Display)
        self.x11.XCloseDisplay.argtypes = [ctypes.POINTER(Display)]
        self.x11.XDefaultScreen.argtypes = [ctypes.POINTER(Display)]
        self.x11.XDefaultScreen.restype = i
        self.x11.XRootWindow.argtypes = [ctypes.POINTER(Display), i]
        self.x11.XRootWindow.restype = ul
        self.x11.XQueryTree.argtypes = [
            ctypes.POINTER(Display),
            ul,
            ctypes.POINTER(ul),
            ctypes.POINTER(ul),
            ctypes.POINTER(ctypes.POINTER(ul)),
            ctypes.POINTER(u),
        ]
        self.x11.XQueryTree.restype = i
        self.x11.XFetchName.argtypes = [
            ctypes.POINTER(Display),
            ul,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        self.x11.XFetchName.restype = i
        self.x11.XFree.argtypes = [d]
        self.x11.XGetGeometry.argtypes = [
            ctypes.POINTER(Display),
            ul,
            ctypes.POINTER(ul),
            ctypes.POINTER(i),
            ctypes.POINTER(i),
            ctypes.POINTER(u),
            ctypes.POINTER(u),
            ctypes.POINTER(u),
            ctypes.POINTER(u),
        ]
        self.x11.XGetGeometry.restype = i
        self.x11.XTranslateCoordinates.argtypes = [
            ctypes.POINTER(Display),
            ul,
            ul,
            i,
            i,
            ctypes.POINTER(i),
            ctypes.POINTER(i),
            ctypes.POINTER(ul),
        ]
        self.x11.XTranslateCoordinates.restype = i
        self.x11.XQueryPointer.argtypes = [
            ctypes.POINTER(Display),
            ul,
            ctypes.POINTER(ul),
            ctypes.POINTER(ul),
            ctypes.POINTER(i),
            ctypes.POINTER(i),
            ctypes.POINTER(i),
            ctypes.POINTER(i),
            ctypes.POINTER(u),
        ]
        self.x11.XQueryPointer.restype = i
        self.x11.XWarpPointer.argtypes = [
            ctypes.POINTER(Display),
            ul,
            ul,
            i,
            i,
            u,
            u,
            i,
            i,
        ]
        self.x11.XSendEvent.argtypes = [
            ctypes.POINTER(Display),
            ul,
            i,
            ctypes.c_long,
            ctypes.c_void_p,
        ]
        self.x11.XSendEvent.restype = i
        self.x11.XRaiseWindow.argtypes = [ctypes.POINTER(Display), ul]
        self.x11.XLowerWindow.argtypes = [ctypes.POINTER(Display), ul]
        self.x11.XMapRaised.argtypes = [ctypes.POINTER(Display), ul]
        self.x11.XSetInputFocus.argtypes = [
            ctypes.POINTER(Display),
            ul,
            i,
            ul,
        ]
        self.x11.XFlush.argtypes = [ctypes.POINTER(Display)]
        self.x11.XSync.argtypes = [ctypes.POINTER(Display), i]
        self.x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
        self.x11.XStringToKeysym.restype = ul
        self.x11.XKeysymToKeycode.argtypes = [ctypes.POINTER(Display), ul]
        self.x11.XKeysymToKeycode.restype = ctypes.c_ubyte
        self.xtst.XTestFakeMotionEvent.argtypes = [
            ctypes.POINTER(Display),
            i,
            i,
            i,
            ul,
        ]
        self.xtst.XTestFakeMotionEvent.restype = i
        self.xtst.XTestFakeRelativeMotionEvent.argtypes = [
            ctypes.POINTER(Display),
            i,
            i,
            ul,
        ]
        self.xtst.XTestFakeDeviceMotionEvent.argtypes = [
            ctypes.POINTER(Display),
            ctypes.c_void_p,
            i,
            i,
            i,
            ul,
        ]
        self.xtst.XTestFakeDeviceMotionEvent.restype = i
        self.xtst.XTestFakeDeviceButtonEvent.argtypes = [
            ctypes.POINTER(Display),
            ctypes.c_void_p,
            u,
            i,
            ul,
            u,
        ]
        self.xtst.XTestFakeDeviceButtonEvent.restype = i
        self.xi = ctypes.CDLL(ctypes.util.find_library("Xi") or "libXi.so.6")
        self.xi.XOpenDevice.argtypes = [ctypes.POINTER(Display), ctypes.c_ulong]
        self.xi.XOpenDevice.restype = ctypes.c_void_p
        self.xtst.XTestFakeButtonEvent.argtypes = [
            ctypes.POINTER(Display),
            u,
            i,
            ul,
        ]
        self.xtst.XTestFakeButtonEvent.restype = i
        self.xtst.XTestFakeKeyEvent.argtypes = [
            ctypes.POINTER(Display),
            u,
            i,
            ul,
        ]
        self.xtst.XTestFakeKeyEvent.restype = i


class X:
    def __init__(self, name):
        libs = X11()
        self.libs = libs
        self.dpy = libs.x11.XOpenDisplay(name.encode() if name else None)
        if not self.dpy:
            raise RuntimeError(f"cannot open display {name or os.environ.get('DISPLAY')!r}")
        self.screen = libs.x11.XDefaultScreen(self.dpy)
        self.root = libs.x11.XRootWindow(self.dpy, self.screen)
        self.libs.x11.XSync(self.dpy, 0)

    def close(self):
        self.libs.x11.XCloseDisplay(self.dpy)

    def title(self, wid):
        name = ctypes.c_char_p()
        if not self.libs.x11.XFetchName(self.dpy, wid, ctypes.byref(name)):
            return None
        if not name.value:
            return ""
        value = name.value.decode("utf-8", "replace")
        self.libs.x11.XFree(name)
        return value

    def geometry(self, wid):
        root = ctypes.c_ulong()
        x = ctypes.c_int()
        y = ctypes.c_int()
        w = ctypes.c_uint()
        h = ctypes.c_uint()
        border = ctypes.c_uint()
        depth = ctypes.c_uint()
        if not self.libs.x11.XGetGeometry(
            self.dpy,
            wid,
            ctypes.byref(root),
            ctypes.byref(x),
            ctypes.byref(y),
            ctypes.byref(w),
            ctypes.byref(h),
            ctypes.byref(border),
            ctypes.byref(depth),
        ):
            return None
        abs_x = ctypes.c_int()
        abs_y = ctypes.c_int()
        child = ctypes.c_ulong()
        self.libs.x11.XTranslateCoordinates(
            self.dpy,
            wid,
            self.root,
            0,
            0,
            ctypes.byref(abs_x),
            ctypes.byref(abs_y),
            ctypes.byref(child),
        )
        return abs_x.value, abs_y.value, w.value, h.value, depth.value

    def children(self, wid):
        root = ctypes.c_ulong()
        parent = ctypes.c_ulong()
        kids = ctypes.POINTER(ctypes.c_ulong)()
        count = ctypes.c_uint()
        if not self.libs.x11.XQueryTree(
            self.dpy,
            wid,
            ctypes.byref(root),
            ctypes.byref(parent),
            ctypes.byref(kids),
            ctypes.byref(count),
        ):
            return []
        try:
            return [kids[i] for i in range(count.value)]
        finally:
            if kids:
                self.libs.x11.XFree(kids)

    def walk(self, wid=None, depth=0):
        """Every window under ``wid`` (default the root), depth first."""
        if wid is None:
            wid = self.root
        if depth > 12:
            return
        yield wid
        for kid in self.children(wid):
            yield from self.walk(kid, depth + 1)

    def find(self, title):
        wanted = title.lower()
        found = []
        for wid in self.walk():
            name = self.title(wid)
            if name and wanted in name.lower():
                found.append((wid, name))
        return found

    def restack(self, wid, raise_window):
        if raise_window:
            self.libs.x11.XRaiseWindow(self.dpy, wid)
        else:
            self.libs.x11.XLowerWindow(self.dpy, wid)
        self.libs.x11.XSync(self.dpy, 0)

    def focus(self, wid):
        self.libs.x11.XMapRaised(self.dpy, wid)
        self.libs.x11.XRaiseWindow(self.dpy, wid)
        self.libs.x11.XSetInputFocus(self.dpy, wid, REVERT_TO_PARENT, CURRENT_TIME)
        self.libs.x11.XSync(self.dpy, 0)

    def scene_to_root(self, wid, point, scene):
        """Map a point in a letterboxed scene to root pixels, as the client does."""
        geom = self.geometry(wid)
        if not geom:
            raise ValueError(f"cannot read the geometry of 0x{wid:08x}")
        ax, ay, w, h, _depth = geom
        scale = min(w / max(scene[0], 1.0), h / max(scene[1], 1.0))
        off_x = (w - scene[0] * scale) / 2.0
        off_y = (h - scene[1] * scale) / 2.0
        return (
            round(ax + off_x + point[0] * scale),
            round(ay + off_y + point[1] * scale),
        )

    def move(self, x, y):
        x, y = int(round(x)), int(round(y))
        self.libs.xtst.XTestFakeMotionEvent(self.dpy, self.screen, x, y, 0)
        self.libs.x11.XSync(self.dpy, 0)

    def device_click(self, device, x, y, button=1, hold=0.08):
        """XTEST device event on a named slave device (not the XTEST device)."""
        dev = self.libs.xi.XOpenDevice(self.dpy, int(device))
        if not dev:
            raise ValueError(f"cannot open X input device {device}")
        self.libs.xtst.XTestFakeDeviceMotionEvent(
            self.dpy, dev, 0, int(x), int(y), 0
        )
        self.libs.x11.XSync(self.dpy, 0)
        time.sleep(0.08)
        self.libs.xtst.XTestFakeDeviceButtonEvent(
            self.dpy, dev, int(button), 1, 0, int(device)
        )
        self.libs.x11.XSync(self.dpy, 0)
        time.sleep(hold)
        self.libs.xtst.XTestFakeDeviceButtonEvent(
            self.dpy, dev, int(button), 0, 0, int(device)
        )
        self.libs.x11.XSync(self.dpy, 0)

    def relative_move(self, dx, dy):
        self.libs.xtst.XTestFakeRelativeMotionEvent(
            self.dpy, int(dx), int(dy), 0
        )
        self.libs.x11.XSync(self.dpy, 0)

    def warp(self, x, y):
        """Move the pointer with XWarpPointer, not the XTEST virtual device."""
        self.libs.x11.XWarpPointer(
            self.dpy, 0, self.root, 0, 0, 0, 0, int(x), int(y)
        )
        self.libs.x11.XSync(self.dpy, 0)

    def where(self):
        root_return = ctypes.c_ulong()
        child = ctypes.c_ulong()
        root_x = ctypes.c_int()
        root_y = ctypes.c_int()
        win_x = ctypes.c_int()
        win_y = ctypes.c_int()
        mask = ctypes.c_uint()
        if not self.libs.x11.XQueryPointer(
            self.dpy,
            self.root,
            ctypes.byref(root_return),
            ctypes.byref(child),
            ctypes.byref(root_x),
            ctypes.byref(root_y),
            ctypes.byref(win_x),
            ctypes.byref(win_y),
            ctypes.byref(mask),
        ):
            return None
        return root_x.value, root_y.value, child.value, mask.value

    def button_event(self, button, pressed):
        self.libs.xtst.XTestFakeButtonEvent(
            self.dpy, int(button), 1 if pressed else 0, 0
        )
        self.libs.x11.XSync(self.dpy, 0)

    def click(self, x, y, button=1, hold=0.08, settle=0.05):
        self.move(x, y)
        time.sleep(settle)
        self.libs.xtst.XTestFakeButtonEvent(self.dpy, button, 1, 0)
        self.libs.x11.XSync(self.dpy, 0)
        time.sleep(hold)
        self.libs.xtst.XTestFakeButtonEvent(self.dpy, button, 0, 0)
        self.libs.x11.XSync(self.dpy, 0)

    def send_event(self, wid, event):
        """Send a synthetic XEvent straight to a window's client."""
        buf = (ctypes.c_long * 24)()
        ctypes.memmove(ctypes.byref(buf), ctypes.byref(event), ctypes.sizeof(event))
        self.libs.x11.XSendEvent(self.dpy, wid, 0, 0, ctypes.byref(buf))
        self.libs.x11.XSync(self.dpy, 0)

    def send_click(self, wid, x, y, button=1, hold=0.08):
        """A core MotionNotify + ButtonPress/Release delivered with XSendEvent."""
        root = self.root
        motion = XMotionEvent()
        motion.type = 6  # MotionNotify
        motion.send_event = 1
        motion.display = ctypes.cast(self.dpy, ctypes.c_void_p)
        motion.window = wid
        motion.root = root
        motion.subwindow = 0
        motion.time = 0
        motion.x = motion.x_root = int(x)
        motion.y = motion.y_root = int(y)
        motion.state = 0
        motion.is_hint = 0
        motion.same_screen = 1
        self.send_event(wid, motion)
        time.sleep(0.05)
        for pressed in (1, 0):
            event = XButtonEvent()
            event.type = 4 if pressed else 5  # ButtonPress / ButtonRelease
            event.send_event = 1
            event.display = ctypes.cast(self.dpy, ctypes.c_void_p)
            event.window = wid
            event.root = root
            event.subwindow = 0
            event.time = 0
            event.x = event.x_root = int(x)
            event.y = event.y_root = int(y)
            event.state = 0
            event.button = button
            event.same_screen = 1
            self.send_event(wid, event)
            time.sleep(hold if pressed else 0.02)

    def key(self, keysym, action="press"):
        sym = self.libs.x11.XStringToKeysym(keysym.encode())
        if not sym:
            raise ValueError(f"unknown keysym {keysym!r}")
        code = self.libs.x11.XKeysymToKeycode(self.dpy, sym)
        if not code:
            raise ValueError(f"keysym {keysym!r} has no keycode")
        if action in ("press", "down"):
            self.libs.xtst.XTestFakeKeyEvent(self.dpy, code, 1, 0)
            self.libs.x11.XSync(self.dpy, 0)
        if action in ("press", "release", "up"):
            if action == "press":
                time.sleep(0.03)
            self.libs.xtst.XTestFakeKeyEvent(self.dpy, code, 0, 0)
            self.libs.x11.XSync(self.dpy, 0)


def parse_window(value):
    try:
        return int(value, 0)
    except ValueError:
        raise argparse.ArgumentTypeError(f"not a window id: {value!r}")


def pointer(x, args):
    """Root pixels for a point, mapping scene coordinates when asked."""
    if args.window_id is None and not args.title:
        return args.x, args.y
    if not args.scene:
        raise ValueError("--window-id/--title needs --scene W H to map the point")
    wid = args.window_id
    if wid is None:
        found = x.find(args.title)
        if not found:
            raise ValueError(f"no window title contains {args.title!r}")
        wid = found[0][0]
    return x.scene_to_root(wid, (args.x, args.y), args.scene)


def main(argv):
    # ``--display`` is pulled out before argparse so it works before or after the
    # subcommand, like the X tools this replaces.
    display = None
    rest = []
    it = iter(argv)
    for arg in it:
        if arg == "--display":
            display = next(it, None)
            if display is None:
                print("xtest: --display needs a value", file=sys.stderr)
                return 2
        elif arg.startswith("--display="):
            display = arg.split("=", 1)[1]
        else:
            rest.append(arg)

    parser = argparse.ArgumentParser(
        description="Synthetic X11 pointer, button and key input through XTEST."
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("windows", help="list titled windows on the display")
    p.add_argument("--title", default="", help="only titles containing this")

    p = sub.add_parser("where", help="print the pointer position and its child window")

    p = sub.add_parser("raise", help="raise a window")
    p.add_argument("window", type=parse_window)

    p = sub.add_parser("lower", help="lower a window")
    p.add_argument("window", type=parse_window)

    p = sub.add_parser("find", help="find a window by (part of) its title")
    p.add_argument("--title", required=True)

    p = sub.add_parser("focus", help="raise and give a window the input focus")
    p.add_argument("window", nargs="?", type=parse_window)
    p.add_argument("--title")

    def window_point(p):
        p.add_argument(
            "--window-id",
            type=parse_window,
            help="map a scene point through this window instead of using root pixels",
        )
        p.add_argument("--title", help="find the window to map through by title")
        p.add_argument(
            "--scene",
            nargs=2,
            type=float,
            metavar=("W", "H"),
            help="the scene size the window letterboxes (e.g. 1280 1005)",
        )

    p = sub.add_parser("move", help="move the pointer to root coordinates")
    p.add_argument("x", type=float)
    p.add_argument("y", type=float)
    window_point(p)

    p = sub.add_parser("warp", help="move the pointer with XWarpPointer (not XTEST)")
    p.add_argument("x", type=float)
    p.add_argument("y", type=float)
    window_point(p)

    p = sub.add_parser("move-rel", help="relative XTEST motion (dx dy)")
    p.add_argument("dx", type=int)
    p.add_argument("dy", type=int)

    p = sub.add_parser(
        "device-click",
        help="XTEST device event on a slave pointer (--device from xinput list)",
    )
    p.add_argument("x", type=int)
    p.add_argument("y", type=int)
    p.add_argument("--device", type=int, required=True)
    p.add_argument("--button", type=int, default=1)
    p.add_argument("--hold", type=float, default=0.08)

    p = sub.add_parser("click", help="move, press, hold and release a button")
    p.add_argument("x", type=float)
    p.add_argument("y", type=float)
    p.add_argument("--button", type=int, default=1)
    p.add_argument("--hold", type=float, default=0.08)
    p.add_argument("--settle", type=float, default=0.05)
    window_point(p)

    p = sub.add_parser("button", help="press or release a button without moving")
    p.add_argument("button", type=int)
    p.add_argument("action", choices=("down", "up"))

    p = sub.add_parser(
        "send-click",
        help="send MotionNotify/ButtonPress/ButtonRelease with XSendEvent",
    )
    p.add_argument("x", type=float)
    p.add_argument("y", type=float)
    p.add_argument("--button", type=int, default=1)
    p.add_argument("--hold", type=float, default=0.08)
    window_point(p)

    p = sub.add_parser(
        "key", help="press and release a keysym, or hold it down/up"
    )
    p.add_argument("keysym")
    p.add_argument(
        "action", nargs="?", choices=("press", "down", "up", "release"), default="press"
    )
    p.add_argument(
        "--hold", type=float, default=None,
        help="press: keep the key down this many seconds (default 0.03) inside this one process",
    )

    args = parser.parse_args(rest)
    try:
        x = X(display)
    except (OSError, RuntimeError) as error:
        print(f"xtest: {error}", file=sys.stderr)
        return 2

    try:
        if args.command == "windows":
            wanted = args.title.lower()
            rows = []
            for wid in x.walk():
                name = x.title(wid)
                if not name or (wanted and wanted not in name.lower()):
                    continue
                geom = x.geometry(wid)
                if geom:
                    rows.append((wid, name, geom))
            for wid, name, (ax, ay, w, h, depth) in rows:
                print(f"0x{wid:08x}\t{ax},{ay}\t{w}x{h}\tdepth {depth}\t{name}")
            if not rows:
                print("(no titled windows)", file=sys.stderr)
            return 0

        if args.command == "where":
            place = x.where()
            if place is None:
                print("xtest: XQueryPointer failed", file=sys.stderr)
                return 2
            px, py, child, mask = place
            name = x.title(child) if child else None
            print(f"pointer {px},{py} child 0x{child:08x} {name or ''} mask 0x{mask:x}")
            return 0

        if args.command in ("raise", "lower"):
            x.restack(args.window, args.command == "raise")
            print(f"{args.command} 0x{args.window:08x}")
            return 0

        if args.command == "find":
            found = x.find(args.title)
            if not found:
                print(f"xtest: no window title contains {args.title!r}", file=sys.stderr)
                return 2
            for wid, name in found:
                geom = x.geometry(wid)
                if geom:
                    ax, ay, w, h, depth = geom
                    print(f"0x{wid:08x}\t{ax},{ay}\t{w}x{h}\tdepth {depth}\t{name}")
            return 0

        if args.command == "focus":
            wid = args.window
            if wid is None:
                if args.title:
                    found = x.find(args.title)
                    if not found:
                        print(
                            f"xtest: no window title contains {args.title!r}",
                            file=sys.stderr,
                        )
                        return 2
                    wid = found[0][0]
                else:
                    print("xtest: focus needs a window id or --title", file=sys.stderr)
                    return 2
            x.focus(wid)
            print(f"focused 0x{wid:08x}")
            return 0

        if args.command == "move":
            px, py = pointer(x, args)
            x.move(px, py)
            print(f"pointer {px},{py}")
            return 0

        if args.command == "warp":
            px, py = pointer(x, args)
            x.warp(px, py)
            print(f"warp {px},{py}")
            return 0

        if args.command == "move-rel":
            x.relative_move(args.dx, args.dy)
            print(f"relative {args.dx},{args.dy}")
            return 0

        if args.command == "device-click":
            x.device_click(args.device, args.x, args.y, args.button, args.hold)
            print(f"device {args.device} click {args.x},{args.y}")
            return 0

        if args.command == "click":
            px, py = pointer(x, args)
            x.click(px, py, args.button, args.hold, args.settle)
            print(f"click {px},{py} button {args.button}")
            return 0

        if args.command == "button":
            x.button_event(args.button, args.action == "down")
            print(f"button {args.button} {args.action}")
            return 0

        if args.command == "send-click":
            if args.window_id is None and not args.title:
                print("xtest: send-click needs --window-id or --title", file=sys.stderr)
                return 2
            wid = args.window_id
            if wid is None:
                found = x.find(args.title)
                if not found:
                    print(f"xtest: no window title contains {args.title!r}", file=sys.stderr)
                    return 2
                wid = found[0][0]
            px, py = pointer(x, args)
            x.send_click(wid, px, py, args.button, args.hold)
            print(f"send-click 0x{wid:08x} {px},{py} button {args.button}")
            return 0

        if args.hold is not None and args.action == "press":
            x.key(args.keysym, "down")
            time.sleep(args.hold)
            x.key(args.keysym, "up")
        else:
            x.key(args.keysym, args.action)
        print(f"key {args.keysym} {args.action}")
        return 0
    except ValueError as error:
        print(f"xtest: {error}", file=sys.stderr)
        return 2
    finally:
        x.close()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
