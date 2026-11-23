#!/usr/bin/env python3
"""A dedicated Horizon server for a tool: started in a scratch folder, driven and read through its control socket.

    with Server(bins, folder, resources) as server:
        server.command('gamerule doDaylightCycle false')
        server.wait_ticks(20)

The server is `myriade-engine --dedicated --nogui --control-socket <folder>/.horizon/engine.json`, as
tools/server_transfer_e2e.py starts it. Requests go to the socket (crates/myriade-engine/src/game/interface/
dedicated/status.rs:45-90): `server-status`, `console <line>` (a vanilla command; its feedback comes back as
`output`), `rcon <line>`. Ticks are counted by `time query gametime` (horizon-gameplay/src/commands/time.rs:42-45):
the game time rises by one each tick.
"""
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from control_client import ControlClient  # noqa: E402

EXE = '.exe' if os.name == 'nt' else ''
PROPERTIES = {'online-mode': 'false', 'white-list': 'false'}
LAST_INT = re.compile(r'(-?\d+)\D*$')


def write_folder(folder, properties):
    """The server.properties and eula of a server folder, and its `.horizon` folder for the control endpoint.
    Returns the endpoint file the game writes."""
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in properties.items()))
    (folder / 'eula.txt').write_text('eula=true\n')
    (folder / '.horizon').mkdir(exist_ok=True)
    return folder / '.horizon' / 'engine.json'


def launch_command(bins, resources, endpoint, extra=()):
    """The command that starts the dedicated server, with its control endpoint at `endpoint`."""
    return [bins / f'myriade-engine{EXE}', '--resources', resources, '--control-socket', endpoint,
            '--dedicated', '--nogui', *extra]


class Server:
    """A dedicated server in `folder`, ready when `server-status` answers. Its output goes to folder/server.log."""

    def __init__(self, bins, folder, resources, properties=None, extra=(), ready_seconds=300, env=None):
        """`env` adds variables to the game's environment (`lab.game_env`), kept across restarts."""
        folder = Path(folder)
        self.env = dict(env or {})
        props = dict(PROPERTIES, **{'server-port': str(lab.free_port())}, **(properties or {}))
        self.endpoint = write_folder(folder, props)
        self.port = int(props['server-port'])
        self.folder = folder
        self.argv = launch_command(Path(bins), Path(resources), self.endpoint, extra)
        self.ready_seconds = ready_seconds
        self.control = None
        self._launch('w')
        self._ready(ready_seconds)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.stop()

    def _launch(self, mode):
        """Starts the server process, its output in server.log (`mode` 'w' for a new run, 'a' after a restart)."""
        self.log = (self.folder / 'server.log').open(mode)
        self.process = subprocess.Popen(self.argv, cwd=self.folder, stdin=subprocess.PIPE, stdout=self.log,
                                        stderr=subprocess.STDOUT, env=lab.game_env(**self.env))

    def _ready(self, seconds):
        deadline = time.time() + seconds
        while True:
            if self.process.poll() is not None:
                raise RuntimeError(f'the server exited, see {self.log.name}')
            try:
                if self.endpoint.exists():
                    self.control = ControlClient(self.endpoint, timeout=30)
                    if 'tps' in self.control.ask('server-status'):
                        return
            except (OSError, ValueError):
                self._drop_control()
            if time.time() > deadline:
                raise RuntimeError(f'the server was not ready within {seconds} s, see {self.log.name}')
            time.sleep(0.5)

    def _drop_control(self):
        if self.control:
            self.control.close()
        self.control = None

    def command(self, line):
        """Runs a console command; returns the lines it logged."""
        return self.control.ask(f'console {line}').get('output', [])

    def status(self):
        """`server-status`: tps, mspt, players, max_players, chunks, levels, paused."""
        return self.control.ask('server-status')

    def block(self, x, y, z):
        """`block x y z`: the block state there, `{x, y, z, id, properties}`."""
        return self._reply(f'block {x} {y} {z}')

    def blocks(self, x1, y1, z1, x2, y2, z2):
        """`blocks`: the block states in the box between two corners, as a list of `block` replies."""
        return self._reply(f'blocks {x1} {y1} {z1} {x2} {y2} {z2}')['blocks']

    def entities_server(self):
        """`entities_server`: `{entities: [{id, type, pos}], players: [{name, uuid, pos}]}`."""
        return self._reply('entities_server')

    def state(self, key, *args):
        """`state <key>`: `raid` ({raids}), `meteo` ({rain, thunder}), `lumiere x y z` ({sky, block})."""
        return self._reply(' '.join(['state', key, *map(str, args)]))

    def run_as(self, player, command):
        """`run_as <player> <command>`: runs the command as that connected player; `{result, success, messages}`."""
        return self._reply(f'run_as {player} {command}')

    def save(self):
        """`save`: writes the world; `{saved}`, with `error` when it failed."""
        return self._reply('save')

    def op(self, player, op, seconds=600):
        """`op <player> <object>`: runs one operation of the command harness for the connected `player`; the reply is
        the record the harness wrote for it. An `error` reply raises."""
        reply = self.control.ask(f'op {player} {json.dumps(op, separators=(",", ":"))}', timeout=seconds)
        if 'error' in reply:
            raise RuntimeError(f'op {op.get("op")}: {reply["error"]}')
        return reply

    def _reply(self, line):
        """A control request's reply; an `error` reply raises."""
        reply = self.control.ask(line)
        if 'error' in reply and 'saved' not in reply:
            raise RuntimeError(f'{line}: {reply["error"]}')
        return reply

    def log_lines(self):
        """The lines the server has written to folder/server.log so far (all its runs)."""
        self.log.flush()
        return (self.folder / 'server.log').read_text(errors='replace').splitlines()

    def wait_log(self, pattern, seconds=60, since=0):
        """Returns the first log line from index `since` on that matches `pattern` (a regular expression)."""
        deadline = time.time() + seconds
        while True:
            for line in self.log_lines()[since:]:
                if re.search(pattern, line):
                    return line
            if time.time() > deadline:
                raise RuntimeError(f'no log line matched {pattern!r} within {seconds} s')
            time.sleep(0.1)

    def game_time(self):
        """The world's game time: one tick is one unit."""
        for line in reversed(self.command('time query gametime')):
            if (m := LAST_INT.search(line)):
                return int(m.group(1))
        raise RuntimeError('time query gametime gave no time')

    def wait_ticks(self, ticks, seconds=None):
        """Returns once the game time has risen by `ticks`."""
        start = self.game_time()
        deadline = time.time() + (seconds or 60 + ticks)
        while self.game_time() - start < ticks:
            if time.time() > deadline:
                raise RuntimeError(f'{ticks} ticks did not pass within the wait')
            time.sleep(0.05)

    def stop(self, seconds=180):
        """`console stop`, then the server's exit; the process is killed only if it does not exit in time."""
        if self.process.poll() is None and self.control:
            try:
                self.control.ask('console stop', timeout=30)
            except (OSError, ValueError):
                pass
        self._wait_exit(seconds)

    def restart(self, seconds=180):
        """`restart`: the server stops, then starts again with the same command on the same world, and this returns
        once it answers `server-status` (within the `ready_seconds` given at start). Returns the stop's reply."""
        reply = self._reply('restart')
        self._wait_exit(seconds)
        self._launch('a')
        self._ready(self.ready_seconds)
        return reply

    def _wait_exit(self, seconds):
        self._drop_control()
        try:
            self.process.wait(seconds)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        if self.process.stdin:
            self.process.stdin.close()
        self.log.close()
