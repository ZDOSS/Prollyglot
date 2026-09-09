"""Tiny X11 driver restricted to the native fixture's own high-numbered Xvfb.

Used for GTK controls which are outside the WebKit DOM. Never opens DISPLAY
from the environment and never scans any other X server. No image capture.
"""
import ctypes as c
import os
from pathlib import Path
import time


class PrivateX11:
    def __init__(self, xvfb, display):
        private = Path(os.environ["PROLLYGLOT_PRIVATE_PIPEWIRE"])
        assert private.name.startswith("prollyglot-pipewire-")
        assert os.environ["XDG_RUNTIME_DIR"] == str(private)
        assert not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY")
        assert 100 <= int(display.removeprefix(":")) < 1000
        assert xvfb.poll() is None and Path(xvfb.args[0]).name == "Xvfb"
        assert xvfb.args[1] == display
        self.x = c.CDLL("libX11.so.6")
        self.t = c.CDLL("libXtst.so.6")
        self.ext = c.CDLL("libXext.so.6")
        # Windows can disappear between a tree query and property lookup. Xlib's
        # default handler calls exit(), which would skip the fixture's cleanup.
        # Failed queries are checked below; test expectations still fail normally.
        handler_type = c.CFUNCTYPE(c.c_int, c.c_void_p, c.c_void_p)
        self.error_handler = handler_type(lambda _display, _error: 0)
        self.x.XSetErrorHandler.argtypes = [handler_type]
        self.x.XSetErrorHandler(self.error_handler)
        pointer, window = c.c_void_p, c.c_ulong
        signatures = {
            "XOpenDisplay": ([c.c_char_p], pointer),
            "XDefaultRootWindow": ([pointer], window),
            "XQueryTree": ([pointer, window, c.POINTER(window), c.POINTER(window), c.POINTER(c.POINTER(window)), c.POINTER(c.c_uint)], c.c_int),
            "XFetchName": ([pointer, window, c.POINTER(pointer)], c.c_int),
            "XInternAtom": ([pointer, c.c_char_p, c.c_int], window),
            "XGetWindowProperty": ([pointer, window, window, c.c_long, c.c_long, c.c_int, window, c.POINTER(window), c.POINTER(c.c_int), c.POINTER(window), c.POINTER(window), c.POINTER(pointer)], c.c_int),
            "XFree": ([pointer], c.c_int),
            "XFlush": ([pointer], c.c_int),
            "XCloseDisplay": ([pointer], c.c_int),
            "XSetInputFocus": ([pointer, window, c.c_int, window], c.c_int),
            "XStringToKeysym": ([c.c_char_p], window),
            "XKeysymToKeycode": ([pointer, window], c.c_ubyte),
            "XMoveWindow": ([pointer, window, c.c_int, c.c_int], c.c_int),
            "XGetGeometry": ([pointer, window, c.POINTER(window), c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_uint), c.POINTER(c.c_uint), c.POINTER(c.c_uint), c.POINTER(c.c_uint)], c.c_int),
        }
        for name, (args, result) in signatures.items():
            fn = getattr(self.x, name)
            fn.argtypes, fn.restype = args, result
        self.t.XTestFakeKeyEvent.argtypes = [pointer, c.c_uint, c.c_int, window]
        self.t.XTestFakeButtonEvent.argtypes = [pointer, c.c_uint, c.c_int, window]
        self.t.XTestFakeMotionEvent.argtypes = [pointer, c.c_int, c.c_int, c.c_int, window]
        self.ext.XShapeGetRectangles.argtypes = [pointer, window, c.c_int, c.POINTER(c.c_int), c.POINTER(c.c_int)]
        self.ext.XShapeGetRectangles.restype = pointer
        self.display = self.x.XOpenDisplay(display.encode())
        assert self.display, "Could not open the fixture Xvfb"
        self.root = self.x.XDefaultRootWindow(self.display)
        self.name_atom = self.x.XInternAtom(self.display, b"_NET_WM_NAME", 0)
        assert self.name_atom

    def find(self, title):
        self.titles = []
        def visit(window):
            name = c.c_void_p()
            actual_type, count, remaining, format = c.c_ulong(), c.c_ulong(), c.c_ulong(), c.c_int()
            self.x.XGetWindowProperty(self.display, window, self.name_atom, 0, 256, 0, 0,
                                     c.byref(actual_type), c.byref(format), c.byref(count), c.byref(remaining), c.byref(name))
            if not name.value:
                self.x.XFetchName(self.display, window, c.byref(name))
            if name.value:
                text = c.string_at(name).decode(errors="replace")
                self.x.XFree(name)
                self.titles.append(text)
                if title.split(" — ")[0] in text:
                    return window
            root, parent, count = c.c_ulong(), c.c_ulong(), c.c_uint()
            children = c.POINTER(c.c_ulong)()
            if self.x.XQueryTree(self.display, window, c.byref(root), c.byref(parent), c.byref(children), c.byref(count)):
                values = [children[i] for i in range(count.value)]
                if children:
                    self.x.XFree(children)
                for child in values:
                    found = visit(child)
                    if found:
                        return found
            return None
        return visit(self.root)

    def geometry(self, window):
        root, x, y = c.c_ulong(), c.c_int(), c.c_int()
        width, height, border, depth = (c.c_uint() for _ in range(4))
        assert self.x.XGetGeometry(self.display, window, c.byref(root), c.byref(x), c.byref(y), c.byref(width), c.byref(height), c.byref(border), c.byref(depth))
        return x.value, y.value, width.value, height.value

    def input_shape(self, window):
        count, ordering = c.c_int(), c.c_int()
        rectangles = self.ext.XShapeGetRectangles(self.display, window, 2, c.byref(count), c.byref(ordering))
        if rectangles:
            self.x.XFree(rectangles)
        return count.value

    def accepts_focus(self, window):
        class Hints(c.Structure):
            _fields_ = [("flags", c.c_long), ("input", c.c_int), ("initial_state", c.c_int),
                        ("icon_pixmap", c.c_ulong), ("icon_window", c.c_ulong),
                        ("icon_x", c.c_int), ("icon_y", c.c_int), ("icon_mask", c.c_ulong), ("window_group", c.c_ulong)]
        self.x.XGetWMHints.argtypes = [c.c_void_p, c.c_ulong]
        self.x.XGetWMHints.restype = c.POINTER(Hints)
        hints = self.x.XGetWMHints(self.display, window)
        assert hints and hints.contents.flags & 1
        result = bool(hints.contents.input)
        self.x.XFree(hints)
        return result

    def key(self, name, down):
        code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(name.encode()))
        assert code, name
        self.t.XTestFakeKeyEvent(self.display, code, int(down), 0)
        self.x.XFlush(self.display)

    def tap(self, name):
        self.key(name, True)
        self.key(name, False)

    def focus(self, window):
        self.x.XSetInputFocus(self.display, window, 1, 0)
        self.x.XFlush(self.display)
        time.sleep(.08)

    def coordinate(self, mnemonic, value, commit=True):
        self.key("Alt_L", True)
        self.tap(mnemonic)
        self.key("Alt_L", False)
        self.key("Control_L", True)
        self.tap("a")
        self.key("Control_L", False)
        for digit in str(value):
            self.tap(digit)
        if commit:
            self.tap("Tab")
        time.sleep(.05)

    def drag(self, start, end):
        self.t.XTestFakeMotionEvent(self.display, -1, *start, 0)
        self.t.XTestFakeButtonEvent(self.display, 1, 1, 0)
        self.x.XFlush(self.display)
        time.sleep(.05)
        self.t.XTestFakeMotionEvent(self.display, -1, *end, 0)
        self.x.XFlush(self.display)
        time.sleep(.05)
        self.t.XTestFakeButtonEvent(self.display, 1, 0, 0)
        self.x.XFlush(self.display)

    def move(self, window, x, y):
        self.x.XMoveWindow(self.display, window, x, y)
        self.x.XFlush(self.display)

    def close(self):
        self.x.XCloseDisplay(self.display)
