# SPDX-License-Identifier: GPL-3.0-or-later
"""Where OpenExample keeps its files, and what start() says. Every folder is a temporary one, named by the
variables every Open3D app reads."""

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import openexample
from openexample import folders


class Folders(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.settings = self.root / "settings"
        self.data = self.root / "data"
        variables = {"OPEN3D_SETTINGS_DIR": str(self.settings), "OPEN3D_DATA_DIR": str(self.data)}
        patcher = mock.patch.dict(os.environ, variables)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_the_folders_are_named_after_the_app(self):
        self.assertEqual(folders.settings_dir(), self.settings / "OpenExample")
        self.assertEqual(folders.data_dir(), self.data / "OpenExample")

    def test_without_any_variable_there_is_no_folder(self):
        with mock.patch.dict(os.environ, clear=True):
            self.assertIsNone(folders.settings_dir())
            self.assertIsNone(folders.data_dir())

    def test_start_says_where_the_files_go(self):
        with contextlib.redirect_stdout(io.StringIO()) as printed:
            line = openexample.start()
        self.assertEqual(printed.getvalue(), line + "\n")
        expected = (
            f"OpenExample {openexample.__version__}: "
            f"Settings: {self.settings / 'OpenExample'}; Other files: {self.data / 'OpenExample'}"
        )
        self.assertEqual(line, expected)

    def test_the_plugin_speaks_english_whatever_the_shared_language(self):
        # The shared language is for the programs and the launcher; a 3ds Max plug-in is in English only.
        with contextlib.redirect_stdout(io.StringIO()):
            without = openexample.start()
        self.settings.mkdir()
        (self.settings / "settings.json").write_text(json.dumps({"language": "es"}), encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            with_spanish = openexample.start()
        self.assertEqual(with_spanish, without)

    def test_start_creates_nothing(self):
        with contextlib.redirect_stdout(io.StringIO()):
            openexample.start()
        self.assertEqual(list(self.root.iterdir()), [])

    def test_a_missing_folder_is_said(self):
        self.assertEqual(folders.describe(None, None), "Settings: (no folder)\nOther files: (no folder)")


if __name__ == "__main__":
    unittest.main()
