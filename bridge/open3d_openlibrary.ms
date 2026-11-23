-- SPDX-License-Identifier: GPL-3.0-or-later
-- OpenLibrary: connects this 3ds Max with the OpenLibrary app. The app copies it into scripts\startup.
-- It stays off in the 3ds Max the app starts in the background (OPENLIBRARY_BATCH).
-- It adds no menu and no window: only an optional button (Customize User Interface > Toolbars, category
-- "Open3D") that opens the app. No Python is built from text: the modules are imported and called.
if (systemTools.getEnvVariable "OPENLIBRARY_BATCH") == undefined do
(
    local root = (getDir #userScripts) + "\Open3D\OpenLibrary"
    try
    (
        local sys = python.import "sys"
        if not (sys.path.__contains__ root) do sys.path.insert 0 root
        (python.import "olib_bridge").start()
    )
    catch ( format "OpenLibrary: the bridge could not start: %\n" (getCurrentException()) )
    try ( fileIn (root + "\olib_bridge\olib_launch.mcr") ) catch ()
)
