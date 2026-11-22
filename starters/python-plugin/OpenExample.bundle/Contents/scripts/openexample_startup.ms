-- SPDX-License-Identifier: GPL-3.0-or-later
-- OpenExample's start in 3ds Max: puts Contents/python on sys.path and calls openexample.start().
-- 3ds Max runs it once it has started, as a "post-start-up script" of the bundle (PackageContents.xml).
(
    local here = substituteString (getFilenamePath (getThisScriptFilename())) "\\" "/"
    python.Execute ( \
        "import sys, os\n" + \
        "root = os.path.normpath(os.path.join(r'" + here + "', '..', 'python'))\n" + \
        "if root not in sys.path: sys.path.insert(0, root)\n" + \
        "import openexample\n" + \
        "openexample.start()\n" )
)
