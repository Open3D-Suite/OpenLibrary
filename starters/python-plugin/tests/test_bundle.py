# SPDX-License-Identifier: GPL-3.0-or-later
"""The bundle 3ds Max loads: its PackageContents.xml says what 3ds Max needs to load it, and the codes that
make it OpenExample's have been made for it."""

import re
import unittest
import xml.etree.ElementTree as ElementTree
from pathlib import Path

import openexample

BUNDLE = Path(__file__).absolute().parent.parent / "OpenExample.bundle"
GUID = re.compile(r"\{[0-9A-F]{8}(?:-[0-9A-F]{4}){3}-[0-9A-F]{12}\}", re.IGNORECASE)
MAKE_ONE = 'python -c "import uuid; print(str(uuid.uuid4()).upper())"'


class Bundle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # S314: the file read is this repository's own, not data from outside.
        cls.package = ElementTree.parse(BUNDLE / "PackageContents.xml").getroot()  # noqa: S314

    def test_it_is_this_app(self):
        self.assertEqual(self.package.get("Name"), openexample.APP)
        for entry in self.package.iter("ComponentEntry"):
            self.assertEqual(entry.get("AppName"), openexample.APP)

    def test_every_3ds_max_from_2025_on_loads_it(self):
        # 3ds Max 2025 has Python 3.11, the oldest this code runs on; 3ds Max 2027 ignores a bundle
        # without SeriesMax.
        requirements = list(self.package.iter("RuntimeRequirements"))
        self.assertTrue(requirements)
        for requirement in requirements:
            self.assertEqual((requirement.get("SeriesMin"), requirement.get("SeriesMax")), ("2025", "2099"))

    def test_its_start_script_is_in_the_bundle(self):
        entries = list(self.package.iter("ComponentEntry"))
        self.assertTrue(entries)
        for entry in entries:
            self.assertTrue((BUNDLE / entry.get("ModuleName")).is_file(), entry.get("ModuleName"))

    def test_its_codes_are_made_for_it(self):
        codes = {name: self.package.get(name) for name in ("ProductCode", "UpgradeCode")}
        for name, code in codes.items():
            made = f"PackageContents.xml: {name} is {code}, not a GUID in braces made for {openexample.APP}"
            self.assertTrue(GUID.fullmatch(code or ""), f"{made}. Make one with  {MAKE_ONE}")
        self.assertNotEqual(codes["ProductCode"].upper(), codes["UpgradeCode"].upper())


if __name__ == "__main__":
    unittest.main()
