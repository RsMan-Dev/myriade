#!/usr/bin/env python3
"""The joueur-automate `myriade-bot`, driven one JSON line per request.

    with Bot() as bot:
        bot.join('127.0.0.1', port, 'Alice')
        bot.command('gamemode creative')

A request is `{"id": N, "verb": ..., "args": {...}}` on the bot's stdin; its answer is one JSON line on stdout
(protocol: plan joueur-automate-plan.md, section 3.3). The bot's journal goes to stderr, inherited. An `ok: false`
answer raises BotError with its error. So do an answer that does not carry the request's id, an exited bot, and an
answer later than `timeout` seconds, which also kills the bot.
"""
import json
import queue
import subprocess
import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402

BOT = 'myriade-bot'
STOP_SECONDS = 10
# The datagen's built-in registries: the bot names entity types with them.
REGISTRIES = lab.ROOT / '.local/datagen/out/reports/registries.json'


class BotError(RuntimeError):
    """A request the bot refused, or an exchange that did not go as the protocol says."""


def default_binary():
    """myriade-bot in Cargo's release folder, else its debug folder (under lab.cargo_target())."""
    for profile in ('release', 'debug'):
        path = Path(lab.binary(lab.cargo_target() / profile, BOT))
        if path.exists():
            return path
    return Path(lab.binary(lab.cargo_target() / 'release', BOT))


def _opt(**args):
    """The arguments that were given: a None argument is left out of the request."""
    return {k: v for k, v in args.items() if v is not None}


class Bot:
    """The bot process: started by `with`, stopped at its exit. `send` returns the answer's dict."""

    def __init__(self, binary=None, timeout=30):
        self.binary = Path(binary) if binary else default_binary()
        self.timeout = timeout
        self.process = None
        self.answers = None
        self.last_id = 0

    def __enter__(self):
        argv = [sys.executable, str(self.binary)] if self.binary.suffix == '.py' else [str(self.binary)]
        self.process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
                                        encoding='utf-8')
        self.answers = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        return self

    def __exit__(self, *exc):
        self.stop()

    def _read(self):
        for line in self.process.stdout:
            self.answers.put(line)
        self.process.stdout.close()
        self.answers.put(None)

    def stop(self):
        """Closes the bot's stdin so that it exits; it is killed if it does not exit in STOP_SECONDS."""
        if self.process is None:
            return
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            self.process.wait(STOP_SECONDS)
        except subprocess.TimeoutExpired:
            self._kill()

    def _kill(self):
        self.process.kill()
        self.process.wait()

    def send(self, verb, **args):
        """Sends one request and returns the answer's dict; raises BotError on a refusal or a broken exchange."""
        self.last_id += 1
        request = {'id': self.last_id, 'verb': verb, 'args': args}
        try:
            self.process.stdin.write(json.dumps(request) + '\n')
            self.process.stdin.flush()
            line = self.answers.get(timeout=self.timeout)
        except queue.Empty:
            self._kill()
            raise BotError(f'{verb}: no answer within {self.timeout} s, the bot was killed') from None
        except OSError as e:
            raise BotError(f'{verb}: the bot does not read its requests ({e})') from e
        if line is None:
            raise BotError(f'{verb}: the bot exited')
        try:
            answer = json.loads(line)
        except ValueError as e:
            raise BotError(f'{verb}: the answer is not JSON: {line.strip()}') from e
        if answer.get('id') != request['id']:
            raise BotError(f'{verb}: answer {answer.get("id")} to request {request["id"]}')
        if not answer.get('ok'):
            raise BotError(answer.get('error', f'{verb}: refused'))
        return answer

    def join(self, host, port, name, session_file=None, render_distance=None, registries=str(REGISTRIES)):
        return self.send('join', **_opt(host=host, port=port, name=name, session_file=session_file,
                                        render_distance=render_distance, registries=registries))

    def leave(self):
        return self.send('leave')

    def chat(self, text):
        return self.send('chat', text=text)

    def command(self, line):
        return self.send('command', line=line)

    def wait_ticks(self, n):
        return self.send('wait_ticks', n=n)

    def wait_for(self, what, timeout=None):
        return self.send('wait_for', **_opt(what=what, timeout=timeout))

    def state(self):
        return self.send('state')

    def move(self, x, y, z, on_ground=None):
        return self.send('move', **_opt(x=x, y=y, z=z, on_ground=on_ground))

    def look(self, yaw, pitch):
        return self.send('look', yaw=yaw, pitch=pitch)

    def look_at(self, x, y, z):
        return self.send('look_at', x=x, y=y, z=z)

    def tp_ack(self):
        return self.send('tp_ack')

    def attack(self, target):
        return self.send('attack', id=target)

    def interact(self, target):
        return self.send('interact', id=target)

    def use_item(self):
        return self.send('use_item')

    def release_use_item(self):
        return self.send('release_use_item')

    def set_carried_item(self, slot):
        return self.send('set_carried_item', slot=slot)

    def player_abilities(self, flying):
        return self.send('player_abilities', flying=flying)

    def use_block(self, x, y, z):
        return self.send('use_block', x=x, y=y, z=z)

    def break_(self, x, y, z):
        return self.send('break', x=x, y=y, z=z)

    def entities(self, entity_type=None, radius=None):
        return self.send('entities', **_opt(type=entity_type, radius=radius))

    def messages(self, since=None):
        return self.send('messages', **_opt(since=since))

    def inventory(self):
        return self.send('inventory')

    def menu(self):
        return self.send('menu')

    def click_slot(self, slot, button=0):
        return self.send('click_slot', slot=slot, button=button)

    def close_menu(self):
        return self.send('close_menu')

    def quiet(self, commands):
        return self.send('quiet', commands=list(commands))

    def capture(self, action, filter_ids=None):
        return self.send('capture', **_opt(action=action, filter=filter_ids))

    def cheat(self, timer=None, ground=None, no_swing=None):
        return self.send('cheat', **_opt(timer=timer, ground=ground, no_swing=no_swing))

    def vehicle_move(self, pos, yaw, pitch):
        return self.send('vehicle_move', pos=list(pos), yaw=yaw, pitch=pitch)

    def paddle(self, left, right):
        return self.send('paddle', left=left, right=right)

    def player_input(self, forward=False, backward=False, left=False, right=False, jump=False,
                     shift=False, sprint=False):
        return self.send('player_input', forward=forward, backward=backward, left=left, right=right,
                         jump=jump, shift=shift, sprint=sprint)

    def report(self):
        return self.send('report')
