#!/usr/bin/env python3
"""Server resource packs, server side: a joueur automate (myriade-bot, through tools/bot_lab.py) joins a dedicated
server (vanilla 26.3's, or Horizon's with --horizon-server BIN) that offers a test pack, reads the offer and answers
it. The client side (servers.dat, the prompt on screen, the download cache and its HTTP headers) is not here: the
pack's bytes are never fetched, only its offer is compared with server.properties.

- required, declined: the server disconnects the player; the outcome is compared between the two servers;
- required, accepted, downloaded, successfully_loaded: the player stays in game;
- required with a wrong SHA-1, accepted then failed_download: the player stays in game;
- optional, declined: the player stays in game;
- optional on a dead URL, accepted then failed_download: the player stays in game;
- optional, accepted then loaded: the player stays in game.

  python3 tools/server_pack_e2e.py                                   # vanilla's server
  python3 tools/server_pack_e2e.py --horizon-server target/release/horizon-client

Every server here runs in fresh scratch homes (lab.game_env). Needs .local (the vanilla jars and prepared resources).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import sys
import tempfile
import time
import zipfile
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
import test_rust_parity as base  # noqa: E402
from bot_lab import Bot, BotError  # noqa: E402
from test_dedicated_server_parity import Server  # noqa: E402

HOST = '127.0.0.1'
PLAYER = 'Steve'
PACK_ID = '5b2c7a4e-1f3d-4c8b-9a6e-2d4f8b1c3e5a'
# Seconds the bot waits after its answers before it asks whether it is still in game.
AFTER_ANSWERS = 2
PROPERTIES = {
    'online-mode': 'false', 'white-list': 'false', 'level-seed': '12345', 'level-type': 'minecraft\\:flat',
    'generate-structures': 'false', 'view-distance': '3', 'simulation-distance': '3', 'spawn-protection': '0',
    'pause-when-empty-seconds': '-1', 'gamemode': 'creative',
}
PROMPT = '["Blue dirt for ",{"text":"this test","color":"aqua"}]'
DEAD_URL = 'http://127.0.0.1:{port}/gone.zip'
GOOD = ['accepted', 'downloaded', 'successfully_loaded']
FAILED = ['accepted', 'failed_download']

# The servers' pack settings, by config; the answers the bot gives, and whether the player stays in game.
SCENARIOS = {
    'required': [('declined', ['declined'], False), ('loaded', GOOD, True)],
    'bad-sha1': [('failed-download', FAILED, True)],
    'optional': [('declined', ['declined'], True), ('loaded', GOOD, True)],
    'dead-url': [('failed-download', FAILED, True)],
}


def check(condition, what):
    if not condition:
        raise AssertionError(what)
    print(f'  ok: {what}')


def png(width, height, pixel):
    rows = b''.join(b'\0' + b''.join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    header = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')


def build_pack(path):
    """The test pack, byte for byte the same every time (fixed dates), and its SHA-1."""
    def blue(x, y):
        return (40, 90, 230, 255) if (x // 4 + y // 4) % 2 else (20, 50, 170, 255)
    files = {
        'pack.mcmeta': json.dumps({'pack': {'description': 'Horizon server pack test',
                                            'min_format': 97, 'max_format': 97}}).encode(),
        'assets/minecraft/textures/block/dirt.png': png(16, 16, blue),
        'assets/minecraft/lang/en_us.json': json.dumps({
            'block.minecraft.dirt': 'Blue Test Dirt',
            'menu.returnToGame': 'Back to Game (server pack)',
            'selectServer.select': 'Join Server (server pack)',
        }).encode(),
    }
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), data)
    return hashlib.sha1(path.read_bytes()).hexdigest()


def configs(sha1, dead):
    """The server.properties pack lines of each config."""
    good = {'resource-pack': f'http://{HOST}:{dead}/test-pack.zip', 'resource-pack-sha1': sha1,
            'resource-pack-id': PACK_ID, 'resource-pack-prompt': PROMPT}
    return {
        'required': dict(good, **{'require-resource-pack': 'true'}),
        'bad-sha1': {'resource-pack': good['resource-pack'], 'resource-pack-sha1': '0' * 40,
                     'require-resource-pack': 'true'},
        'optional': dict(good, **{'require-resource-pack': 'false'}),
        'dead-url': {'resource-pack': DEAD_URL.format(port=dead), 'require-resource-pack': 'false'},
    }


def visit(bot_binary, port, statuses):
    """Joins, reads the offer, answers every pack with `statuses`, then says whether the player is still in game.
    Returns the offer and the outcome: 'in game', or the error the bot got (a disconnection's reason)."""
    with Bot(binary=bot_binary, timeout=60) as bot:
        try:
            bot.join(HOST, port, PLAYER)
        except BotError as error:
            return {'offer': None, 'outcome': str(error)}
        offer = bot.state().get('packs', [])
        for pack in offer:
            for status in statuses:
                bot.send('pack_status', id=pack['id'], status=status)
        time.sleep(AFTER_ANSWERS)
        try:
            bot.state()
        except BotError as error:
            return {'offer': offer, 'outcome': str(error)}
        return {'offer': offer, 'outcome': 'in game'}


def free_port():
    with socket.socket() as s:
        s.bind((HOST, 0))
        return s.getsockname()[1]


def run_side(label, command, bot_binary, work, configs_by_name, out):
    """Every config on one server (vanilla or Horizon), each with its scenarios; returns the outcomes by name."""
    results = {}
    for config, properties in configs_by_name.items():
        directory = work / f'{label}-{config}'
        directory.mkdir(parents=True)
        port = free_port()
        props = dict(PROPERTIES, **properties, **{'server-port': str(port)})
        (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()),
                                                     encoding='iso-8859-1')
        (directory / 'eula.txt').write_text('eula=true\n')
        server = Server(command, directory, f'{label}-{config}')
        try:
            server.wait_for(r'Done \(', timeout=300)
            for scenario, statuses, stays in SCENARIOS[config]:
                outcome = visit(bot_binary, port, statuses)
                print(f'- {label}: {config}/{scenario} -> {outcome["outcome"]}')
                check((outcome['outcome'] == 'in game') == stays, f'{label} {config}/{scenario} stays {stays}')
                results[f'{config}/{scenario}'] = outcome
        finally:
            server.stop()
            shutil.copy(server.log_path, out / server.log_path.name)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bot', type=Path, help='the myriade-bot binary (default: the one under lab.cargo_target())')
    parser.add_argument('--horizon-server', help='a horizon-client binary to run as the dedicated server')
    parser.add_argument('--out', default=str(lab.LOCAL / 'server-pack-e2e'))
    args = parser.parse_args()
    out = Path(args.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    lock, jars, libraries = base.verify_artifacts()
    deps = os.pathsep.join(map(str, [jars['server'], *libraries]))
    vanilla = ['java', '-Xmx2g', '-cp', deps, 'net.minecraft.server.Main', 'nogui']
    resources = lab.LOCAL / 'horizon-client' / lab.VERSION
    work = Path(tempfile.mkdtemp(prefix='server-packs-'))
    os.environ.update(lab.game_env())
    try:
        sha1 = build_pack(out / 'test-pack.zip')
        dead = free_port()
        by_config = configs(sha1, dead)
        sides = {'vanilla': vanilla}
        if args.horizon_server:
            sides['horizon'] = [args.horizon_server, '--resources', resources, '--dedicated', '--nogui']
        runs = {label: run_side(label, command, args.bot, work, by_config, out) for label, command in sides.items()}
    finally:
        shutil.rmtree(work, ignore_errors=True)
    (out / 'outcomes.json').write_text(json.dumps(runs, indent=2) + '\n')
    if 'horizon' in runs:
        differ = [k for k in runs['vanilla'] if runs['vanilla'][k] != runs['horizon'][k]]
        check(not differ, f'horizon and vanilla agree on every outcome and offer (differ: {differ})')
    print(f'server packs: {sum(len(v) for v in runs.values())} visits, evidence in {out}')


if __name__ == '__main__':
    main()
