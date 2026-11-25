#!/usr/bin/env python3
"""Check X11 window branding using an isolated synthetic launcher fixture."""

import ctypes
import pathlib
import subprocess
import sys
import tempfile
import time

APP_ID = b"dev.anuness.elotalks"


def main():
    x11 = ctypes.CDLL("libX11.so.6")
    pointer = ctypes.c_void_p
    word = ctypes.c_ulong
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = pointer
    x11.XDefaultRootWindow.argtypes = [pointer]
    x11.XDefaultRootWindow.restype = word
    x11.XInternAtom.argtypes = [pointer, ctypes.c_char_p, ctypes.c_int]
    x11.XInternAtom.restype = word
    x11.XQueryTree.argtypes = [
        pointer,
        word,
        ctypes.POINTER(word),
        ctypes.POINTER(word),
        ctypes.POINTER(pointer),
        ctypes.POINTER(ctypes.c_uint),
    ]
    x11.XGetWindowProperty.argtypes = [
        pointer,
        word,
        word,
        ctypes.c_long,
        ctypes.c_long,
        ctypes.c_int,
        word,
        ctypes.POINTER(word),
        ctypes.POINTER(ctypes.c_int),
        ctypes.POINTER(word),
        ctypes.POINTER(word),
        ctypes.POINTER(pointer),
    ]
    x11.XFree.argtypes = [pointer]
    x11.XCloseDisplay.argtypes = [pointer]
    display = x11.XOpenDisplay(None)
    if not display:
        raise RuntimeError("An isolated X11 display is required")

    def properties(window, name):
        atom = x11.XInternAtom(display, name, 0)
        kind, count, remaining = word(), word(), word()
        bits, data = ctypes.c_int(), pointer()
        result = x11.XGetWindowProperty(
            display,
            window,
            atom,
            0,
            524288,
            0,
            0,
            ctypes.byref(kind),
            ctypes.byref(bits),
            ctypes.byref(count),
            ctypes.byref(remaining),
            ctypes.byref(data),
        )
        try:
            if result or remaining.value or not data.value:
                return None
            if bits.value == 32:
                values = ctypes.cast(data, ctypes.POINTER(word))
                return [values[index] for index in range(count.value)]
            if bits.value == 8:
                return ctypes.string_at(data, count.value)
            return None
        finally:
            if data.value:
                x11.XFree(data)

    try:
        with tempfile.TemporaryFile() as log:
            process = subprocess.Popen(
                [str(pathlib.Path(sys.argv[1]).resolve()), "error"],
                stdout=log,
                stderr=log,
            )
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline and process.poll() is None:
                    root, parent = word(), word()
                    children, count = pointer(), ctypes.c_uint()
                    x11.XQueryTree(
                        display,
                        x11.XDefaultRootWindow(display),
                        ctypes.byref(root),
                        ctypes.byref(parent),
                        ctypes.byref(children),
                        ctypes.byref(count),
                    )
                    try:
                        if count.value > 256:
                            raise RuntimeError(
                                "Unexpected windows on the synthetic display"
                            )
                        windows = ctypes.cast(children, ctypes.POINTER(word))
                        for index in range(count.value):
                            window = windows[index]
                            if properties(window, b"_NET_WM_PID") != [process.pid]:
                                continue
                            identity = properties(window, b"WM_CLASS")
                            icon = properties(window, b"_NET_WM_ICON")
                            if (
                                not identity
                                or APP_ID not in identity.split(b"\0")
                                or not icon
                            ):
                                continue
                            offset = 0
                            sizes = []
                            while offset + 2 <= len(icon):
                                width, height = icon[offset : offset + 2]
                                end = offset + 2 + width * height
                                if (
                                    width != height
                                    or width not in (16, 32, 48, 64, 128, 256, 512)
                                    or end > len(icon)
                                    or not any(icon[offset + 2 : end])
                                ):
                                    raise RuntimeError(
                                        "The native window has an invalid logo image"
                                    )
                                sizes.append(width)
                                offset = end
                            if offset != len(icon) or not sizes:
                                raise RuntimeError(
                                    "The native window has no valid embedded logo"
                                )
                            if process.wait(timeout=10) != 0:
                                raise RuntimeError("Synthetic launcher failed")
                            print(
                                "PASS: X11 application identity and embedded window logo"
                            )
                            return
                    finally:
                        if children.value:
                            x11.XFree(children)
                    time.sleep(0.01)
                raise RuntimeError(
                    "Native logo or desktop window identity was not published"
                )
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)
    finally:
        x11.XCloseDisplay(display)


if __name__ == "__main__":
    main()
