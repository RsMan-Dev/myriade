#!/usr/bin/env python3
"""Compare the fishing hook's collision with a Breeze against the real 26.3 projectile code, on the whole server's rule.

The Java side runs VanillaBreezeHookOracle (Projectile.hitTargetOrDeflectSelf on a FishingHook, pinned client jar as
its level); the Rust side is `op breeze_hook` on a dedicated server, through a bot joined as Player, which applies the
ECS deflection to the same inputs. Compares the velocity bits, the sounds, the lands, needsSync and the attachment of
every hit.
"""
import os
from pathlib import Path
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
    import random
    rng = random.Random(0x484f4f4b)
    velocities = [(0.0, 0.0, -0.0), (2.0, -0.4, 1.0), (-2.0, 3.0, -4.0),
                  (1e-100, -1e100, 0.5)]
    velocities.extend(tuple(rng.uniform(-50, 50) for _ in range(3)) for _ in range(128))
    sequences = [(0,), (1,), (0, 0), (0, 1), (0, 0, 1, 1, 0), (1, 0, 1, 0)]
    for i, velocity in enumerate(velocities):
        for sequence in sequences:
            yield ' '.join(map(str, (i - 64, *velocity, *sequence)))


def main():
    lock, directory = lab.config()
    manifest, client = lab.import_client(lock, directory)
    natives = {v['downloads']['artifact']['path'] for v in manifest['libraries']
               if v['name'].split(':')[-1].startswith('natives-')}
    libraries = [lab.import_library(a, directory / 'libraries/client')
                 for a in lab.client_library_artifacts(manifest) if a['path'] not in natives]
    build = lab.LOCAL / 'breeze-hook-parity'
    build.mkdir(parents=True, exist_ok=True)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    rows = list(cases())
    inputs = build / 'inputs.txt'
    inputs.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    parity = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity'
    cp = os.pathsep.join(map(str, [client, *libraries]))

    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w') as log:
            subprocess.run(list(map(str, command)), stdout=log,
                           stderr=subprocess.STDOUT, check=True, timeout=900, **kwargs)

    run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-cp', cp, '-d', classes,
         parity / 'VanillaBreezeHookOracle.java', parity / 'ParityData.java'], 'compile-java')
    expected_path = build / 'java.txt'
    run(['java', '-Xmx1G', '-cp', str(classes) + os.pathsep + cp,
         'dev.horizonlab.parity.VanillaBreezeHookOracle', inputs, expected_path], 'java')
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'], 'compile-rust',
        cwd=lab.ROOT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        actual = server.op('Player', {'op': 'breeze_hook', 'cases': rows})
    expected = expected_path.read_text(encoding='utf-8').splitlines()
    assert len(expected) == len(rows) == len(actual), (len(expected), len(rows), len(actual))
    for line, want, got in zip(rows, expected, actual):
        assert want == got, (line, want, got)
    print(f'breeze hook: {len(rows)} Java/Rust sequences match on the dedicated server')


if __name__ == '__main__':
    main()
