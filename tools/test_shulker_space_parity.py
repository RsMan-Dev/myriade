#!/usr/bin/env python3
"""Compare native shulker attachment queries with Horizon's production block queries, on the whole server."""
import json
from pathlib import Path
try:
    from . import lab, oracle_harness as harness, test_rust_parity as base
    from .bot_lab import Bot
    from .server_lab import Server
except ImportError:
    import lab, oracle_harness as harness, test_rust_parity as base
    from bot_lab import Bot
    from server_lab import Server


def main():
    lock, directory = lab.config()
    manifest, client = lab.import_client(lock, directory)
    natives = {v['downloads']['artifact']['path'] for v in manifest['libraries']
               if v['name'].split(':')[-1].startswith('natives-')}
    libraries = [lab.import_library(a, directory / 'libraries/client')
                 for a in lab.client_library_artifacts(manifest) if a['path'] not in natives]
    build = lab.LOCAL / 'rust-shulker-space-parity'
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    sources = [Path(__file__), lab.ROOT / 'tools/lab.py', lab.ROOT / 'tools/oracle_harness.py',
               lab.ROOT / 'tools/test_rust_parity.py', lab.ROOT / 'tools/integration_env.py',
               lab.ROOT / 'tools/server_lab.py', lab.ROOT / 'tools/bot_lab.py',
               lab.ROOT / 'Cargo.toml', lab.ROOT / 'Cargo.lock',
               lab.ROOT / 'crates/horizon-client/src/game/interface/dedicated/command_ops_leash_space.rs',
               lab.ROOT / 'crates/horizon-ecs-proto/src/behaviours/teleport_on.rs',
               lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaShulkerSpaceOracle.java',
               lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaBlockUpdateOracle.java',
               lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/ParityData.java']
    hashes = base.source_hashes(sources)
    rows = list(cases())
    inputs = build / 'inputs.txt'
    inputs.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    input_hash = lab.digest(inputs, 'sha256')
    classes, dependencies, _ = harness.compile_oracle(build, 'VanillaShulkerSpaceOracle',
                                                     {'client': client}, libraries, lock['minecraft']['java_major'])
    harness.java_run(build, classes, dependencies, 'VanillaShulkerSpaceOracle',
                     [inputs, build / 'client.txt'], 'client')
    harness.run_logged(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'],
                       build, 'compile-rust', timeout=900)
    bins = Path(lab.cargo_target()) / 'fast'
    with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        bot.join('127.0.0.1', server.port, 'Player')
        bot.wait_for('loaded', 300)
        results = server.op('Player', {'op': 'shulker_space', 'rows': [json.loads(r) for r in rows]})
    (build / 'rust.txt').write_text(''.join('true\n' if r else 'false\n' for r in results), encoding='utf-8')
    matched = harness.compare(build / 'client.txt', build / 'rust.txt', len(rows))
    base.ensure_sources_unchanged(sources, hashes)
    if lab.digest(inputs, 'sha256') != input_hash:
        raise AssertionError('Input fixtures changed')
    lab.write_json(report, dict(schema=1, minecraft=lock['minecraft']['version'], sources_sha256=hashes,
        client_jar_sha256=lab.digest(client, 'sha256'), input_sha256=input_hash,
        matched_output_lines=matched, output_sha256=lab.digest(build / 'rust.txt', 'sha256'),
        scope='Actual native Shulker.canStayAt, loadedAndEntityCanStandOnFace and noCollision on a recording level, '
              'compared with the ECS attachment query of the whole dedicated server (`op shulker_space`). '
              'Five support blocks,all six faces,blocked targets,nine current heights and three chunk-boundary positions. '
              'Real vanilla block data on the server. '
              'No nearby entities,world border,random teleport destination selection,AI ticks,networking or rendered interpolation comparison.'))
    print(f'{matched} shulker attachment cases match the pinned client: {report}', flush=True)


if __name__ == '__main__':
    main()
