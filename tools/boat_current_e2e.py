#!/usr/bin/env python3
"""Check that a boat in flowing water drifts with the current (#6435).

A dedicated server (server_lab.Server) runs a scratch world; a one-deep, three-wide stone
trench is filled from a single source block at its head, an oak boat is summoned
onto the flowing water, and its `Pos` is read every ten ticks. Vanilla's
`Entity.updateFluidInteraction` pushes a boat by the water current it sits in
(`AbstractBoat` does not opt out of `isPushedByFluid`), with water's 0.014 scale,
and `AbstractBoat.tick` runs it before `floatBoat` scales the velocity; a boat
that stays put is missing that step.

The reported case rides the boat, but a ridden boat ticks through the same
`tick_boat` (the rider's client moves it), so the unridden boat covers the same
current step.

    python3 tools/boat_current_e2e.py --bin-dir target/fast
"""
import argparse
import shutil
import sys
import tempfile
from pathlib import Path

try:
    from . import lab
    from .server_lab import Server
except ImportError:
    import lab
    from server_lab import Server

SAMPLE_TICKS = 10
SAMPLES = 4
MIN_DRIFT = 1.0
BOAT = '@e[type=minecraft:oak_boat,limit=1,sort=nearest]'


class Failed(Exception):
    pass


def boat_pos(server):
    for line in server.command(f'data get entity {BOAT} Pos'):
        if '[' in line and ']' in line:
            body = line[line.index('[') + 1: line.rindex(']')]
            return [float(v.strip().removesuffix('d')) for v in body.split(',')]
    raise Failed('no boat position in the reply of data get entity')


def build_trench(server):
    # The chunk has to be loaded before a fill reaches it: aim the player at the
    # trench, then retry until the floor lands.
    server.command('tp @s 2.5 160 1')
    server.wait_ticks(60)
    for _ in range(5):
        if not any('not loaded' in line for line in server.command('fill 0 150 -1 9 150 3 stone')):
            break
        server.wait_ticks(40)
    else:
        raise Failed('the trench floor never loaded')
    server.command('fill -1 151 -1 -1 151 3 stone')
    server.command('fill 0 151 -1 9 151 -1 stone')
    server.command('fill 0 151 3 9 151 3 stone')
    server.command('fill 0 151 0 0 151 2 water')
    # A source spreads one block every five ticks; nine blocks settle in 45.
    server.wait_ticks(120)
    # Stand on the south wall, out of the current, next to the boat.
    server.command('tp @s 2.5 152 3.5')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', default='target/fast', help='folder with myriade-engine')
    parser.add_argument('--resources', default=str(lab.RESOURCES), help='the prepared resources (default: .local)')
    parser.add_argument('--keep', action='store_true', help='keep the scratch game home')
    args = parser.parse_args()
    bindir = Path(args.bin_dir).resolve()
    client = lab.binary(bindir, 'myriade-engine')
    if not client.exists():
        raise Failed(f'{client} is missing; build it (cargo build --profile fast -p horizon-game '
                     '-p myriade-engine-lib --bins) or pass --bin-dir')
    resources = Path(args.resources).resolve()
    if not resources.exists():
        raise Failed(f'{resources} is missing; run tools/lab.py prepare')
    work = Path(tempfile.mkdtemp(prefix='hz-boat-current-'))
    try:
        with Server(bindir, work, resources, {'level-seed': '7', 'gamemode': 'creative', 'view-distance': '4'}) as server:
            server.wait_ticks(20)
            build_trench(server)
            server.command('summon minecraft:oak_boat 2.5 151.0 1.9')
            server.wait_ticks(5)
            xs = []
            for _ in range(SAMPLES):
                xs.append(boat_pos(server)[0])
                server.wait_ticks(SAMPLE_TICKS)
        drift = xs[-1] - xs[0]
        print(f'boat x over {(SAMPLES - 1) * SAMPLE_TICKS} ticks: {xs[0]:.3f} -> {xs[-1]:.3f} ({drift:+.3f})')
        if drift < MIN_DRIFT:
            raise Failed(f'the flowing water did not push the boat (drift {drift:+.3f}, want >= {MIN_DRIFT}); '
                         'Entity.baseTick\'s fluid step is missing from the boat tick')
        print('PASS: the boat drifts with the flowing water')
        return 0
    finally:
        if not args.keep:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Failed as error:
        print(f'FAIL: {error}')
        sys.exit(1)
