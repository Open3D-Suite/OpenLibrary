# SPDX-License-Identifier: GPL-3.0-or-later
"""The bridge's local server: the OpenLibrary app talks to this 3ds Max through it.

- It listens only on 127.0.0.1, on the first free port of 47820-47829 (one for each 3ds Max that is open).
- One JSON line per request, {"cmd": ..., "args": {...}, "token": ...}, and one JSON line per answer,
  {"ok": true, "result": ...} or {"ok": false, "error": ...}.
- Every request carries the token that the app wrote into app.json when it installed the bridge; a request
  without it, or with another, is refused and the connection closed. So is a line that is not a JSON object,
  which is what a web page calling the port would send.
- Only the commands of COMMANDS run: there is no way to run code of one's own.
- pymxs is touched only on 3ds Max's main thread: the socket's thread queues the job with a Qt signal and
  waits for it.
"""

import gc
import hmac
import json
import os
import socketserver
import threading

from PySide6 import QtCore

from . import config, ops

PROTOCOL = 2
PORTS = range(47820, 47830)
MAX_LINE = 1 << 20  # a request longer than 1 MB is not one of the app's

_server = None
_thread = None
_invoker = None
_state = {"engine": "other", "file": "", "folder": "", "selection": 0, "busy": False}
_state_lock = threading.Lock()
_token = {"value": ""}


class _Job:
    def __init__(self, fn, args):
        self.fn = fn
        self.args = args
        self.done = threading.Event()
        self.result = None
        self.error = None

    def run(self):
        with _state_lock:
            _state["busy"] = True
        try:
            self.result = self.fn(**self.args)
        except Exception as error:  # noqa: BLE001 - every failure of a command goes back to the app
            self.error = str(error)
            error.__traceback__ = None
            del error
        finally:
            self.fn = self.args = None
            with _state_lock:
                _state["busy"] = False
            refresh_state()
            # What is left of the command and touches MAXScript values (an error with its trace, say) is let go
            # here, on 3ds Max's thread. Collected by Python on the socket's thread, MAXScript would end up with
            # "unknown error ... garbage collection".
            gc.collect()
            self.done.set()


class _Invoker(QtCore.QObject):
    """Lives on 3ds Max's main thread."""

    request = QtCore.Signal(object)

    def __init__(self):
        super().__init__()
        self.request.connect(self._run, QtCore.Qt.QueuedConnection)
        # Callbacks of 3ds Max (olib_watchState) keep the state up to date; this is only a rare check.
        self.timer = QtCore.QTimer(self)
        self.timer.timeout.connect(refresh_state)
        self.timer.start(10000)

    def _run(self, job):
        job.run()


def refresh_state():
    try:
        state = ops.scene_state()
    except Exception as error:  # noqa: BLE001 - a callback of 3ds Max: it must never stop
        print("OpenLibrary: the scene's state could not be read: %s" % error)
        return
    with _state_lock:
        _state.update(state)


def _info():
    with _state_lock:
        state = dict(_state)
    state.update(
        {
            "pid": os.getpid(),
            "protocol": PROTOCOL,
            "max_version": _max_version["year"],
            "port": _server.server_address[1] if _server else 0,
        }
    )
    return state


COMMANDS = {
    "merge": ops.merge,
    "merge_proxy": ops.merge_as_proxy,
    "make_preview": ops.make_preview,
    "relink_scene": ops.relink_scene,
    "relink_file": ops.relink_max_file,
    "check_drop": ops.check_drop,
    "apply_material": ops.apply_material,
    "apply_materials": ops.apply_materials,
    "apply_library": ops.apply_library,
    "add_textures": ops.add_textures,
    "apply_interior": ops.apply_interior,
    "open_file": ops.open_max_file,
    "focus": ops.focus_max,
}


def token_ok(given):
    """Whether a request's token is the app's. The file is read again once when it does not match: the app may
    have written a new one since 3ds Max started."""
    if not isinstance(given, str) or not given:
        return False
    if _token["value"] and hmac.compare_digest(given, _token["value"]):
        return True
    _token["value"] = config.token()
    return bool(_token["value"]) and hmac.compare_digest(given, _token["value"])


def answer(raw):
    """The answer to one request line, and whether the connection may go on."""
    try:
        request = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return {"ok": False, "error": "Not a request of OpenLibrary."}, False
    if not isinstance(request, dict):
        return {"ok": False, "error": "Not a request of OpenLibrary."}, False
    if not token_ok(request.get("token")):
        print("OpenLibrary: a request without the app's token was refused.")
        return {"ok": False, "error": "Refused: the request does not carry OpenLibrary's token."}, False
    cmd = request.get("cmd")
    args = request.get("args") or {}
    if cmd == "ping":
        return {"ok": True, "result": _info()}, True  # does not touch 3ds Max: always answers
    if cmd not in COMMANDS:
        return {"ok": False, "error": "Unknown command: %s" % cmd}, True
    if not isinstance(args, dict):
        return {"ok": False, "error": "The arguments of %s are not valid." % cmd}, True
    if _state.get("busy"):
        # still placing the model before, say: it is not interrupted
        return {"ok": False, "error": "3ds Max is busy with the command before.", "busy": True}, True
    job = _Job(COMMANDS[cmd], args)
    _invoker.request.emit(job)
    job.done.wait()
    return {"ok": job.error is None, "result": job.result, "error": job.error}, True


class _Handler(socketserver.StreamRequestHandler):
    def handle(self):
        while True:
            raw = self.rfile.readline(MAX_LINE)
            if not raw:
                return
            response, go_on = answer(raw)
            try:
                self.wfile.write((json.dumps(response) + "\n").encode("utf-8"))
                self.wfile.flush()
            except OSError:
                return
            if not go_on:
                return


class _Server(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = False  # on Windows it would let another program take the port


_max_version = {"year": 0}


def start():
    """Starts the bridge, once. Call it on 3ds Max's main thread."""
    global _server, _thread, _invoker
    if _server is not None:
        return _server.server_address[1]
    ops.load_core()
    _max_version["year"] = ops.max_version()
    _token["value"] = config.token()
    ops.watch_state(True)
    _invoker = _Invoker()
    refresh_state()
    for port in PORTS:
        try:
            _server = _Server(("127.0.0.1", port), _Handler)
            break
        except OSError:
            continue
    if _server is None:
        print("OpenLibrary: no free port (47820-47829)")
        return 0
    _thread = threading.Thread(target=_server.serve_forever, name="OpenLibraryBridge", daemon=True)
    _thread.start()
    print("OpenLibrary: bridge listening on port %d" % _server.server_address[1])
    return _server.server_address[1]


def stop():
    global _server, _thread
    try:
        ops.watch_state(False)
    except Exception as error:  # noqa: BLE001 - stopping goes on whatever 3ds Max says
        print("OpenLibrary: %s" % error)
    if _server is not None:
        _server.shutdown()
        _server.server_close()
    _server = None
    _thread = None
