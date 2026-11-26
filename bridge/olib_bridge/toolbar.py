# SPDX-License-Identifier: GPL-3.0-or-later
"""The button that opens OpenLibrary, on a toolbar of 3ds Max.

3ds Max has no call to add a button to a toolbar, so, as OpenManager does, the interface is saved, the button is
written into its .cuix file (XML, UTF-8 with a byte order mark) and the file is loaded again. It goes beside the
button of another Open3D app when a toolbar has one (OpenManager's, OpenTools'), or else on a toolbar of its own,
"OpenLibrary". It is put once: a person who then moves it or takes it away is not overruled. The file is backed up
first in OpenLibrary's data folder and put back if anything fails.

The text functions need neither 3ds Max nor Qt; place() and remove() use pymxs, imported inside them.
"""

import os
import shutil

HERE = os.path.dirname(os.path.abspath(__file__))
NAME = "OpenLibrary"
WINDOW = "Open3D OpenLibrary"
ACTION = 'actionID="OpenLibrary`Open3D"'
ITEM = (
    '<Item typeID="2" type="CTB_MACROBUTTON" width="0" height="0" controlID="0" macroTypeID="3" '
    'macroType="MB_TYPE_ACTION" actionTableID="647394" imageID="-1" imageName="" ' + ACTION + ' tip="OpenLibrary" '
    'label="OpenLibrary" />'
)
# The buttons of the other Open3D apps: the first one found has OpenLibrary's beside it.
NEIGHBOURS = ('actionID="OpenManager`OpenManager"', 'actionID="OpenTools_Hub`OpenTools"', '`OpenTools"')
DONE = "toolbar_placed.txt"
BACKUP = "interface_backup"
SIZES = (16, 24, 32, 48)


def has_button(text):
    return ACTION in text


def with_button(text):
    """The interface with the button: after the first button of another Open3D app, or on a toolbar of its own at the
    end of the list of windows. None when it has the button already, or no list of windows."""
    if has_button(text):
        return None
    nl = "\r\n" if "\r\n" in text else "\n"
    lines = text.splitlines(keepends=True)
    for neighbour in NEIGHBOURS:
        for i, line in enumerate(lines):
            if neighbour in line and "<Item " in line:
                indent = line[: len(line) - len(line.lstrip())]
                lines.insert(i + 1, indent + ITEM + nl)
                return "".join(lines)
    at = text.find("</CUIWindows>")
    if at < 0:
        return None
    start = at
    while start > 0 and text[start - 1] == " ":
        start -= 1
    block = (
        f'    <Window objectName="{WINDOW}" name="{NAME}" type="T" cType="1" toolbarRows="1">{nl}'
        f"            <Items>{nl}"
        f"                {ITEM}{nl}"
        f"            </Items>{nl}"
        f"        </Window>{nl}"
    )
    return text[:start] + "    " + block + text[start:]


def without_button(text):
    """The interface without the button, and without the toolbar of its own if it is left empty."""
    lines = [line for line in text.splitlines(keepends=True) if ACTION not in line]
    text = "".join(lines)
    s = text.find(f'<Window objectName="{WINDOW}"')
    if s >= 0:
        e = text.find("</Window>", s)
        if e >= 0 and "<Item " not in text[s:e]:
            stop = e + len("</Window>")
            while s > 0 and text[s - 1] == " ":
                s -= 1
            while stop < len(text) and text[stop] in "\r\n":
                stop += 1
            text = text[:s] + text[stop:]
    return text


def data_folder(environ=None):
    """OpenLibrary's data folder, as the app has it: OPEN3D_DATA_DIR, or %LOCALAPPDATA%\\Open3D."""
    environ = os.environ if environ is None else environ
    base = environ.get("OPEN3D_DATA_DIR") or (
        os.path.join(environ["LOCALAPPDATA"], "Open3D") if environ.get("LOCALAPPDATA") else ""
    )
    return os.path.join(base, NAME) if base else None


def copy_icons(user_icons):
    """The icon of the button, in the folders of the dark and the light theme of 3ds Max."""
    for theme in ("Dark", "Light"):
        target = os.path.join(user_icons, theme)
        os.makedirs(target, exist_ok=True)
        for size in SIZES:
            name = "%s_%d.png" % (NAME, size)
            shutil.copyfile(os.path.join(HERE, "icons", theme, name), os.path.join(target, name))


def _change(edit, environ):
    """Saves the interface, changes it with `edit` and loads it again; a line for the Listener, or None."""
    from pymxs import runtime as rt

    if rt.GetQuietMode() or rt.IsNetServer():
        return None  # no interface: 3dsmaxbatch, a render node
    folder = data_folder(environ)
    if folder is None:
        return "no data folder for the backup of the interface"
    rt.cui.saveConfig()
    path = str(rt.cui.getConfigFile() or "")
    if not path or not os.path.isfile(path):
        return "3ds Max did not say which interface file it uses"
    with open(path, encoding="utf-8-sig", newline="") as f:
        text = f.read()
    new = edit(text)
    if new is None or new == text:
        return None
    backup = os.path.join(folder, BACKUP)
    os.makedirs(backup, exist_ok=True)
    backup = os.path.join(backup, os.path.basename(path))
    shutil.copy2(path, backup)
    try:
        with open(path, "w", encoding="utf-8-sig", newline="") as f:
            f.write(new)
        rt.cui.loadConfig(path)
        if f'<Window objectName="{WINDOW}"' in new:
            rt.cui.showToolbar(WINDOW)
    except Exception as error:  # noqa: BLE001 - any failure puts the interface back; it is said in the Listener
        shutil.copy2(backup, path)
        rt.cui.loadConfig(path)
        return "the interface could not be changed (%s), and was put back" % error
    return "the interface was changed (its backup: %s)" % backup


def place(environ=None):
    """Once: puts the button, or notes that the interface has it already."""
    folder = data_folder(environ)
    if folder is None or os.path.isfile(os.path.join(folder, DONE)):
        return None
    from pymxs import runtime as rt

    try:
        copy_icons(str(rt.getDir(rt.Name("userIcons"))))
        rt.colorman.reInitIcons()
    except Exception as error:  # noqa: BLE001 - the button shows its name without its icon
        print("OpenLibrary: the icon of the button could not be copied: %s" % error)
    said = _change(with_button, environ)
    if said is None or said.startswith("the interface was changed"):
        os.makedirs(folder, exist_ok=True)
        with open(os.path.join(folder, DONE), "w", encoding="utf-8") as f:
            f.write("1\n")
    return said


def remove(environ=None):
    """Takes the button away, when it was put and the person chose not to have it."""
    folder = data_folder(environ)
    if folder is None or not os.path.isfile(os.path.join(folder, DONE)):
        return None
    said = _change(without_button, environ)
    if said is None or said.startswith("the interface was changed"):
        os.remove(os.path.join(folder, DONE))
    return said
