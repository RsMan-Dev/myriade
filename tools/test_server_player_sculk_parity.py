#!/usr/bin/env python3
"""Both players' ordinary walking activates a sculk sensor on the server.

Exercise Entity.move movement emission through bot input (`move` each tick), alongside
sculk's deterministic vanilla oracle. The host's world runs on the dedicated server; the
guest uses the same server-player path. The sensor is read from the server (`block`).
"""
import math
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from bot_lab import Bot  # noqa: E402
from server_lab import Server  # noqa: E402

HOST, GUEST = 'Alpha', 'Beta'
WALK_TICKS, IDLE_TICKS, STEP = 15, 85, 0.2


def until(bot, observe, predicate, ticks=100):
    value = observe()
    for _ in range(ticks):
        if predicate(value):
            return value
        bot.wait_ticks(1)
        value = observe()
    assert predicate(value), value
    return value


def phase(server, sensor):
    """The sensor's phase, read twice through the server's control socket: `block`, then `blocks` over the sensor's
    own box. The server is the witness; the two reads must agree."""
    by_block = server.block(*sensor)['properties'].get('sculk_sensor_phase')
    by_box = server.blocks(*sensor, *sensor)[0]['properties'].get('sculk_sensor_phase')
    assert by_block == by_box, (by_block, by_box)
    return by_block


def server_player(server, name):
    return next(p['pos'] for p in server.entities_server()['players'] if p['name'] == name)


def wait_for_quiet_sensor(host, server, sensor, ticks=100):
    # Inactive does not mean idle: a selected vibration still has to travel to
    # the sensor. Require a quiet window longer than its eight-block range,
    # letting any setup vibration finish its active and cooldown phases first.
    quiet = 0
    state = None
    for _ in range(ticks):
        host.wait_ticks(1)
        state = phase(server, sensor)
        quiet = quiet + 1 if state == 'inactive' else 0
        if quiet == 10:
            return
    raise AssertionError(f'sensor did not settle during setup: {state}')


def check_walk(server, host, guest, role):
    player = host if role == 'host' else guest
    name = HOST if role == 'host' else GUEST
    hx, y, hz = (math.floor(v) for v in host.state()['position'])
    x, z = hx + 6, hz + (-8 if role == 'host' else 8)
    sensor = (x + 3, y, z + 1)
    player.command('gamemode survival')
    host.command(f'fill {x - 2} {y - 1} {z - 2} {x + 5} {y - 1} {z + 5} minecraft:stone')
    host.command(f'fill {x - 2} {y} {z - 2} {x + 5} {y + 3} {z + 5} minecraft:air')
    start = (x + 0.5, y, z + 0.5)
    player.command(f'tp @s {start[0]} {start[1]} {start[2]} 0 0')
    until(player, lambda: player.state()['position'], lambda p: math.dist(p, start) < 0.1)
    player.look(0, 0)
    player.wait_ticks(2)
    host.command(f'setblock {sensor[0]} {sensor[1]} {sensor[2]} minecraft:sculk_sensor')
    try:
        wait_for_quiet_sensor(host, server, sensor)
        # The sensor stays active 30 ticks: the server's sensor is sampled every tick while the player walks
        # forward (one `move` per tick), then stands still.
        samples = []
        for i in range(WALK_TICKS):
            player.move(start[0], start[1], start[2] + STEP * (i + 1))
            player.wait_ticks(1)
            samples.append(phase(server, sensor))
        for _ in range(IDLE_TICKS):
            player.wait_ticks(1)
            samples.append(phase(server, sensor))
        moved = {'client': math.dist(player.state()['position'], start),
                 'server': math.dist(server_player(server, name), start)}
        assert all(d > 1 for d in moved.values()), moved
        active = next((i for i, s in enumerate(samples) if s == 'active'), None)
        assert active is not None, f'sensor never active: {samples}'
        return {'moved': moved, 'first_active_sample': active}
    finally:
        host.command(f'fill {x - 2} {y} {z - 2} {x + 5} {y + 3} {z + 5} minecraft:air')


def main():
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-game', '-p', 'myriade-engine-lib',
                    '--bin', 'myriade-engine'], cwd=lab.ROOT, check=True)
    bins = Path(lab.cargo_target()) / 'fast'
    out = lab.LOCAL / 'server-player-sculk-parity'
    out.mkdir(parents=True, exist_ok=True)
    report = {}
    with Server(bins, out / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot() as host, Bot() as guest:
        server.command(f'op {HOST}')
        server.command(f'op {GUEST}')
        host.join('127.0.0.1', server.port, HOST)
        guest.join('127.0.0.1', server.port, GUEST)
        for role in ('host', 'guest'):
            report[f'sculk-walk-{role}'] = {'pass': True, **check_walk(server, host, guest, role)}
    lab.write_json(out / 'report.json', report)
    assert all(r['pass'] for r in report.values()), report


if __name__ == '__main__':
    main()
