"""The control socket's state reads, decoded by tools/server_lab.Server from a fake flow (no server runs)."""
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.append(str(Path(__file__).resolve().parent.parent / 'tools'))
import server_lab  # noqa: E402


class FakeControl:
    """Answers each request line from a table, and remembers the lines asked."""

    def __init__(self, replies):
        self.replies = replies
        self.asked = []

    def ask(self, line, timeout=None):
        self.asked.append(line)
        return self.replies[line]

    def close(self):
        pass


def server_with(replies):
    server = server_lab.Server.__new__(server_lab.Server)
    server.control = FakeControl(replies)
    return server


class ServerOp(unittest.TestCase):
    def test_op_sends_the_player_and_the_operation_on_one_line(self):
        reply = {'cmd': '#tick', 'messages': [], 'results': []}
        server = server_with({'op Player {"op":"tick","n":2,"record":true}': reply})
        self.assertEqual(server.op('Player', {'op': 'tick', 'n': 2, 'record': True}), reply)
        self.assertEqual(server.control.asked, ['op Player {"op":"tick","n":2,"record":true}'])

    def test_an_error_reply_raises(self):
        server = server_with({'op Player {"op":"tick"}': {'error': 'no player Player'}})
        with self.assertRaisesRegex(RuntimeError, 'no player'):
            server.op('Player', {'op': 'tick'})


class ServerStateReads(unittest.TestCase):
    def test_block_decodes_the_state_and_its_properties(self):
        reply = {'x': 1, 'y': 64, 'z': -2, 'id': 'minecraft:oak_door', 'properties': {'facing': 'north'}}
        server = server_with({'block 1 64 -2': reply})
        self.assertEqual(server.block(1, 64, -2), reply)
        self.assertEqual(server.control.asked, ['block 1 64 -2'])

    def test_blocks_returns_the_list_of_states(self):
        first = {'x': 0, 'y': 0, 'z': 0, 'id': 'minecraft:stone', 'properties': {}}
        second = {'x': 0, 'y': 0, 'z': 1, 'id': 'minecraft:air', 'properties': {}}
        server = server_with({'blocks 0 0 0 0 0 1': {'blocks': [first, second]}})
        self.assertEqual(server.blocks(0, 0, 0, 0, 0, 1), [first, second])

    def test_an_error_reply_raises(self):
        server = server_with({'blocks 0 0 0 16 15 16': {'error': 'blocks: more than 4096 blocks'}})
        with self.assertRaisesRegex(RuntimeError, 'more than 4096'):
            server.blocks(0, 0, 0, 16, 15, 16)

    def test_entities_server_keeps_the_ecs_and_the_players_apart(self):
        reply = {'entities': [{'id': 7, 'type': 'minecraft:zombie', 'pos': [1.0, 2.0, 3.0]}],
                 'players': [{'name': 'RsMan', 'uuid': '00000000-0000-0000-0000-000000000001', 'pos': [0, 0, 0]}]}
        server = server_with({'entities_server': reply})
        self.assertEqual(server.entities_server(), reply)

    def test_state_sends_the_key_and_its_arguments(self):
        server = server_with({'state lumiere 1 2 3': {'sky': 15, 'block': 0},
                              'state raid': {'raids': [1, 2]}})
        self.assertEqual(server.state('lumiere', 1, 2, 3), {'sky': 15, 'block': 0})
        self.assertEqual(server.state('raid'), {'raids': [1, 2]})

    def test_an_unknown_state_key_raises(self):
        server = server_with({'state marchand': {'error': 'unknown state "marchand"'}})
        with self.assertRaises(RuntimeError):
            server.state('marchand')

    def test_run_as_returns_the_result_and_messages_of_the_player(self):
        reply = {'result': 1, 'success': True, 'messages': ['Set the time to 6000']}
        server = server_with({'run_as RsMan time set 6000': reply})
        self.assertEqual(server.run_as('RsMan', 'time set 6000'), reply)

    def test_run_as_raises_for_an_absent_player(self):
        server = server_with({'run_as Nobody say hi': {'error': 'no player Nobody'}})
        with self.assertRaisesRegex(RuntimeError, 'no player Nobody'):
            server.run_as('Nobody', 'say hi')

    def test_save_reports_a_failure_without_raising(self):
        failed = {'saved': False, 'error': 'disk full'}
        server = server_with({'save': failed})
        self.assertEqual(server.save(), failed)

    def test_restart_stops_then_starts_the_same_command_until_it_answers(self):
        control = FakeControl({'restart': {'stopping': True}, 'server-status': {'tps': 20.0}})
        with tempfile.TemporaryDirectory() as folder, \
                mock.patch.object(server_lab.subprocess, 'Popen', return_value=FakeProcess()) as popen, \
                mock.patch.object(server_lab.lab, 'game_env', return_value={}), \
                mock.patch.object(server_lab, 'ControlClient', return_value=control):
            server = relaunchable(folder, control, FakeProcess())
            self.assertEqual(server.restart(), {'stopping': True})
            popen.assert_called_once()
            self.assertEqual(popen.call_args.args[0], ['horizon-client', '--dedicated'])
            self.assertEqual(control.asked, ['restart', 'server-status'])
            server.log.close()

    def test_restart_raises_when_the_relaunched_server_exits(self):
        control = FakeControl({'restart': {'stopping': True}})
        with tempfile.TemporaryDirectory() as folder, \
                mock.patch.object(server_lab.subprocess, 'Popen', return_value=FakeProcess(exits=True)), \
                mock.patch.object(server_lab.lab, 'game_env', return_value={}), \
                mock.patch.object(server_lab, 'ControlClient', return_value=control):
            server = relaunchable(folder, control, FakeProcess())
            with self.assertRaisesRegex(RuntimeError, 'the server exited'):
                server.restart()
            server.log.close()


class FakeProcess:
    """A server process: running until `exits`, then exited at once."""

    def __init__(self, exits=False):
        self.exits = exits
        self.stdin = None

    def poll(self):
        return 0 if self.exits else None

    def wait(self, seconds=None):
        return 0

    def kill(self):
        pass


class FakeEndpoint:
    def exists(self):
        return True


def relaunchable(folder, control, process):
    """A Server whose launch and endpoint are fakes, its folder a scratch one."""
    server = server_lab.Server.__new__(server_lab.Server)
    server.folder = Path(folder)
    server.argv = ['horizon-client', '--dedicated']
    server.endpoint = FakeEndpoint()
    server.ready_seconds = 5
    server.control = control
    server.process = process
    server.log = io.StringIO()
    return server


if __name__ == '__main__':
    unittest.main()
