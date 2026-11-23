#!/usr/bin/env python3
"""Compare the dragon's wing and head rules on a living mob with the real Java 26.3 methods, on the whole server.

The Java side runs VanillaDragonContactsOracle (knockBack and hurt on a list); the Rust side is `op dragon_contacts` on a
dedicated server, through a bot joined as Player. Checks health, resulting velocity and revenge attribution for seven
scenarios. Does not claim parity of the whole fight, AI pathfinding, or gear with post-attack enchantments.
"""
import argparse
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--classpath', help='already verified vanilla server and libraries')
    args = parser.parse_args()
    build = lab.LOCAL / 'dragon-contacts-parity'
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    cp = args.classpath
    major = '25'
    if not cp:
        lock, jars, libraries = base.verify_artifacts()
        cp = os.pathsep.join(map(str, [jars['server'], *libraries]))
        major = str(lock['minecraft']['java_major'])
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    source = lab.ROOT / 'mods/horizon/src/test/java'
    oracle = source / 'dev/horizonlab/parity'
    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w') as log:
            subprocess.run(list(map(str, command)), stdout=log,
                           stderr=subprocess.STDOUT, check=True, timeout=240, **kwargs)
    run(['javac', '--release', major, '-proc:none', '-cp', cp, '-d', classes,
         oracle / 'VanillaCommandExecOracle.java',
         oracle / 'VanillaDragonContactsOracle.java'], 'compile-java')
    script = build / 'script.jsonl'
    script.write_text('{"op":"dragon_contacts"}\n')
    expected_path = build / 'java.json'
    expected_path.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix='hz-dragon-contacts-') as scratch:
        env = game_environment(os.environ, scratch)
        run(['java', '-Xmx2G', '-cp', str(classes) + os.pathsep + cp,
             'dev.horizonlab.parity.VanillaCommandExecOracle',
             Path(scratch) / 'universe', script, expected_path, '1'], 'java', env=env)
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'], 'compile-rust', cwd=lab.ROOT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        actual = server.op('Player', {'op': 'dragon_contacts'})
    expected = json.loads(expected_path.read_text())
    assert len(actual) == len(expected) == 7
    for want, got in zip(expected, actual):
        for key in ['case', 'health', 'revenge']:
            assert want[key] == got[key], (key, want, got)
        for a, b in zip(want['velocity'], got['velocity']):
            assert math.isclose(a, b, rel_tol=0, abs_tol=1e-12), (want, got)
    report.write_text(json.dumps({'passed': True, 'cases': len(actual)}, indent=2) + '\n')
    print('dragon contacts: 7 Java/Rust cases match')


if __name__ == '__main__':
    main()
