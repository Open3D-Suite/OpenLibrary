# SPDX-License-Identifier: GPL-3.0-or-later
"""The "OpenLibrary" toolbar inside 3ds Max, with one button that opens the app.

It is made only when chosen at the bridge's install (app.json: "toolbar": true), and moves and docks like any
other toolbar of 3ds Max.
"""

import os

HERE = os.path.dirname(os.path.abspath(__file__))
OBJECT_NAME = "Open3DOpenLibraryToolbar"
_toolbar = {"widget": None}


def show():
    import qtmax
    from PySide6 import QtCore, QtGui, QtWidgets

    main = qtmax.GetQMaxMainWindow()
    if main is None:
        return False
    bar = main.findChild(QtWidgets.QToolBar, OBJECT_NAME)
    if bar is None:
        from . import launch_app

        bar = QtWidgets.QToolBar("OpenLibrary", main)
        bar.setObjectName(OBJECT_NAME)
        icon = os.path.join(HERE, "openlibrary.svg")
        action = QtGui.QAction(QtGui.QIcon(icon) if os.path.isfile(icon) else QtGui.QIcon(), "OpenLibrary", bar)
        action.setToolTip("Open OpenLibrary")
        action.triggered.connect(lambda *_: launch_app())
        bar.addAction(action)
        if hasattr(main, "addToolBar"):
            main.addToolBar(QtCore.Qt.TopToolBarArea, bar)
    bar.show()
    _toolbar["widget"] = bar
    return True


def hide():
    bar = _toolbar["widget"]
    if bar is not None:
        try:
            bar.hide()
            bar.deleteLater()
        except RuntimeError:
            pass  # 3ds Max already deleted it
        _toolbar["widget"] = None
