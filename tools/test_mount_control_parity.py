#!/usr/bin/env python3
"""Compare pig/strider steering authority with the real 26.3 controlling-passenger methods, on the whole server.

The Java side runs VanillaCommandExecOracle, which writes the 32 vanilla rows of the controlling-passenger
methods; the Rust side is `op mount_control` on a dedicated server, through a bot joined as Player. Checks
controlled, server_simulates and forward for every row. Client prediction and the MoveVehicle packet are client
code and are not compared here.
"""
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile

try:
    from . import lab, test_rust_parity as base
    from .bot_lab import Bot
    from .integration_env import game_environment
    from .server_lab import Server
except ImportError:
    import lab
    import test_rust_parity as base
    from bot_lab import Bot
    from integration_env import game_environment
    from server_lab import Server


def main():
    lock, jars, libraries = base.verify_artifacts()
    cp = os.pathsep.join(map(str, [jars['server'], *libraries]))
    build = lab.LOCAL / 'mount-control-parity'
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaCommandExecOracle.java'

    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w') as log:
            subprocess.run(list(map(str, command)), stdout=log,
                           stderr=subprocess.STDOUT, check=True, timeout=900, **kwargs)

    run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-cp', cp, '-d', classes,
         oracle], 'compile-java')
    script = build / 'script.jsonl'
    script.write_text('')
    reference = build / 'mount-control-reference.json'
    reference.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix='hz-mount-control-') as scratch:
        env = game_environment(os.environ, scratch)
        run(['java', '-Xmx2G', '-Dhorizon.mountControlReference=true', '-cp', str(classes) + os.pathsep + cp,
             'dev.horizonlab.parity.VanillaCommandExecOracle', Path(scratch) / 'universe', script,
             build / 'vanilla.json', '1'], 'java', env=env)
    rows = json.loads(reference.read_text())
    if (len(rows) != 32 or sum(r['controlled'] for r in rows) != 4
            or sum(not r['server_simulates'] for r in rows) != 4):
        raise SystemExit('vanilla mount-control fixture did not cover 32 cases / four controllers')
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'], 'compile-rust',
        cwd=lab.ROOT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        actual = server.op('Player', {'op': 'mount_control'})
    assert len(actual) == len(rows) == 32, (len(actual), len(rows))
    for want, got in zip(rows, actual):
        for key in ['kind', 'saddled', 'first', 'hand', 'controlled', 'server_simulates']:
            assert want[key] == got[key], (key, want, got)
        assert math.isclose(want['forward'], got['forward'], rel_tol=0, abs_tol=1e-12), (want, got)
    report.write_text(json.dumps({'passed': True, 'cases': len(actual)}, indent=2) + '\n')
    print('mount control: 32 vanilla cases match on the dedicated server')


if __name__ == '__main__':
    main()
