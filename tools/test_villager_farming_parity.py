#!/usr/bin/env python3
"""Villager farming on the whole dedicated server, against vanilla 26.3 and the content-mod farming regressions.

The vanilla block classification is compared exactly with VanillaVillagerFarmingOracle. The content-mod cases write a
`fieldtest` mod into a scratch folder, start the server with it as `--dev-mod`, and play the farmer through server
commands: blocks placed and read with `block`, the inventory read with the `entity_inventory` operation. Sounds are not
read by this path and are not checked. Outputs are private dev data, not game resources.
"""
import os
from pathlib import Path
import tempfile

try:
    from . import lab, oracle_harness, test_rust_parity as base, test_model_baking_parity as baking
    from .bot_lab import Bot
    from .server_lab import Server
except ImportError:
    import lab, oracle_harness, test_rust_parity as base, test_model_baking_parity as baking
    from bot_lab import Bot
    from server_lab import Server

FARMER = '00000001000000020000000300000004'
FARMER_NBT = '{UUID:[I;1,2,3,4],VillagerData:{profession:"minecraft:farmer",level:1,type:"minecraft:plains"}'
MOD_FILES = {
    'horizon-mod.toml': 'id = "fieldtest"\nname = "Field test"\nversion = "1.0"\napi = "0.2"\nside = "both"\n'
                        'capabilities = ["registries"]\n',
    'data/fieldtest/horizon/block/soil.json': '{"template":"minecraft:farmland"}',
    'data/fieldtest/horizon/block/grain.json': '{"template":"minecraft:wheat"}',
    'data/fieldtest/horizon/block/roots.json': '{"template":"minecraft:beetroots"}',
    'data/fieldtest/horizon/block/flower.json': '{"template":"minecraft:torchflower_crop"}',
    'data/fieldtest/horizon/block/pod.json': '{"template":"minecraft:pitcher_crop"}',
    'data/fieldtest/horizon/block/fake_crop.json': '{"template":"minecraft:stone"}',
    'data/minecraft/tags/item/villager_plantable_seeds.json':
        '{"replace":true,"values":["fieldtest:grain","fieldtest:roots","fieldtest:seeds","minecraft:stick"]}',
    'data/fieldtest/horizon/item/seeds.json': '{"template":"minecraft:wheat_seeds","block":"fieldtest:grain"}',
}


def write_mod(folder):
    """The fieldtest content mod, unpacked in `folder`."""
    for name, text in MOD_FILES.items():
        path = folder / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
    return folder


def reset(server):
    server.command('kill @e[type=!player]')
    server.command('fill -8 0 -8 8 0 8 minecraft:stone')
    server.command('fill -8 1 -8 8 3 8 minecraft:air')


def farmer(server, x, z, inventory=''):
    server.command(f'summon minecraft:villager {x} 1 {z} {FARMER_NBT}{inventory}}}')


def inventory(server):
    reply = server.op('Player', {'op': 'entity_inventory', 'uuid': FARMER})
    return [(stack['item'], stack['count']) for stack in reply]


def settle(server, done, ticks=400, step=20):
    """Waits until `done()` holds, a `step` of ticks at a time, for at most `ticks`."""
    for _ in range(ticks // step):
        if done():
            return True
        server.wait_ticks(step)
    return done()


def block_id(server, x, y, z):
    return server.block(x, y, z)['id']


def case_secondary_site(server, griefing):
    """`mod_farmland_is_a_secondary_site_and_farming_target`: farmland of a mod is a farming site; griefing gates the work."""
    reset(server)
    server.command(f'gamerule mob_griefing {"true" if griefing else "false"}')
    server.command('setblock 0 0 0 fieldtest:soil')
    server.command('setblock 0 1 0 minecraft:air')
    farmer(server, 0.5, 2.5, ',Inventory:[{id:"fieldtest:seeds",Count:2b,Slot:0b}]')
    planted = settle(server, lambda: block_id(server, 0, 1, 0) == 'fieldtest:grain', ticks=600)
    return planted


def case_ripe_at_template_age(server):
    """`mod_crops_are_harvested_only_at_their_vanilla_templates_max_age`: only the mod crops at their template's
    maximum age are broken; an age-6 wheat, a torchflower at age 1, a pitcher crop and a stone are left standing."""
    reset(server)
    server.command('gamerule mob_griefing true')
    cells = {
        0: 'fieldtest:grain[age=6]', 1: 'fieldtest:grain[age=7]', 2: 'fieldtest:roots[age=3]',
        3: 'fieldtest:flower[age=1]', 4: 'fieldtest:pod[age=4]', 5: 'fieldtest:fake_crop',
    }
    for x, state in cells.items():
        server.command(f'setblock {x} 1 0 {state}')
    farmer(server, 2.5, 3.5)
    settle(server, lambda: block_id(server, 1, 1, 0) == 'minecraft:air' and block_id(server, 2, 1, 0) == 'minecraft:air',
           ticks=800)
    return {x: block_id(server, x, 1, 0) for x in cells}


def case_plants_declared_item_block(server):
    """`farmer_plants_a_declared_items_block`: seeds of a mod item in the inventory plant its block; one seed is used."""
    reset(server)
    server.command('gamerule mob_griefing true')
    server.command('setblock 0 0 0 fieldtest:soil')
    farmer(server, 0.5, 2.5, ',Inventory:[{id:"fieldtest:seeds",Count:2b,Slot:0b}]')
    settle(server, lambda: block_id(server, 0, 1, 0) == 'fieldtest:grain', ticks=600)
    return block_id(server, 0, 1, 0), inventory(server)


def case_first_tagged_item(server):
    """`farming_uses_the_first_tagged_block_item_and_consumes_one_seed`: the first tagged block item is planted (roots),
    then one grain seed; the inventory keeps the other stacks."""
    reset(server)
    server.command('gamerule mob_griefing true')
    server.command('setblock 0 0 0 fieldtest:soil')
    server.command('setblock 1 0 0 fieldtest:soil')
    items = ('[{id:"minecraft:wheat_seeds",Count:2b,Slot:0b},{id:"minecraft:stick",Count:2b,Slot:1b},'
             '{id:"fieldtest:flower",Count:2b,Slot:2b},{id:"fieldtest:roots",Count:1b,Slot:3b},'
             '{id:"fieldtest:grain",Count:2b,Slot:4b}]')
    farmer(server, 0.5, 2.5, f',Inventory:{items}')
    settle(server, lambda: block_id(server, 0, 1, 0) == 'fieldtest:roots', ticks=600)
    settle(server, lambda: block_id(server, 1, 1, 0) == 'fieldtest:grain', ticks=600)
    return block_id(server, 0, 1, 0), block_id(server, 1, 1, 0), inventory(server)


# Expected values, from the Rust test each case replaces (villager_farming_tests.rs, deleted with this tool).
EXPECTED_MOD_CASES = {
    'secondary_site_planted_with_griefing': True,
    'secondary_site_not_planted_without_griefing': False,
    'ripe_at_template_age': {0: 'fieldtest:grain', 1: 'minecraft:air', 2: 'minecraft:air', 3: 'fieldtest:flower',
                             4: 'fieldtest:pod', 5: 'fieldtest:fake_crop'},
    'plants_declared_item_block': ('fieldtest:grain', [('fieldtest:seeds', 1)]),
    'first_tagged_item': ('fieldtest:roots', 'fieldtest:grain',
                          [('minecraft:wheat_seeds', 2), ('minecraft:stick', 2), ('fieldtest:flower', 2),
                           ('fieldtest:grain', 1)]),
}


def main():
    build = Path(os.environ.get('HORIZON_FARMING_PARITY_DIR', lab.LOCAL / 'villager-farming-parity'))
    build.mkdir(parents=True, exist_ok=True)
    lock, jars, libraries = base.verify_artifacts()
    classes, dependencies, _ = oracle_harness.compile_oracle(
        build, 'VanillaVillagerFarmingOracle', jars, baking.client_libraries(), lock['minecraft']['java_major'])
    expected = build / 'vanilla.txt'
    oracle_harness.java_run(build, classes, dependencies, 'VanillaVillagerFarmingOracle', [expected], 'vanilla')
    oracle_harness.run_logged(['cargo', 'build', '--profile', 'fast', '-p', 'myriade-engine', '--bin', 'myriade-engine'],
                              build, 'compile-rust')
    bins = Path(lab.cargo_target()) / 'fast'
    with tempfile.TemporaryDirectory(prefix='hz-farming-mod-') as scratch:
        mod = write_mod(Path(scratch) / 'fieldtest')
        extra = ('--dev-mod', str(mod))
        with Server(bins, build / 'server', lab.RESOURCES.resolve(), {'level-seed': '42', 'view-distance': '4'},
                    extra=extra) as server, Bot(timeout=600) as bot:
            bot.join('127.0.0.1', server.port, 'Player')
            bot.wait_for('loaded', 300)
            vanilla = server.op('Player', {'op': 'villager_farming'})
            if vanilla != expected.read_text().splitlines():
                raise SystemExit('villager farming classification differs from vanilla')
            failures = []
            griefing = case_secondary_site(server, True)
            if griefing != EXPECTED_MOD_CASES['secondary_site_planted_with_griefing']:
                failures.append('secondary_site_planted_with_griefing')
            if case_secondary_site(server, False) != EXPECTED_MOD_CASES['secondary_site_not_planted_without_griefing']:
                failures.append('secondary_site_not_planted_without_griefing')
            if case_ripe_at_template_age(server) != EXPECTED_MOD_CASES['ripe_at_template_age']:
                failures.append('ripe_at_template_age')
            if case_plants_declared_item_block(server) != EXPECTED_MOD_CASES['plants_declared_item_block']:
                failures.append('plants_declared_item_block')
            if case_first_tagged_item(server) != EXPECTED_MOD_CASES['first_tagged_item']:
                failures.append('first_tagged_item')
    if failures:
        raise SystemExit('content-mod farming cases differ: ' + ', '.join(failures))
    print(f'{len(vanilla)} vanilla block states match; 5 content-mod farming cases pass')


if __name__ == '__main__':
    main()
