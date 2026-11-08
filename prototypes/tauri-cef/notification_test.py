#!/usr/bin/env python3
"""Capture only synthetic CEF notifications on a dedicated D-Bus session."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import time

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="GetServerInformation"><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/></method>
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
</interface></node>"""


def main():
    if os.environ.get("ELO_SYNTHETIC_DBUS") != "1":
        raise RuntimeError("Run with ELO_SYNTHETIC_DBUS=1 dbus-run-session -- ...")
    browser = Path(sys.argv[1]).resolve(strict=True)
    xvfb = Path(sys.argv[2]).resolve(strict=True)
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    result = connection.call_sync(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        GLib.Variant("(su)", ("org.freedesktop.Notifications", 4)),
        None,
        Gio.DBusCallFlags.NONE,
        2000,
        None,
    ).unpack()
    if result != (1,):
        raise RuntimeError(
            "Notification service already exists; refusing to replace it"
        )
    records = []

    def handle(_connection, _sender, _path, _interface, method, params, invocation):
        if method == "GetCapabilities":
            invocation.return_value(
                GLib.Variant("(as)", (["body", "actions", "body-markup"],))
            )
        elif method == "GetServerInformation":
            invocation.return_value(
                GLib.Variant("(ssss)", ("Synthetic", "Elo", "1", "1.2"))
            )
        elif method == "Notify":
            values = params.unpack()
            records.append(
                {
                    "genericSummary": values[3] == "Elo Talks",
                    "genericBody": values[4] == "You have a new message.",
                    "privateBodyDelivered": "Synthetic CEF private body" in values[4],
                }
            )
            invocation.return_value(GLib.Variant("(u)", (len(records),)))
        else:
            invocation.return_value(GLib.Variant("()", ()))

    registration = connection.register_object(
        "/org/freedesktop/Notifications",
        Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0],
        handle,
        None,
        None,
    )
    with tempfile.TemporaryDirectory(prefix="elo-cef-notification-") as directory:
        env = os.environ.copy()
        env.update(
            {
                "LD_LIBRARY_PATH": str(browser.parent),
                "GDK_BACKEND": "x11",
                "XDG_DATA_HOME": directory + "/data",
                "XDG_CACHE_HOME": directory + "/cache",
                "XDG_CONFIG_HOME": directory + "/config",
            }
        )
        display = subprocess.Popen(
            [
                str(xvfb),
                "-displayfd",
                "1",
                "-screen",
                "0",
                "1280x800x24",
                "-nolisten",
                "tcp",
            ],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        child = None
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(display.stdout, selectors.EVENT_READ)
                if not selector.select(10):
                    raise RuntimeError("Synthetic display timeout")
                number = display.stdout.readline().strip().decode("ascii")
                if not number.isdigit():
                    raise RuntimeError("Synthetic display failed")
            env["DISPLAY"] = ":" + number
            child = subprocess.Popen(
                [str(browser), "--notifications"],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 45
            context = GLib.MainContext.default()
            while child.poll() is None and time.monotonic() < deadline:
                while context.pending():
                    context.iteration(False)
                time.sleep(0.01)
            if child.poll() is None:
                child.kill()
            output, _ = child.communicate(timeout=3)
            delivery = (
                child.returncode == 0
                and b"Synthetic WebRTC result: 0" in output
                and bool(records)
            )
            privacy = bool(records) and all(
                r["genericSummary"]
                and r["genericBody"]
                and not r["privateBodyDelivered"]
                for r in records
            )
            success = delivery and privacy
            print(
                json.dumps(
                    {
                        "browserSuccess": child.returncode == 0,
                        "nativeNotifications": len(records),
                        "privateModePassed": privacy,
                        "records": records,
                    }
                )
            )
            return 0 if success else 1
        finally:
            if child is not None and child.poll() is None:
                child.kill()
                child.wait()
            display.terminate()
            display.wait(timeout=3)
            connection.unregister_object(registration)


if __name__ == "__main__":
    sys.exit(main())
