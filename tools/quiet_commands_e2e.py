#!/usr/bin/env python3
"""Mods' quiet commands on Horizon's dedicated server, end to end (horizon_protocol::quiet).

Horizon's dedicated server (a flat world) and the joueur-automate bot, which joins, then sends a batch of quiet
commands as a mod would (`quiet`):

- as a player who isn't an operator: `setblock` is refused (the block stays air), as it would be typed;
- as an operator: `setblock` runs, a `fill` that changes nothing fails, and `say` is refused (a chat
  message needs the player's signature);
- either way the player's chat gets nothing of them, and the server logs one line per batch: the player,
  the mod, how many ran and how many failed.

    python3 tools/quiet_commands_e2e.py --bin-dir target/fast

The server runs in --out (fresh); its log is read from there.
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

ROOT = Path(__file__).resolve().parents[1]
PLAYER = 'Tester'
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'spawn-protection': '0',
    'view-distance': '3',
    'simulation-distance': '3',
    'pause-when-empty-seconds': '-1',
}
BLOCK = '2 -55 2'
# What a player would see of these commands, were they typed.
FEEDBACK = ['Changed the block', 'No blocks were filled', 'Unknown or incomplete command', 'hi from a mod',
            'Could not set the block']


def quiet_batch(server, lines):
    """The bot joins, sends `lines` as one quiet batch and leaves: its report. The server's log lines about the
    batch and the leaving are waited for after the mark taken before the join."""
    mark = len(server.log_lines())
    with Bot() as bot:
        bot.join('127.0.0.1', server.port, PLAYER)
        bot.quiet(lines)
        got = bot.report()
        bot.leave()
    server.wait_log(rf'{PLAYER} left the game', seconds=60, since=mark)
    return got, mark


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin-dir', help='a prebuilt horizon-client (default: build it with the fast profile)')
    parser.add_argument('--out', default=str(ROOT / '.local/quiet-commands-e2e'), help='the server runs here')
    args = parser.parse_args()
    out = Path(args.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    if args.bin_dir:
        bins = Path(args.bin_dir)
    else:
        subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-game', '--bin', 'horizon-client'],
                       cwd=ROOT, check=True)
        bins = Path(lab.cargo_target()) / 'fast'
    failures = []

    def check(what, ok, detail=''):
        print(('PASS ' if ok else 'FAIL ') + what + ('' if ok else f': {detail}'))
        if not ok:
            failures.append(what)

    def test_block(server, want):
        """The console's `execute if block`: 'Test passed' when the block is `want`."""
        return any('Test passed' in line for line in server.command(f'execute if block {BLOCK} {want}'))

    with Server(bins, out / 'server', lab.RESOURCES.resolve(), PROPERTIES) as server:
        # Not an operator: the setblock is refused, as typed.
        got, mark = quiet_batch(server, [f'setblock {BLOCK} minecraft:gold_block'])
        server.wait_log(rf'{PLAYER} ran 1 quiet command\(s\) from mod net_harness: 1 failed', seconds=30, since=mark)
        check('not an operator: connected throughout', got['error'] is None, got['error'])
        check("not an operator: a quiet setblock is refused", not test_block(server, 'minecraft:gold_block'))
        heard = [c for c in got['chat'] if any(f in c for f in FEEDBACK)]
        check('not an operator: nothing in the chat', not heard, heard)
        # An operator: it runs, quietly.
        server.command(f'op {PLAYER}')
        server.wait_log(rf'Made {PLAYER} a server operator', seconds=30)
        got, mark = quiet_batch(server, [f'setblock {BLOCK} minecraft:gold_block',
                                         f'fill {BLOCK} {BLOCK} minecraft:gold_block', 'say hi from a mod'])
        server.wait_log(rf'{PLAYER} ran 3 quiet command\(s\) from mod net_harness: 2 failed', seconds=30, since=mark)
        check('operator: connected throughout', got['error'] is None, got['error'])
        check('operator: the quiet setblock ran', test_block(server, 'minecraft:gold_block'))
        heard = [c for c in got['chat'] if any(f in c for f in FEEDBACK)]
        check('operator: nothing in the chat (no feedback, no failure, no say)', not heard, heard)
        said = [l for l in server.log_lines() if 'hi from a mod' in l]
        check('operator: say is refused', not said, said)
    print(f'logs: {out}')
    sys.exit(1 if failures else 0)


if __name__ == '__main__':
    main()
