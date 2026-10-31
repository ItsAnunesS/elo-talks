#!/usr/bin/env python3
"""Stateful GitHub CLI fixture used by publication integration tests."""
import json
import os
from pathlib import Path
import sys

state_path = Path(os.environ["FAKE_STATE"])
state = json.loads(state_path.read_text())
arguments = sys.argv[1:]
state["calls"].append(arguments)

if arguments[0] == "api":
    if "/compare/" in arguments[1]:
        print(json.dumps({"status": state.get("comparison", "identical")}))
    elif state.get("lookup_denied") and "/releases/tags/" in arguments[1]:
        print("HTTP 403: Resource not accessible by integration", file=sys.stderr)
        state_path.write_text(json.dumps(state))
        sys.exit(1)
    elif "--slurp" in arguments:
        print(json.dumps([state["releases"]]))
    elif arguments[1].endswith("/latest"):
        print(
            json.dumps(
                next(
                    release
                    for release in state["releases"]
                    if release["tag_name"] == state["latest"]
                )
            )
        )
    else:
        tag = arguments[1].split("/")[-1]
        release = next(
            (release for release in state["releases"] if release["tag_name"] == tag),
            None,
        )
        if release:
            print(json.dumps(release))
        else:
            state_path.write_text(json.dumps(state))
            print("HTTP 404: Not Found", file=sys.stderr)
            sys.exit(1)
elif arguments[:2] == ["release", "create"]:
    if state.get("creation_denied"):
        print("HTTP 403: Resource not accessible by integration", file=sys.stderr)
        state_path.write_text(json.dumps(state))
        sys.exit(1)
    state["releases"].append(
        {
            "tag_name": arguments[2],
            "target_commitish": arguments[arguments.index("--target") + 1],
            "draft": True,
            "prerelease": False,
        }
    )
elif arguments[:2] == ["release", "edit"]:
    release = next(
        release for release in state["releases"] if release["tag_name"] == arguments[2]
    )
    if "--draft=false" in arguments:
        release["draft"] = False
    if "--prerelease=true" in arguments:
        release["prerelease"] = True
    if "--latest" in arguments:
        state["latest"] = arguments[2]

state_path.write_text(json.dumps(state))
