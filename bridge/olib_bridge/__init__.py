# SPDX-License-Identifier: GPL-3.0-or-later
"""OpenLibrary's bridge inside 3ds Max.

The OpenLibrary app installs it in %LOCALAPPDATA%\\Autodesk\\3dsMax\\<version>\\ENU\\scripts\\Open3D\\OpenLibrary,
and the start-up script open3d_openlibrary.ms starts it. It has no interface but the optional button that opens the
app, beside those of the other Open3D apps (toolbar.py).
"""

import ctypes
import os
import subprocess
from ctypes import wintypes

from . import config

VERSION = "0.1.0"


def start():
    from . import server

    port = server.start()
    try:
        from PySide6 import QtCore

        QtCore.QTimer.singleShot(0, _button)
    except Exception as error:  # noqa: BLE001 - 3ds Max goes on without the button
        print("OpenLibrary: the button could not be put: %s" % error)
    return port


def _button():
    """Puts the button that opens the app, once, or takes it away when it is not wanted."""
    from . import toolbar

    try:
        data = config.read()
        wanted = data.get("toolbar", True) or data.get("toolbar_chosen") is not True
        said = toolbar.place() if wanted else toolbar.remove()
    except Exception as error:  # noqa: BLE001 - 3ds Max goes on without the button
        said = "the button could not be put: %s" % error
    if said:
        print("OpenLibrary: %s" % said)


def refresh_state():
    """Called by the callbacks of 3ds Max (olib_stateChanged)."""
    from . import server

    server.refresh_state()


def stop():
    from . import server

    server.stop()


def _app_exe():
    """The app's exe, as app.json names it, if it is there."""
    command = config.read().get("command")
    if not isinstance(command, list) or not command or not isinstance(command[0], str):
        return None
    exe = command[0]
    return exe if exe.lower().endswith(".exe") and os.path.isfile(exe) else None


def _exe_of_window(hwnd):
    kernel = ctypes.windll.kernel32
    pid = wintypes.DWORD()
    ctypes.windll.user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    process = kernel.OpenProcess(0x1000, False, pid.value)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not process:
        return ""
    try:
        buf = ctypes.create_unicode_buffer(1024)
        size = wintypes.DWORD(1024)
        if kernel.QueryFullProcessImageNameW(process, 0, buf, ctypes.byref(size)):
            return buf.value
        return ""
    finally:
        kernel.CloseHandle(process)


def _app_window(exe):
    """The app's main window if it is open: titled "OpenLibrary" and of the exe app.json names."""
    user32 = ctypes.windll.user32
    found = []
    proto = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd, _lparam):
        buf = ctypes.create_unicode_buffer(64)
        user32.GetWindowTextW(hwnd, buf, 64)
        visible = user32.IsWindowVisible(hwnd) or user32.IsIconic(hwnd)
        if buf.value == "OpenLibrary" and visible and os.path.normcase(_exe_of_window(hwnd)) == os.path.normcase(exe):
            found.append(hwnd)
            return False
        return True

    user32.EnumWindows(proto(each), 0)
    return found[0] if found else None


def _raise_window(hwnd):
    """Puts it in front of everything. 3ds Max is in front (its button was just pressed), so Windows lets it."""
    user32 = ctypes.windll.user32
    user32.ShowWindow(hwnd, 9 if user32.IsIconic(hwnd) else 5)  # SW_RESTORE / SW_SHOW
    user32.BringWindowToTop(hwnd)
    if not user32.SetForegroundWindow(hwnd):
        # a second way: above everything for a moment, and back to its place
        flags = 0x0001 | 0x0002 | 0x0040  # NOSIZE | NOMOVE | SHOWWINDOW
        user32.SetWindowPos(hwnd, -1, 0, 0, 0, 0, flags)  # HWND_TOPMOST
        user32.SetWindowPos(hwnd, -2, 0, 0, 0, 0, flags)  # HWND_NOTOPMOST
        user32.SetForegroundWindow(hwnd)


def launch_app():
    """The 3ds Max button: brings OpenLibrary to the front if it is open, or else opens it."""
    exe = _app_exe()
    if exe is None:
        print(
            "OpenLibrary: the app is not where the bridge was told. Open it once (it points the bridge at itself) "
            "or install the bridge again from the app."
        )
        return False
    try:
        hwnd = _app_window(exe)
        if hwnd:
            _raise_window(hwnd)
            return True
    except OSError as error:
        print("OpenLibrary: %s" % error)
    try:
        ctypes.windll.user32.AllowSetForegroundWindow(-1)  # so that the new app may come to the front
    except OSError:
        pass  # without it the app opens behind 3ds Max: still open
    # The exe comes from app.json, which only the app writes, and is checked to be an existing .exe.
    subprocess.Popen([exe], close_fds=True, cwd=os.path.dirname(exe), creationflags=subprocess.DETACHED_PROCESS)  # noqa: S603
    return True
