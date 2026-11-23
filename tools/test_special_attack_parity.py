#!/usr/bin/env python3
"""Compare a player's attack on a special target, on the whole server.

The bot hits a boat, minecart, armor stand, dragon or hanging entity through the same commands on a vanilla dedicated
server and on Horizon's. After the hit the bot reads what the server shows: the player's food exhaustion, the player's
and the target's motion, the target's health and the sprint bit of the bot's own flags. Falls are scripted by the bot:
it sets its position each game tick with vanilla's free fall (gravity 0.08, drag 0.98) and attacks on the last airborne
tick, so the server computes the same fall distance on both sides. Attack delays are the game ticks between the hits.
The controls with zero attack damage or weakness compare the attacker only: their target outcome is not asserted equal.
No window, renderer or audio device is started.
"""
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

try:
    from . import lab, test_rust_parity as base
    from .bot_lab import Bot
    from .server_lab import Server
    from . import test_dedicated_server_parity as dedicated
except ImportError:
    import lab
    import test_rust_parity as base
    from bot_lab import Bot
    from server_lab import Server
    import test_dedicated_server_parity as dedicated

PLAYER = dedicated.PLAYER
GROUND = 100.0
GRAVITY = 0.08
DRAG = 0.98
TARGETS = {'boat': 'minecraft:oak_boat', 'cart': 'minecraft:minecart', 'stand': 'minecraft:armor_stand',
           'dragon': 'minecraft:ender_dragon', 'painting': 'minecraft:painting', 'frame': 'minecraft:item_frame',
           'filled_frame': 'minecraft:item_frame', 'fixed_frame': 'minecraft:glow_item_frame'}
HANGING = ('painting', 'frame', 'filled_frame', 'fixed_frame')
ITEMS = {'-': 'minecraft:air', 'sword': 'minecraft:stone_sword',
         'knockback': 'minecraft:stick[enchantments={levels:{"minecraft:knockback":2}}]',
         'mace': 'minecraft:mace',
         'wind_burst': 'minecraft:mace[enchantments={levels:{"minecraft:wind_burst":1}}]',
         'density': 'minecraft:mace[enchantments={levels:{"minecraft:density":5}}]',
         'breach': 'minecraft:mace[enchantments={levels:{"minecraft:breach":4}}]'}
SETUP = [
    'gamemode {mode}',
    'fill -4 99 -4 4 99 4 minecraft:stone',
    'fill -4 100 -4 4 104 4 minecraft:air',
    'fill -1 100 1 1 104 1 minecraft:stone',
    'kill @e[type=!minecraft:player]',
    'effect clear {player}',
    'tp {player} 0.5 {y} -2 0 0',
    'attribute {player} minecraft:attack_damage base set {damage}',
    'item replace entity {player} weapon.mainhand with {item}',
    'item replace entity {player} hotbar.1 with ' + ITEMS['sword'],
]
WEAKNESS = ['effect give {player} minecraft:weakness 30 0 true']
HEIGHTS = (1.5, 3, 5, 8, 12)
FALL_ITEMS = ('mace', 'wind_burst', 'density', 'breach')
DELAYS = (1, 5, 10, 12, 20)


def base_case(**changes):
    case = dict(target='stand', creative=False, sprint=False, ticks=0, hits=1, item='-', damage=1, weakness=False,
                tail_only=False, height=0, delay=0, mode=None)
    case.update(changes)
    return case


def cases():
    for target in TARGETS:
        for creative in (False, True):
            for sprint in (False, True):
                for ticks in (0, 20):
                    for knockback in (False, True):
                        case = base_case(target=target, creative=creative, sprint=sprint, ticks=ticks,
                                         item='knockback' if knockback else '-')
                        yield case
                        if target == 'stand':
                            yield {**case, 'hits': 2}
    # Player.attack tests base/magic damage BEFORE the mace's item bonus: none of these may pay the new tail.
    for control in (dict(damage=0, item='knockback'), dict(weakness=True, item='-'), dict(damage=0, item='mace')):
        for sprint in (False, True):
            for ticks in (0, 20):
                yield base_case(sprint=sprint, ticks=ticks, tail_only=True, **control)
    for height in HEIGHTS:
        for item in FALL_ITEMS:
            yield base_case(height=height, item=item)
    for mode in ('attack', 'switch'):
        for delay in DELAYS:
            yield base_case(item='sword' if mode == 'attack' else '-', delay=delay, mode=mode)


def numbers(text):
    return [float(v) for v in re.findall(r'-?\d+(?:\.\d+)?(?:E-?\d+)?', text.split(':', 1)[-1])]


def read_data(bot, selector, path):
    """The numbers of `data get entity SELECTOR PATH` as the bot reads them in the server's answer, or None."""
    since = bot.state()['tick']
    bot.command(f'data get entity {selector} {path}')
    bot.wait_ticks(1)
    texts = [m['plain'] for m in bot.messages(since=since)['messages']]
    values = numbers(texts[-1]) if texts else []
    return [round(v, 6) for v in values] or None


def target_id(bot, kind):
    for entity in bot.entities(entity_type=TARGETS[kind], radius=8)['entities']:
        return entity['id']
    raise SystemExit(f'no {kind} in reach')


def sprint_bit(bot):
    flags = [d['value'] for d in bot.state()['data'] if d['index'] == 0]
    return bool(flags and isinstance(flags[0], int) and flags[0] & 0x08)


def fall_path(height):
    """The y of each airborne game tick of a free fall from `height` above the ground, then the ground itself."""
    y, speed, path = GROUND + height, 0.0, []
    while True:
        speed = (speed - GRAVITY) * DRAG
        y += speed
        if y <= GROUND:
            return path + [GROUND]
        path.append(y)


def fall_and_hit(bot, entity, height):
    """Falls with the bot's position set each game tick; hits on the last airborne tick, before the landing."""
    path = fall_path(height)
    for i, y in enumerate(path):
        bot.move(0.5, y, -2, on_ground=i == len(path) - 1)
        bot.wait_ticks(1)
        if i == len(path) - 2:
            bot.attack(entity)


def episode(bot, case):
    lines = [line.format(mode='creative' if case['creative'] else 'survival', player=PLAYER,
                         damage=case['damage'], item=ITEMS[case['item']], y=GROUND + case['height'])
             for line in SETUP]
    if case['weakness']:
        lines += [line.format(player=PLAYER) for line in WEAKNESS]
    for line in lines:
        bot.command(line)
    if case['target'] in HANGING:
        bot.command(f'summon {TARGETS[case["target"]]} 0 101 0 {{facing:2b}}')
    else:
        bot.command(f'summon {TARGETS[case["target"]]} 0.5 100 0.5')
    bot.wait_ticks(2)
    bot.set_carried_item(0)
    entity = target_id(bot, case['target'])
    selector = f'@e[type={TARGETS[case["target"]]},sort=nearest,limit=1]'
    exhaustion = read_data(bot, PLAYER, 'foodExhaustionLevel')
    bot.player_input(forward=True, sprint=case['sprint'])
    bot.wait_ticks(1)
    if case['height']:
        fall_and_hit(bot, entity, case['height'])
    elif case['delay']:
        bot.attack(entity)
        if case['mode'] == 'switch':
            bot.set_carried_item(1)
        bot.wait_ticks(case['delay'])
        bot.attack(entity)
    else:
        if case['ticks']:
            bot.attack(entity)
            bot.wait_ticks(case['ticks'])
        for _ in range(case['hits']):
            bot.attack(entity)
    bot.player_input()
    bot.wait_ticks(1)
    after = read_data(bot, PLAYER, 'foodExhaustionLevel')
    record = dict(exhaustion=None if None in (exhaustion, after) else [round(after[0] - exhaustion[0], 6)],
                  player=read_data(bot, PLAYER, 'Motion'), sprint=sprint_bit(bot))
    if not case['tail_only']:
        record.update(target=read_data(bot, selector, 'Motion'), health=read_data(bot, selector, 'Health'))
    return record


def run_side(bot, label):
    records = []
    for case in cases():
        records.append(episode(bot, case))
    print(f'{label}: {len(records)} cases run')
    return records


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'special-attack-parity'
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    dependencies = ':'.join(map(str, [jars['server'], *libraries]))
    vanilla_command = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'myriade-engine', '--bin', 'myriade-engine'],
                   cwd=lab.ROOT, check=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    bins = Path(metadata['target_directory']) / 'fast'

    vanilla_dir = build / 'vanilla'
    port = dedicated.free_port()
    dedicated.write_server_dir(vanilla_dir, port)
    server = dedicated.Server(vanilla_command, vanilla_dir, 'vanilla-special-attack')
    try:
        server.wait_for(r'Done \(')
        server.console(f'op {PLAYER}')
        with Bot(timeout=600) as bot:
            bot.join('127.0.0.1', port, PLAYER)
            bot.wait_for('loaded', 300)
            vanilla = run_side(bot, 'vanilla')
    finally:
        server.stop()

    with Server(bins, build / 'horizon', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        server.command(f'op {PLAYER}')
        bot.join('127.0.0.1', server.port, PLAYER)
        bot.wait_for('loaded', 300)
        horizon = run_side(bot, 'Horizon')

    fixtures = list(cases())
    assert len(fixtures) == 186 and len(vanilla) == len(horizon) == 186
    differences = []
    for case, a, b in zip(fixtures, vanilla, horizon):
        if case['tail_only']:
            a, b = {k: a[k] for k in ('exhaustion', 'player', 'sprint')}, {k: b[k] for k in ('exhaustion', 'player', 'sprint')}
        elif case['target'] in ('boat', 'cart', 'stand', 'dragon') and not case['creative']:
            assert a['exhaustion'] is not None and a['exhaustion'][0] > 0, ('oracle did not accept', case, a)
        if a != b:
            differences.append(dict(case=case, vanilla=a, horizon=b))
    (build / 'differences.json').write_text(json.dumps(differences, indent=2), encoding='utf-8')
    if differences:
        sys.exit(f'{len(differences)}/{len(fixtures)} differ: {differences[0]}')
    shutil.rmtree(vanilla_dir, ignore_errors=True)
    shutil.rmtree(build / 'horizon', ignore_errors=True)
    print(f'{len(fixtures)}/{len(fixtures)} special-target attacks match the vanilla server')


if __name__ == '__main__':
    main()
