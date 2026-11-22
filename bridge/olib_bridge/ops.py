# SPDX-License-Identifier: GPL-3.0-or-later
"""What the bridge does inside 3ds Max, with pymxs and olib_core.ms.

Everything here runs on 3ds Max's main thread (server.py makes sure of it). Settings and paths come from the
app already worked out; nothing here reads a settings file.
"""

import contextlib
import ctypes
import os
import time

import pymxs
from pymxs import runtime as rt

HERE = os.path.dirname(os.path.abspath(__file__))
CORE_MS = os.path.join(HERE, "olib_core.ms")
TEX_EXTS = {
    ".jpg",
    ".jpeg",
    ".png",
    ".tif",
    ".tiff",
    ".exr",
    ".hdr",
    ".tga",
    ".bmp",
    ".psd",
    ".tx",
    ".webp",
    ".ies",
    ".cgeo",
    ".cproxy",
    ".vrmesh",
    ".abc",
}
ENGINES = ("corona", "vray")
IMPORT_KEYS = (
    "group",
    "layer_mode",
    "object_dups",
    "material_dups",
    "import_lights",
    "import_cameras",
    "place_on_surface",
    "place_multiple",
    "proxy_mode",
    "proxy_display",
)


def load_core():
    rt.fileIn(CORE_MS)


@contextlib.contextmanager
def _undo(label):
    """An undo block of 3ds Max. pymxs.undo swallows the exceptions inside it: they are kept and raised again
    on the way out, so that the app gets the error instead of an empty result."""
    caught = []
    with pymxs.undo(True, label):
        try:
            yield
        except Exception as error:  # noqa: BLE001 - raised again below, outside the undo block
            caught.append(error)
    if caught:
        error = caught.pop()  # out of the list, so that it does not stay in a cycle with this frame
        try:
            raise error
        finally:
            del error


def _import_settings(imp):
    """Only the known import settings, each of the kind expected; a request sets nothing else."""
    if not isinstance(imp, dict):
        raise RuntimeError("The import settings are not valid.")
    out = {}
    for key in IMPORT_KEYS:
        value = imp.get(key)
        if isinstance(value, (bool, str)):
            out[key] = value
    return out


def _max_file(path):
    if not isinstance(path, str) or not path.lower().endswith(".max") or not os.path.isfile(path):
        raise RuntimeError("The .max file is not there: %s" % path)
    return path


# ---------------------------------------------------------------------------------------------------------------
# The state of the scene
# ---------------------------------------------------------------------------------------------------------------
def max_version():
    """The year of this 3ds Max (28000 is 2026)."""
    try:
        return 1998 + int(rt.maxVersion()[0]) // 1000
    except (RuntimeError, TypeError, ValueError, IndexError):
        return 0


def watch_state(on):
    """Callbacks of 3ds Max that tell the bridge when the renderer, the file or the selection changes."""
    rt.olib_watchState(bool(on))


def scene_state():
    return {
        "engine": str(rt.olib_sceneEngine()),
        "file": str(rt.maxFileName),
        "folder": str(rt.maxFilePath),
        "selection": int(rt.selection.count),
    }


def focus_max():
    """Brings 3ds Max's window to the front (the app allows it first)."""
    try:
        hwnd = int(rt.windows.getMAXHWND())
        user32 = ctypes.windll.user32
        if user32.IsIconic(hwnd):
            user32.ShowWindow(hwnd, 9)  # SW_RESTORE
        user32.SetForegroundWindow(hwnd)
    except (RuntimeError, OSError, ValueError):
        pass  # a window that does not come forward is no reason to stop the command


# ---------------------------------------------------------------------------------------------------------------
# Lost paths
# ---------------------------------------------------------------------------------------------------------------
_map_cache = {}  # folders -> (time, map): relinking many files one after another does not search again


def build_file_map(dirs, max_files=200000):
    """name in lower case -> path, searching dirs and their subfolders."""
    key = tuple(os.path.normcase(d) for d in dirs if isinstance(d, str) and d)
    hit = _map_cache.get(key)
    if hit is not None and time.time() - hit[0] < 120:
        return hit[1]
    found = _walk_files(key, max_files)
    if len(_map_cache) > 40:
        _map_cache.clear()
    _map_cache[key] = (time.time(), found)
    return found


def _walk_files(dirs, max_files):
    found = {}
    count = 0
    for d in dirs:
        if not os.path.isdir(d):
            continue
        for root, subdirs, files in os.walk(d):
            subdirs[:] = [s for s in subdirs if not s.startswith(".") and s.lower() != "$recycle.bin"]
            for f in files:
                if os.path.splitext(f)[1].lower() in TEX_EXTS:
                    found.setdefault(f.lower(), os.path.join(root, f))
                    count += 1
            if count > max_files:
                return found
    return found


def _missing(path, base_dir=None):
    if not path or os.path.isfile(path):
        return False
    if base_dir and not os.path.isabs(path) and os.path.isfile(os.path.join(base_dir, path)):
        return False
    return True


def relink_scene(search_dirs):
    """Points the lost files of the scene at files of the same name in search_dirs. -> [fixed, still lost]."""
    files = [str(f) for f in rt.olib_sceneFiles()]
    missing = [f for f in files if os.path.splitext(f)[1] and _missing(f)]
    if not missing:
        return [0, 0]
    fmap = build_file_map(search_dirs)
    fixed = 0
    for f in missing:
        hit = fmap.get(os.path.basename(f).lower())
        if hit and rt.olib_setFileDir(f, os.path.dirname(hit)):
            fixed += 1
    return [fixed, len(missing) - fixed]


def relink_max_file(max_path, search_dirs):
    """Relinks the paths INSIDE a .max of the library without opening it. -> [fixed, still lost]."""
    max_path = _max_file(max_path)
    meta = rt.getMAXFileAssetMetadata(max_path)
    if meta is None:
        return [0, 0]
    base_dir = os.path.dirname(max_path)
    dirs = [base_dir] + [d for d in search_dirs if isinstance(d, str)]
    fmap = None
    fixed = missing = 0
    for a in meta:
        p = str(a.filename)
        if not os.path.splitext(p)[1] or not _missing(p, base_dir):
            continue
        if fmap is None:
            fmap = build_file_map(dirs)
        hit = fmap.get(os.path.basename(p).lower())
        if hit:
            a.filename = hit
            fixed += 1
        else:
            missing += 1
    if fixed:
        rt.setMAXFileAssetMetadata(max_path, meta)
    return [fixed, missing]


# ---------------------------------------------------------------------------------------------------------------
# Merge
# ---------------------------------------------------------------------------------------------------------------
def merge(max_path, asset_name, imp, place=True, relink_dirs=None):
    max_path = _max_file(max_path)
    imp = _import_settings(imp)
    focus_max()
    with _undo("OpenLibrary: Merge " + str(asset_name)):
        nodes = rt.olib_merge(
            max_path,
            str(asset_name),
            bool(imp.get("group", True)),
            str(imp.get("layer_mode", "asset")),
            str(imp.get("object_dups", "autorename")),
            str(imp.get("material_dups", "scene")),
            bool(imp.get("import_lights", False)),
            bool(imp.get("import_cameras", False)),
        )
        if relink_dirs and len(nodes):
            relink_scene(relink_dirs)
        if place and len(nodes):
            nodes = rt.olib_place(nodes, bool(imp.get("place_on_surface", True)), bool(imp.get("place_multiple", True)))
    rt.redrawViews()
    return len(nodes)


def merge_as_proxy(max_path, asset_name, engine, imp, proxy_dir, place=True, relink_dirs=None):
    if engine not in ENGINES:
        raise RuntimeError("Merge as proxy needs Corona or V-Ray as the renderer of the scene.")
    max_path = _max_file(max_path)
    imp = _import_settings(imp)
    if not isinstance(proxy_dir, str) or not os.path.isabs(proxy_dir):
        raise RuntimeError("The folder for the proxy is not valid.")
    focus_max()
    with _undo("OpenLibrary: Proxy " + str(asset_name)):
        nodes = rt.olib_mergeProxy(
            max_path,
            str(asset_name),
            proxy_dir,
            str(imp.get("proxy_mode", "single")),
            engine,
            str(imp.get("layer_mode", "asset")),
            str(imp.get("material_dups", "scene")),
            str(imp.get("proxy_display", "preview")),
        )
        if relink_dirs and len(nodes):
            relink_scene(relink_dirs)
        if place and len(nodes):
            nodes = rt.olib_place(nodes, bool(imp.get("place_on_surface", True)), bool(imp.get("place_multiple", True)))
    rt.redrawViews()
    return len(nodes)


# ---------------------------------------------------------------------------------------------------------------
# Preview, open
# ---------------------------------------------------------------------------------------------------------------
def make_preview(out_file, prev, from_view=False):
    if not isinstance(out_file, str) or not out_file.lower().endswith((".jpg", ".jpeg", ".png")):
        raise RuntimeError("The preview must be a .jpg or a .png.")
    if not from_view and int(rt.selection.count) == 0:
        raise RuntimeError("Select in 3ds Max the objects the preview shows.")
    return bool(
        rt.olib_makePreview(
            out_file,
            int(prev["size"]),
            str(prev["method"]),
            float(prev["margin"]),
            float(prev["elevation"]),
            float(prev["azimuth"]),
            bool(from_view),
        )
    )


def open_max_file(max_path):
    max_path = _max_file(max_path)
    focus_max()
    if rt.checkForSave():
        rt.loadMaxFile(max_path, useFileUnits=True, quiet=True)
    return True
