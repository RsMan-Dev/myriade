#!/usr/bin/env python3
"""A ghast killed by the fireball the player struck back grants Return to Sender.

Vanilla 26.3: the player's attack deflects the fireball (`Projectile.deflect`, the
player is its owner now). `LargeFireball.onHitEntity` hurts the ghast with
`damageSources().fireball(this, owner)`; `Ghast.hurtServer` takes a reflected
fireball (`isReflectedFireball`: a large fireball, caused by a player) as 1000
damage, and `LivingEntity.hurtServer` remembers the player (`setLastHurtByPlayer`).
On death, `LivingEntity.die` credits that player (`awardKillScore`), whose
`player_killed_entity` trigger matches `nether/return_to_sender`'s killing blow:
a `#is_projectile` damage whose direct entity is a `minecraft:fireball`.

The ghast died, but the advancement never came on the default single player
(the host on its server thread plays as a guest there): the fireball's hit did
not record which player hurt the ghast, as an arrow's does, so the kill went to
nobody (#4212).

  test_return_to_sender_parity.py [--bin-dir DIR] [--keep]
"""
import argparse
import json
import math
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bot_lab import Bot  # noqa: E402
from server_lab import Server  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
RESOURCES = ROOT / '.local' / 'myriade-engine' / '26.3'
RUN = ROOT / '.local' / 'return-to-sender'


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin-dir', help='prebuilt myriade-engine (default: build it with the fast profile)')
    parser.add_argument('--keep', action='store_true', help='keep the run folder')
    args = parser.parse_args()
    bin_dir = Path(args.bin_dir).resolve() if args.bin_dir else None
    if bin_dir is None:
        subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-game', '-p', 'myriade-engine-lib', '--bins'], cwd=ROOT, check=True)
        bin_dir = ROOT / os.environ.get('CARGO_TARGET_DIR', 'target') / 'fast'
    shutil.rmtree(RUN, ignore_errors=True)
    RUN.mkdir(parents=True)
    folder = RUN / 'h0'
    folder.mkdir()
    with Server(bin_dir, folder, RESOURCES, {'level-seed': '1', 'view-distance': '4'}) as server, Bot() as bot:
        bot.join('127.0.0.1', server.port, 'Tester')
        # A glass floor in the sky: the player at its south end, facing a ghast that
        # does not move or shoot, and a still fireball just in front of the player.
        x, _, z = (math.floor(v) for v in bot.state()['position'])
        y = 200
        bot.command('effect give @s resistance infinite 255 true')
        bot.command(f'fill {x - 3} {y - 1} {z - 2} {x + 3} {y - 1} {z + 12} minecraft:glass')
        bot.command(f'tp @s {x + 0.5} {y} {z + 0.5}')
        bot.wait_ticks(10)
        bot.command(f'summon minecraft:ghast {x + 0.5} {y - 1} {z + 10.5} {{NoAI:1b}}')
        bot.command(f'summon minecraft:fireball {x + 0.5} {y + 1.3} {z + 2.5} '
                    '{Motion:[0.0d,0.0d,0.0d],acceleration_power:0.0d}')
        bot.wait_ticks(5)
        # Aim at the fireball's middle: struck back along the view, it flies into the ghast.
        bot.look_at(x + 0.5, y + 1.3, z + 2.5)
        fireball = bot.entities(entity_type='minecraft:fireball')['entities'][0]['id']
        bot.attack(fireball)
        bot.wait_ticks(60)
        assert not bot.entities(entity_type='minecraft:ghast')['entities'], 'the ghast did not die'
        # The kill's advancement is broadcast as a styled message (its title is translated).
        messages = bot.messages()['messages']
        assert any('advancements.nether.return_to_sender.title' in json.dumps(m['json']) for m in messages), messages
    if not args.keep:
        shutil.rmtree(RUN, ignore_errors=True)


if __name__ == '__main__':
    main()
