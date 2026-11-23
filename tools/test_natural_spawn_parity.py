#!/usr/bin/env python3
"""Natural spawning of a whole server, vanilla against Horizon's, in the same flat world, in the overworld, the nether and the end.

Alice and Bob (joueur-automate bots) stand in survival, Bob 60 blocks away so both spawn around players. Every
SAMPLE_TICKS ticks the server steps (`tick step`), then each category's representative species is looked up with
`execute if entity @e[type=…]`. Over SAMPLES samples, the frequency of each species being present is compared on
both sides. The tolerance for one species is 3 standard deviations of the difference of two binomial frequencies,
with the pooled frequency p: 3 * sqrt(2 * p * (1 - p) / SAMPLES) + 1 / SAMPLES. A species absent from both sides
is equal. Both sides get the same commands; vanilla's run must show at least one species present.

Not compared: the exact positions and counts per chunk (natural spawning draws from the level random, whose order is
not pinned on a whole server), so only presence frequencies are read.
"""
import argparse
import json
import math
import time

try:
    from . import test_locator_bar_parity as locator
    from .bot_lab import Bot
    from .test_dedicated_server_parity import Server, game_time
except ImportError:
    import test_locator_bar_parity as locator
    from bot_lab import Bot
    from test_dedicated_server_parity import Server, game_time

NAME = 'natural-spawn-parity'
SAMPLE_TICKS = 20
SAMPLES = 50
ALICE, BOB = 'Alice', 'Bob'
BOB_POS = '60.5 -60 0.5'
# category, representative species.
SPECIES = {
    'monster': ['zombie', 'skeleton', 'creeper', 'spider'],
    'creature': ['cow', 'pig', 'sheep', 'chicken'],
    'ambient': ['bat'],
    'water_creature': ['squid'],
}


DIMENSIONS = ['overworld', 'the_nether', 'the_end']


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


def sample(server, index, dim):
    """One sample in a dimension: the species present, each with a line answered by the server."""
    prefix = f'execute in minecraft:{dim} run '
    step(server, SAMPLE_TICKS)
    start = len(server.lines)
    for names in SPECIES.values():
        for species in names:
            server.console(f'{prefix}execute if entity @e[type=minecraft:{species}] run say HERE {species} {index}')
    mark = f'DONE {index}'
    server.console(f'say {mark}')
    server.wait_for(mark, timeout=120)
    return sorted({line.split('HERE ')[1].split()[0] for line in server.lines[start:] if 'HERE ' in line})


def run(label, server_command, world, build):
    directory = build / label
    port = locator.server_dir(directory, world)
    server = Server(server_command, directory, label)
    presence = {dim: {species: 0 for names in SPECIES.values() for species in names} for dim in DIMENSIONS}
    try:
        server.wait_for(r'Done \(', timeout=600)
        server.console('tick freeze')
        server.console('say READY')
        server.wait_for(r'READY', timeout=120)
        with Bot(timeout=600) as alice, Bot(timeout=600) as bob:
            alice.join('127.0.0.1', port, ALICE)
            alice.wait_for('loaded', 300)
            bob.join('127.0.0.1', port, BOB)
            bob.wait_for('loaded', 300)
            server.console(f'gamemode survival {ALICE}')
            server.console(f'gamemode survival {BOB}')
            for dim in DIMENSIONS:
                server.console(f'execute in minecraft:{dim} run forceload add -16 -16 16 16')
                server.console(f'execute in minecraft:{dim} run tp {ALICE} 0.5 -60 3.5')
                server.console(f'execute in minecraft:{dim} run tp {BOB} {BOB_POS}')
                for index in range(SAMPLES):
                    for species in sample(server, index, dim):
                        presence[dim][species] += 1
            alice.leave()
            bob.leave()
    finally:
        server.stop()
    return presence


def tolerance(p_vanilla, p_horizon):
    p = (p_vanilla + p_horizon) / 2
    return 3 * math.sqrt(2 * p * (1 - p) / SAMPLES) + 1 / SAMPLES


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    lock, build, vanilla_server, horizon_server, _client = locator.sides(NAME)
    world = locator.generate_world(vanilla_server, build)
    vanilla = run('vanilla', vanilla_server, world, build)
    if not any(n for dim in vanilla.values() for n in dim.values()):
        raise AssertionError('vanilla is no proof: no species was ever present')
    horizon = run('horizon', horizon_server, world, build)
    freq_v = {f'{d}/{s}': n / SAMPLES for d, species in vanilla.items() for s, n in species.items()}
    freq_h = {f'{d}/{s}': n / SAMPLES for d, species in horizon.items() for s, n in species.items()}
    differing = sorted(s for s in freq_v if abs(freq_v[s] - freq_h[s]) > tolerance(freq_v[s], freq_h[s]))
    (build / 'report.json').write_text(json.dumps({'minecraft': lock['minecraft']['version'], 'samples': SAMPLES,
                                                   'vanilla': vanilla, 'horizon': horizon,
                                                   'differing': differing}, indent=2) + '\n')
    for s in differing:
        print(f'- {s}: {freq_v[s]:.2f}\n+ {s}: {freq_h[s]:.2f}')
    if differing:
        raise AssertionError(f'species outside tolerance: {differing}')
    print(f'natural spawn: {len(freq_v)} species and dimensions within tolerance over {SAMPLES} samples')


if __name__ == '__main__':
    main()
