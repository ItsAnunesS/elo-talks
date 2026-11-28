"""Probe a single solid-color pixel on the isolated native fixture's Xvfb screen."""

import ctypes
import ctypes.util
import sys


x11 = ctypes.CDLL(ctypes.util.find_library("X11"))
window_type = ctypes.c_ulong
pointer = ctypes.c_void_p
integer = ctypes.c_int
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XOpenDisplay.restype = pointer
x11.XCloseDisplay.argtypes = [pointer]
x11.XDefaultRootWindow.argtypes = [pointer]
x11.XDefaultRootWindow.restype = window_type
x11.XRaiseWindow.argtypes = [pointer, window_type]
x11.XSync.argtypes = [pointer, integer]
x11.XGetGeometry.argtypes = [
    pointer,
    window_type,
    ctypes.POINTER(window_type),
    ctypes.POINTER(integer),
    ctypes.POINTER(integer),
    ctypes.POINTER(ctypes.c_uint),
    ctypes.POINTER(ctypes.c_uint),
    ctypes.POINTER(ctypes.c_uint),
    ctypes.POINTER(ctypes.c_uint),
]
x11.XTranslateCoordinates.argtypes = [
    pointer,
    window_type,
    window_type,
    integer,
    integer,
    ctypes.POINTER(integer),
    ctypes.POINTER(integer),
    ctypes.POINTER(window_type),
]
x11.XGetImage.argtypes = [
    pointer,
    window_type,
    integer,
    integer,
    ctypes.c_uint,
    ctypes.c_uint,
    ctypes.c_ulong,
    integer,
]
x11.XGetImage.restype = pointer
x11.XGetPixel.argtypes = [pointer, integer, integer]
x11.XGetPixel.restype = ctypes.c_ulong
x11.XDestroyImage.argtypes = [pointer]


def probe():
    display = x11.XOpenDisplay(None)
    if not display:
        return False
    image = None
    try:
        window = int(sys.argv[1])
        if sys.argv[2] == "raise":
            x11.XRaiseWindow(display, window)
            x11.XSync(display, 0)
            return True
        root = window_type()
        x, y = integer(), integer()
        width, height = ctypes.c_uint(), ctypes.c_uint()
        border, depth = ctypes.c_uint(), ctypes.c_uint()
        if not x11.XGetGeometry(
            display,
            window,
            ctypes.byref(root),
            ctypes.byref(x),
            ctypes.byref(y),
            ctypes.byref(width),
            ctypes.byref(height),
            ctypes.byref(border),
            ctypes.byref(depth),
        ):
            return False
        screen = x11.XDefaultRootWindow(display)
        child = window_type()
        if not x11.XTranslateCoordinates(
            display,
            window,
            screen,
            width.value // 2,
            height.value // 2,
            ctypes.byref(x),
            ctypes.byref(y),
            ctypes.byref(child),
        ):
            return False
        x11.XSync(display, 0)
        image = x11.XGetImage(display, screen, x.value, y.value, 1, 1, ~0, 2)
        return bool(image) and x11.XGetPixel(image, 0, 0) & 0xFFFFFF == int(
            sys.argv[2], 16
        )
    finally:
        if image:
            x11.XDestroyImage(image)
        x11.XCloseDisplay(display)


sys.exit(0 if probe() else 1)
