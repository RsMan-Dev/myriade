#!/usr/bin/env python3
"""FireworkRocketEntity event 17 on the server: a rocket fired by one player is seen by the other, on both hosting
paths (the host fires, the guest watches; then the reverse).

Server part only. The shooting bot summons a rocket with its FireworksItem; the observing bot reads, tick by tick,
the rocket in `entities` (`data` holds its synced values) and the server's entity events (`events`, returned by the
same `entities` reply and emptied by each call: {tick, id, event}). The burst's drawing and the trail and fade
frames are a client's: they are not checked here.
"""
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from bot_lab import Bot  # noqa: E402
from server_lab import Server  # noqa: E402

HOST, GUEST = 'Alpha', 'Beta'
LIFE = 20  # the rocket's LifeTime
# Borne (javap runtime/vanilla-26.3/server-26.3.jar, FireworkRocketEntity : readAdditionalSaveData lit « LifeTime » dans
# lifetime ; tick() fait life++ puis « life > lifetime » -> broadcastEntityEvent(17)) : l'événement 17 tombe au
# (LIFE+1)e tick de la fusée, donc entre summon + LIFE et summon + LIFE + 1 selon le tick où la fusée est ticked.
BURST_EVENT = 17
FIREWORKS = ('{id:"minecraft:firework_rocket",count:1,components:{"minecraft:fireworks":'
             '{flight_duration:1,explosions:[{shape:"small_ball",colors:[16711680],fade_colors:[65280]}]}}}')
WATCH_TICKS = LIFE + 10


def fire_and_watch(server, shooter, observer, role):
    """Summons one rocket above the shooter, then watches it until it is gone. Returns what the observer saw."""
    before = server.game_time()
    shooter.command(f'summon minecraft:firework_rocket ~ ~2 ~ {{LifeTime:{LIFE},FireworksItem:{FIREWORKS}}}')
    after = server.game_time()
    first_seen, last_seen, data, bursts = {}, {}, {}, []
    for _ in range(WATCH_TICKS):
        now = observer.wait_ticks(1)['tick']
        reply = observer.entities(entity_type='minecraft:firework_rocket')
        for rocket in reply['entities']:
            first_seen.setdefault(rocket['id'], now)
            last_seen[rocket['id']] = now
            data.setdefault(rocket['id'], rocket.get('data'))
        bursts += [e for e in reply['events'] if e['event'] == BURST_EVENT]
    rockets = sorted(first_seen)
    return {'role': role, 'rockets': len(rockets), 'rocket_ids': rockets, 'burst_ids': [b['id'] for b in bursts],
            'burst_ticks': [b['tick'] for b in bursts], 'summoned': (before, after), 'seen_ticks': [first_seen[r] for r in rockets],
            'gone_after': [last_seen[r] for r in rockets], 'data': [data[r] for r in rockets]}


def check(run, problems):
    """One rocket, one burst (event 17 for that entity) within the bound of its summon, rocket gone."""
    if run['rockets'] != 1:
        problems.append(f"{run['role']}: {run['rockets']} rockets seen, expected 1")
        return
    if run['burst_ids'] != run['rocket_ids']:
        problems.append(f"{run['role']}: burst events for {run['burst_ids']}, expected one for {run['rocket_ids']}")
    elif not run['summoned'][0] + LIFE <= run['burst_ticks'][0] <= run['summoned'][1] + LIFE + 1:
        problems.append(f"{run['role']}: burst at tick {run['burst_ticks'][0]}, outside the bound of summon "
                        f"ticks {run['summoned']} + {LIFE} (+1)")
    elif run['gone_after'][0] >= run['burst_ticks'][0]:
        problems.append(f"{run['role']}: the rocket was still seen at tick {run['gone_after'][0]}, after its burst")


def main():
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-game', '-p', 'myriade-engine-lib',
                    '--bin', 'myriade-engine'], cwd=lab.ROOT, check=True)
    bins = Path(lab.cargo_target()) / 'fast'
    out = lab.LOCAL / 'remote-fireworks-parity'
    out.mkdir(parents=True, exist_ok=True)
    report = {}
    with Server(bins, out / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot() as host, Bot() as guest:
        server.command(f'op {HOST}')
        server.command(f'op {GUEST}')
        host.join('127.0.0.1', server.port, HOST)
        guest.join('127.0.0.1', server.port, GUEST)
        for shooter, observer, role in ((host, guest, 'host-fires'), (guest, host, 'guest-fires')):
            problems = []
            run = fire_and_watch(server, shooter, observer, role)
            check(run, problems)
            report[f'fireworks-{role}'] = {'pass': not problems, 'problems': problems, **run}
    lab.write_json(out / 'report.json', report)
    print(json.dumps({k: v['pass'] for k, v in report.items()}))
    assert all(r['pass'] for r in report.values()), report


if __name__ == '__main__':
    main()
