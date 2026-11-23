#!/usr/bin/env python3
"""Compare the air and frozen metadata of living creatures with the real 26.3 setters, on the whole server.

The Java side runs VanillaMobAirOracle (setAirSupply and setTicksFrozen, spawn and dirty values); the Rust side
is `op mob_air` on a dedicated server, through a bot joined as Player. Compares the record of every sequence:
the metadata sent and the air and frozen values after each step.
"""
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
    rng = random.Random(0x414952)
    for kind, maximum in [('cow', 300), ('cod', 300), ('dolphin', 4800), ('axolotl', 6000)]:
        sequences = [[(maximum, 0)], [(100, -3)],
                     [(maximum, 0), (100, -3), (100, -3), (maximum, 0), (maximum, 0)],
                     [(-2147483648, -2147483648), (2147483647, 2147483647), (maximum, 0)]]
        for _ in range(64):
            sequence = []
            for _ in range(16):
                sequence.append(rng.choice([(maximum, 0), (100, -3), (-20, 140),
                                            (rng.randrange(-100, 7000), rng.randrange(-100, 300))]))
            sequences.append(sequence)
        for sequence in sequences:
            yield kind + ' ' + ' '.join(f'{air},{frozen}' for air, frozen in sequence)


def main():
    lock, jars, libraries = base.verify_artifacts()
    cp = os.pathsep.join(map(str, [jars['server'], *libraries]))
    build = lab.LOCAL / 'mob-air-parity'
    build.mkdir(parents=True, exist_ok=True)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    rows = list(cases())
    inputs = build / 'inputs.txt'
    inputs.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaMobAirOracle.java'

    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w') as log:
            subprocess.run(list(map(str, command)), stdout=log,
                           stderr=subprocess.STDOUT, check=True, timeout=900, **kwargs)

    run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-cp', cp, '-d', classes,
         oracle, oracle.with_name('ParityData.java')], 'compile-java')
    expected_path = build / 'java.txt'
    run(['java', '-Xmx1G', '-cp', str(classes) + os.pathsep + cp,
         'dev.horizonlab.parity.VanillaMobAirOracle', inputs, expected_path], 'java')
    run(['cargo', 'build', '--profile', 'fast', '-p', 'myriade-engine', '--bin', 'myriade-engine'], 'compile-rust',
        cwd=lab.ROOT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        actual = server.op('Player', {'op': 'mob_air', 'cases': rows})
    expected = expected_path.read_text(encoding='utf-8').splitlines()
    assert len(expected) == len(rows) == len(actual), (len(expected), len(rows), len(actual))
    for line, want, got in zip(rows, expected, actual):
        assert want == got, (line, want, got)
    print(f'mob air: {len(rows)} Java/Rust sequences match on the dedicated server')


if __name__ == '__main__':
    main()
