# SPDX-License-Identifier: GPL-3.0-or-later
"""The bridge's server without 3ds Max: pymxs and Qt are stood in for, and only the answers are checked."""

import json
import os
import sys
import tempfile
import types
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))


def _stand_ins():
    """Small stand-ins for pymxs and PySide6, which only exist inside 3ds Max."""
    pymxs = types.ModuleType("pymxs")
    pymxs.runtime = types.SimpleNamespace()
    pymxs.undo = None
    qtcore = types.ModuleType("PySide6.QtCore")
    qtcore.QObject = object
    qtcore.Signal = lambda *_a: None
    pyside = types.ModuleType("PySide6")
    pyside.QtCore = qtcore
    sys.modules.update({"pymxs": pymxs, "PySide6": pyside, "PySide6.QtCore": qtcore})


_stand_ins()

from olib_bridge import config, ops, server  # noqa: E402 - after the stand-ins


class ServerTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.app_json = os.path.join(self.dir, "app.json")
        with open(self.app_json, "w", encoding="utf-8") as fh:
            json.dump({"token": "secret-token", "command": []}, fh)
        self.old_path = config.APP_JSON
        config.APP_JSON = self.app_json
        server._token["value"] = ""

    def tearDown(self):
        config.APP_JSON = self.old_path

    def ask(self, request):
        raw = request if isinstance(request, bytes) else json.dumps(request).encode("utf-8")
        return server.answer(raw)

    def test_ping_with_the_token(self):
        response, go_on = self.ask({"cmd": "ping", "token": "secret-token"})
        self.assertTrue(response["ok"])
        self.assertEqual(response["result"]["protocol"], server.PROTOCOL)
        self.assertTrue(go_on)

    def test_without_the_token_nothing_runs(self):
        for request in ({"cmd": "ping"}, {"cmd": "merge", "token": "other"}, {"cmd": "ping", "token": 7}):
            response, go_on = self.ask(request)
            self.assertFalse(response["ok"])
            self.assertFalse(go_on)  # the connection is closed

    def test_what_a_web_page_sends_closes_the_connection(self):
        for line in (b"POST / HTTP/1.1\r\n", b"\xff\xfe", b"[1, 2]"):
            response, go_on = self.ask(line)
            self.assertFalse(response["ok"])
            self.assertFalse(go_on)

    def test_unknown_commands_and_bad_arguments(self):
        response, go_on = self.ask({"cmd": "execute", "token": "secret-token"})
        self.assertIn("Unknown command", response["error"])
        self.assertTrue(go_on)
        response, _ = self.ask({"cmd": "merge", "token": "secret-token", "args": ["x"]})
        self.assertFalse(response["ok"])

    def test_a_new_token_is_read_again(self):
        self.assertTrue(self.ask({"cmd": "ping", "token": "secret-token"})[0]["ok"])
        with open(self.app_json, "w", encoding="utf-8") as fh:
            json.dump({"token": "new-token"}, fh)
        self.assertTrue(self.ask({"cmd": "ping", "token": "new-token"})[0]["ok"])
        self.assertFalse(self.ask({"cmd": "ping", "token": "secret-token"})[0]["ok"])

    def test_no_app_json_refuses_everything(self):
        os.remove(self.app_json)
        self.assertFalse(self.ask({"cmd": "ping", "token": ""})[0]["ok"])
        self.assertFalse(self.ask({"cmd": "ping", "token": "secret-token"})[0]["ok"])


class TokenTests(unittest.TestCase):
    def test_a_token_that_cannot_be_decrypted_is_none(self):
        self.assertEqual(config.token({"token_dpapi": "00ff"}), "")
        self.assertEqual(config.token({"token_dpapi": "not hex"}), "")
        self.assertEqual(config.token({}), "")
        self.assertEqual(config.token({"token": "plain"}), "plain")


class MaterialArgumentTests(unittest.TestCase):
    """What a request may ask 3ds Max to load."""

    def test_only_images_that_are_there_become_maps(self):
        with tempfile.TemporaryDirectory() as folder:
            image = os.path.join(folder, "a.png")
            script = os.path.join(folder, "a.ms")
            for path in (image, script):
                with open(path, "wb"):
                    pass
            self.assertEqual(ops._maps({"color": image, "other": script, "rough": ""}), {"color": image})
            for bad in (script, image + "x.png", 7):
                with self.assertRaises(RuntimeError):
                    ops._maps({"color": bad})
            with self.assertRaises(RuntimeError):
                ops.apply_interior("x", image, script)

    def test_sizes_and_targets(self):
        self.assertEqual(ops._xyz([30, 60]), [30.0, 60.0, 60.0])
        for bad in ([0], [-1, 2], [float("inf")]):
            with self.assertRaises(RuntimeError):
                ops._xyz(bad)
        with self.assertRaises(RuntimeError):
            ops._route("anywhere")
        self.assertEqual(ops._route("slate"), "slate")


if __name__ == "__main__":
    unittest.main()
