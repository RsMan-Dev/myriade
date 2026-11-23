#!/usr/bin/env python3
"""Compare actual Cow.baseTick air supply and drowning health with pinned vanilla.

Java controls eye immersion and active effect queries, then runs the real breathing
and health-damage path until tickEffects. Horizon runs on a dedicated server: the fluid
blocks are placed with fill, then `Server.op` runs one ECS tick per case on a cow and reads
its air and health back. Covers dry air, water and bubble
columns (including opposite eye/feet blocks), all Water Breathing/Conduit Power/Breath of the Nautilus combinations,
and air values around full supply and the drowning threshold. Full fluid movement,
effect expiry, network transport and specialized aquatic breathing are out of scope.
"""
import argparse
import os
from pathlib import Path
import subprocess
try:
    from . import lab, oracle_harness as harness, test_rust_parity as base
    from .bot_lab import Bot
    from .server_lab import Server
except ImportError:
    import lab, oracle_harness as harness, test_rust_parity as base
    from bot_lab import Bot
    from server_lab import Server

# The scenes of the oracle: air, water, and the bubble columns at the eye and feet blocks.
FILLS = [
    'fill 0 60 0 12 70 8 minecraft:air',
    'fill 6 63 2 7 67 3 minecraft:water',
    'fill 10 63 2 11 67 3 minecraft:bubble_column',
    'fill 6 63 6 7 64 7 minecraft:water',
    'fill 6 65 6 7 67 7 minecraft:bubble_column',
    'fill 10 63 6 11 64 7 minecraft:bubble_column',
    'fill 10 65 6 11 67 7 minecraft:water',
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resources', type=Path,
                        default=Path(os.environ.get('HORIZON_TEST_RESOURCES', lab.RESOURCES)))
    args = parser.parse_args()
    resources = args.resources.resolve()
    if not (resources / 'entity-types.json').is_file():
        raise SystemExit('prepared resources required; pass --resources PATH')
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'rust-mob-breathing-parity'
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    java_root = lab.ROOT / 'mods/horizon/src/test/java'
    oracle = java_root / 'dev/horizonlab/parity/VanillaMobBreathingOracle.java'
    sources = [Path(__file__), oracle, lab.ROOT / 'tools/lab.py',
               lab.ROOT / 'tools/oracle_harness.py', lab.ROOT / 'tools/test_rust_parity.py',
               lab.ROOT / 'tools/server_lab.py',
               lab.ROOT / 'Cargo.toml', lab.ROOT / 'Cargo.lock',
               lab.ROOT / 'crates/myriade-engine/src/game/interface/dedicated/command_ops_air_fall.rs',
               lab.ROOT / 'crates/horizon-ecs-proto/src/cow/fluids.rs',
               lab.ROOT / 'crates/horizon-ecs-proto/src/effects.rs',
               java_root / 'dev/horizonlab/parity/ParityData.java',
               java_root / 'dev/horizonlab/parity/VanillaBlockUpdateOracle.java']
    hashes = base.source_hashes(sources)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    harness.run_logged(['javac', '--release', lock['minecraft']['java_major'], '-proc:none',
                        '-sourcepath', java_root, '-cp', dependencies, '-d', classes, oracle],
                       build, 'compile-java')
    harness.run_logged(['java', '-Xmx512M', '-Djava.awt.headless=true', '-cp',
                        str(classes) + os.pathsep + dependencies,
                        'dev.horizonlab.parity.VanillaMobBreathingOracle', build / 'server.txt'],
                       build, 'server')
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'myriade-engine', '--bin', 'myriade-engine'],
                   cwd=lab.ROOT, check=True, timeout=900,
                   stdout=(build / 'compile-rust.log').open('w'), stderr=subprocess.STDOUT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', resources, {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        for line in FILLS:
            server.command(line)
        rows = server.op('Player', {'op': 'mob_breathing'})
    lines = [f"{r['scene']} {r['mask']} {r['air']} {r['air_after']} {r['health']}" for r in rows]
    (build / 'rust.txt').write_text('\n'.join(lines) + '\n')
    matched = harness.compare(build / 'server.txt', build / 'rust.txt', 280)
    base.ensure_sources_unchanged(sources, hashes)
    lab.write_json(report, dict(schema=1, minecraft=lock['minecraft']['version'],
        sources_sha256=hashes, server_jar_sha256=lab.digest(jars['server'], 'sha256'),
        matched_output_lines=matched, output_sha256=lab.digest(build / 'rust.txt', 'sha256'),
        scope=__doc__))
    print(f'{matched} mob breathing cases match the pinned server: {report}', flush=True)


if __name__ == '__main__':
    main()
