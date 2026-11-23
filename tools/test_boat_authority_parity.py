#!/usr/bin/env python3
"""Boats on a server: a rider bot (Alpha) boards a boat, drives it, rows and strikes it; an observer bot (Beta) reads
the boat in `entities` (pos, data) tick by tick. Server part only, qualitative: the authority of the pilot over the
boat's moves, the paddle bits seen as the boat's data (index 11 left, 12 right), the destruction of the boat by a hit.
The bit-for-bit comparison with VanillaBoatAuthorityOracle was abandoned (see the journal). Families a client runs
(the boat's motion predicted on its own screen, the remote rowers' animation) are not checked here.
Dismounting is a crouch: player_input(shift=True).
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from bot_lab import Bot  # noqa: E402
from server_lab import Server  # noqa: E402


def cases():
    out = []
    for spin in (0.0, 0.125, -0.5, 2.0, -17.0):
        for velocity in ([0.0, 0.0, 0.0], [0.125, -0.0625, -0.25], [-1.0, 0.5, 0.75]):
            for owned in ([False], [True], [True, False], [True, False, True],
                          [False, True, True], [True, True, False, False, True]):
                out.append(dict(name=f'authority-{len(out)}', spin=spin, velocity=velocity, owned=owned))
    # A rider who leaves while the boat is moving, sits out long enough for the
    # empty boat to stop, and boards again (#6589): the re-boarded boat must
    # keep the velocity it had while empty, not the one the rider left with.
    for spin in (0.0, 0.125, -17.0):
        for velocity in ([1.0, 0.25, -0.5], [-0.125, 0.0, 0.0625]):
            for empty in (5, 25, 60):
                for back in (1, 5, 12):
                    owned = [True] * 8 + [False] + [False] * empty + [True] * back
                    out.append(dict(name=f'reboard-{len(out)}', spin=spin, velocity=velocity, owned=owned))
            # Empty from the first tick, then boarded: the boat has only ever
            # moved while nobody rode it.
            out.append(dict(name=f'reboard-{len(out)}', spin=spin, velocity=velocity,
                            owned=[False] * 40 + [True] * 8))
    # Remote observers keep rowing smoothly, then an empty boat ignores stale paddle bits.
    for initial in ([0.0, 0.0], [1.5707964, 0.7853982], [7.0, 3.0]):
        for flags in ([True, True], [True, False], [False, True]):
            steps = [dict(occupied=True, flags=flags) for _ in range(8)]
            steps += [dict(occupied=False, flags=flags) for _ in range(3)]
            steps += [dict(occupied=False, flags=[False, False])]
            steps += [dict(occupied=True, flags=flags) for _ in range(3)]
            out.append(dict(name=f'paddles-{len(out)}', spin=0, velocity=[0, 0, 0],
                            paddles=dict(initial=initial, steps=steps)))
    for flags in ([True, True], [True, False], [False, True]):
        steps = [dict(occupied=True, owned=True, flags=flags) for _ in range(5)]
        steps += [dict(occupied=False, flags=flags) for _ in range(2)]
        steps += [dict(occupied=True, owned=True, flags=[False, False])]
        steps += [dict(occupied=True, owned=True, flags=flags) for _ in range(5)]
        out.append(dict(name=f'paddles-owned-{len(out)}', spin=0, velocity=[0, 0, 0],
                        paddles=dict(initial=[0, 0], steps=steps)))
    # A non-player in the first seat is not a paddle controller, even if a
    # player occupies the other seat and metadata still has true paddle bits.
    out.append(dict(name='paddles-mob-first', spin=0, velocity=[0, 0, 0],
                    paddles=dict(initial=[1.0, 2.0], steps=[
                        dict(occupied=True, first_player=False, flags=[True, True]),
                        dict(occupied=True, first_player=False, flags=[True, False]),
                        dict(occupied=True, first_player=True, flags=[True, True]),
                        dict(occupied=False, flags=[True, True]),
                    ])))
    # Player.causeExtraKnockback's nonliving branch, through the actual boat
    # attack in Horizon: uncharged/non-sprinting controls, axis and oblique yaw,
    # retained target motion and the attacker's slowdown/sprint cancellation.
    for yaw in (0.0, 90.0, 180.0, -90.0, 37.0):
        for sprint in (False, True):
            for ticks in (0, 4, 5, 100):
                for velocity in ([0.0, 0.0, 0.0], [0.125, -0.0625, -0.25]):
                    for knockback in (0, 1, 2):
                        out.append(dict(name=f'attack-{len(out)}', spin=0, velocity=velocity, owned=[],
                                        attack=dict(yaw=yaw, sprint=sprint, ticks=ticks, knockback=knockback, creative=False,
                                                    player_velocity=[0.25, 0.42, -0.1])))
    for sprint in (False, True):
        for knockback in (0, 1, 2):
            out.append(dict(name=f'attack-destroyed-{len(out)}', spin=0, velocity=[0.0, 0.0, 0.0], owned=[],
                            attack=dict(yaw=0.0, sprint=sprint, ticks=100, knockback=knockback,
                                        creative=True, player_velocity=[0.25, 0.42, -0.1])))
    return out


FLOOR = 99
ALPHA, BETA = 'Alpha', 'Beta'
BOAT = 'minecraft:oak_boat'
ZOMBIE = 'minecraft:zombie'
PADDLE_INDEX = {'left': 11, 'right': 12}


def family(case):
    return case['name'].rsplit('-', 1)[0]


def arena(server, rider, observer, mob_first):
    """A flat stone floor under open air; the rider stands on it and boards a boat summoned beside it. With mob_first,
    a zombie takes the first seat before the rider boards."""
    server.command(f'fill -8 {FLOOR} -8 8 {FLOOR} 8 minecraft:stone')
    server.command(f'fill -8 {FLOOR + 1} -8 8 {FLOOR + 8} 8 minecraft:air')
    server.command(f'kill @e[type={BOAT}]')
    server.command(f'kill @e[type={ZOMBIE}]')
    rider.command(f'tp {ALPHA} 0.5 {FLOOR + 1} 0.5 0 0')
    observer.command(f'tp {BETA} 4.5 {FLOOR + 1} 0.5 0 0')
    server.command(f'summon {BOAT} 0.5 {FLOOR + 1} 0.5')
    boat = observer.entities(entity_type=BOAT)['entities'][0]['id']
    if mob_first:
        server.command(f'summon {ZOMBIE} 0.5 {FLOOR + 1} 0.5')
        server.command(f'ride @e[type={ZOMBIE},limit=1] mount @e[type={BOAT},limit=1]')
    rider.interact(boat)
    return boat


def boat_of(observer, boat):
    return next((e for e in observer.entities(entity_type=BOAT)['entities'] if e['id'] == boat), None)


def paddle_bits(entity):
    data = {d['index']: d['value'] for d in (entity or {}).get('data', [])}
    return [data.get(PADDLE_INDEX['left']), data.get(PADDLE_INDEX['right'])]


def dismount(rider, observer):
    """Leaves the boat by a crouch (shift held for one tick, then released)."""
    rider.player_input(shift=True)
    observer.wait_ticks(1)
    rider.player_input()


def drive_owned(rider, observer, case, boat, problems):
    """Authority: while the rider is the boat's controller, the server takes its moves (the boat follows them)."""
    riding = True
    for i, owned in enumerate(case['owned']):
        if not owned:
            if riding:
                dismount(rider, observer)
                riding = False
            observer.wait_ticks(1)
            continue
        if not riding:
            rider.interact(boat)
            riding = True
        pos = [0.5 + 0.1 * i, FLOOR + 1.0, 0.5]
        rider.vehicle_move(pos, 0.0, 0.0)
        observer.wait_ticks(1)
        seen = (boat_of(observer, boat) or {}).get('pos')
        if seen is None or max(abs(a - b) for a, b in zip(seen, pos)) > 1e-6:
            problems.append(f'tick {i}: boat at {seen}, rider moved it to {pos}')


def drive_paddles(rider, observer, case, boat, problems):
    """Paddles: the rider's paddle bits reach an observer as the boat's synced data, while the rider sits in it."""
    riding = True
    for i, step in enumerate(case['paddles']['steps']):
        if not step['occupied']:
            if riding:
                dismount(rider, observer)
                riding = False
            observer.wait_ticks(1)
            continue
        if not riding:
            rider.interact(boat)
            riding = True
        flags = step['flags']
        rider.paddle(flags[0], flags[1])
        observer.wait_ticks(1)
        seen = paddle_bits(boat_of(observer, boat))
        if seen != flags:
            problems.append(f'tick {i}: observer sees paddles {seen}, rider sent {flags}')


def strike(rider, observer, case, boat, problems):
    """One hit on the boat. VehicleEntity.hurtServer (javap -p -c, runtime/vanilla-26.3/server-26.3.jar) : a creative
    attacker discards the boat with no drop (`instabuild` then `discard()`); otherwise it adds 10 * amount to damage
    and destroys the boat only past 40 (`getDamage() > 40`). A player's bare hand is 1.0 (createAttributes dconst_1),
    at most 1.5 with a critical hit and no enchantment (Player.attack `fload_2 * 1.5`), so 15 at most: one survival
    hit leaves the boat. Destruction by survival hits (5 hits of 1.0, 3 of 1.5) and the dropped oak_boat are not
    driven: no case reaches it."""
    spec = case['attack']
    rider.look(spec['yaw'], 0)
    if spec['sprint']:
        rider.player_input(forward=True, sprint=True)
    observer.wait_ticks(spec['ticks'])
    rider.attack(boat)
    observer.wait_ticks(2)
    now = boat_of(observer, boat)
    if spec['creative'] != (now is None):
        problems.append(f"boat present={now is not None}, creative hit={spec['creative']}")


def main():
    bins = Path(lab.cargo_target()) / 'fast'
    out = lab.LOCAL / 'boat-authority-parity'
    out.mkdir(parents=True, exist_ok=True)
    report = {}
    with Server(bins, out / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot() as rider, Bot() as observer:
        server.command(f'op {ALPHA}')
        server.command(f'op {BETA}')
        rider.join('127.0.0.1', server.port, ALPHA)
        observer.join('127.0.0.1', server.port, BETA)
        for case in cases():
            problems = []
            mob_first = any('first_player' in s for s in case.get('paddles', {}).get('steps', []))
            boat = arena(server, rider, observer, mob_first)
            kind = family(case)
            if kind in ('authority', 'reboard'):
                drive_owned(rider, observer, case, boat, problems)
            elif kind.startswith('paddles'):
                drive_paddles(rider, observer, case, boat, problems)
            else:
                strike(rider, observer, case, boat, problems)
            report[case['name']] = {'family': kind, 'pass': not problems, 'problems': problems}
    lab.write_json(out / 'report.json', report)
    for kind in sorted({r['family'] for r in report.values()}):
        bad = [n for n, r in report.items() if r['family'] == kind and not r['pass']]
        print(f"{'FAIL' if bad else 'pass'}  {kind} ({sum(r['family'] == kind for r in report.values())} cas)")
    assert all(r['pass'] for r in report.values()), [n for n, r in report.items() if not r['pass']]


if __name__ == '__main__':
    main()
