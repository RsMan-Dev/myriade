#!/usr/bin/env python3
"""Vaults on a vanilla dedicated server and on Horizon's, case by case, tick by tick.

Each case clears a room at the flat world's spawn and places a vault with its block state and block entity NBT
(`setblock`). Alice (a joueur-automate bot) stands at the case's position in its game mode, holds the case's item
and uses the vault (`use_block`) twice at most; Bob joins too, far away or in range. Each tick the server steps one
tick (`tick step`), then reads the vault's state (`execute if block`), its block entity NBT (`data get block`), the
nearest item entities' Pos and Motion (`data get entity`); the level events and the sounds Alice receives are
captured for the whole case. Both sides get the same commands; the transcripts must match line for line, and
vanilla's must show the vault unlocking or ejecting.

Not compared: the sound's pitch and seed (drawn from the level random, whose order is not pinned on a whole server).
"""
import argparse
import hashlib
import itertools
import json
import re
import time
import uuid as uuidlib

try:
    from . import packet_lab, test_locator_bar_parity as locator
    from .bot_lab import Bot
    from .test_dedicated_server_parity import Server, game_time
except ImportError:
    import packet_lab
    import test_locator_bar_parity as locator
    from bot_lab import Bot
    from test_dedicated_server_parity import Server, game_time

NAME = 'vault-parity'
VAULT = '0 -60 0'
ALICE, BOB = 'Alice', 'Bob'
FAR = '0.5 -60 40.5'
STATES = ['inactive', 'active', 'unlocking', 'ejecting']
KEY = 'minecraft:trial_key'
OMINOUS = 'minecraft:ominous_trial_key'
# name, block state, block entity NBT, Alice's position, her game mode, her item, uses, ticks, Bob's position.
CASES = [
    ('config-none', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'survival', None, 0, 60, FAR),
    ('config-ominous', 'minecraft:vault[ominous=true,vault_state=inactive]',
     '{config:{key_item:{count:1,id:"minecraft:ominous_trial_key"},loot_table:"minecraft:chests/trial_chambers/reward_ominous"}}',
     '0.5 -60 3.5', 'survival', OMINOUS, 1, 60, FAR),
    ('config-inline-edges', 'minecraft:vault[ominous=false,vault_state=inactive]',
     '{config:{activation_range:0.0d,deactivation_range:0.5d,key_item:{count:2,id:"minecraft:diamond"}}}',
     '0.5 -60 3.5', 'survival', KEY, 2, 60, FAR),
    ('config-override-table', 'minecraft:vault[ominous=false,vault_state=inactive]',
     '{config:{override_loot_table_to_display:"minecraft:chests/simple_dungeon",loot_table:"minecraft:empty"}}',
     '0.5 -60 3.5', 'survival', None, 0, 200, FAR),
    ('server-rewarded-player', 'minecraft:vault[ominous=false,vault_state=inactive]',
     '{server_data:{rewarded_players:[{uuid}]}}', '0.5 -60 3.5', 'survival', KEY, 1, 60, FAR),
    ('server-resumes-at', 'minecraft:vault[ominous=false,vault_state=active]',
     '{server_data:{state_updating_resumes_at:40L}}', '0.5 -60 3.5', 'survival', None, 0, 60, FAR),
    ('server-items-to-eject', 'minecraft:vault[ominous=false,vault_state=ejecting]',
     '{server_data:{items_to_eject:[{id:"minecraft:diamond",count:2}]}}', '0.5 -60 12.5', 'survival', None, 0, 60, FAR),
    ('shared-display-item', 'minecraft:vault[ominous=false,vault_state=inactive]',
     '{shared_data:{display_item:{id:"minecraft:emerald",count:1}}}', '0.5 -60 3.5', 'survival', None, 0, 60, FAR),
    ('player-out-of-range', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 12.5', 'survival', KEY, 1, 60, FAR),
    ('player-creative', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'creative', KEY, 1, 60, FAR),
    ('player-spectator', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'spectator', KEY, 1, 60, FAR),
    ('use-wrong-item', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'survival',
     'minecraft:arrow', 2, 60, FAR),
    ('use-ominous-key-on-trial', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'survival',
     OMINOUS, 1, 60, FAR),
    ('use-key-twice', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'survival', KEY, 2, 200, FAR),
    ('two-players-in-range', 'minecraft:vault[ominous=false,vault_state=inactive]', '', '0.5 -60 3.5', 'survival', KEY, 1, 60,
     '1.5 -60 3.5'),
]
SERIAL = itertools.count()
# The same vault in the other dimensions: the player and the commands go there through `execute in`.
DIM_CASES = [
    ('config-ominous-nether', 'the_nether', 'minecraft:vault[ominous=true,vault_state=inactive]',
     '{config:{key_item:{count:1,id:"minecraft:ominous_trial_key"},loot_table:"minecraft:chests/trial_chambers/reward_ominous"}}',
     '0.5 -60 3.5', 'survival', OMINOUS, 1, 60, FAR),
    ('config-ominous-end', 'the_end', 'minecraft:vault[ominous=true,vault_state=inactive]',
     '{config:{key_item:{count:1,id:"minecraft:ominous_trial_key"},loot_table:"minecraft:chests/trial_chambers/reward_ominous"}}',
     '0.5 -60 3.5', 'survival', OMINOUS, 1, 60, FAR),
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


def int_array_uuid(name):
    """Alice's offline UUID (the server's ops.json uses the same) as the NBT int array."""
    digest = bytearray(hashlib.md5(f'OfflinePlayer:{name}'.encode()).digest())
    digest[6] = (digest[6] & 0x0F) | 0x30
    digest[8] = (digest[8] & 0x3F) | 0x80
    value = uuidlib.UUID(bytes=bytes(digest)).int
    parts = [(value >> shift) & 0xFFFFFFFF for shift in (96, 64, 32, 0)]
    return '[I;' + ','.join(str(p - 2**32 if p >= 2**31 else p) for p in parts) + ']'


def observe(server, prefix):
    """One tick's reading of the vault, as text rows, in the order the server answers them."""
    mark = next(SERIAL)
    start = len(server.lines)
    for state in STATES:
        server.console(f'{prefix}execute if block {VAULT} minecraft:vault[vault_state={state}] run say STATE {state}')
    server.console(f'{prefix}data get block {VAULT}')
    server.console(f'{prefix}execute as @e[type=minecraft:item,sort=nearest,limit=3] run data get entity @s Pos')
    server.console(f'{prefix}execute as @e[type=minecraft:item,sort=nearest,limit=3] run data get entity @s Motion')
    server.console(f'say OBS {mark}')
    server.wait_for(rf'OBS {mark}\b', timeout=120)
    rows = []
    for line in server.lines[start:]:
        if m := re.search(r'STATE (\w+)$', line):
            rows.append(f'state {m.group(1)}')
        elif m := re.search(r'has the following (?:block|entity) data: (.*)$', line):
            rows.append(m.group(1))
    return rows


def case(server, alice, bob, block, nbt, pos, mode, item, uses, ticks, bob_pos, dim=None):
    prefix = f'execute in minecraft:{dim} run ' if dim else ''
    server.console(f'execute in minecraft:overworld run tp {ALICE} 0.5 -60 40.5')
    server.console(f'{prefix}fill -12 -60 -12 12 -54 12 minecraft:air')
    server.console(f'{prefix}fill -12 -61 -12 12 -61 12 minecraft:stone')
    server.console(f'{prefix}kill @e[type=!minecraft:player]')
    server.console(f'{prefix}setblock {VAULT} {block}{nbt.replace("{uuid}", int_array_uuid(ALICE))} replace')
    server.console(f'{prefix}tp {ALICE} {pos}')
    server.console(f'gamemode {mode} {ALICE}')
    server.console(f'{prefix}tp {BOB} {bob_pos}')
    server.console(f'gamemode survival {BOB}')
    if item:
        server.console(f'item replace entity {ALICE} weapon.mainhand with {item}')
    alice.capture('start', filter_ids=[packet_lab.LEVEL_EVENT, packet_lab.SOUND])
    rows = []
    for tick in range(ticks):
        if tick % 2 == 0 and tick // 2 < uses:
            alice.use_block(0, -60, 0)
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
    """Why vanilla's transcript does not show the vault working, or None."""
    if not any('state unlocking' in row or 'state ejecting' in row for rows in vanilla.values() for row in rows):
        return 'the vault never unlocked or ejected'
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
    lock_version = lock['minecraft']['version']
    (build / 'report.json').write_text(json.dumps({'minecraft': lock_version, 'cases': len(vanilla),
                                                   'differing': differing}, indent=2) + '\n')
    for name in differing:
        for v, h in zip(vanilla[name], horizon[name]):
            if v != h:
                print(f'- {name}: {v}\n+ {name}: {h}')
                break
    if differing:
        raise AssertionError(f'{len(differing)} of {len(vanilla)} vault cases differ from vanilla: {differing}')
    print(f'vault: {len(vanilla)} cases identical to vanilla')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        raise SystemExit(f'{NAME}: {error}')
