# SPDX-License-Identifier: GPL-3.0-or-later
"""Runs the tests that need no 3ds Max, with any Python from 3.11 on or with the one of 3ds Max:

python -I tests/run.py
"""

import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.dont_write_bytecode = True  # no __pycache__ beside the sources
# The package lives inside the bundle, where 3ds Max finds it.
sys.path.insert(0, os.path.join(REPO, "OpenExample.bundle", "Contents", "python"))


def main():
    suite = unittest.defaultTestLoader.discover(HERE, pattern="test_*.py", top_level_dir=HERE)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
