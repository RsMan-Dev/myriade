#!/usr/bin/env python3
"""Compare adult/baby Happy Ghast leash dispatch and forces with pinned vanilla, on the whole server.

A pre-existing path and move-control target expose unwanted close-range navigation
and the elastic-pull callback. Cow cases retain the ordinary six/twelve-block leash.
The vanilla rows come from the Java reference; `op happy_ghast_leash` runs the same cases on the
dedicated server through a bot joined as Player. This checks one leash update, not a rendered flight trajectory.
"""
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile

try:
    from . import lab
    from .bot_lab import Bot
    from .integration_env import game_environment
    from .server_lab import Server
except ImportError:
    import lab
    from bot_lab import Bot
    from integration_env import game_environment
    from server_lab import Server


def main():
    work = Path(tempfile.mkdtemp(prefix="happy-ghast-leash-", dir=lab.LOCAL))
    env = game_environment(os.environ, work)
    env["JAVA_TOOL_OPTIONS"] = env.get("JAVA_TOOL_OPTIONS", "") + " -Dhorizon.happyGhastLeashReference=true"
    subprocess.run([sys.executable, str(lab.ROOT / "tools/test_command_exec_parity.py"),
                    "--only", "scoreboard", "--reference-only", "--work-dir", str(work)],
                   cwd=lab.ROOT, env=env, check=True)
    reference = work / "happy-ghast-leash-reference.json"
    rows = json.loads(reference.read_text())
    if len(rows) != 36 or sum(r["snapped"] for r in rows) != 7:
        raise SystemExit("vanilla fixture must cover 36 leash cases and seven snapped leads")
    subprocess.run(["cargo", "build", "--profile", "fast", "-p", "myriade-engine", "--bin", "myriade-engine"],
                   cwd=lab.ROOT, check=True)
    bins = Path(lab.cargo_target()) / "fast"
    with Server(bins, work / "server", lab.RESOURCES.resolve(), {"level-seed": "42", "view-distance": "4"}) as server, \
            Bot(timeout=600) as bot:
        bot.join("127.0.0.1", server.port, "Player")
        bot.wait_for("loaded", 300)
        actual = server.op("Player", {"op": "happy_ghast_leash", "rows": rows})
    assert len(actual) == len(rows) == 36, "missing leash cases"
    for want, got in zip(rows, actual):
        for key in ["kind", "distance", "phase", "snapped", "wanted", "path"]:
            assert want[key] == got[key], (key, want, got)
        for a, b in zip(want["velocity"], got["velocity"]):
            assert math.isclose(a, b, rel_tol=0, abs_tol=1e-10), (want, got)
    print("Happy Ghast leash: 36 adult/baby/cow dispatch, threshold and force cases match vanilla")


if __name__ == "__main__":
    main()
