# SPDX-License-Identifier: GPL-3.0-or-later
"""The button in the interface file of 3ds Max, without 3ds Max: only the text changes are checked."""

import os
import shutil
import sys
import tempfile
import types
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from olib_bridge import toolbar  # noqa: E402 - after the path

OPENMANAGER = (
    '<Item typeID="2" type="CTB_MACROBUTTON" width="0" height="0" controlID="0" macroTypeID="3" '
    'macroType="MB_TYPE_ACTION" actionTableID="647394" imageID="-1" imageName="" actionID="OpenManager`OpenManager" '
    'tip="OpenManager" label="OpenManager" />'
)


def interface(*windows):
    body = "".join(windows)
    return (
        '<?xml version="1.0" encoding="utf-8"?>\r\n<ADSK_CUI>\r\n    <CUIWindows>\r\n'
        + body
        + "    </CUIWindows>\r\n</ADSK_CUI>\r\n"
    )


def window(name, *items):
    lines = "".join("                %s\r\n" % item for item in items)
    return (
        '        <Window objectName="%s" name="%s" type="T" cType="1" toolbarRows="1">\r\n' % (name, name)
        + "            <Items>\r\n"
        + lines
        + "            </Items>\r\n        </Window>\r\n"
    )


class ButtonTests(unittest.TestCase):
    def test_it_goes_beside_the_button_of_openmanager(self):
        text = interface(window("Mine", OPENMANAGER))
        new = toolbar.with_button(text)
        lines = new.splitlines()
        at = next(i for i, line in enumerate(lines) if "OpenManager`OpenManager" in line)
        self.assertIn(toolbar.ACTION, lines[at + 1])
        self.assertEqual(lines[at + 1][: lines[at + 1].index("<")], lines[at][: lines[at].index("<")])
        self.assertNotIn(toolbar.WINDOW, new)
        self.assertIn("\r\n", new)
        self.assertNotIn("\r\r", new)
        self.assertIsNone(toolbar.with_button(new))  # once only

    def test_without_another_app_it_has_a_toolbar_of_its_own(self):
        text = interface(window("Main Toolbar"))
        new = toolbar.with_button(text)
        self.assertIn('<Window objectName="%s"' % toolbar.WINDOW, new)
        self.assertLess(new.index(toolbar.WINDOW), new.index("</CUIWindows>"))
        self.assertIsNone(toolbar.with_button("<ADSK_CUI></ADSK_CUI>"))

    def test_taking_it_away_leaves_the_rest_as_it_was(self):
        for text in (interface(window("Mine", OPENMANAGER)), interface(window("Main Toolbar"))):
            self.assertEqual(toolbar.without_button(toolbar.with_button(text)), text)

    def test_the_data_folder_is_the_apps(self):
        found = toolbar.data_folder({"LOCALAPPDATA": r"L:\local"})
        self.assertEqual(found, os.path.join(r"L:\local", "Open3D", "OpenLibrary"))
        self.assertEqual(toolbar.data_folder({"OPEN3D_DATA_DIR": r"D:\data"}), os.path.join(r"D:\data", "OpenLibrary"))
        self.assertIsNone(toolbar.data_folder({}))

    def test_every_icon_is_there(self):
        for theme in ("Dark", "Light"):
            for size in toolbar.SIZES:
                self.assertTrue(os.path.isfile(os.path.join(toolbar.HERE, "icons", theme, "OpenLibrary_%d.png" % size)))


class FakeMax:
    """What place() asks of 3ds Max, with an interface file in a folder of its own."""

    def __init__(self, folder, text):
        self.file = os.path.join(folder, "ui.cuix")
        self.icons = os.path.join(folder, "usericons")
        self.saved = 0
        with open(self.file, "w", encoding="utf-8-sig", newline="") as f:
            f.write(text)
        self.cui = types.SimpleNamespace(
            getConfigFile=lambda: self.file,
            saveConfig=self._save,
            loadConfig=lambda path: True,
            showToolbar=lambda name: True,
        )
        self.colorman = types.SimpleNamespace(reInitIcons=lambda: True)

    def _save(self):
        self.saved += 1

    def GetQuietMode(self):  # noqa: N802 - the name 3ds Max gives it
        return False

    def IsNetServer(self):  # noqa: N802 - the name 3ds Max gives it
        return False

    def Name(self, name):  # noqa: N802 - the name 3ds Max gives it
        return name

    def getDir(self, name):  # noqa: N802 - the name 3ds Max gives it
        return self.icons

    def text(self):
        with open(self.file, encoding="utf-8-sig", newline="") as f:
            return f.read()


class PlaceTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.max = FakeMax(self.dir, interface(window("Mine", OPENMANAGER)))
        sys.modules["pymxs"] = types.SimpleNamespace(runtime=self.max)
        self.env = {"OPEN3D_DATA_DIR": os.path.join(self.dir, "data")}

    def tearDown(self):
        sys.modules.pop("pymxs", None)
        shutil.rmtree(self.dir)

    def test_a_button_3ds_max_lost_is_put_back(self):
        os.makedirs(toolbar.data_folder(self.env))
        with open(os.path.join(toolbar.data_folder(self.env), toolbar.DONE), "w") as f:
            f.write("1\n")  # it was put before, and a 3ds Max that closed later saved its interface without it
        self.assertTrue(toolbar.place(self.env).startswith("the interface was changed"))
        self.assertTrue(toolbar.has_button(self.max.text()))
        self.assertTrue(os.path.isfile(os.path.join(self.max.icons, "Dark", "OpenLibrary_24.png")))

    def test_an_interface_with_the_button_is_not_saved_again(self):
        toolbar.place(self.env)
        saved = self.max.saved
        self.assertIsNone(toolbar.place(self.env))
        self.assertEqual(self.max.saved, saved)
        self.assertEqual(self.max.text().count(toolbar.ACTION), 1)

    def test_taking_it_away_when_it_is_not_wanted(self):
        toolbar.place(self.env)
        self.assertTrue(toolbar.remove(self.env).startswith("the interface was changed"))
        self.assertFalse(toolbar.has_button(self.max.text()))
        self.assertIsNone(toolbar.remove(self.env))  # once


if __name__ == "__main__":
    unittest.main()
