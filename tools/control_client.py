#!/usr/bin/env python3
"""Client of a running Horizon server's control endpoint (`--control-socket FILE.json`).

The game listens on a random localhost port and writes `{"transport":"tcp","port":N,"token":"..."}` to the
file (`<folder>/.horizon/engine.json` under the supervisor). The first line sent must be `auth TOKEN`; then
each command is one line and each reply one JSON line (crates/horizon-client/src/control.rs).

    with ControlClient(folder / '.horizon' / 'engine.json') as control:
        control.ask('server-status')
"""
from pathlib import Path
import json
import socket


class ControlError(Exception):
    """The endpoint refused the token."""


class ControlClient:
    def __init__(self, endpoint, timeout=30):
        """Connects to the endpoint a server wrote, and authenticates. `timeout` is the default per request."""
        info = json.loads(Path(endpoint).read_text())
        self.timeout = timeout
        self.sock = socket.create_connection(('127.0.0.1', info['port']), timeout=timeout)
        self.reader = self.sock.makefile('r', encoding='utf-8')
        self.sock.sendall(f'auth {info["token"]}\n'.encode())
        reply = self.reader.readline()
        if '"ok"' not in reply:
            self.close()
            raise ControlError(reply.strip())

    def ask(self, line, timeout=None):
        """Sends one command line and returns its reply, parsed. `timeout` overrides the default for this request."""
        self.sock.settimeout(timeout or self.timeout)
        self.sock.sendall((line + '\n').encode())
        return json.loads(self.reader.readline())

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
