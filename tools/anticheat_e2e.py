#!/usr/bin/env python3
"""The dedicated server's anticheat (docs/anticheat.md), end to end: a real `horizon-client --dedicated`
and myriade-bot, legit bots beside cheating ones.

    python3 tools/anticheat_e2e.py --bin-dir target/fast            # every check
    python3 tools/anticheat_e2e.py --bin-dir target/fast --check timer

Checks:
- off:   `[anticheat] enabled = false` in server.toml: the anticheat says nothing, writes no log and kicks
         no one, the cheating bot included (vanilla's checks only). Then, on a survival server (allow-flight
         on, so vanilla's floating kick stays out of it), the same for the checks that act by default: a bot
         attacking without swinging, one attacking two players in the same tick and a no-fall one are never
         flagged, kicked or banned, and their hits land.
- timer: `[anticheat]` on, with the timer's thresholds once trusted (it logs only by default): a bot ticking 1.5 times as fast as vanilla's client is flagged, sent back,
         reported to the console and kicked; the legit bots (walking and exploring at 20 Hz) are never
         flagged.
- log-off: `[anticheat] log = ""`, survival: no flag log is written, and a bot attacking without swinging is
         still kicked, with a warning on the console.
- ground: the default thresholds, survival, allow-flight off: a bot walking 24 blocks up that claims the
         ground in every move (no-fall) and one walking on water that does (jesus) are flagged, their claim
         dropped (never sent back), and kicked by the anticheat, warned of the ban the next kick brings, never
         by vanilla's floating kick; bots walking on the ground and standing still are never flagged.
- prediction: `[anticheat]` on, survival: the vanilla client (vanilla_client.py, VanillaMover), through a relay
         that holds every packet 20-120 ms, plays honest_play (walks, sprint-jumps, strafes and sneaks with effects,
         over ice and soul sand, swims, walks into cows, flies in creative, glides with an elytra, launches with
         riptide) and water_play and items_play: it is never flagged. A bot, which moves by its own rules and sends
         no keys, beside it is flagged and sent back. Only with --check.
- combat: no server.toml (the anticheat's defaults: `combat` acts), survival: a bot attacking a player
         twice a second and swinging each time, as vanilla's client does, is never flagged and its hits
         land; one that doesn't swing, and one that attacks two players in the same tick, are flagged,
         their attacks dropped, and kicked.
- combat-restart: a player's kicks are kept across a restart (`anticheat-strikes.json`): a bot attacking
         without swinging is kicked once (1 of 2 before a ban), the server stops and starts again on the same
         folder, and the same bot, back and cheating again, is banned.
- strikes-off: the same with `[anticheat] strikes = ""`: no strikes file is written, and after the restart the
         bot's kick is a first kick again (1 of 2 before a ban), with no ban.
- prediction-vanilla (only with --check): the same with the real vanilla 26.3 client (tools/vanilla_client.py, from the owner's
         own files; on macOS from the logged-in screen session, even a locked one), its keys pressed by
         VanillaMover (mods/horizon/src/test/java): it walks, sprint-jumps, strafes and sneaks, with Speed
         given and cleared, over packed ice, swims, walks into cows, jumps at a block set in front of it and
         glides with an elytra and a firework, through the same relay: it is never flagged. It hits the
         cow too, and `combat` never flags it.

Each server runs in a fresh folder with fresh HORIZON_HOME and XDG folders (lab.game_env), on a free port,
in a flat world (creative unless the check says otherwise: the bots fly). Build first:
    cargo build --profile fast -p horizon-client --bin horizon-client
    cargo build --profile fast -p myriade-bot
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import queue
import random
import shutil
import socket
import subprocess
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from server_bench import DONE, Server  # noqa: E402

EXE = '.exe' if os.name == 'nt' else ''
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'gamemode': 'creative',
    'force-gamemode': 'true',
    'spawn-protection': '0',
    'view-distance': '4',
    'simulation-distance': '4',
    'max-players': '100',
}
# The timer and the ground with the thresholds they get once trusted (they log only by default). The bots
# move by their own rules, not vanilla's: the prediction, which plays each move again with vanilla's
# movement code, would flag every one of them. Its own check runs real clients.
ON = '''[anticheat]
enabled = true
[anticheat.timer]
setback = 2
alert = 5
kick = 20
[anticheat.prediction]
enabled = false
'''
OFF = '[anticheat]\nenabled = false\n'
PREDICTION = ('[anticheat]\nenabled = true\n[anticheat.timer]\nenabled = false\n[anticheat.ground]\nenabled = false\n'
              '[anticheat.prediction]\nsetback = 5\nalert = 15\nkick = 60\n')


class Failed(Exception):
    pass


def player_lines(log, name, check):
    """The lines of the flag log where player `name` failed `check`, in order: `[time] name failed check (VL ...): ...`."""
    return [line for line in log.splitlines() if f'] {name} failed {check} (' in line]


def warned_of_ban(log, name, check):
    """Whether `check` kicked `name` with the warning of the ban the next kick brings (`kicked (1 of 2 before a ban)`)."""
    return any('kicked (1 of 2 before a ban)' in line for line in player_lines(log, name, check))


def kicked_before_banned(log, name, check):
    """Whether `check` kicked `name` with the warning of the ban, then banned it."""
    lines = player_lines(log, name, check)
    kick = next((i for i, line in enumerate(lines) if 'kicked (1 of 2 before a ban)' in line), None)
    return kick is not None and any(', banned' in line for line in lines[kick + 1:])


def start(bins, folder, anticheat, out, name, **extra):
    # A folder already there is a server started again (combat-restart): its world and files stay.
    folder.mkdir(parents=True, exist_ok=True)
    port = lab.free_port()
    props = dict(PROPERTIES, **extra, **{'server-port': str(port)})
    (folder / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()))
    (folder / 'eula.txt').write_text('eula=true\n')
    if anticheat is not None:
        (folder / 'server.toml').write_text(anticheat)
    command = [bins / f'horizon-client{EXE}', '--resources', lab.RESOURCES, '--dedicated', '--nogui']
    server = Server(command, folder, out / f'{name}-server.log')
    server.wait_for(DONE, 300)
    return server, port


def bots(bins, port, out, name, count, behaviour, seconds, *extra):
    """myriade-bot, run to the end; its report."""
    report = out / f'{name}.json'
    subprocess.run([bins / f'myriade-bot{EXE}', '--server', f'127.0.0.1:{port}', '--bots', str(count),
                    '--behaviour', behaviour, '--seconds', str(seconds), '--join-every', '0.2',
                    '--name', name, '--report', report, *extra],
                   check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=seconds + 180)
    return json.loads(report.read_text())


def check_log_off(bins, scratch, out, seconds):
    """`log = ""` writes no flag log (docs/anticheat.md), and the checks still act and the console still hears
    of a kick: a bot attacking without swinging, as in `combat`, is kicked."""
    folder = scratch / 'log-off'
    server, port = start(bins, folder, '[anticheat]\nlog = ""\n', out, 'log-off', gamemode='survival',
                         difficulty='peaceful')
    try:
        server.console('gamerule respawn_radius 0')
        with ThreadPoolExecutor(2) as pool:
            target = pool.submit(bots, bins, port, out, 'LTarget', 1, 'stand', seconds + 10)
            server.wait_for('LTarget0 joined the game', 120)
            cheater = bots(bins, port, out, 'LNoSwing', 1, 'stand', seconds, '--attack', 'LTarget0',
                           '--no-swing')['per_bot'][0]
            target.result()
    finally:
        server.stop()
    if (folder / 'logs/anticheat.log').exists():
        raise Failed(f'log-off: a flag log was written:\n{(folder / "logs/anticheat.log").read_text()}')
    if 'disconnected' not in (cheater['error'] or ''):
        raise Failed(f'log-off: the no-swing bot was not kicked ({cheater["error"]})')
    told = [l for l in server.since(0) if f'[Anticheat] {cheater["name"]} failed combat' in l and 'kicked' in l]
    if not told:
        raise Failed('log-off: the console was not told of the kick')
    print(f'log-off: no flag log written; the no-swing bot was still kicked and the console told ({told[0].strip()})')


def run_pair(bins, folder, out, name, anticheat, seconds):
    """Legit bots and a timer cheat on one server, at the same time; (server lines, legit, cheat, log)."""
    server, port = start(bins, folder, anticheat, out, name)
    try:
        with ThreadPoolExecutor(2) as pool:
            legit = pool.submit(bots, bins, port, out, f'{name}Legit', 6, 'mixed', seconds)
            cheat = pool.submit(bots, bins, port, out, f'{name}Timer', 1, 'walk', seconds, '--timer', '1.5')
            legit, cheat = legit.result(), cheat.result()
    finally:
        server.stop()
    log = folder / 'logs/anticheat.log'
    return server.since(0), legit, cheat, log.read_text() if log.exists() else None


def check_off(bins, scratch, out, seconds):
    lines, legit, cheat, log = run_pair(bins, scratch / 'off', out, 'Off', OFF, seconds)
    if log is not None:
        raise Failed(f'off: an anticheat log was written:\n{log}')
    noise = [l for l in lines if 'Anticheat' in l]
    if noise:
        raise Failed('off: the anticheat spoke:\n' + '\n'.join(noise))
    for bot in legit['per_bot'] + cheat['per_bot']:
        if bot['error']:
            raise Failed(f'off: {bot["name"]} was dropped: {bot["error"]}')
    print(f'off: {legit["joined"] + cheat["joined"]} bots, the 1.5x one included, never flagged or kicked')
    check_off_acting(bins, scratch, out, seconds)


def check_off_acting(bins, scratch, out, seconds):
    """`enabled = false` silences the checks that act by default too (`combat` and `ground`): the bots that
    `check_combat` and `check_ground` show caught, on a survival server, are never flagged, dropped, kicked or
    banned, and their hits land. allow-flight is on, so vanilla's floating kick (which ends a no-fall bot hovering
    in the air whatever it claims) leaves the no-fall bot alone, and only the anticheat could kick it."""
    folder = scratch / 'off-acting'
    server, port = start(bins, folder, OFF, out, 'off-acting', gamemode='survival', difficulty='peaceful',
                         **{'allow-flight': 'true'})
    try:
        server.console('gamerule respawn_radius 0')
        server.console('gamerule natural_health_regeneration false')
        with ThreadPoolExecutor(4) as pool:
            targets = pool.submit(bots, bins, port, out, 'OffTarget', 2, 'stand', seconds + 10)
            server.wait_for('OffTarget1 joined the game', 120)
            runs = {
                'NoSwing': pool.submit(bots, bins, port, out, 'OffNoSwing', 1, 'stand', seconds,
                                       '--attack', 'OffTarget0', '--no-swing'),
                'Aura': pool.submit(bots, bins, port, out, 'OffAura', 1, 'stand', seconds,
                                    '--attack', 'OffTarget0,OffTarget1'),
                'NoFall': pool.submit(bots, bins, port, out, 'OffNoFall', 1, 'walk', seconds, '--ground'),
            }
            runs = {k: v.result() for k, v in runs.items()}
            runs['Target'] = targets.result()
    finally:
        server.stop()
    if (folder / 'logs/anticheat.log').exists():
        raise Failed(f'off: an anticheat log was written:\n{(folder / "logs/anticheat.log").read_text()}')
    lines = server.since(0)
    noise = [l for l in lines if 'Anticheat' in l or 'floating' in l]
    if noise:
        raise Failed('off: the anticheat spoke, or a bot was kicked for floating:\n' + '\n'.join(noise))
    for run in runs.values():
        for bot in run['per_bot']:
            if bot['error']:
                raise Failed(f'off: {bot["name"]} was dropped: {bot["error"]}')
    bans = folder / 'banned-players.json'
    if bans.exists() and json.loads(bans.read_text()):
        raise Failed(f'off: a player was banned: {bans.read_text()}')
    attacks = {name: runs[name]['per_bot'][0]['attacks'] for name in ('NoSwing', 'Aura')}
    if min(attacks.values()) < 10:
        raise Failed(f'off: the cheating bots attacked too little to show anything: {attacks}')
    health = {bot['name']: bot['health'] for bot in runs['Target']['per_bot']}
    # Nothing drops their attacks: the target both cheaters hit loses most of its health.
    if not health['OffTarget0'] < 10:
        raise Failed(f'off: the cheaters\' hits were dropped, the targets ended with {health} health')
    print(f'off: survival: the no-swing, two-target and no-fall bots are never flagged, kicked '
          f'or banned, and their hits land ({attacks} attacks; health left {health})')


def check_timer(bins, scratch, out, seconds):
    lines, legit, cheat, log = run_pair(bins, scratch / 'timer', out, 'On', ON, seconds)
    if not any('Anticheat on' in l for l in lines):
        raise Failed('timer: the server did not say the anticheat is on')
    log = log or ''
    for bot in legit['per_bot']:
        if bot['name'] in log:
            raise Failed(f'timer: legit {bot["name"]} was flagged:\n{log}')
        if bot['error']:
            raise Failed(f'timer: legit {bot["name"]} was dropped: {bot["error"]}')
    cheater = cheat['per_bot'][0]
    flags = log.count(f'{cheater["name"]} failed timer')
    if flags < 3:
        raise Failed(f'timer: the 1.5x bot was flagged {flags} times:\n{log}')
    if 'sent back' not in log or cheater['pulled_back'] == 0:
        raise Failed('timer: the 1.5x bot was never sent back')
    if not any(f'[Anticheat] {cheater["name"]} failed timer' in l for l in lines):
        raise Failed('timer: no alert on the console')
    if 'kicked' not in log or 'disconnected' not in (cheater['error'] or ''):
        raise Failed(f'timer: the 1.5x bot was not kicked ({cheater["error"]})')
    print(f'timer: 1.5x bot flagged {flags} times, sent back {cheater["pulled_back"]} times, alerted and '
          f'kicked; {legit["joined"]} legit bots never flagged')


def check_ground(bins, scratch, out, seconds):
    folder = scratch / 'ground'
    # allow-flight off: a dropped claim must never feed vanilla's floating kick.
    server, port = start(bins, folder, ON, out, 'ground', **{'gamemode': 'survival', 'allow-flight': 'false'})
    try:
        with ThreadPoolExecutor(4) as pool:
            runs = {
                'Walk': pool.submit(bots, bins, port, out, 'Walk', 3, 'walk', seconds, '--height', '0', '--ground'),
                'Stand': pool.submit(bots, bins, port, out, 'Stand', 2, 'stand', seconds),
                # They move, as the no-fall and jesus players of the official server did: a claim
                # standing still isn't judged (no fall to skip).
                'NoFall': pool.submit(bots, bins, port, out, 'NoFall', 1, 'walk', seconds, '--ground'),
                'Jesus': pool.submit(bots, bins, port, out, 'Jesus', 1, 'walk', seconds, '--height', '0',
                                     '--ground'),
            }
            # The jesus bot goes far from the others, then the ground under it turns to water.
            server.wait_for('Jesus0 joined the game', 120)
            server.console('tp Jesus0 200.5 -60 200.5')
            time.sleep(4)
            server.console('fill 176 -61 176 224 -61 224 minecraft:water')
            runs = {k: v.result() for k, v in runs.items()}
    finally:
        server.stop()
    log = folder / 'logs/anticheat.log'
    log = log.read_text() if log.exists() else ''
    for name in ('Walk', 'Stand'):
        for bot in runs[name]['per_bot']:
            if bot['name'] in log:
                raise Failed(f'ground: legit {bot["name"]} was flagged:\n{log}')
            if bot['error']:
                raise Failed(f'ground: legit {bot["name"]} was dropped: {bot["error"]}')
    for name, where in (('NoFall', 'in the air'), ('Jesus', 'on a fluid')):
        cheater = runs[name]['per_bot'][0]
        flags = log.count(f'{cheater["name"]} failed ground')
        if flags < 3 or f'claims the ground {where}' not in log:
            raise Failed(f'ground: {cheater["name"]} was flagged {flags} times ({where}):\n{log}')
        if 'disconnected' not in (cheater['error'] or ''):
            raise Failed(f'ground: {cheater["name"]} was not kicked ({cheater["error"]})')
        # The defaults act: the first kick warns of the ban the second brings.
        if not warned_of_ban(log, cheater['name'], 'ground'):
            raise Failed(f'ground: {cheater["name"]}: no kick with its ban count in the log:\n{log}')
    if 'sent back' in log:
        raise Failed(f'ground: a player was sent back (ground only drops the claim):\n{log}')
    if 'claim dropped' not in log:
        raise Failed(f'ground: no claim dropped in the log:\n{log}')
    console = (out / 'ground-server.log').read_text()
    if 'floating too long' in console:
        raise Failed('ground: vanilla\'s floating kick ended a dropped claim (only the anticheat kicks):\n'
                     + '\n'.join(l for l in console.splitlines() if 'floating' in l))
    walkers = runs['Walk']['joined'] + runs['Stand']['joined']
    print(f'ground: the no-fall and jesus bots are flagged, their claims dropped (never sent back), and kicked by the '
          f'anticheat (1 of 2 before a ban), never by the floating kick; {walkers} bots on the ground never flagged')


class Jitter:
    """A relay to the server on `port` that holds everything it carries 20-120 ms, in order: a player on
    a bad connection."""

    def __init__(self, port):
        self.server = port
        self.listener = socket.create_server(('127.0.0.1', 0))
        self.port = self.listener.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            try:
                client, _ = self.listener.accept()
            except OSError:
                return
            upstream = socket.create_connection(('127.0.0.1', self.server))
            for a, b in ((client, upstream), (upstream, client)):
                held = queue.Queue()
                threading.Thread(target=self.read, args=(a, held), daemon=True).start()
                threading.Thread(target=self.write, args=(b, held), daemon=True).start()

    @staticmethod
    def read(conn, held):
        due = 0.0
        while True:
            try:
                data = conn.recv(65536)
            except OSError:
                data = b''
            due = max(due, time.monotonic() + random.uniform(0.02, 0.12))
            held.put((due, data))
            if not data:
                return

    @staticmethod
    def write(conn, held):
        while True:
            due, data = held.get()
            time.sleep(max(0.0, due - time.monotonic()))
            if not data:
                conn.close()
                return
            try:
                conn.sendall(data)
            except OSError:
                return

    def close(self):
        self.listener.close()


def honest_play(keys, console, wait_for):
    """What the honest client does on the prediction's server, in a survival, peaceful flat world: walks,
    sprint-jumps, strafes and sneaks, with Speed and Jump Boost given and cleared, over packed ice and soul sand,
    swims, riptides, flies in creative, glides with an elytra and a firework, and hits a cow."""
    at = lambda command: console(f'execute as Vanilla at Vanilla run {command}')  # noqa: E731
    keys('look 0 0', 'down forward', then=3)
    keys('down sprint', 'down jump', then=4)
    keys('up jump', 'up sprint', 'down left', 'look 45 0', then=2)
    keys('up left', 'up forward', 'down sneak', 'down back', then=2)
    keys('up sneak', 'up back')
    console('effect give Vanilla minecraft:speed 30 2 true')
    console('effect give Vanilla minecraft:jump_boost 30 1 true')
    keys('down forward', 'down jump', then=4)
    keys('up jump', 'up forward')
    console('effect clear Vanilla')
    at('fill ~-3 ~-1 ~-3 ~3 ~-1 ~12 minecraft:packed_ice')
    keys('down forward', 'down sprint', 'down jump', then=3)
    keys('up jump', 'up sprint', 'up forward', then=1)
    at('fill ~-3 ~-1 ~-3 ~3 ~-1 ~12 minecraft:soul_sand')
    keys('down forward', 'down sprint', then=3)
    keys('up sprint', 'up forward', then=1)
    at('fill ~-3 ~-2 ~-3 ~3 ~-1 ~12 minecraft:water')
    keys('down forward', then=4)
    keys('down jump', then=2)
    keys('up jump', 'up forward')
    # Riptide, from the pool: a trident drawn for 25 ticks, then let go.
    console('item replace entity Vanilla weapon.mainhand with minecraft:trident'
            '[minecraft:enchantments={"minecraft:riptide":3}]')
    keys('down use', then=1.25)
    keys('up use', then=3)
    console('item replace entity Vanilla weapon.mainhand with minecraft:air')
    at('fill ~-4 ~-4 ~-4 ~4 ~-1 ~13 minecraft:stone')
    time.sleep(2)
    # Creative flight (double jump), then back to survival in the air.
    console('gamemode creative Vanilla')
    time.sleep(1)
    keys('down jump', then=0.1)
    keys('up jump', then=0.1)
    keys('down jump', then=0.1)
    keys('up jump', then=1)
    keys('down forward', then=3)
    keys('up forward', then=0.5)
    keys('down sneak', then=2)
    keys('up sneak', then=1)
    console('gamemode survival Vanilla')
    time.sleep(3)
    # Hits on a cow, half a second apart, looking down at it (for `combat`).
    at('summon minecraft:cow ~ ~ ~2')
    keys('look 0 25', then=0.5)
    for _ in range(4):
        keys('attack', then=0.5)
    keys('look 0 0', 'down forward', then=2)
    keys('up forward')
    console('kill @e[type=minecraft:cow]')
    at('setblock ^ ^ ^1 minecraft:stone')
    keys('down forward', 'down jump', then=2)
    keys('up forward', 'up jump', then=1)
    # An elytra from 60 blocks up (Resistance V against the landing), with a firework, then a teleport back down.
    console('effect give Vanilla minecraft:resistance 60 4 true')
    console('item replace entity Vanilla armor.chest with minecraft:elytra')
    console('item replace entity Vanilla weapon.mainhand with minecraft:firework_rocket')
    at('tp @s ~ ~60 ~ 0 0')
    time.sleep(1)
    keys('down jump', then=0.2)
    keys('up jump', then=0.5)
    keys('down jump', then=0.2)
    keys('up jump', then=0.5)
    keys('down use', then=0.2)
    keys('up use', then=4)
    at('tp @s ~ -60 ~')
    time.sleep(3)
    # A boat on a pool: placed, boarded with a click on it, paddled forward, left with a sneak.
    at('fill ~-2 ~-1 ~-2 ~2 ~-1 ~10 minecraft:water')
    time.sleep(1)
    at('summon minecraft:oak_boat ~ ~-0.1 ~2')
    time.sleep(1.5)
    keys('look 0 0', then=0.2)
    keys('down use', then=0.1)
    keys('up use', then=1)
    keys('down forward', then=2)
    keys('up forward', then=0.5)
    keys('down sneak', then=0.1)
    keys('up sneak', then=1)
    at('fill ~-3 ~-2 ~-3 ~3 ~-1 ~11 minecraft:stone')
    time.sleep(2)


def water_play(keys, console, wait_for):
    """In water as players are on the official server: sprint-swimming down and up a deep pool, held at the
    surface by jump, sinking, sneaking down, climbing out onto a ledge; then carried up a bubble column."""
    def tp(x, y, z, yaw=0, pitch=0):
        console(f'tp Vanilla {x} {y} {z} {yaw} {pitch}')
        time.sleep(1)

    def expect(*blocks):
        """Each (x, y, z, block) is in the server's world as built: `execute if block` says so, and the server
        answers with the tag (the old check read the client's world, which the server's world replaces)."""
        for x, y, z, block in blocks:
            tag = f'built-{x}-{y}-{z}'
            console(f'execute if block {x} {y} {z} {block} run say {tag}')
            try:
                wait_for(tag, 10)
            except RuntimeError:
                raise Failed(f'{x} {y} {z} is not {block} in the server world') from None
    # Above the flat ground (its top is at -61): a pool 10 deep, with a ledge one block above the water at its far end.
    console('fill 40 -60 0 50 -50 31 minecraft:stone')
    console('fill 41 -59 1 49 -50 29 minecraft:water')
    console('fill 41 -49 30 49 -49 31 minecraft:stone')
    # A bubble column over soul sand.
    console('fill 70 -60 0 72 -50 2 minecraft:stone')
    console('fill 71 -59 1 71 -50 1 minecraft:water')
    console('setblock 71 -60 1 minecraft:soul_sand')
    time.sleep(2)
    tp(45.5, -51, 3.5)
    keys('look 0 40', 'down forward', 'down sprint', then=1.5)
    keys('up forward', 'up sprint', 'look 0 -40', then=0.5)
    keys('down forward', 'down sprint', then=1.5)
    keys('up forward', 'up sprint', 'look 0 0', then=0.5)
    keys('down jump', then=3)
    keys('up jump', then=2)
    keys('down sneak', then=1.5)
    keys('up sneak', then=1)
    tp(45.5, -50.5, 25.5)
    keys('down jump', 'down forward', then=1.5)
    keys('up jump', 'up forward', then=1)
    tp(71.5, -59, 1.5)
    time.sleep(4)
    # A kelp forest in a pool 10 deep: kelp from the floor up to two blocks under the surface, grown (age 25),
    # so it doesn't change while the player swims in it.
    tp(85.5, -60, -2.5)
    console('fill 80 -60 0 90 -50 31 minecraft:stone')
    console('fill 81 -59 1 89 -50 29 minecraft:water')
    console('fill 81 -59 1 89 -53 29 minecraft:kelp_plant')
    console('fill 81 -52 1 89 -52 29 minecraft:kelp[age=25]')
    time.sleep(2)
    expect((85, -56, 10, 'minecraft:kelp_plant'), (85, -52, 10, 'minecraft:kelp[age=25]'),
           (85, -51, 10, 'minecraft:water[level=0]'))
    tp(85.5, -51, 3.5)
    keys('look 0 40', 'down forward', 'down sprint', then=1.5)
    keys('up forward', 'up sprint', 'look 0 -40', then=0.5)
    keys('down forward', 'down sprint', then=1.5)
    keys('up forward', 'up sprint', 'look 0 0', then=0.5)
    keys('down sneak', then=1.5)
    keys('up sneak', then=1)
    tp(85.5, -59, 20.5)
    keys('down forward', then=2)
    keys('down jump', then=1)
    keys('up jump', 'up forward', then=0.5)
    # A waterfall: a source 11 blocks up against a cliff falls to the ground and spreads over it. Jump held at
    # its foot, walking into it across the water it spreads, sinking down it from its top; then stopped and
    # started again over the player standing at its foot.
    tp(95.5, -60, -2.5)
    console('fill 101 -60 3 101 -48 7 minecraft:stone')
    console('setblock 100 -49 5 minecraft:water')
    time.sleep(6)
    expect((100, -55, 5, 'minecraft:water[level=8]'), (100, -60, 5, 'minecraft:water[level=8]'),
           (97, -60, 5, 'minecraft:water'))
    tp(100.5, -60, 5.5)
    keys('down jump', then=3)
    keys('up jump', then=1)
    tp(96.5, -60, 5.5, -90)
    keys('down forward', then=2)
    keys('down jump', then=2)
    keys('up jump', 'up forward', then=0.5)
    tp(100.5, -51, 5.5)
    time.sleep(3)
    tp(100.5, -60, 5.5)
    console('setblock 100 -49 5 minecraft:air')
    time.sleep(4)
    console('setblock 100 -49 5 minecraft:water')
    time.sleep(4)
    # A current: a channel one block deep whose source goes (the current dries up block by block) and comes
    # back, while the player stands and walks in it.
    tp(118.5, -60, -2.5)
    console('fill 120 -60 -1 124 -60 9 minecraft:stone')
    console('fill 122 -60 0 122 -60 8 minecraft:air')
    console('setblock 122 -60 0 minecraft:water')
    time.sleep(3)
    expect((122, -60, 3, 'minecraft:water[level=3]'), (122, -60, 8, 'minecraft:air'))
    tp(122.5, -60, 2.5)
    keys('down forward', then=1)
    keys('up forward', 'look 180 0', then=0.2)
    keys('down forward', then=1)
    keys('up forward', then=0.5)
    tp(122.5, -60, 4.5)
    console('setblock 122 -60 0 minecraft:air')
    time.sleep(3)
    console('setblock 122 -60 0 minecraft:water')
    time.sleep(3)
    keys('down forward', then=1)
    keys('up forward', then=0.5)
    console('setblock 122 -60 0 minecraft:air')
    keys('down back', then=2)
    keys('up back', then=1)
    # Waterlogged blocks in a pool 3 deep: bottom slabs for a floor, a row of fences across it, and two rows of
    # stairs up to its rim.
    tp(135.5, -60, -2.5)
    console('fill 130 -60 0 140 -57 12 minecraft:stone')
    console('fill 131 -59 1 139 -57 11 minecraft:water')
    console('fill 131 -59 1 139 -59 9 minecraft:stone_slab[type=bottom,waterlogged=true]')
    console('fill 131 -58 5 135 -58 5 minecraft:oak_fence[waterlogged=true]')
    console('fill 131 -59 10 139 -59 10 minecraft:oak_stairs[facing=south,waterlogged=true]')
    console('fill 131 -58 11 139 -58 11 minecraft:oak_stairs[facing=south,waterlogged=true]')
    time.sleep(2)
    expect((133, -59, 3, 'minecraft:stone_slab[waterlogged=true]'),
           (133, -58, 5, 'minecraft:oak_fence[waterlogged=true]'),
           (133, -58, 11, 'minecraft:oak_stairs[waterlogged=true]'))
    # Standing on the slabs, walking into the fences, around them, and up the stairs.
    tp(133.5, -58, 1.5)
    time.sleep(1)
    keys('down forward', then=2)
    keys('up forward', 'down left', then=2)
    keys('up left', 'down forward', 'down jump', then=4)
    keys('up forward', 'up jump', then=1)
    tp(30.5, -60, -10.5)


def items_play(keys, console, wait_for):
    """Items used on the move, as on the official server: a shield raised from the off hand while walking,
    hotbar slots switched with keys 1..4, a golden apple eaten and a bow drawn while walking; the shield raised
    and lowered in quick taps."""
    for slot, item in (('weapon.offhand', 'shield'), ('hotbar.0', 'diamond_sword'),
                       ('hotbar.1', 'golden_apple 8'), ('hotbar.2', 'bow'), ('hotbar.3', 'dirt 32'),
                       ('hotbar.4', 'arrow 32')):
        console(f'item replace entity Vanilla {slot} with minecraft:{item}')
    time.sleep(1)
    keys('hotbar 1', 'down use', 'down forward', then=2)
    keys('down sprint', then=1)
    keys('up sprint', 'hotbar 4', then=1)
    keys('down left', then=1)
    keys('up left', 'down jump', then=0.5)
    keys('up jump', 'up use', 'up forward', then=1)
    keys('hotbar 2', 'down forward', 'down use', then=2)
    keys('up use', 'up forward', then=1)
    keys('hotbar 3', 'down use', then=1.5)
    keys('up use', then=1)
    keys('hotbar 1', then=0.5)
    for _ in range(6):
        keys('down use', then=0.15)
        keys('up use', then=0.15)
    keys('up use', then=0.5)


def check_prediction(bins, scratch, out, seconds, name='prediction', play=honest_play):
    """The honest player is the vanilla client, through a relay, playing `play`: it is never flagged. A bot, which
    moves by its own rules and sends no keys, beside it is flagged and sent back."""
    folder = scratch / name
    server, port = start(bins, folder, PREDICTION, out, name, gamemode='survival', difficulty='peaceful')
    relay = Jitter(port)
    client = None
    try:
        with ThreadPoolExecutor(1) as pool:
            cheat = pool.submit(bots, bins, port, out, 'Cheat', 1, 'walk', seconds)
            try:
                client = VanillaClient(f'127.0.0.1:{relay.port}', 'Vanilla', folder / 'vanilla',
                                       out / f'{name}-client.log')
                if not client.ready.wait(300):
                    raise Failed(f'{name}: the vanilla client never got into the world')
                time.sleep(5)
                play(client, server.console, server.wait_for)
            finally:
                if client:
                    client.close()
                relay.close()
            cheat = cheat.result()
    finally:
        server.stop()
    log = folder / 'logs/anticheat.log'
    log = log.read_text() if log.exists() else ''
    if player_lines(log, 'Vanilla', 'prediction'):
        raise Failed(f'{name}: the vanilla client was flagged:\n{log}')
    cheater = cheat['per_bot'][0]
    flags = log.count(f'{cheater["name"]} failed prediction')
    if flags < 3 or 'sent back' not in log or cheater['pulled_back'] == 0:
        raise Failed(f'{name}: the bot was flagged {flags} times, sent back {cheater["pulled_back"]}:\n{log}')
    print(f'{name}: the vanilla client (20-120 ms jitter) was never flagged; the bot was flagged {flags} times '
          f'and sent back {cheater["pulled_back"]} times')


class VanillaClient:
    """The vanilla client, joined to `address` as `name`, its keys pressed by VanillaMover from `send`."""

    def __init__(self, address, name, work, log):
        import vanilla_client  # noqa: E402 (only this check runs it)
        classes = work / 'classes'
        classes.mkdir(parents=True)
        jars = os.pathsep.join(map(str, vanilla_client.classpath()))
        release = str(lab.config()[0]['minecraft']['java_major'])
        mover = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaMover.java'
        subprocess.run(['javac', '--release', release, '-proc:none', '-nowarn', '-cp', jars, '-d', classes, mover],
                       check=True, capture_output=True)
        args = argparse.Namespace(name=name, join=address, world=None, game_dir=work / 'client', assets=None,
                                  heap='2G', width=854, height=480, lan_port=None, backend=None)
        args.game_dir.mkdir()
        vanilla_client.write_options(args.game_dir, {})
        command = vanilla_client.command(args)
        cp = command.index('-cp') + 1
        command[cp] = os.pathsep.join([str(classes), command[cp]])
        command[command.index('net.minecraft.client.main.Main')] = 'dev.horizonlab.parity.VanillaMover'
        self.log = open(log, 'w')
        self.ready = threading.Event()
        self.positions = []
        self.hits = 0
        self.process = subprocess.Popen(command, cwd=args.game_dir, env=vanilla_client.environment(), text=True,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        threading.Thread(target=self.pump, daemon=True).start()

    def pump(self):
        for line in self.process.stdout:
            self.log.write(line)
            if '[VanillaMover] ready' in line:
                self.ready.set()
            elif '[VanillaMover] attacked ENTITY' in line:
                self.hits += 1
            elif '[VanillaMover] at ' in line:
                self.positions.append([float(v) for v in line.split('[VanillaMover] at ')[1].split()[:3]])

    def __call__(self, *lines, then=0.0):
        """Send `lines` ("down forward", "look 90 0"...), then wait `then` seconds."""
        for line in lines:
            self.process.stdin.write(line + '\n')
        self.process.stdin.flush()
        time.sleep(then)

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(20)
        except subprocess.TimeoutExpired:
            self.process.kill()
        self.log.close()


def vanilla_play(keys, console, wait_for):
    """The scenes of the prediction checks in one vanilla session: the walk, the swim and the flight, then the
    water and the items."""
    honest_play(keys, console, wait_for)
    water_play(keys, console, wait_for)
    items_play(keys, console, wait_for)


def check_prediction_vanilla(bins, scratch, out, seconds):
    folder = scratch / 'prediction-vanilla'
    toml = ('[anticheat]\nenabled = true\n[anticheat.timer]\nenabled = false\n[anticheat.ground]\nenabled = false\n'
            '[anticheat.prediction]\nsetback = 5\nalert = 15\nkick = 60\n')
    server, port = start(bins, folder, toml, out, 'prediction-vanilla', gamemode='survival', difficulty='peaceful')
    relay = Jitter(port)
    client = None
    try:
        try:
            client = VanillaClient(f'127.0.0.1:{relay.port}', 'Vanilla', folder / 'vanilla',
                                   out / 'prediction-vanilla-client.log')
            if not client.ready.wait(300):
                raise Failed('prediction-vanilla: the vanilla client never got into the world')
            time.sleep(5)
            client('where')
            vanilla_play(client, server.console, server.wait_for)
            client('where', then=1)
        finally:
            if client:
                client.close()
            relay.close()
    finally:
        server.stop()
    log = folder / 'logs/anticheat.log'
    if log.exists():
        shutil.copy(log, out / 'prediction-vanilla-anticheat.log')
        if 'Vanilla' in log.read_text():
            raise Failed(f'prediction-vanilla: the vanilla client was flagged:\n{log.read_text()}')
    if len(client.positions) < 2:
        raise Failed('prediction-vanilla: the vanilla client did not say where it was')
    (x0, _, z0), (x1, _, z1) = client.positions[0], client.positions[-1]
    walked = ((x1 - x0) ** 2 + (z1 - z0) ** 2) ** 0.5
    if client.hits == 0:
        raise Failed('prediction-vanilla: the vanilla client never hit the cow')
    if walked < 20:
        raise Failed(f'prediction-vanilla: the vanilla client only got {walked:.1f} blocks away: its keys did nothing')
    print(f'prediction-vanilla: the vanilla client (20-120 ms jitter) was never flagged; it ended {walked:.0f} blocks '
          f'from where it joined, and its {client.hits} hits on a cow were never flagged by combat')



def check_combat(bins, scratch, out, seconds):
    folder = scratch / 'combat'
    server, port = start(bins, folder, None, out, 'combat', gamemode='survival', difficulty='peaceful')
    try:
        # Everyone spawns on one spot, within reach of the others, and a hit's damage stays.
        server.console('gamerule respawn_radius 0')
        server.console('gamerule natural_health_regeneration false')
        with ThreadPoolExecutor(4) as pool:
            targets = pool.submit(bots, bins, port, out, 'Target', 3, 'stand', seconds + 10)
            server.wait_for('Target2 joined the game', 120)
            runs = {
                'Fair': pool.submit(bots, bins, port, out, 'Fair', 1, 'stand', seconds, '--attack', 'Target0'),
                'NoSwing': pool.submit(bots, bins, port, out, 'NoSwing', 1, 'stand', seconds,
                                       '--attack', 'Target1', '--no-swing'),
                'Aura': pool.submit(bots, bins, port, out, 'Aura', 1, 'stand', seconds,
                                    '--attack', 'Target1,Target2'),
            }
            runs = {k: v.result() for k, v in runs.items()}
            # Kicked once, the cheaters come back and cheat again: that kick bans (ban_after = 2), and
            # then the server refuses them.
            again = {
                'NoSwing': pool.submit(bots, bins, port, out, 'NoSwing', 1, 'stand', seconds,
                                       '--attack', 'Target1', '--no-swing'),
                'Aura': pool.submit(bots, bins, port, out, 'Aura', 1, 'stand', seconds,
                                    '--attack', 'Target1,Target2'),
            }
            again = {k: v.result() for k, v in again.items()}
            refused = bots(bins, port, out, 'NoSwing', 1, 'stand', 5)
            runs['Target'] = targets.result()
    finally:
        server.stop()
    bans = json.loads((folder / 'banned-players.json').read_text())
    log = folder / 'logs/anticheat.log'
    # The other checks only log by default, and the bots move by their own rules.
    log = '\n'.join(l for l in (log.read_text() if log.exists() else '').splitlines() if 'failed combat' in l)
    fair = runs['Fair']['per_bot'][0]
    if fair['attacks'] < 10:
        raise Failed(f'combat: the fair bot attacked {fair["attacks"]} times')
    for bot in runs['Fair']['per_bot'] + runs['Target']['per_bot']:
        if bot['name'] in log:
            raise Failed(f'combat: legit {bot["name"]} was flagged:\n{log}')
    health = {bot['name']: bot['health'] for bot in runs['Target']['per_bot']}
    # The fair bot's hits land; the cheaters' land only until their level reaches `setback`.
    if not health['Target0'] < 10 < min(health['Target1'], health['Target2']):
        raise Failed(f'combat: the targets ended with {health} health')
    for name, what in (('NoSwing', 'attacked without swinging'), ('Aura', 'in one tick')):
        cheater = runs[name]['per_bot'][0]
        flags = [l for l in log.splitlines() if f'{cheater["name"]} failed combat' in l]
        if len(flags) < 3 or not all(what in l for l in flags):
            raise Failed(f'combat: {cheater["name"]} was flagged {len(flags)} times ({what}):\n{log}')
        if not any('attacks dropped' in l for l in flags):
            raise Failed(f'combat: {cheater["name"]}\'s attacks were never dropped:\n{log}')
        if 'disconnected' not in (cheater['error'] or ''):
            raise Failed(f'combat: {cheater["name"]} was not kicked ({cheater["error"]})')
    banned = {b['name']: b for b in bans}
    for name in ('NoSwing', 'Aura'):
        cheater = again[name]['per_bot'][0]
        ban = banned.get(cheater['name'])
        if not ban or ban.get('reason') != 'Horizon anticheat: combat' or ban.get('source') != 'Horizon anticheat':
            raise Failed(f'combat: {cheater["name"]} came back, cheated again and was not banned: {bans}')
        if 'disconnected' not in (cheater['error'] or ''):
            raise Failed(f'combat: banned {cheater["name"]} was not sent away ({cheater["error"]})')
    if len(banned) != 2:
        raise Failed(f'combat: only the two cheaters are banned: {bans}')
    if 'banned' not in (refused['per_bot'][0]['error'] or '').lower():
        raise Failed(f'combat: banned NoSwing logged in again ({refused["per_bot"][0]["error"]})')
    lines = (folder / 'logs/anticheat.log').read_text()
    if not all(kicked_before_banned(lines, f'{n}0', 'combat') for n in ('NoSwing', 'Aura')):
        raise Failed(f'combat: the log doesn\'t show the kick before the ban:\n{lines}')
    print(f'combat: the no-swing and two-target bots are flagged, their attacks dropped, and kicked; back, they '
          f'cheat again and are banned, then refused; the fair bot attacked {fair["attacks"]} times, never '
          f'flagged; health left: {health}')


def cheat_across_a_restart(bins, folder, out, name, anticheat, prefix, seconds):
    """A survival server in `folder` with `anticheat` (server.toml), run twice: each time a bot attacking a
    standing target without swinging (`<prefix>NoSwing0`, the same player both times) plays until it is sent away.
    Returns, for each run, the bot's report and `anticheat-strikes.json` after it (None without the file)."""
    rounds = []
    for run in ('first', 'second'):
        server, port = start(bins, folder, anticheat, out, f'{name}-{run}', gamemode='survival',
                             difficulty='peaceful')
        try:
            server.console('gamerule respawn_radius 0')
            with ThreadPoolExecutor(2) as pool:
                target = pool.submit(bots, bins, port, out, f'{prefix}Target', 1, 'stand', seconds + 10)
                server.wait_for(f'{prefix}Target0 joined the game', 120)
                cheater = bots(bins, port, out, f'{prefix}NoSwing', 1, 'stand', seconds, '--attack',
                               f'{prefix}Target0', '--no-swing')['per_bot'][0]
                target.result()
        finally:
            server.stop()
        strikes = folder / 'anticheat-strikes.json'
        rounds.append((cheater, json.loads(strikes.read_text()) if strikes.exists() else None))
    return rounds


def check_strikes_off(bins, scratch, out, seconds):
    """`strikes = ""` keeps a player's kicks only while the server runs (docs/anticheat.md): the no-swing bot of
    `combat-restart`, kicked once, comes back after a restart and is kicked again as a first kick, not banned, and
    no strikes file is written."""
    folder = scratch / 'strikes-off'
    (first, after_first), (second, after_second) = cheat_across_a_restart(
        bins, folder, out, 'strikes-off', '[anticheat]\nstrikes = ""\n', 'S', seconds)
    log = (folder / 'logs/anticheat.log').read_text()
    for run, bot in (('first', first), ('second', second)):
        if 'disconnected' not in (bot['error'] or ''):
            raise Failed(f'strikes-off: the no-swing bot was not kicked the {run} time ({bot["error"]})')
    if after_first is not None or after_second is not None:
        raise Failed(f'strikes-off: a strikes file was written: {after_first} then {after_second}')
    kicks = [l for l in log.splitlines() if 'SNoSwing0 failed combat' in l and 'kicked' in l]
    if len(kicks) != 2 or not all('kicked (1 of 2 before a ban)' in l for l in kicks):
        raise Failed(f'strikes-off: after the restart the kick was not a first kick again:\n{log}')
    bans = folder / 'banned-players.json'
    if bans.exists() and any(b['name'] == 'SNoSwing0' for b in json.loads(bans.read_text())):
        raise Failed(f'strikes-off: the bot was banned: {bans.read_text()}')
    print('strikes-off: no strikes file; after the restart the no-swing bot\'s kick was a first kick again '
          '(1 of 2 before a ban), and it was not banned')


def check_combat_restart(bins, scratch, out, seconds):
    """docs/anticheat.md: a check's kicks are counted "across sessions and restarts". The first kick of a
    no-swing bot is saved in `anticheat-strikes.json`; after a restart on the same folder, the next one bans."""
    folder = scratch / 'combat-restart'
    (first, after_first), (second, after_second) = cheat_across_a_restart(bins, folder, out, 'combat-restart',
                                                                          None, 'R', seconds)
    log = (folder / 'logs/anticheat.log').read_text()
    if 'disconnected' not in (first['error'] or ''):
        raise Failed(f'combat-restart: the no-swing bot was not kicked the first time ({first["error"]})')
    if not after_first or [counts.get('combat') for counts in after_first.values()] != [1]:
        raise Failed(f'combat-restart: the first kick was not saved as one strike: {after_first}')
    if 'RNoSwing0 failed combat' not in log or 'kicked (1 of 2 before a ban)' not in log:
        raise Failed(f'combat-restart: no first kick with its ban count in the log:\n{log}')
    bans = json.loads((folder / 'banned-players.json').read_text())
    ban = next((b for b in bans if b['name'] == 'RNoSwing0'), None)
    if not ban or ban.get('reason') != 'Horizon anticheat: combat':
        raise Failed(f'combat-restart: back after the restart, the bot cheated again and was not banned: {bans}')
    if 'disconnected' not in (second['error'] or ''):
        raise Failed(f'combat-restart: the banned bot was not sent away ({second["error"]})')
    # The ban starts the count again (docs/anticheat.md).
    if [counts.get('combat') for counts in after_second.values()] != [0]:
        raise Failed(f'combat-restart: the ban did not start the count again: {after_second}')
    print('combat-restart: kicked once (1 strike saved), the server restarted on the same folder, and the same '
          'bot, cheating again, was banned (its count back at 0)')


def check_prediction_water(bins, scratch, out, seconds):
    check_prediction(bins, scratch, out, seconds, 'prediction-water', water_play)


def check_prediction_items(bins, scratch, out, seconds):
    check_prediction(bins, scratch, out, seconds, 'prediction-items', items_play)


CHECKS = {'off': check_off, 'timer': check_timer, 'ground': check_ground, 'prediction': check_prediction,
          'prediction-water': check_prediction_water, 'prediction-items': check_prediction_items,
          'log-off': check_log_off,
          'prediction-vanilla': check_prediction_vanilla,
          'combat': check_combat, 'combat-restart': check_combat_restart, 'strikes-off': check_strikes_off}
# Run when no --check is given. The vanilla client needs a screen session, so the prediction checks, which play
# it, and prediction-vanilla run only when asked.
DEFAULT = ['combat', 'combat-restart', 'ground', 'log-off', 'off', 'strikes-off', 'timer']


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin-dir', default=str(lab.ROOT / 'target/fast'))
    parser.add_argument('--check', action='append', choices=sorted(CHECKS))
    parser.add_argument('--seconds', type=int, default=40)
    parser.add_argument('--out', default=str(lab.LOCAL / 'anticheat-e2e'))
    args = parser.parse_args()
    bins, out = Path(args.bin_dir).resolve(), Path(args.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    # One set of scratch HORIZON_HOME and XDG folders for every server below.
    os.environ.update(lab.game_env())
    failed = []
    with lab.temp_folder('anticheat-e2e-') as scratch:
        for name in args.check or DEFAULT:
            try:
                CHECKS[name](bins, Path(scratch), out, args.seconds)
            except (Failed, RuntimeError) as e:
                failed.append(name)
                print(f'FAILED {e}')
    if failed:
        sys.exit(f'anticheat_e2e: {", ".join(failed)} failed (logs in {out})')


if __name__ == '__main__':
    main()
