#!/usr/bin/env python3
"""Compare the villager murder witnesses with the real Java 26.3 sensor, on the whole server.

The Java side runs VanillaVillagerWitnessOracle (NearestVisibleLivingEntities on isolated villagers); the Rust side is
`op villager_witness` on a dedicated server, through a bot joined as Player. Rows: witnesses, cached, fresh. The Rust
side has no scan cache: a row whose vanilla value depends on the age of the scan is printed under DELIBERATE_GAPS with
its reason and does not fail the run. The game binary must already be built (tools/parity.py builds it).
"""
import argparse
import json
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

ROWS = ['witnesses', 'cached', 'fresh']
DELIBERATE_GAPS = {
    'cached': 'vanilla keeps the scan of the first murder after two villagers change visibility; the ECS reads visibility at once (no scan cache per villager)',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--classpath', help='already verified vanilla server and libraries')
    args = parser.parse_args()
    build = lab.LOCAL / 'villager-witness-parity'
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
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaVillagerWitnessOracle.java'
    sources = [Path(__file__), oracle,
               lab.ROOT / 'crates/horizon-ecs-proto/src/behaviours/gossip.rs',
               lab.ROOT / 'crates/myriade-engine/src/game/interface/dedicated/command_ops_witness.rs']
    hashes = base.source_hashes(sources)

    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w') as log:
            subprocess.run(list(map(str, command)), stdout=log,
                           stderr=subprocess.STDOUT, check=True, timeout=240, **kwargs)
    run(['javac', '--release', major, '-proc:none', '-cp', cp, '-d', classes, oracle], 'compile-java')
    expected_path = build / 'java.txt'
    expected_path.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix='hz-villager-witness-') as scratch:
        env = game_environment(os.environ, scratch)
        run(['java', '-Xmx1G', '-cp', str(classes) + os.pathsep + cp,
             'dev.horizonlab.parity.VanillaVillagerWitnessOracle', expected_path], 'java', env=env)
    expected = {}
    for line in expected_path.read_text(encoding='utf-8').splitlines():
        label, ids = line.split(' ', 1)
        expected[label] = json.loads(ids)
    if list(expected) != ROWS:
        raise AssertionError('Missing/extra actual Java records')
    bins = lab.LOCAL / 'parity-target' / 'fast'
    if not lab.binary(bins, 'myriade-engine').exists():
        raise RuntimeError(f'no game binary in {bins}: build it with tools/parity.py first')
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        actual = server.op('Player', {'op': 'villager_witness'})
    gaps = []
    for label in ROWS:
        want, got = sorted(expected[label]), sorted(actual[label])
        if want == got:
            continue
        if label not in DELIBERATE_GAPS:
            raise AssertionError(f'{label}: vanilla {want}, Rust {got}')
        gaps.append(f'{label}: vanilla {want}, Rust {got}: {DELIBERATE_GAPS[label]}')
    base.ensure_sources_unchanged(sources, hashes)
    report.write_text(json.dumps({'passed': True, 'java': expected, 'rust': actual, 'deliberate_gaps': gaps}, indent=2) + '\n')
    print('\n'.join(['DELIBERATE_GAPS:', *gaps]) if gaps else 'DELIBERATE_GAPS: none')
    print(f'villager witnesses: {len(ROWS) - len(gaps)} of {len(ROWS)} Java/Rust rows match')


if __name__ == '__main__':
    main()
