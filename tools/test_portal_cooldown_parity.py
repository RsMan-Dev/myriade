#!/usr/bin/env python3
"""Compare the player's PortalCooldown after a nether portal handoff, on the whole server, through the same commands.

The player stands in a nether portal (the same setup as the command corpus's portal case), the portal op runs the
vanilla handoff on both sides, then `data get` reads PortalCooldown at the handoff and after each tick count. The vanilla
side runs VanillaCommandExecOracle on the script; the Horizon side runs it through run_as_player on a dedicated server.
Contact counters are not compared: they have no command or NBT readout.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

try:
    from . import lab, test_rust_parity as base, test_command_exec_parity as commands
except ImportError:
    import lab
    import test_rust_parity as base
    import test_command_exec_parity as commands

SEED = 1
READ = '/data get entity @s PortalCooldown'


def script():
    def cmd(text):
        return {'op': 'cmd', 'cmd': text, 'label': text, 'admission': True}

    ops = [
        cmd('/gamemode survival @s'),
        cmd('/fill 2 231 12 5 236 12 minecraft:obsidian'),
        cmd('/fill 3 232 12 4 235 12 minecraft:nether_portal[axis=x]'),
        cmd('/tp @s 3.5 232 12.5 0 0'),
        cmd('/forceload add 0 0'),
        {'op': 'portal'},
        {'op': 'settle'},
        cmd(READ),
    ]
    for ticks in (1, 100, 200, 300):
        ops += [{'op': 'tick', 'n': ticks}, cmd(READ)]
    return ops


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'portal-cooldown-parity'
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    ops = script()
    script_path = build / 'script.jsonl'
    script_path.write_text(''.join(json.dumps(op, ensure_ascii=False) + '\n' for op in ops), encoding='utf-8')
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaCommandExecOracle.java'
    classes = build / 'classes'
    classes.mkdir()
    dependency_path = os.pathsep.join(map(str, libraries))

    def run(command, name, **kwargs):
        with (build / (name + '.log')).open('w', encoding='utf-8') as log:
            result = subprocess.run(list(map(str, command)), stdout=log, stderr=subprocess.STDOUT, **kwargs)
        if result.returncode:
            tail = (build / (name + '.log')).read_text(errors='replace')[-3000:]
            sys.exit(f'{name} failed ({result.returncode}):\n{tail}')

    run(['javac', '--release', lock['minecraft']['java_major'], '-proc:none', '-cp',
         str(jars['server']) + os.pathsep + dependency_path, '-d', classes, oracle], 'compile-java')
    vanilla_dir = build / 'vanilla'
    vanilla_dir.mkdir()
    run(['java', '-Xmx3G', '-Duser.timezone=UTC', '-cp',
         os.pathsep.join([str(classes), str(jars['server']), dependency_path]),
         'dev.horizonlab.parity.VanillaCommandExecOracle', vanilla_dir / 'universe', script_path,
         build / 'vanilla.jsonl', str(SEED)], 'java', cwd=vanilla_dir)
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--bin', 'horizon-client'], 'compile-rust',
        cwd=lab.ROOT)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / 'horizon-client'
    rust_dir = build / 'rust'
    rust_dir.mkdir()
    commands.run_as_player(binary, script_path, build / 'rust.jsonl', commands.SERVER_ENV, rust_dir)
    expected = (build / 'vanilla.jsonl').read_text(encoding='utf-8').splitlines()
    actual = (build / 'rust.jsonl').read_text(encoding='utf-8').splitlines()
    if expected != actual:
        sys.exit(f'portal cooldown differs: {len(expected)} vanilla records, {len(actual)} Horizon records')
    print(f'portal cooldown: {len(expected)} records match the vanilla server')


if __name__ == '__main__':
    main()
