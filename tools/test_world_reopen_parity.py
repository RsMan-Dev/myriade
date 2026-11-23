#!/usr/bin/env python3
"""Leaving a world and opening it again keeps its entities as they were (#374).

Vanilla's integrated server unloads every entity with the level when the player saves
and quits (`MinecraftServer.stopServer`), so a world reopened in the same session holds
each entity once. Horizon kept non-mob entities (minecarts, boats, armor stands...) in
memory after Save and Quit to Title; opening the world loaded them again from disk and
the next save wrote both copies, doubling them each time, until the game crawled.

The dedicated server runs with a joueur-automate bot: a minecart is summoned beside the
player (seed 7 also has mineshaft chest minecarts in view), then the bot leaves, the server
restarts on the same world, and the bot joins again, three times. The minecarts must be
the same ones each time, each once.

  test_world_reopen_parity.py [--bin-dir DIR] [--keep]
"""
import argparse
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from bot_lab import Bot  # noqa: E402
from server_lab import Server  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
RESOURCES = ROOT / '.local' / 'horizon-client' / '26.3'
RUN = ROOT / '.local' / 'world-reopen'
PLAYER = 'Tester'
CYCLES = 3


def carts(server):
    """The loaded minecarts once their list stops changing: on a loaded runner the mineshaft's chunks and their
    entities keep arriving after the world counts as loaded, so one reading can be short of a cart."""
    last = None
    for _ in range(30):
        found = sorted((c['type'], *(round(v, 1) for v in c['pos'])) for c in server.entities_server()['entities']
                       if 'minecart' in c['type'])
        if found == last:
            return found
        last = found
        server.wait_ticks(40)
    return last


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin-dir', help='prebuilt horizon-client (default: build it with the fast profile)')
    parser.add_argument('--keep', action='store_true', help='keep the run folder')
    args = parser.parse_args()
    bin_dir = Path(args.bin_dir).resolve() if args.bin_dir else None
    if bin_dir is None:
        subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-game', '-p', 'horizon-client-lib', '--bins'], cwd=ROOT, check=True)
        bin_dir = Path(lab.cargo_target()) / 'fast'
    shutil.rmtree(RUN, ignore_errors=True)
    RUN.mkdir(parents=True)
    failures = 0
    with Server(bin_dir, RUN / 'server', RESOURCES, {'level-seed': '7', 'gamemode': 'creative', 'view-distance': '4'}) as server, \
            Bot() as bot:
        bot.join('127.0.0.1', server.port, PLAYER)
        bot.command('summon minecraft:minecart ~2 ~ ~2')
        bot.wait_ticks(20)
        expected = carts(server)
        print('minecarts:', len(expected))
        if not any(kind == 'minecraft:minecart' for kind, *_ in expected):
            sys.exit(f'the summoned minecart is not there: {expected}')
        for cycle in range(1, CYCLES + 1):
            # Leave the world, stop the server (saving it), start it again on the same world, join again.
            bot.leave()
            server.restart()
            bot.join('127.0.0.1', server.port, PLAYER)
            bot.wait_ticks(20)
            got = carts(server)
            ok = got == expected
            failures += not ok
            print(f'reopen {cycle}: {len(got)} minecarts', 'ok' if ok else f'FAIL (expected {len(expected)})')
    if not args.keep and not failures:
        shutil.rmtree(RUN, ignore_errors=True)
    print('world reopen:', 'PASS' if not failures else f'{failures} FAILED')
    sys.exit(1 if failures else 0)


if __name__ == '__main__':
    main()
