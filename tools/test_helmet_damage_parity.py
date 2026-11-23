#!/usr/bin/env python3
"""Compare armour wear and armour reduction on a hit, on the whole server, through the same commands on both sides.

Each fixture row equips the player (helmet, boots, Unbreaking, unbreakable), heals it, lets the hurt cooldown pass,
hits it with `/damage` (the row's damage type, 8 damage) and reads its health and slots with `data get`. The vanilla
side runs VanillaCommandExecOracle on the script; the Horizon side runs the same script through run_as_player on a
dedicated server with a bot joined as Player. Rows without Unbreaking are exact: their records must be equal line by
line. Unbreaking draws come from the level random, which no command seeds: those rows repeat the hit STAT_REPS times
and compare the frequency of a head durability loss, which must agree within TOLERANCE on both servers.
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

FIXTURE = lab.ROOT / 'tests/fixtures/helmet_damage.json'
SEED = 1
HURT_COOLDOWN_TICKS = 20
HIT = 8
STAT_REPS = 200
TOLERANCE = 0.1


def components(damage, unbreaking, unbreakable=False):
    parts = []
    if damage:
        parts.append(f'damage={damage}')
    if unbreaking:
        parts.append(f'enchantments={{levels:{{"minecraft:unbreaking":{unbreaking}}}}}')
    if unbreakable:
        parts.append('unbreakable={}')
    return f'[{",".join(parts)}]' if parts else ''


def cmd(text, stat=None):
    op = {'op': 'cmd', 'cmd': text, 'label': text, 'admission': True}
    if stat:
        op['stat'] = stat
    return op


def episode(row, stat=None):
    """One equip-hit-read round; `stat` = [row index, repetition] tags the head reads of a statistic."""
    ops = [
        cmd('/gamemode survival @s'),
        cmd('/effect clear @s'),
        cmd('/item replace entity @s armor.head with air'),
        cmd('/item replace entity @s armor.feet with air'),
        cmd('/effect give @s minecraft:instant_health 1 255'),
        {'op': 'tick', 'n': HURT_COOLDOWN_TICKS},
    ]
    if row['head']:
        head = f"minecraft:{row['head']}" + components(row['head_damage'], row['unbreaking'], row.get('unbreakable'))
        ops.append(cmd(f'/item replace entity @s armor.head with {head}'))
    if row.get('boots_damage') is not None:
        feet = 'minecraft:iron_boots' + components(row['boots_damage'], row['unbreaking'])
        ops.append(cmd(f'/item replace entity @s armor.feet with {feet}'))
    head_read = '/data get entity @s Inventory[{Slot:103b}]'
    ops.append(cmd(head_read, stat and stat + ['before']))
    ops.append(cmd(f"/damage @s {HIT} minecraft:{row['source']}"))
    ops.append(cmd('/data get entity @s Health'))
    ops.append(cmd(head_read, stat and stat + ['after']))
    ops.append(cmd('/data get entity @s Inventory[{Slot:100b}]'))
    ops.append({'op': 'tick', 'n': HURT_COOLDOWN_TICKS})
    return ops


def script(rows):
    ops = []
    for index, row in enumerate(rows):
        if row['unbreaking']:
            for rep in range(STAT_REPS):
                ops += episode(row, [index, rep])
        else:
            ops += episode(row)
    return ops


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'helmet-damage-parity'
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    rows = json.loads(FIXTURE.read_text(encoding='utf-8'))
    ops = script(rows)
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
    run(['cargo', 'build', '--profile', 'fast', '-p', 'myriade-engine', '--bin', 'myriade-engine'], 'compile-rust',
        cwd=lab.ROOT)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / 'myriade-engine'
    rust_dir = build / 'rust'
    rust_dir.mkdir()
    commands.run_as_player(binary, script_path, build / 'rust.jsonl', commands.SERVER_ENV, rust_dir)
    expected = (build / 'vanilla.jsonl').read_text(encoding='utf-8').splitlines()
    actual = (build / 'rust.jsonl').read_text(encoding='utf-8').splitlines()
    recorded = [op for op in ops if op['op'] == 'cmd']
    if len(expected) != len(recorded) or len(actual) != len(recorded):
        sys.exit(f'helmet damage record counts differ: {len(expected)} vanilla, {len(actual)} Horizon, '
                 f'{len(recorded)} commands')
    exact = [i for i, op in enumerate(recorded) if 'stat' not in op]
    for i in exact:
        if expected[i] != actual[i]:
            sys.exit(f'helmet damage differs at {recorded[i]["cmd"]}: vanilla {expected[i]}; Horizon {actual[i]}')
    pairs = {}
    for i, op in enumerate(recorded):
        if 'stat' in op:
            row, rep, phase = op['stat']
            pairs.setdefault(row, {}).setdefault(rep, {})[phase] = i
    for row, reps in sorted(pairs.items()):
        shares = []
        for lines in (expected, actual):
            losses = sum(lines[reps[r]['before']] != lines[reps[r]['after']] for r in reps)
            shares.append(losses / len(reps))
        print(f'row {row}: head durability lost in {shares[0]:.3f} vanilla, {shares[1]:.3f} Horizon, '
              f'{len(reps)} hits')
        if abs(shares[0] - shares[1]) > TOLERANCE:
            sys.exit(f'row {row}: durability loss frequency differs beyond {TOLERANCE}')
    print(f'helmet damage: {len(exact)} exact records match; {len(pairs)} Unbreaking rows within {TOLERANCE}')


if __name__ == '__main__':
    main()
