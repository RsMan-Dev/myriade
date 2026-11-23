#!/usr/bin/env python3
"""Monster spawners on a vanilla dedicated server and on Horizon's, case by case, tick by tick.

Each case clears a flat room, places a spawner with its block entity NBT (`setblock`), puts Alice (a joueur-automate
bot) at the case's position in its game mode, summons the case's mobs next to it, and steps the server one tick at a
time (`tick step`). Bob joins too. Each tick reads the spawner's block entity NBT (`data get block`) and every mob in
the room with its Pos and Rotation (the exact yaw bits). Level events and sounds Alice receives are captured for the
whole case. Both sides get the same commands; the transcripts must match line for line, and vanilla's must show mobs.

Not compared: the sound's pitch and seed (drawn from the level random, whose order is not pinned on a whole server),
and finalizeSpawn's outcomes beyond the mobs' Pos and Rotation.
"""
import argparse
import itertools
import json
import re
import time

try:
    from . import packet_lab, test_locator_bar_parity as locator
    from .bot_lab import Bot
    from .test_dedicated_server_parity import Server, game_time
except ImportError:
    import packet_lab
    import test_locator_bar_parity as locator
    from bot_lab import Bot
    from test_dedicated_server_parity import Server, game_time

NAME = 'spawner-parity'
SPAWNER = '0 -60 0'
ALICE, BOB = 'Alice', 'Bob'
FAR = '0.5 -60 40.5'
ROOM = 'distance=..16,type=!minecraft:player,type=!minecraft:item'
SERIAL = itertools.count()
# The same spawner in the other dimensions: the player and the commands go there through `execute in`.
DIM_CASES = [
    ('zombie-nether', 'the_nether', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 200, FAR),
    ('zombie-end', 'the_end', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 200, FAR),
]


def step(server, count, timeout=300):
    """Steps `count` ticks and returns when the game clock shows them: the console answers in order, so a time reply
    past the target means the steps ran (the clock does not move on its own while ticking is frozen)."""
    target = game_time(server, timeout) + count
    server.console(f'tick step {count}')
    deadline = time.time() + timeout
    while game_time(server, timeout) < target:
        if time.time() > deadline:
            raise AssertionError(f'{server.log_path.name}: {count} ticks did not run in {timeout} s')
        time.sleep(0.2)
# name, block entity NBT, Alice's position, her game mode, mobs summoned next to the spawner, ticks, Bob's position.
CASES = [
    ('generated-zombie', '{SpawnData:{entity:{id:"minecraft:zombie"}},SpawnPotentials:[],Delay:0s}',
     '0.5 -60 3.5', 'survival', [], 200, FAR),
    ('potentials-missing', '{SpawnData:{entity:{id:"minecraft:skeleton"}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 200, FAR),
    ('weighted-with-zero-weight', '{SpawnPotentials:[{data:{entity:{id:"minecraft:spider"}},weight:0},'
     '{data:{entity:{id:"minecraft:zombie"}},weight:5}],Delay:0s}', '0.5 -60 3.5', 'survival', [], 200, FAR),
    ('custom-light-rules', '{SpawnData:{entity:{id:"minecraft:zombie",'
     'custom_spawn_rules:{block_light_limit:[0,4],sky_light_limit:15}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 200, FAR),
    ('fixed-pos', '{SpawnData:{entity:{id:"minecraft:zombie",Pos:[0.5d,-60.0d,6.5d]}},Delay:0s}',
     '0.5 -60 3.5', 'survival', [], 200, FAR),
    ('not-an-entity', '{SpawnData:{entity:{id:"minecraft:not_an_entity"}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('capitalised-id', '{SpawnData:{entity:{id:"Zombie"}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('delay-minus-one', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:-1s}', '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('zero-delays', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s,MinSpawnDelay:0s,MaxSpawnDelay:0s}',
     '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('spawn-count-zero', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s,SpawnCount:0s}',
     '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('max-nearby-zero', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s,MaxNearbyEntities:0s}',
     '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('required-range-minus-one', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s,RequiredPlayerRange:-1s}',
     '0.5 -60 40.5', 'survival', [], 60, FAR),
    ('player-far-outside-range', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s}', '0.5 -60 30.5', 'survival', [], 60, FAR),
    ('player-spectator', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s}', '0.5 -60 3.5', 'spectator', [], 60, FAR),
    ('player-creative', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s}', '0.5 -60 3.5', 'creative', [], 60, FAR),
    ('nearby-cap-reached', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s,MaxNearbyEntities:2s}',
     '0.5 -60 3.5', 'survival', ['zombie', 'zombie', 'zombie'], 120, FAR),
    ('empty-spawner', '{Delay:0s}', '0.5 -60 3.5', 'survival', [], 60, FAR),
    ('two-players-in-range', '{SpawnData:{entity:{id:"minecraft:zombie"}},Delay:0s}', '0.5 -60 3.5', 'survival', [], 200,
     '1.5 -60 3.5'),
]


def observe(server, prefix):
    """One tick's reading of the spawner and of the mobs in the room, as text rows."""
    mark = next(SERIAL)
    start = len(server.lines)
    server.console(f'{prefix}data get block {SPAWNER}')
    server.console(f'{prefix}execute as @e[{ROOM}] run data get entity @s Pos')
    server.console(f'{prefix}execute as @e[{ROOM}] run data get entity @s Rotation')
    server.console(f'say OBS {mark}')
    server.wait_for(rf'OBS {mark}\b', timeout=120)
    answers = [m.group(1) for line in server.lines[start:]
               if (m := re.search(r'has the following (?:block|entity) data: (.*)$', line))]
    return answers


def case(server, alice, bob, nbt, pos, mode, mobs, ticks, bob_pos, dim=None):
    prefix = f'execute in minecraft:{dim} run ' if dim else ''
    server.console(f'execute in minecraft:overworld run tp {ALICE} 0.5 -60 40.5')
    server.console(f'{prefix}fill -16 -60 -16 16 -50 16 minecraft:air')
    server.console(f'{prefix}fill -16 -61 -16 16 -61 16 minecraft:stone')
    server.console(f'{prefix}kill @e[type=!minecraft:player]')
    server.console(f'{prefix}setblock {SPAWNER} minecraft:spawner{nbt} replace')
    for mob in mobs:
        server.console(f'{prefix}summon minecraft:{mob} 3.5 -60 3.5')
    server.console(f'{prefix}tp {ALICE} {pos}')
    server.console(f'gamemode {mode} {ALICE}')
    server.console(f'{prefix}tp {BOB} {bob_pos}')
    server.console(f'gamemode survival {BOB}')
    alice.capture('start', filter_ids=[packet_lab.LEVEL_EVENT, packet_lab.SOUND])
    rows = []
    for tick in range(ticks):
        step(server, 1)
        rows.append(f'tick {tick} ' + ' | '.join(observe(server, prefix)))
    levels, sounds = packet_lab.captured(alice.capture('stop')['packets'])
    return rows + [f'level {row}' for row in levels] + [f'sound {row}' for row in sounds]


def run(label, server_command, world, build):
    directory = build / label
    port = locator.server_dir(directory, world)
    server = Server(server_command, directory, label)
    transcript = {}
    try:
        server.wait_for(r'Done \(', timeout=600)
        server.console('tick freeze')
        server.console('forceload add -16 -16 16 16')
        server.console('say READY')
        server.wait_for(r'READY', timeout=120)
        with Bot(timeout=600) as alice, Bot(timeout=600) as bob:
            alice.join('127.0.0.1', port, ALICE)
            alice.wait_for('loaded', 300)
            bob.join('127.0.0.1', port, BOB)
            bob.wait_for('loaded', 300)
            for name, *rest in CASES:
                transcript[name] = case(server, alice, bob, *rest)
            for name, dim, *rest in DIM_CASES:
                transcript[name] = case(server, alice, bob, *rest, dim=dim)
            bob.leave()
            alice.leave()
    finally:
        server.stop()
    return transcript


def proves(vanilla):
    """Why vanilla's transcript does not show mobs in the room, or None."""
    if not any(re.search(r'\[-?[\d.]+d, ', row) for rows in vanilla.values() for row in rows):
        return 'no mob was ever in the room'
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    lock, build, vanilla_server, horizon_server, _client = locator.sides(NAME)
    world = locator.generate_world(vanilla_server, build)
    vanilla = run('vanilla', vanilla_server, world, build)
    why = proves(vanilla)
    if why:
        raise AssertionError(f'vanilla is no proof: {why}')
    horizon = run('horizon', horizon_server, world, build)
    differing = sorted(name for name in vanilla if vanilla[name] != horizon.get(name))
    (build / 'report.json').write_text(json.dumps({'minecraft': lock['minecraft']['version'], 'cases': len(vanilla),
                                                   'differing': differing}, indent=2) + '\n')
    for name in differing:
        for v, h in zip(vanilla[name], horizon[name]):
            if v != h:
                print(f'- {name}: {v}\n+ {name}: {h}')
                break
    if differing:
        raise AssertionError(f'{len(differing)} of {len(vanilla)} spawner cases differ from vanilla: {differing}')
    print(f'spawner: {len(vanilla)} cases identical to vanilla')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        raise SystemExit(f'{NAME}: {error}')
