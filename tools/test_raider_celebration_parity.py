#!/usr/bin/env python3
"""Compare the raider celebration flag's metadata with the real 26.3 Raider setters, on the whole server.

The Java side runs VanillaRaiderCelebrationOracle (setCelebrating on six raider types, spawn and dirty values);
the Rust side is `op raider_celebration` on a dedicated server, through a bot joined as Player. Compares the
record of every sequence: the metadata sent and the flag after each set.
"""
import itertools
import os
from pathlib import Path
import random
import subprocess

try:
    from . import lab, test_rust_parity as base
    from .bot_lab import Bot
    from .server_lab import Server
except ImportError:
    import lab
    import test_rust_parity as base
    from bot_lab import Bot
    from server_lab import Server


def cases():
    sequences = [bits for size in range(1, 6) for bits in itertools.product((0, 1), repeat=size)]
    rng = random.Random(0x52414944)
    sequences.extend(tuple(rng.randrange(2) for _ in range(64)) for _ in range(16))
    for kind in ('pillager', 'vindicator', 'evoker', 'illusioner', 'witch', 'ravager'):
        for sequence in sequences:
            yield kind + ' ' + ' '.join(map(str, sequence))


def main():
    lock, jars, libraries = base.verify_artifacts()
    cp = os.pathsep.join(map(str, [jars['server'], *libraries]))
    build = lab.LOCAL / 'raider-celebration-parity'
    build.mkdir(parents=True, exist_ok=True)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    rows = list(cases())
    inputs = build / 'inputs.txt'
    inputs.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaRaiderCelebrationOracle.java'

    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w') as log:
            subprocess.run(list(map(str, command)), stdout=log,
                           stderr=subprocess.STDOUT, check=True, timeout=900, **kwargs)

    run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-cp', cp, '-d', classes,
         oracle], 'compile-java')
    expected_path = build / 'java.txt'
    run(['java', '-Xmx1G', '-cp', str(classes) + os.pathsep + cp,
         'dev.horizonlab.parity.VanillaRaiderCelebrationOracle', inputs, expected_path], 'java')
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'], 'compile-rust',
        cwd=lab.ROOT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        actual = server.op('Player', {'op': 'raider_celebration', 'cases': rows})
    expected = expected_path.read_text(encoding='utf-8').splitlines()
    assert len(expected) == len(rows) == len(actual), (len(expected), len(rows), len(actual))
    for line, want, got in zip(rows, expected, actual):
        assert want == got, (line, want, got)
    print(f'raider celebration: {len(rows)} Java/Rust sequences match on the dedicated server')


if __name__ == '__main__':
    main()
