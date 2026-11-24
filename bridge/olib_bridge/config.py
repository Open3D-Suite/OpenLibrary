# SPDX-License-Identifier: GPL-3.0-or-later
"""app.json, which the OpenLibrary app writes beside the bridge when it installs it: where the app is
("command"), whether 3ds Max shows the OpenLibrary toolbar ("toolbar") and the token the app's requests carry,
encrypted for this Windows user with DPAPI ("token_dpapi", in hex)."""

import ctypes
import json
import os
from ctypes import wintypes

HERE = os.path.dirname(os.path.abspath(__file__))
APP_JSON = os.path.join(os.path.dirname(HERE), "app.json")


def read():
    """The settings in app.json; empty when it is missing or broken (the bridge then refuses every request)."""
    try:
        with open(APP_JSON, encoding="utf-8-sig") as fh:
            data = json.load(fh)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


class _Blob(ctypes.Structure):
    _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_char))]


def _unprotect(data):
    """DPAPI: what the app encrypted for this Windows user; None when it cannot be decrypted."""
    buffer = ctypes.create_string_buffer(data, len(data))
    blob_in = _Blob(len(data), ctypes.cast(buffer, ctypes.POINTER(ctypes.c_char)))
    blob_out = _Blob()
    crypt32 = ctypes.windll.crypt32
    if not crypt32.CryptUnprotectData(ctypes.byref(blob_in), None, None, None, None, 1, ctypes.byref(blob_out)):
        return None
    try:
        return ctypes.string_at(blob_out.data, blob_out.size)
    finally:
        ctypes.windll.kernel32.LocalFree(blob_out.data)


def token(data=None):
    """The token the app's requests must carry; empty when there is none (every request is then refused)."""
    data = read() if data is None else data
    sealed = data.get("token_dpapi")
    if isinstance(sealed, str) and sealed:
        try:
            plain = _unprotect(bytes.fromhex(sealed))
        except (ValueError, OSError):
            plain = None
        return plain.decode("ascii", "replace") if plain else ""
    return ""
