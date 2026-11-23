#!/usr/bin/env python3
"""A dedicated server saves connected players on save-all/stop/SIGTERM and keeps safe data after a panic.

`MinecraftServer.runServer` catches what the tick throws and still stops the server. Horizon's
`stop_after_crash` does the same, but a panic can interrupt a guest's turn (the turn's state is
half done, and is written back into the player's record as the panic leaves the turn) or a
network tick that has the hub out. `HORIZON_CRASH_FIXTURE` makes the tick panic at one of those
points. Each run below joins a bot (joueur-automate), panics the server, and
checks, from the files the server leaves:

- Connected player: save-all writes five diamonds while the player stays joined;
  stop writes the newer inventory of twelve diamonds.
- `guest`: the panic comes in the joined player's turn, with the hub in the game. The log says
  that player keeps their last saved file; the player's files are byte for byte what the previous
  clean stop wrote, and a block placed before the panic is still there on the next start.
- `hub`: the panic comes in the network tick, which has the hub out, with a player in. The log
  says no player is saved; same checks.
"""
import hashlib
import os
import shutil
import signal
import subprocess
import sys
import time
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import anvil_dump  # noqa: E402
import lab  # noqa: E402
from bot_lab import Bot  # noqa: E402
from server_lab import Server  # noqa: E402

PLAYER = 'Tester'


def bins():
    return Path(lab.cargo_target()) / 'fast'


def start(folder, fixture=None):
    if fixture:
        os.environ['HORIZON_CRASH_FIXTURE'] = fixture
    try:
        return Server(bins(), folder, lab.RESOURCES.resolve(), ready_seconds=600)
    finally:
        os.environ.pop('HORIZON_CRASH_FIXTURE', None)


def offline_uuid(name):
    """`UUIDUtil.createOfflinePlayerUUID`."""
    digest = bytearray(hashlib.md5(f'OfflinePlayer:{name}'.encode()).digest())
    digest[6] = digest[6] & 0x0F | 0x30
    digest[8] = digest[8] & 0x3F | 0x80
    return str(uuid.UUID(bytes=bytes(digest)))


def player_files(directory):
    """The joined player's files (data, advancements, stats), by content. The server's own profile is
    not theirs: its files are written by every save."""
    folder = directory / 'world' / 'players'
    me = offline_uuid(PLAYER)
    return {str(p.relative_to(folder)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(folder.rglob(f'*{me}*')) if p.is_file()}


def crash(directory, fixture, trigger):
    server = start(directory, fixture)
    try:
        server.command('forceload add 0 0')
        # The chunk loads a moment after the forceload.
        for _ in range(100):
            answer = server.command(f'setblock 0 {200 + trigger["y"]} 0 minecraft:gold_block')
            time.sleep(0.5)
            if any('Changed the block' in l for l in answer):
                break
        else:
            raise AssertionError('the block was never placed:\n' + '\n'.join(server.log_lines()[-20:]))
        with Bot() as bot:
            bot.join('127.0.0.1', server.port, PLAYER)
            server.wait_log(rf'{PLAYER} joined the game', seconds=300)
            deadline = time.time() + 120
            while server.process.poll() is None and time.time() < deadline:
                time.sleep(0.2)
        assert server.process.poll() not in (None, 0), 'the server did not exit with an error after the panic'
    finally:
        server.stop()
    return '\n'.join(server.log_lines())


def connected_saves(build):
    """Save-all and stop persist a player who is still connected, before logout can save them."""
    directory = build / 'connected-server'
    server = start(directory)
    file = directory / 'world' / 'players' / 'data' / f'{offline_uuid(PLAYER)}.dat'

    def diamonds():
        player = anvil_dump.plain(anvil_dump.read_dat(file), full=True)
        return sum(item['count'] for item in player['Inventory'] if item['id'] == 'minecraft:diamond')

    try:
        with Bot() as client:
            client.join('127.0.0.1', server.port, PLAYER)
            client.wait_for('loaded', timeout=300)
            server.wait_log(rf'{PLAYER} joined the game', seconds=300)
            assert not file.exists(), 'the player was already saved before save-all'
            server.command(f'give {PLAYER} minecraft:diamond 5')
            server.command('save-all flush')
            # Console commands execute in order: the marker follows the save barrier.
            server.command('say SAVED-CONNECTED-PLAYER')
            server.wait_log(r'\[Server\] SAVED-CONNECTED-PLAYER')
            assert not any(f'{PLAYER} left the game' in line for line in server.log_lines())
            assert diamonds() == 5, 'save-all did not persist the connected player inventory'
            server.command(f'give {PLAYER} minecraft:diamond 7')
            server.command('say STOP-WITH-CONNECTED-PLAYER')
            server.wait_log(r'\[Server\] STOP-WITH-CONNECTED-PLAYER')
            assert not any(f'{PLAYER} left the game' in line for line in server.log_lines())
            server.stop()
            assert diamonds() == 12, 'stop did not persist the newer connected player inventory'
    finally:
        server.stop()
    print('connected player save-all and stop: ok', flush=True)


def signal_saves(build):
    """SIGTERM stops the server as `stop` does (vanilla's shutdown hook): a connected player's
    inventory and ender chest changed since the last save are in their file afterwards."""
    directory = build / 'signal-server'
    server = start(directory)
    file = directory / 'world' / 'players' / 'data' / f'{offline_uuid(PLAYER)}.dat'
    try:
        with Bot() as client:
            client.join('127.0.0.1', server.port, PLAYER)
            client.wait_for('loaded', timeout=300)
            server.wait_log(rf'{PLAYER} joined the game', seconds=300)
            server.command(f'give {PLAYER} minecraft:diamond 9')
            server.command(f'item replace entity {PLAYER} enderchest.0 with minecraft:emerald 5')
            server.command('say GIVEN-BEFORE-SIGTERM')
            server.wait_log(r'\[Server\] GIVEN-BEFORE-SIGTERM')
            assert not file.exists(), 'the player was saved before the signal'
            server.process.send_signal(signal.SIGTERM)
            try:
                server.process.wait(timeout=120)
            except subprocess.TimeoutExpired:
                raise AssertionError('the server did not stop within 120 s of SIGTERM')
            assert any('Saving players' in line for line in server.log_lines()), \
                'SIGTERM ended the server without saving:\n' + '\n'.join(server.log_lines()[-10:])
            assert file.exists(), 'SIGTERM ended the server without writing the player file'
            player = anvil_dump.plain(anvil_dump.read_dat(file), full=True)
            diamonds = sum(i['count'] for i in player['Inventory'] if i['id'] == 'minecraft:diamond')
            emeralds = sum(i['count'] for i in player.get('EnderItems', []) if i['id'] == 'minecraft:emerald')
            assert (diamonds, emeralds) == (9, 5), f'saved {diamonds} diamonds and {emeralds} ender chest emeralds, not 9 and 5'
    finally:
        server.stop()
    print('connected player saved on SIGTERM: ok', flush=True)


def main():
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-game', '-p', 'horizon-client-lib', '--bin', 'horizon-client'], cwd=lab.ROOT, check=True)
    build = lab.LOCAL / 'dedicated-crash-save'
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    directory = build / 'server'
    problems = []
    connected_saves(build)
    signal_saves(build)

    # A clean stop writes the player's file.
    server = start(directory)
    try:
        with Bot() as bot:
            bot.join('127.0.0.1', server.port, PLAYER)
            bot.leave()
        server.wait_log(rf'{PLAYER} left the game', seconds=60)
    finally:
        server.stop()
    saved = player_files(directory)
    if not saved:
        problems.append('the clean run wrote no player file')

    for fixture, y, trigger, expect in (
        ('guest', 0, {'y': 0}, "The panic interrupted a player's turn: that player keeps the last saved file"),
        ('hub', 1, {'y': 1}, 'The panic interrupted the network tick'),
    ):
        log = crash(directory, fixture, trigger)
        if f'The server thread panicked: crash fixture at {fixture}' not in log:
            problems.append(f'{fixture}: no panic in the log')
        if expect not in log:
            problems.append(f'{fixture}: the log does not say {expect!r}')
        if 'Saving worlds' not in log:
            problems.append(f'{fixture}: the world was not saved')
        if player_files(directory) != saved:
            problems.append(f'{fixture}: a player file changed: {saved} -> {player_files(directory)}')
        # The next start finds the block placed before the panic.
        server = start(directory)
        try:
            server.command('forceload add 0 0')
            time.sleep(3)
            server.command(f'execute if block 0 {200 + y} 0 minecraft:gold_block run say FOUND-{fixture}')
            server.wait_log(rf'\[Server\] FOUND-{fixture}', seconds=60)
        except RuntimeError:
            problems.append(f'{fixture}: the block placed before the panic is gone')
        finally:
            server.stop()
    for p in problems:
        print('FAIL', p)
    print('dedicated crash save:', 'ok' if not problems else 'FAILED')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
