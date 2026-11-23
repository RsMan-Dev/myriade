import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

# Loaded by path: putting tools/ on sys.path would shadow same-named test modules in tests/ during discovery.
SPEC = importlib.util.spec_from_file_location('bot_lab', Path(__file__).resolve().parents[1] / 'tools' / 'bot_lab.py')
bot_lab = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bot_lab)

# A stand-in for myriade-bot: one JSON answer per request line, echoing the request's id and arguments.
FAKE_BOT = '''
import json, sys, time
for line in sys.stdin:
    request = json.loads(line)
    verb, rid = request['verb'], request['id']
    if verb == 'hang':
        time.sleep(60)
    if verb == 'wrong_id':
        rid += 100
    if verb == 'refuse':
        print(json.dumps({'id': rid, 'ok': False, 'error': 'disconnected: test'}), flush=True)
    else:
        print(json.dumps({'id': rid, 'ok': True, 'verb': verb, 'args': request['args']}), flush=True)
'''


class BotLabTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.script = Path(self.folder.name) / 'fake_bot.py'
        self.script.write_text(FAKE_BOT)

    def tearDown(self):
        self.folder.cleanup()

    def bot(self, timeout=10):
        return bot_lab.Bot(binary=self.script, timeout=timeout)

    def test_ok_answer_is_returned(self):
        with self.bot() as bot:
            answer = bot.send('chat', text='bonjour')
        self.assertTrue(answer['ok'])
        self.assertEqual(answer['args'], {'text': 'bonjour'})

    def test_refusal_raises_bot_error_with_the_error(self):
        with self.bot() as bot:
            with self.assertRaises(bot_lab.BotError) as caught:
                bot.send('refuse')
        self.assertEqual(str(caught.exception), 'disconnected: test')

    def test_answers_are_paired_with_requests(self):
        with self.bot() as bot:
            first = bot.send('state')
            second = bot.send('report')
            with self.assertRaises(bot_lab.BotError):
                bot.send('wrong_id')
        self.assertEqual((first['id'], second['id']), (1, 2))

    def test_timeout_raises_and_stops_the_bot(self):
        with self.bot(timeout=0.5) as bot:
            with self.assertRaises(bot_lab.BotError):
                bot.send('hang')
            self.assertIsNotNone(bot.process.poll())

    def test_leaving_the_context_terminates_the_process(self):
        with self.bot() as bot:
            bot.send('state')
        self.assertEqual(bot.process.wait(5), 0)

    def test_named_method_sends_its_verb_and_arguments(self):
        with self.bot() as bot:
            answer = bot.command('gamemode creative')
            entities = bot.entities(entity_type='zombie')
        self.assertEqual((answer['verb'], answer['args']), ('command', {'line': 'gamemode creative'}))
        self.assertEqual(entities['args'], {'type': 'zombie'})

    def test_join_passes_the_datagen_registries_by_default(self):
        with self.bot() as bot:
            answer = bot.join('127.0.0.1', 25565, 'bot')
        self.assertTrue(answer['args']['registries'].endswith('.local/datagen/out/reports/registries.json'))

    def test_release_use_item_sends_its_verb(self):
        with self.bot() as bot:
            answer = bot.release_use_item()
        self.assertEqual((answer['verb'], answer['args']), ('release_use_item', {}))

    def test_set_carried_item_sends_the_slot(self):
        with self.bot() as bot:
            answer = bot.set_carried_item(3)
        self.assertEqual((answer['verb'], answer['args']), ('set_carried_item', {'slot': 3}))

    def test_player_abilities_sends_the_flying_flag(self):
        with self.bot() as bot:
            answer = bot.player_abilities(True)
        self.assertEqual((answer['verb'], answer['args']), ('player_abilities', {'flying': True}))


if __name__ == '__main__':
    unittest.main()
