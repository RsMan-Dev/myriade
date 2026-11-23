#!/usr/bin/env python3
"""Trial spawners on a vanilla dedicated server and on Horizon's, case by case, tick by tick.

Each case clears a room at the flat world's spawn and places a trial spawner with its block state and block entity
NBT (`setblock`). Alice (a joueur-automate bot) stands at the case's position in its game mode, takes the case's
omen effect when it has one; Bob joins too, far away or in range. Each tick the server steps one tick (`tick step`),
then reads the spawner's state (`execute if block`), its block entity NBT (`data get block`), and every mob in the
room with its Pos and Rotation (the exact yaw bits). Every `kill_every` ticks the room's mobs are killed, so waves
finish and cooldowns run out. Level events and sounds Alice receives are captured for the whole case. Both sides get
the same commands; the transcripts must match line for line, and vanilla's must show mobs.

Not compared: the sound's pitch and seed (drawn from the level random, whose order is not pinned on a whole server),
and finalizeSpawn's outcomes (not read by the commands).
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

NAME = 'trial-spawner-parity'
SPAWNER = '0 -60 0'
ALICE, BOB = 'Alice', 'Bob'
FAR = '0.5 -60 40.5'
STATES = ['inactive', 'waiting_for_players', 'active', 'waiting_for_reward_ejection', 'ejecting_reward', 'cooldown']
OMEN = 'effect give Alice minecraft:trial_omen 1000000 0'
BAD_OMEN = 'effect give Alice minecraft:bad_omen 1000000 1'
BLOCK = 'minecraft:trial_spawner[ominous=false,trial_spawner_state=%s]'
SKELETON = ('{normal_config:"minecraft:trial_chamber/ranged/skeleton/normal",'
            'ominous_config:"minecraft:trial_chamber/ranged/skeleton/ominous"}')
ZOMBIE = '{normal_config:"minecraft:trial_chamber/melee/zombie/normal"}'
EDGE = ('{normal_config:{spawn_range:1,total_mobs:2.0f,simultaneous_mobs:1.0f,ticks_between_spawn:5,'
        'spawn_potentials:[{data:{entity:{id:"minecraft:zombie"}},weight:1}]}')
# name, block state, block entity NBT, Alice's position, her game mode, her effect, kill_every, ticks, Bob's position.
CASES = [
    ('registry-breeze-normal', BLOCK % 'waiting_for_players',
     '{normal_config:"minecraft:trial_chamber/breeze/normal",ominous_config:"minecraft:trial_chamber/breeze/ominous"}',
     '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
    ('registry-melee-zombie-ominous', BLOCK.replace('ominous=false', 'ominous=true') % 'active',
     '{normal_config:"minecraft:trial_chamber/melee/zombie/normal",ominous_config:"minecraft:trial_chamber/melee/zombie/ominous"}',
     '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
    ('inline-config-edges', BLOCK % 'waiting_for_players', EDGE + '}', '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
    ('invalid-config-id', BLOCK % 'waiting_for_players', '{normal_config:"minecraft:trial_chamber/not_a_config"}',
     '0.5 -60 3.5', 'survival', None, 0, 60, FAR),
    ('required-range-4-player-at-6', BLOCK % 'waiting_for_players', EDGE + ',required_player_range:4}',
     '0.5 -60 6.5', 'survival', None, 0, 60, FAR),
    ('player-far-waits', BLOCK % 'waiting_for_players', ZOMBIE, '0.5 -60 40.5', 'survival', None, 0, 60, FAR),
    ('player-spectator', BLOCK % 'waiting_for_players', ZOMBIE, '0.5 -60 3.5', 'spectator', None, 0, 60, FAR),
    ('player-creative', BLOCK % 'waiting_for_players', ZOMBIE, '0.5 -60 3.5', 'creative', None, 0, 60, FAR),
    ('trial-omen-turns-ominous', BLOCK % 'waiting_for_players', SKELETON, '0.5 -60 3.5', 'survival', OMEN, 0, 60, FAR),
    ('bad-omen-becomes-trial-omen', BLOCK % 'waiting_for_players', SKELETON, '0.5 -60 3.5', 'survival', BAD_OMEN, 0, 60, FAR),
    ('cooldown-saved', BLOCK % 'cooldown',
     '{normal_config:"minecraft:trial_chamber/melee/zombie/normal",cooldown_ends_at:100L,target_cooldown_length:36000}',
     '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
    ('kill-waves', BLOCK % 'active',
     '{normal_config:{spawn_range:2,total_mobs:6.0f,simultaneous_mobs:2.0f,ticks_between_spawn:20,'
     'spawn_potentials:[{data:{entity:{id:"minecraft:zombie"}},weight:1}]},total_mobs_spawned:3}',
     '0.5 -60 3.5', 'survival', None, 60, 400, FAR),
    ('ejecting-saved-loot', BLOCK % 'ejecting_reward',
     '{normal_config:"minecraft:trial_chamber/melee/zombie/normal",ejecting_loot_table:"minecraft:spawners/trial_chamber/consumables"}',
     '0.5 -60 3.5', 'survival', None, 0, 120, FAR),
    ('two-players-in-range', BLOCK % 'waiting_for_players', ZOMBIE, '0.5 -60 3.5', 'survival', None, 0, 200,
     '1.5 -60 3.5'),
]
SERIAL = itertools.count()
# The same spawner in the other dimensions: the player and the commands go there through `execute in`.
DIM_CASES = [
    ('registry-melee-zombie-nether', 'the_nether', BLOCK % 'waiting_for_players', ZOMBIE, '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
    ('registry-melee-zombie-end', 'the_end', BLOCK % 'waiting_for_players', ZOMBIE, '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
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


def observe(server, prefix):
    """One tick's reading of the spawner and of the mobs in the room, as text rows."""
    mark = next(SERIAL)
    start = len(server.lines)
    for state in STATES:
        server.console(f'{prefix}execute if block {SPAWNER} minecraft:trial_spawner[trial_spawner_state={state}] run say STATE {state}')
    server.console(f'{prefix}execute if block {SPAWNER} minecraft:trial_spawner[ominous=true] run say OMINOUS')
    server.console(f'{prefix}data get block {SPAWNER}')
    server.console(f'{prefix}execute as @e[type=!minecraft:player,type=!minecraft:item,distance=..16] run data get entity @s Pos')
    server.console(f'{prefix}execute as @e[type=!minecraft:player,type=!minecraft:item,distance=..16] run data get entity @s Rotation')
    server.console(f'say OBS {mark}')
    server.wait_for(rf'OBS {mark}\b', timeout=120)
    answers = []
    for line in server.lines[start:]:
        if m := re.search(r'STATE (\w+)$', line):
            answers.append(f'state {m.group(1)}')
        elif line.endswith('OMINOUS'):
            answers.append('ominous')
        elif m := re.search(r'has the following (?:block|entity) data: (.*)$', line):
            answers.append(m.group(1))
    return answers


def case(server, alice, bob, block, nbt, pos, mode, effect, kill_every, ticks, bob_pos, dim=None):
    prefix = f'execute in minecraft:{dim} run ' if dim else ''
    server.console(f'execute in minecraft:overworld run tp {ALICE} 0.5 -60 40.5')
    server.console(f'{prefix}fill -16 -60 -16 16 -50 16 minecraft:air')
    server.console(f'{prefix}fill -16 -61 -16 16 -61 16 minecraft:stone')
    server.console(f'{prefix}kill @e[type=!minecraft:player]')
    server.console(f'{prefix}setblock {SPAWNER} {block}{nbt} replace')
    server.console(f'{prefix}tp {ALICE} {pos}')
    server.console(f'gamemode {mode} {ALICE}')
    server.console(f'{prefix}tp {BOB} {bob_pos}')
    server.console(f'gamemode survival {BOB}')
    if effect:
        server.console(effect)
    alice.capture('start', filter_ids=[packet_lab.LEVEL_EVENT, packet_lab.SOUND])
    rows = []
    for tick in range(ticks):
        step(server, 1)
        if kill_every and tick and tick % kill_every == 0:
            server.console(f'{prefix}kill @e[type=!minecraft:player,distance=..16]')
        answers = observe(server, prefix)
        rows.append(f'tick {tick} ' + ' | '.join(answers))
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
    """Why vanilla's transcript does not show mobs spawning, or None."""
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
        raise AssertionError(f'{len(differing)} of {len(vanilla)} trial spawner cases differ from vanilla: {differing}')
    print(f'trial spawner: {len(vanilla)} cases identical to vanilla')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        raise SystemExit(f'{NAME}: {error}')
