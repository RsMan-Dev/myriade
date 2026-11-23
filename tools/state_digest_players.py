#!/usr/bin/env python3
"""State-digest scene with players: BINARY RESOURCES TEMPLATE OUT BOTS [--nether] [--detail] [--ticks N] [--no-digest], or --compare OUT_A OUT_B; template from `state_digest_scene.py --generate`, gate in HORIZON_SCENE_GATE."""
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import lab
import state_digest_scene as base

STATE_PATHS = ('Pos', 'Health', 'foodLevel', 'XpLevel', 'XpTotal', 'Inventory', 'active_effects[0].id', 'active_effects[1].id', 'Dimension')
PLAYERS = ['Alice0', 'Bob0']  # two scripted bots
SETTLE_ATTEMPTS = 400  # frozen server ticks to wait for a join or a settle, before giving up


def free_port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def player_commands(names, nether):
    """The scene after the bots joined: each one among the mobs, hurt, fed, given items and a pickup, one in the Nether."""
    a, b = names[0], names[-1]
    y = base.Y
    commands = [
        f'gamemode survival {a}', f'gamemode survival {b}',
        f'tp {a} 16.5 {y} 9.5', f'tp {b} 66.5 {y} 61.5',
        f'give {a} minecraft:iron_sword', f'give {a} minecraft:bread 16', f'give {a} minecraft:cooked_beef 8',
        f'give {b} minecraft:diamond_pickaxe', f'give {b} minecraft:cobblestone 64', f'give {b} minecraft:oak_planks 32',
        f'item replace entity {a} armor.chest with minecraft:iron_chestplate',
        f'item replace entity {b} weapon.offhand with minecraft:shield',
        f'effect give {b} minecraft:regeneration 600 1', f'effect give {a} minecraft:speed 600 0',
        f'xp add {a} 5 levels', f'xp add {b} 120 points',
        f'effect give {a} minecraft:resistance 600 3', f'damage {a} 3 minecraft:generic', f'damage {b} 4 minecraft:fall',
        # next to a player: an item to pick up, an orb, and mobs that go for it
        f'summon minecraft:item 17.5 {y + 1} 9.5 {{Item:{{id:"minecraft:emerald",count:3}}}}',
        f'summon minecraft:experience_orb 15.5 {y + 1} 10.5 {{Value:7s}}',
        f'summon minecraft:zombie 20.5 {y} 9.5', f'summon minecraft:skeleton 12.5 {y} 12.5',
        f'summon minecraft:zombie 70.5 {y} 61.5', f'summon minecraft:spider 62.5 {y} 58.5',
        f'summon minecraft:tnt 66.5 {y} 64.5 {{fuse:60s}}',
        f'setblock 66 {y} 58 minecraft:chest{{Items:[{{Slot:0b,id:"minecraft:apple",count:5}}]}}',
        f'setblock 64 {y} 58 minecraft:furnace',
        f'setblock 70 {y} 65 minecraft:crafting_table',
        f'time set 1000',
    ]
    if nether:
        commands.append(f'execute in minecraft:the_nether run tp {b} 8.5 70 8.5')
    return commands


class Bots:
    """Scripted `myriade-bot` that stand where the server puts them: their per-tick moves repeat one position."""

    def __init__(self, binary, port, directory):
        self.binary, self.port, self.directory = binary, port, directory
        self.processes = []

    def join(self, server, prefix):
        at = len(server.lines)
        log = open(self.directory / f'bot-{prefix}.log', 'w')
        self.processes.append(subprocess.Popen(
            [self.binary, '--server', f'127.0.0.1:{self.port}', '--bots', '1', '--behaviour', 'stand', '--name', prefix,
             '--seconds', '3600', '--view-distance', '4', '--seed', '12345'], stdout=log, stderr=subprocess.STDOUT))
        server.wait_for(f'{prefix}0[/127.0.0.1:', at, 120)

    def stop(self):
        for process in self.processes:
            process.terminate()
        for process in self.processes:
            try:
                process.wait(20)
            except subprocess.TimeoutExpired:
                process.kill()


def settle(server, names, targets):
    """Frozen ticks run until each player stands where the scene put it, so the stepped ticks see steady moves."""
    for _ in range(SETTLE_ATTEMPTS):
        lines = server.run([f'data get entity {n} Pos' for n in names])
        text = '\n'.join(lines)
        if all(any(f'{t:.1f}' in text for t in want) for want in targets):
            return
        time.sleep(0.05)
    raise SystemExit(f'players did not settle:\n{text}')


def final_state(server):
    """What the players ended with: their data and scores, which the digest does not hash."""
    lines = []
    for name in PLAYERS:
        for path in STATE_PATHS:
            lines += [l.split(']: ', 1)[-1] for l in server.run([f'data get entity {name} {path}'])
                      if 'following entity data' in l or 'No entity' in l]
    return '\n'.join(lines) + '\n'


def wait_loaded(server, dim, at):
    """Frozen ticks until the chunk under the player is loaded, then a few more for its neighbours."""
    for _ in range(SETTLE_ATTEMPTS):
        if any('Test passed' in l for l in server.run([f'execute in {dim} if loaded {at[0]} {at[1]} {at[2]}'])):
            break
        time.sleep(0.05)
    else:
        raise SystemExit(f'{dim} did not load')
    for _ in range(40):
        server.run(['say settle'])
        time.sleep(0.1)


def by_dimension(digest):
    """digest.txt cut into one text per dimension (a level's lines can start on a different step run to run)."""
    parts, current = {}, None
    for line in digest.read_text().splitlines(keepends=True):
        if line.startswith('tick='):
            current = line.split()[1].removeprefix('dim=')
        if current is not None:
            parts.setdefault(current, []).append(line)
    return {dim: ''.join(lines) for dim, lines in parts.items()}


def compare(a, b):
    """The digest compare for each dimension, and the players' final data: exit code 0 only if all are identical."""
    status = 0
    with tempfile.TemporaryDirectory() as tmp:
        left, right = by_dimension(a / 'digest.txt'), by_dimension(b / 'digest.txt')
        for dim in sorted(left.keys() | right.keys()):
            print(f'== {dim}', flush=True)
            paths = []
            for side, parts in (('a', left), ('b', right)):
                path = Path(tmp) / f'{side}-{dim.replace(":", "_")}.txt'
                path.write_text(parts.get(dim, ''))
                paths.append(path)
            if dim not in left or dim not in right:
                print('only in one run')
                status = 1
                continue
            if ' run=1 ' not in left[dim] and ' run=1 ' not in right[dim]:
                print('no stepped tick digested in this level (frozen ticks only)')
                continue
            status |= subprocess.run([sys.executable, Path(__file__).with_name('state_digest_compare.py'), *paths]).returncode
    same = (a / 'players.txt').read_text() == (b / 'players.txt').read_text()
    print('== players\n' + ('identical' if same else 'DIFFERENT'))
    return status or (0 if same else 1)


def server_properties(port):
    return dict(base.PROPERTIES, **{'server-port': str(port), 'white-list': 'false', 'enforce-whitelist': 'false'})


def run_dedicated(args, home, out, timings):
    binary, resources, template, _, bots = args[:5]
    run_dir = out / 'server'
    if run_dir.exists():
        shutil.rmtree(run_dir)
    run_dir.mkdir(parents=True)
    port = free_port()
    (run_dir / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in server_properties(port).items()))
    (run_dir / 'eula.txt').write_text('eula=true\n')
    shutil.copytree(template / 'world', run_dir / 'world')
    (run_dir / 'world' / 'session.lock').unlink(missing_ok=True)
    digest = None if '--no-digest' in sys.argv else out / 'digest.txt'
    server = base.Server(binary, resources, run_dir, lab.game_env(base.scratch_env(home, digest, '--detail' in sys.argv)), freeze=True)
    players = Bots(bots, port, out)
    ticks = int(sys.argv[sys.argv.index('--ticks') + 1]) if '--ticks' in sys.argv else 200
    try:
        server.wait_for('Done (')
        server.run(['forceload query'])
        time.sleep(3)
        server.run(base.scene_commands(False))
        players.join(server, 'Alice')
        players.join(server, 'Bob')
        server.run(player_commands(PLAYERS, '--nether' in sys.argv))
        settle(server, PLAYERS, [(16.5, 9.5), (66.5, 61.5) if '--nether' not in sys.argv else (8.5, 8.5)])
        if '--nether' in sys.argv:
            wait_loaded(server, 'minecraft:the_nether', (8, 70, 8))
        before = server.game_time()
        start = time.time()
        server.run(['tick rate 10000', f'tick step {ticks}'])
        while server.game_time() < before + ticks:
            time.sleep(0.05)
        timings['step_seconds'] = time.time() - start
        (out / 'players.txt').write_text(final_state(server))
    finally:
        players.stop()
        server.stop()


def main():
    if sys.argv[1] == '--compare':
        raise SystemExit(compare(Path(sys.argv[2]), Path(sys.argv[3])))
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    args = [Path(a).resolve() for a in args]
    out = args[3]
    out.mkdir(parents=True, exist_ok=True)
    home = Path(tempfile.mkdtemp(prefix='state-digest-home-'))
    base.hold_gate()
    timings = {}
    try:
        run_dedicated(args, home, out, timings)
    finally:
        shutil.rmtree(home, ignore_errors=True)
    (out / 'timings.txt').write_text(f'{timings}\n')
    print(out, timings)


if __name__ == '__main__':
    main()
