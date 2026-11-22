# SPDX-License-Identifier: GPL-3.0-or-later
"""OpenExample: <one sentence: what it does in 3ds Max>.

3ds Max runs Contents/scripts/openexample_startup.ms once it has started; that script puts
Contents/python on sys.path and calls start().

This package must stay importable outside 3ds Max, so that the tests can load it: pymxs and Qt are
imported inside the functions that need them, never at module level.
"""

__version__ = "0.1.0"
# The app's name, as the Open3D launcher shows it and as its folders are called.
APP = "OpenExample"


def start():
    """What runs when 3ds Max starts: one line in the MAXScript Listener that says where OpenExample keeps
    its files. Returns that line. Nothing is created."""
    from . import folders

    line = f"{APP} {__version__}: " + folders.describe(folders.settings_dir(), folders.data_dir(), separator="; ")
    print(line)  # Inside 3ds Max, print writes to the MAXScript Listener.
    return line
