#!/usr/bin/env python3
"""Select system NSS modules from the packaging host's libnss3 package."""
import pathlib
import subprocess

MODULES = ("libsoftokn3.so", "libfreeblpriv3.so", "libnssckbi.so")


def resolve(package_files):
    paths = [pathlib.Path(line) for line in package_files.splitlines()]
    modules = []
    for name in MODULES:
        candidates = [
            path for path in paths if path.name == name and path.is_absolute()
        ]
        if len(candidates) != 1 or not candidates[0].is_file():
            raise RuntimeError(f"Expected one installed libnss3 module: {name}")
        modules.append(str(candidates[0]))
    return modules


def main():
    try:
        files = subprocess.check_output(
            ["dpkg-query", "--listfiles", "libnss3"], text=True
        )
        modules = resolve(files)
    except (OSError, subprocess.CalledProcessError, RuntimeError) as error:
        raise SystemExit(f"Cannot package system NSS modules: {error}") from error
    print("\n".join(modules))


if __name__ == "__main__":
    main()
