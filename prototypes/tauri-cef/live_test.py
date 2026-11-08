#!/usr/bin/env python3
"""Supervise an explicitly authorized test account in a disposable encrypted vault."""
import argparse
import ctypes
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import sys
import tempfile
import time

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

stopping = False


def stop_requested(*_):
    global stopping
    stopping = True


def child_setup():
    # Terminate the helper/browser if its supervisor unexpectedly disappears.
    if ctypes.CDLL(None).prctl(1, signal.SIGTERM, 0, 0, 0) != 0:
        raise OSError("Cannot enforce parent-death termination")


def call(connection, path, interface, method, signature, args):
    return connection.call_sync(
        "org.freedesktop.login1",
        path,
        interface,
        method,
        GLib.Variant(signature, args),
        None,
        Gio.DBusCallFlags.NONE,
        2000,
        None,
    ).unpack()


def properties(connection, path, interface):
    return call(
        connection,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        "(s)",
        (interface,),
    )[0]


def safe_session(connection, session):
    manager = properties(
        connection, "/org/freedesktop/login1", "org.freedesktop.login1.Manager"
    )
    status = properties(connection, session, "org.freedesktop.login1.Session")
    return (
        not manager["PreparingForSleep"]
        and not status["LockedHint"]
        and status["Active"]
        and status["State"] == "active"
    )


def mounted(path):
    return any(
        line.split(" - ", 1)[0].split()[4] == str(path)
        for line in Path("/proc/self/mountinfo").read_text().splitlines()
    )


def stop_browser(browser):
    if browser is None:
        return
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(browser.pid, sig)
        except ProcessLookupError:
            pass
        try:
            browser.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass
    browser.wait(timeout=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", required=True, type=Path)
    parser.add_argument("--gocryptfs", required=True, type=Path)
    parser.add_argument(
        "--vault-smoke",
        action="store_true",
        help="Test encrypted mount/closure without opening WhatsApp",
    )
    parser.add_argument(
        "--supervision-smoke",
        action="store_true",
        help="Verify forced browser exit before normal unmount using a synthetic process",
    )
    args = parser.parse_args()
    os.umask(0o077)
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    ctypes.CDLL(None).prctl(4, 0, 0, 0, 0)
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, stop_requested)
    connection = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
    manager = "/org/freedesktop/login1"
    try:
        session = call(
            connection,
            manager,
            "org.freedesktop.login1.Manager",
            "GetSessionByPID",
            "(u)",
            (os.getpid(),),
        )[0]
    except GLib.Error:
        user = call(
            connection,
            manager,
            "org.freedesktop.login1.Manager",
            "GetUser",
            "(u)",
            (os.getuid(),),
        )[0]
        session_id, session = properties(
            connection, user, "org.freedesktop.login1.User"
        )["Display"]
        if not session_id:
            raise RuntimeError("No graphical login session")
    if not safe_session(connection, session):
        raise RuntimeError("Session is locked, inactive, or preparing to sleep")
    reply, fd_list = connection.call_with_unix_fd_list_sync(
        "org.freedesktop.login1",
        manager,
        "org.freedesktop.login1.Manager",
        "Inhibit",
        GLib.Variant(
            "(ssss)", ("sleep", "Elo CEF test", "Close encrypted test vault", "delay")
        ),
        None,
        Gio.DBusCallFlags.NONE,
        2000,
        None,
        None,
    )
    inhibitor = fd_list.get(reply.unpack()[0])
    root = Path(tempfile.mkdtemp(prefix="elo-cef-vault-"))
    cipher, mount = root / "cipher", root / "mount"
    cipher.mkdir()
    mount.mkdir()
    key = bytearray(os.urandom(32).hex().encode() + b"\n")
    browser = fuse = None
    helper = str(args.gocryptfs.resolve(strict=True))

    def vault_command(initialize):
        command = [helper, "-q", "-passfile", "/dev/stdin"]
        command += ["-init"] if initialize else ["-fg", "-nosyslog"]
        command += [str(cipher)] if initialize else [str(cipher), str(mount)]
        process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
            preexec_fn=child_setup,
        )
        try:
            process.stdin.write(key)
            process.stdin.close()
        except Exception:
            process.kill()
            process.wait()
            raise
        return process

    try:
        initializer = vault_command(True)
        try:
            if initializer.wait(timeout=15) != 0:
                raise RuntimeError("Could not initialize encrypted test vault")
        except subprocess.TimeoutExpired:
            initializer.kill()
            initializer.wait()
            raise
        fuse = vault_command(False)
        deadline = time.monotonic() + 15
        while not mounted(mount):
            if stopping or fuse.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("Encrypted test vault did not mount")
            time.sleep(0.05)
        if stopping or not safe_session(connection, session):
            raise RuntimeError("Session became unsafe during vault setup")
        if args.vault_smoke:
            test_file = mount / "synthetic"
            test_file.write_text("synthetic encrypted fixture")
            assert test_file.read_text() == "synthetic encrypted fixture"
            print("Encrypted test vault read/write passed", flush=True)
            return
        profile = mount / "cef-profile"
        profile.mkdir()
        env = os.environ.copy()
        env["ELO_CEF_SUPERVISOR_PID"] = str(os.getpid())
        for name in ("XDG_CACHE_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"):
            directory = mount / name.lower()
            directory.mkdir()
            env[name] = str(directory)
        for name in (
            "SSLKEYLOGFILE",
            "CEF_CRASH_REPORTER_SERVER_URL",
            "BREAKPAD_DUMP_LOCATION",
        ):
            env.pop(name, None)
        command = [
            str(args.browser.resolve(strict=True)),
            "--live-profile",
            str(profile),
        ]
        if args.supervision_smoke:
            command = [
                sys.executable,
                "-c",
                "import signal,time,sys; signal.signal(signal.SIGTERM, signal.SIG_IGN); f=open(sys.argv[1], 'w'); f.write('synthetic'); f.flush(); time.sleep(60)",
                str(profile / "synthetic-open-file"),
            ]
        browser = subprocess.Popen(
            command,
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
            preexec_fn=child_setup,
        )
        print(
            (
                "Synthetic supervision test started"
                if args.supervision_smoke
                else "Test account window opened. Close it to destroy the temporary test profile."
            ),
            flush=True,
        )
        # Bounded test session; closing/locking/suspending never leaves account processes running.
        deadline = time.monotonic() + (2 if args.supervision_smoke else 15 * 60)
        while browser.poll() is None and not stopping and time.monotonic() < deadline:
            if not safe_session(connection, session):
                break
            time.sleep(0.1)
        if args.supervision_smoke:
            if (
                browser.poll() is not None
                or (profile / "synthetic-open-file").read_text() != "synthetic"
            ):
                raise RuntimeError("Synthetic browser did not hold the vault open")
    finally:
        stop_browser(browser)
        for _ in range(20):
            if not mounted(mount):
                break
            try:
                subprocess.run(
                    ["/usr/bin/fusermount3", "-u", str(mount)],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    timeout=0.4,
                    check=False,
                )
            except subprocess.TimeoutExpired:
                pass
            time.sleep(0.1)
        key[:] = b"\0" * len(key)
        if mounted(mount):
            print(f"TEST VAULT CLOSURE FAILED: recovery required at {root}", flush=True)
            # Preserve encrypted data and the mount; never force/lazy-unmount or claim success.
            raise RuntimeError("Test vault remains mounted")
        if fuse is not None:
            if fuse.poll() is None:
                fuse.terminate()
            fuse.wait(timeout=3)
        shutil.rmtree(root)
        os.close(inhibitor)
        print(
            "Test processes stopped; encrypted test vault unmounted and removed",
            flush=True,
        )


if __name__ == "__main__":
    main()
