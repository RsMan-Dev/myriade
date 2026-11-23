#!/usr/bin/env python3
"""Compare mob flight-effect fall distance with the pinned server's actual aiStep.

The Java cow runs through real travel/movement, stopping before inside-block effects.
Its fluid query is fixed to dry air; active effects are controlled and NoAI is real.
Horizon runs one ECS tick of a cow on a dedicated server, in air filled by the harness.
Gravity is disabled. No effect, Levitation and Slow Falling are compared for both
NoAI states and zero/upward/downward movement. Positions, damage and landing are
outside this scoped comparison.
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resources', type=Path,
                        default=Path(os.environ.get('HORIZON_TEST_RESOURCES', lab.RESOURCES)))
    args = parser.parse_args()
    resources = args.resources.resolve()
    if not (resources / 'entity-types.json').is_file():
        raise SystemExit('prepared resources required; pass --resources PATH')
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'rust-mob-fall-reset-parity'
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    java_root = lab.ROOT / 'mods/horizon/src/test/java'
    oracle = java_root / 'dev/horizonlab/parity/VanillaMobFallResetOracle.java'
    sources = [Path(__file__), oracle, lab.ROOT / 'tools/lab.py',
               lab.ROOT / 'tools/oracle_harness.py', lab.ROOT / 'tools/test_rust_parity.py',
               lab.ROOT / 'tools/server_lab.py', lab.ROOT / 'Cargo.toml', lab.ROOT / 'Cargo.lock',
               lab.ROOT / 'crates/horizon-client/src/game/interface/dedicated/command_ops_air_fall.rs',
               lab.ROOT / 'crates/horizon-ecs-proto/src/fall_reset.rs',
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
                        'dev.horizonlab.parity.VanillaMobFallResetOracle', build / 'server.txt'],
                       build, 'server')
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'],
                   cwd=lab.ROOT, check=True, timeout=900,
                   stdout=(build / 'compile-rust.log').open('w'), stderr=subprocess.STDOUT)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', resources, {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        server.command('fill 0 60 0 5 70 5 minecraft:air')
        rows = server.op('Player', {'op': 'mob_fall_reset'})
    lines = [f"{r['effect']} {'true' if r['no_ai'] else 'false'} {r['velocity_y']} {r['fall_distance']}"
             for r in rows]
    (build / 'rust.txt').write_text('\n'.join(lines) + '\n')
    matched = harness.compare(build / 'server.txt', build / 'rust.txt', 18)
    base.ensure_sources_unchanged(sources, hashes)
    lab.write_json(report, dict(schema=1, minecraft=lock['minecraft']['version'],
        sources_sha256=hashes, server_jar_sha256=lab.digest(jars['server'], 'sha256'),
        matched_output_lines=matched, output_sha256=lab.digest(build / 'rust.txt', 'sha256'),
        scope=__doc__))
    print(f'{matched} mob fall-distance cases match the pinned server: {report}', flush=True)


if __name__ == '__main__':
    main()
