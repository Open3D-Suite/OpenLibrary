# SPDX-License-Identifier: GPL-3.0-or-later
"""The button in the interface file of 3ds Max, without 3ds Max: only the text changes are checked."""

import os
import sys
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


if __name__ == "__main__":
    unittest.main()
