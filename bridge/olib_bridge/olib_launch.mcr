-- SPDX-License-Identifier: GPL-3.0-or-later
macroScript OpenLibrary
    category:"Open3D"
    buttonText:"OpenLibrary"
    toolTip:"Open the OpenLibrary app"
    iconName:"OpenLibrary"
(
    on execute do
    (
        (python.import "olib_bridge").launch_app()
    )
)
