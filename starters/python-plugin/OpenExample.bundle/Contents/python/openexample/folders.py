# SPDX-License-Identifier: GPL-3.0-or-later
"""Where OpenExample keeps its files, from what every Open3D app shares (open3d_settings).

Its settings go to %APPDATA%\\Open3D\\OpenExample and everything else it writes (logs, caches) to
%LOCALAPPDATA%\\Open3D\\OpenExample, or to OpenExample inside the folders that OPEN3D_SETTINGS_DIR and
OPEN3D_DATA_DIR name. Nothing here creates them: the code that first writes into one does.

The plug-in's interface is in English only: open3d_settings gives it its folders, never a language.
"""

from . import APP, open3d_settings


def settings_dir():
    """%APPDATA%\\Open3D\\OpenExample, or OpenExample in the folder OPEN3D_SETTINGS_DIR names; None without
    either variable."""
    return open3d_settings.app_settings_dir(APP)


def data_dir():
    """%LOCALAPPDATA%\\Open3D\\OpenExample, or OpenExample in the folder OPEN3D_DATA_DIR names; None without
    either variable."""
    return open3d_settings.app_data_dir(APP)


def describe(settings, data, separator="\n"):
    """The two folders, one after the other; a missing one is said."""

    def shown(folder):
        return str(folder) if folder is not None else "(no folder)"

    return separator.join((f"Settings: {shown(settings)}", f"Other files: {shown(data)}"))
