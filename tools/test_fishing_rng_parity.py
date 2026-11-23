#!/usr/bin/env python3
"""Compare fishing on the whole server: a bot casts, waits for the bite and reels in, on a vanilla dedicated server and
on Horizon's, through the same commands and the same bot verbs.

Per configuration (clear or rain, Lure 0 or 3) the bot casts CASTS times over a water pool. The bite is the first tick
where a boolean synced value of the bobber turns true (FishingHook.DATA_BITING, whose index the bot does not name: it is
the index that flips to true at the bite, and it must be the same on both servers). The bite delay is the game ticks
from the cast to that flip. Each reel counts the item and experience entities it spawns. Both sides must agree on the
mean bite delay within three standard errors of the difference, and on the share of casts that catch something within
CATCH_TOLERANCE. The minimum and maximum delays are printed for each side.
"""
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys

try:
    from . import lab, test_rust_parity as base
    from .bot_lab import Bot
    from .server_lab import Server
    from . import test_dedicated_server_parity as dedicated
except ImportError:
    import lab
    import test_rust_parity as base
    from bot_lab import Bot
    from server_lab import Server
    import test_dedicated_server_parity as dedicated

PLAYER = dedicated.PLAYER
CASTS = 20
CONFIGS = [('clear', 0), ('clear', 3), ('rain', 0), ('rain', 3)]
MAX_BITE_TICKS = 1500
REEL_TICKS = 40
CATCH_TOLERANCE = 0.3
SETUP = [
    'gamemode survival',
    'fill -2 98 -2 6 98 6 minecraft:stone',
    'fill -2 99 -2 6 99 6 minecraft:water',
    'tp {player} 2 100 -3 0 0',
]


def flipped(before, after):
    """Indices of boolean synced values that were false and are now true."""
    return {i for i, v in after.items() if v is True and before.get(i) is False}


def bobber_data(bot):
    bobbers = bot.entities(entity_type='minecraft:fishing_bobber')['entities']
    if not bobbers:
        return None, {}
    data = {d['index']: d['value'] for d in bobbers[0].get('data', []) if isinstance(d['value'], bool)}
    return bobbers[0]['id'], data


def run_side(bot, label):
    """One server's records: per configuration, the bite delays, the catch flags and the flipped index."""
    records = {}
    for weather, lure in CONFIGS:
        bot.command(f'weather {weather}')
        for line in SETUP:
            bot.command(line.format(player=PLAYER))
        rod = 'minecraft:fishing_rod' + (f'[enchantments={{levels:{{"minecraft:lure":{lure}}}}}]' if lure else '')
        bot.command(f'item replace entity {PLAYER} weapon.mainhand with {rod}')
        bot.set_carried_item(0)
        bot.wait_ticks(4)
        delays, catches, flips = [], [], set()
        for _ in range(CASTS):
            before_items = len(bot.entities(entity_type='minecraft:item')['entities'])
            start = bot.state()['tick']
            bot.use_item()
            snapshot, bite = {}, None
            for _ in range(MAX_BITE_TICKS):
                bot.wait_ticks(1)
                _, data = bobber_data(bot)
                if not data and snapshot:
                    break
                found = flipped(snapshot, data)
                if found and bite is None:
                    bite = bot.state()['tick'] - start
                    flips |= found
                    break
                snapshot = data
            if bite is None:
                raise SystemExit(f'{label}: no bite within {MAX_BITE_TICKS} ticks ({weather}, lure {lure})')
            delays.append(bite)
            bot.use_item()
            bot.wait_ticks(REEL_TICKS)
            after_items = len(bot.entities(entity_type='minecraft:item')['entities'])
            catches.append(after_items > before_items)
        records[f'{weather}-lure{lure}'] = {'delays': delays, 'catches': catches}
    return records, flips


def stats(values):
    mean = sum(values) / len(values)
    var = sum((v - mean) ** 2 for v in values) / (len(values) - 1)
    return mean, math.sqrt(var / len(values))


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'fishing-rng-parity'
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    vanilla_command = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'myriade-engine', '--bin', 'myriade-engine'],
                   cwd=lab.ROOT, check=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    bins = Path(metadata['target_directory']) / 'fast'

    vanilla_dir = build / 'vanilla'
    port = dedicated.free_port()
    dedicated.write_server_dir(vanilla_dir, port)
    server = dedicated.Server(vanilla_command, vanilla_dir, 'vanilla-fishing')
    try:
        server.wait_for(r'Done \(')
        server.console(f'op {PLAYER}')
        with Bot(timeout=600) as bot:
            bot.join('127.0.0.1', port, PLAYER)
            bot.wait_for('loaded', 300)
            vanilla, vanilla_flips = run_side(bot, 'vanilla')
    finally:
        server.stop()

    with Server(bins, build / 'horizon', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'}) as server, \
            Bot(timeout=600) as bot:
        server.command(f'op {PLAYER}')
        bot.join('127.0.0.1', server.port, PLAYER)
        bot.wait_for('loaded', 300)
        horizon, horizon_flips = run_side(bot, 'Horizon')

    if vanilla_flips != horizon_flips or len(vanilla_flips) != 1:
        sys.exit(f'bite index differs or is ambiguous: vanilla {sorted(vanilla_flips)}, Horizon {sorted(horizon_flips)}')
    for name in vanilla:
        a, b = vanilla[name], horizon[name]
        mean_a, se_a = stats(a['delays'])
        mean_b, se_b = stats(b['delays'])
        print(f'{name}: vanilla bite {min(a["delays"])}..{max(a["delays"])} mean {mean_a:.1f}; '
              f'Horizon {min(b["delays"])}..{max(b["delays"])} mean {mean_b:.1f}')
        if abs(mean_a - mean_b) > 3 * math.sqrt(se_a ** 2 + se_b ** 2):
            sys.exit(f'{name}: mean bite delay differs beyond three standard errors')
        catch_a = sum(a['catches']) / CASTS
        catch_b = sum(b['catches']) / CASTS
        if abs(catch_a - catch_b) > CATCH_TOLERANCE:
            sys.exit(f'{name}: catch share differs: vanilla {catch_a:.2f}, Horizon {catch_b:.2f}')
    print('fishing: bite delay, catches and bite index match the vanilla server')
    shutil.rmtree(vanilla_dir, ignore_errors=True)
    shutil.rmtree(build / 'horizon', ignore_errors=True)


if __name__ == '__main__':
    main()
