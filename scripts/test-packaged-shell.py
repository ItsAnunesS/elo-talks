#!/usr/bin/env python3
"""Start the packaged production shell against an isolated synthetic controller."""

import json
import os
import pathlib
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time


def receive(connection, length):
    result = b""
    while len(result) < length:
        chunk = connection.recv(length - len(result))
        if not chunk:
            raise RuntimeError("Incomplete synthetic IPC frame")
        result += chunk
    return result


def main():
    image = pathlib.Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="elo-package-") as temporary:
        root = pathlib.Path(temporary)
        subprocess.run(
            [str(image), "--appimage-extract"],
            cwd=root,
            stdout=subprocess.DEVNULL,
            check=True,
        )
        appdir = root / "squashfs-root"
        binary = appdir / "usr/bin/elo-talks"
        profile = root / "apparmor"
        loaded = False
        process = None
        listener = socket.socket(socket.AF_UNIX)
        seen = set()
        errors = []
        finished = threading.Event()
        try:
            restriction = pathlib.Path(
                "/proc/sys/kernel/apparmor_restrict_unprivileged_userns"
            )
            if restriction.exists() and restriction.read_text().strip() == "1":
                if os.environ.get("GITHUB_ACTIONS") != "true":
                    raise RuntimeError(
                        "Host restricts user namespaces; no policy changed"
                    )
                profile.write_text(
                    "abi <abi/4.0>,\ninclude <tunables/global>\n"
                    f'profile elo-package-ci "{binary}" flags=(unconfined) {{ userns, }}\n'
                )
                subprocess.run(["sudo", "apparmor_parser", "-r", profile], check=True)
                loaded = True
            runtime = root / "run"
            runtime.mkdir(mode=0o700)
            listener.bind(str(runtime / "control.sock"))
            listener.listen(8)
            listener.settimeout(0.2)

            def serve():
                while not finished.is_set():
                    try:
                        connection, _ = listener.accept()
                    except TimeoutError:
                        continue
                    try:
                        with connection:
                            connection.settimeout(2)
                            length = struct.unpack(">I", receive(connection, 4))[0]
                            if length > 65536:
                                raise RuntimeError("Oversized synthetic IPC frame")
                            request = json.loads(receive(connection, length))
                            operation = request["op"]
                            if operation == "status":
                                result = {
                                    "created": False,
                                    "unlocked": False,
                                    "pin": False,
                                    "keys": [],
                                    "workspaces": [],
                                    "accounts": [],
                                    "categories": [],
                                    "idleMinutes": 15,
                                    "version": "0.5.15",
                                    "notifications": {
                                        "detail": "private",
                                        "doNotDisturb": False,
                                        "mutedAccounts": [],
                                    },
                                }
                            elif operation in ("update.status", "update.startup"):
                                result = {"phase": "current", "canCheck": False}
                            elif operation == "backup.status":
                                result = {"phase": "idle"}
                            elif operation == "activity":
                                result = None
                            else:
                                raise RuntimeError(
                                    "Unexpected synthetic controller operation"
                                )
                            payload = json.dumps({"result": result}).encode()
                            connection.sendall(
                                struct.pack(">I", len(payload)) + payload
                            )
                            seen.add(operation)
                    except Exception as error:
                        errors.append(error)
                        finished.set()

            worker = threading.Thread(target=serve)
            worker.start()
            environment = os.environ.copy()
            environment.pop("LD_LIBRARY_PATH", None)
            environment.pop("APPDIR", None)
            for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
                directory = root / name
                directory.mkdir(mode=0o700)
                environment[name] = str(directory)
            environment["GIO_USE_VFS"] = "local"
            environment["GTK_THEME"] = "Adwaita:dark"
            with (root / "startup.log").open("wb") as log:
                process = subprocess.Popen(
                    [
                        str(appdir / "AppRun"),
                        "--ui",
                        str(runtime),
                        str(root / "profile"),
                    ],
                    env=environment,
                    stdout=log,
                    stderr=log,
                    start_new_session=True,
                )
                deadline = time.monotonic() + 25
                while not {"status", "update.startup"}.issubset(seen):
                    if errors:
                        raise errors[0]
                    if process.poll() is not None:
                        raise RuntimeError(
                            f"Packaged shell exited: {process.returncode}"
                        )
                    if time.monotonic() >= deadline:
                        raise RuntimeError(
                            "Packaged shell did not initialize native IPC"
                        )
                    time.sleep(0.1)
                process.send_signal(signal.SIGTERM)
                if process.wait(timeout=10) != 0:
                    raise RuntimeError("Packaged shell did not close normally")
            print("PASS: packaged CEF shell initializes native IPC and closes normally")
        finally:
            if process and process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            finished.set()
            if "worker" in locals():
                worker.join(timeout=3)
            listener.close()
            if loaded:
                subprocess.run(["sudo", "apparmor_parser", "-R", profile], check=True)


if __name__ == "__main__":
    main()
