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
SCRIPTS = ("olib_core.ms", "olib_materials.ms")
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
TARGETS = ("selection", "cursor", "slate", "plane")
IMAGE_EXTS = (".jpg", ".jpeg", ".png", ".tif", ".tiff", ".exr", ".tga", ".bmp", ".webp", ".hdr")
CHANNELS = (
    "color",
    "rough",
    "gloss",
    "refl",
    "metal",
    "normal",
    "normaldx",
    "bump",
    "disp",
    "opacity",
    "emission",
    "transl",
)
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
    for name in SCRIPTS:
        rt.fileIn(os.path.join(HERE, name))


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
def _asset_id(value):
    """The mark of a model in the scene: letters and digits the app makes, or nothing."""
    if not value:
        return ""
    if not isinstance(value, str) or len(value) > 40 or not value.isalnum():
        raise RuntimeError("The mark of the model is not valid.")
    return value


def merge(max_path, asset_name, imp, place=True, relink_dirs=None, asset_id=""):
    max_path = _max_file(max_path)
    asset_id = _asset_id(asset_id)
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
        if asset_id and len(nodes):
            rt.olib_tag(nodes, asset_id, "model")
        if relink_dirs and len(nodes):
            relink_scene(relink_dirs)
        if place and len(nodes):
            nodes = rt.olib_place(nodes, bool(imp.get("place_on_surface", True)), bool(imp.get("place_multiple", True)))
    rt.redrawViews()
    return len(nodes)


def merge_as_proxy(max_path, asset_name, engine, imp, proxy_dir, place=True, relink_dirs=None, asset_id=""):
    if engine not in ENGINES:
        raise RuntimeError("Merge as proxy needs Corona or V-Ray as the renderer of the scene.")
    max_path = _max_file(max_path)
    asset_id = _asset_id(asset_id)
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
        if asset_id and len(nodes):
            rt.olib_tag(nodes, asset_id, "proxy")
        if relink_dirs and len(nodes):
            relink_scene(relink_dirs)
        if place and len(nodes):
            nodes = rt.olib_place(nodes, bool(imp.get("place_on_surface", True)), bool(imp.get("place_multiple", True)))
    rt.redrawViews()
    return len(nodes)


def check_asset(asset_id, asset_name):
    """How many objects of a model are in the scene, as a proxy and as geometry."""
    asset_id = _asset_id(asset_id)
    return {kind: len(rt.olib_findAsset(asset_id, str(asset_name), kind)) for kind in ("proxy", "model")}


def instance_asset(asset_id, asset_name, kind, imp):
    """Another instance of a model that is in the scene, placed with the mouse."""
    asset_id = _asset_id(asset_id)
    imp = _import_settings(imp)
    if kind not in ("proxy", "model"):
        raise RuntimeError("The kind is not valid.")
    focus_max()
    with _undo("OpenLibrary: Instance " + str(asset_name)):
        found = rt.olib_findAsset(asset_id, str(asset_name), kind)
        if not len(found):
            raise RuntimeError("It is not in the scene any more.")
        nodes = rt.olib_instanceOf(found, asset_id, kind)
        if len(nodes):
            nodes = rt.olib_place(nodes, bool(imp.get("place_on_surface", True)), bool(imp.get("place_multiple", True)))
    rt.redrawViews()
    return len(nodes)


# ---------------------------------------------------------------------------------------------------------------
# What is under the mouse
# ---------------------------------------------------------------------------------------------------------------
class _Point(ctypes.Structure):
    _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]


class _Rect(ctypes.Structure):
    _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long), ("right", ctypes.c_long), ("bottom", ctypes.c_long)]


def _user32():
    user32 = ctypes.windll.user32
    user32.WindowFromPoint.restype = ctypes.c_void_p
    user32.WindowFromPoint.argtypes = [_Point]
    user32.GetAncestor.restype = ctypes.c_void_p
    user32.GetAncestor.argtypes = [ctypes.c_void_p, ctypes.c_uint]
    user32.IsChild.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    user32.ScreenToClient.argtypes = [ctypes.c_void_p, ctypes.POINTER(_Point)]
    user32.GetClientRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(_Rect)]
    user32.GetWindowTextW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
    return user32


def _view_window(index):
    """The window of a viewport, or None."""
    try:
        return int(rt.viewport.getHWnd(index=index)) or None
    except (RuntimeError, TypeError, ValueError):
        return None


def look_at():
    """What of 3ds Max is under the mouse: ("view", index, x, y, width, height), a viewport with the pixel inside
    its window and the size of it (in Windows pixels); ("slate",), the Slate editor; or ("other",)."""
    user32 = _user32()
    point = _Point()
    user32.GetCursorPos(ctypes.byref(point))
    hwnd = user32.WindowFromPoint(point)
    if not hwnd:
        return ("other",)
    for i in range(1, int(rt.viewport.numViews) + 1):
        view = _view_window(i)
        if view and (view == hwnd or user32.IsChild(view, hwnd)):
            at = _Point(point.x, point.y)
            user32.ScreenToClient(view, ctypes.byref(at))
            box = _Rect()
            user32.GetClientRect(view, ctypes.byref(box))
            return ("view", i, at.x, at.y, box.right - box.left, box.bottom - box.top)
    root = user32.GetAncestor(hwnd, 2) or hwnd
    title = ctypes.create_unicode_buffer(256)
    user32.GetWindowTextW(root, title, 256)
    if "slate" in title.value.lower() or "material editor" in title.value.lower():
        return ("slate",)
    # out of the viewports and inside Slate's frame (docked in 3ds Max's window too)
    return ("slate",) if bool(rt.olib_mouseOverSlate()) else ("other",)


def _route(target):
    """On a drop ("cursor"): over Slate, only to Slate; over a viewport, the object at that pixel (or, without
    one, the selection)."""
    if target not in TARGETS:
        raise RuntimeError("The target is not valid.")
    if target != "cursor":
        return target
    where = look_at()
    if where[0] == "slate":
        return "slate"
    rt.olib_cursorHit = rt.olib_nodeAtView(*where[1:]) if where[0] == "view" else None
    rt.olib_cursorKnown = True
    return target


# ---------------------------------------------------------------------------------------------------------------
# Materials, textures and interiors
# ---------------------------------------------------------------------------------------------------------------
def _file(path, exts, what):
    """A file of the library of the kind expected."""
    if not isinstance(path, str) or os.path.splitext(path)[1].lower() not in exts or not os.path.isfile(path):
        raise RuntimeError("The %s is not there: %s" % (what, path))
    return path


def _name(name):
    if not isinstance(name, str) or not name.strip():
        raise RuntimeError("The name is not valid.")
    return name[:200]


def _maps(maps):
    """Only known channels, each with an image that is there."""
    if not isinstance(maps, dict):
        raise RuntimeError("The maps are not valid.")
    return {k: _file(v, IMAGE_EXTS, "map") for k, v in maps.items() if k in CHANNELS and v}


def _engine(engine):
    return engine if engine in ("corona", "vray", "max") else str(rt.olib_sceneEngine())


def _mapping(mapping):
    return mapping if mapping in ("uv", "uvw", "realworld", "triplanar") else "uv"


def _tex_mapping(mapping):
    return mapping if mapping in ("triplanar", "realworld") else "uv"


def _xyz(size):
    """(width, length[, height]) in cm as three numbers; without a height, it is the length."""
    values = [float(v) for v in (size or [100.0])][:3]
    if not values or any(not 0.0 < v < 1e6 for v in values):
        raise RuntimeError("The size is not valid.")
    while len(values) < 3:
        values.append(values[-1])
    return values


def _mxs_array(items):
    out = rt.Array()
    for item in items:
        rt.append(out, item)
    return out


def _result(text):
    where, count, name = str(text).split("|", 2)
    return {"where": where, "count": int(count), "name": name}


def _put(mats, target, mapping, size):
    """Assigns, or takes to Slate. The mapping box is only added to objects, never in Slate."""
    mode = {"uvw": "box", "realworld": "realworld"}.get(mapping, "") if target != "slate" else ""
    result = _result(rt.olib_putMaterials(_mxs_array(mats), target, mode, *_xyz(size)))
    rt.redrawViews()
    return result


def check_drop(target="cursor", names=()):
    """Before a material is put: where it goes (a drop looks now at what is under the mouse, and keeps it for the
    next command) and which of those names the scene, the editor or Slate have."""
    route = _route(target)
    return {
        "route": route,
        "existing": [n for n in names if isinstance(n, str) and rt.olib_findMaterial(n) is not None],
    }


def _pbr(name, maps, engine, mapping, size, displacement, existing):
    """The material asked for: the one the scene has by that name (existing "use") or a new one."""
    name = _name(name)
    if existing == "use":
        found = rt.olib_findMaterial(name)
        if found is not None:
            return found
    maps = _maps(maps)
    if not maps:
        raise RuntimeError("The material has no maps.")
    spec = "|".join("%s=%s" % item for item in maps.items())
    x, y, _z = _xyz(size)
    return rt.olib_pbrMaterial(name, _engine(engine), spec, _tex_mapping(mapping), x, y, bool(displacement))


def apply_material(
    name,
    maps,
    engine="auto",
    mapping="uv",
    size=(100, 100),
    displacement=False,
    target="selection",
    existing="new",
    routed=False,
):
    """Makes the material and deals it out. mapping: "uv", "uvw" (a box of X, Y, Z cm), "realworld" or "triplanar".
    routed: check_drop has looked at the target already (the mouse is not looked at again)."""
    target = target if routed and target in TARGETS else _route(target)
    mapping = _mapping(mapping)
    focus_max()
    with _undo("OpenLibrary: material " + _name(name)):
        material = _pbr(name, maps, engine, mapping, size, displacement, existing)
        if target == "slate":
            added = int(rt.olib_slateShow(_mxs_array([material])))
            return {"where": "slate", "count": 1, "name": str(material.name), "added": added}
        return _put([material], target, mapping, size)


def apply_materials(items, engine="auto", displacement=False, existing="use"):
    """Several materials to Slate. items: [{"name", "maps", "mapping", "size"}]. Those the scene has by that name
    are looked for and selected (existing "use")."""
    if not isinstance(items, list) or not items:
        raise RuntimeError("There are no materials.")
    focus_max()
    with _undo("OpenLibrary: materials"):
        mats = [
            _pbr(
                item.get("name"),
                item.get("maps"),
                engine,
                _mapping(item.get("mapping")),
                item.get("size"),
                displacement,
                existing,
            )
            for item in items[:60]
            if isinstance(item, dict)
        ]
        added = int(rt.olib_slateShow(_mxs_array(mats)))
    return {"where": "slate", "count": len(mats), "name": str(mats[0].name) if len(mats) == 1 else "", "added": added}


def apply_library(mat_file, target="selection"):
    """The materials of a .mat library: to Slate, all of them; to the objects, the first."""
    mat_file = _file(mat_file, (".mat",), "library")
    target = _route(target)
    mats = list(rt.olib_matLibMaterials(mat_file))
    if not mats:
        raise RuntimeError("No material could be read from %s" % os.path.basename(mat_file))
    focus_max()
    with _undo("OpenLibrary: material " + os.path.basename(mat_file)):
        result = _put(mats[:24], target, "uv", (100,))
    result["library"] = len(mats)
    return result


def add_textures(files, roles=None, engine="auto", mapping="uv", size=(100, 100), target="slate"):
    """Loose textures: as maps in Slate, or the first one as a quick material on objects."""
    if not isinstance(files, list) or not files:
        raise RuntimeError("There are no textures.")
    files = [_file(f, IMAGE_EXTS, "texture") for f in files[:40]]
    roles = roles if isinstance(roles, list) else []
    roles = (roles + ["color"] * len(files))[: len(files)]
    mapping = _mapping(mapping)
    target = _route(target)
    if target != "slate":
        channel = "normal" if roles[0] == "normal" else "color"
        name = os.path.splitext(os.path.basename(files[0]))[0]
        return apply_material(name, {channel: files[0]}, engine, mapping, size, False, target, routed=True)
    focus_max()
    eng = _engine(engine)
    x, y, _z = _xyz(size)
    with _undo("OpenLibrary: textures"):
        nodes = [
            rt.olib_texNode(f, eng, "color" if role == "color" else "data", _tex_mapping(mapping), x, y)
            for f, role in zip(files, roles, strict=True)
        ]
        count = int(rt.olib_toSlate(_mxs_array(nodes)))
    return {"where": "slate", "count": count, "name": ""}


def apply_interior(
    name, exr, osl, preview="", size_code="S", width=350.0, height=350.0, engine="auto", depth=-1, target="cursor"
):
    """A room: a material with the OSL that reads the EXR, for the object under the mouse, the selection, Slate or a
    new plane (target "plane", or "cursor" without objects)."""
    exr = _file(exr, (".exr",), "EXR")
    osl = _file(osl, (".osl",), "OSL shader")
    preview = _file(preview, IMAGE_EXTS, "preview") if preview else ""
    if size_code not in ("S", "M", "L") or not -1 <= int(depth) <= 8:
        raise RuntimeError("The window is not valid.")
    width, height, _z = _xyz((width, height))
    target = _route(target)
    focus_max()
    with _undo("OpenLibrary: interior " + _name(name)):
        material = rt.olib_interiorMaterial(_name(name), _engine(engine), osl, exr, preview, size_code, int(depth))
        result = _result(rt.olib_putInterior(material, target, width, height))
    rt.redrawViews()
    return result


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
