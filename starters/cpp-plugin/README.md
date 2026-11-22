# OpenExample

<What OpenExample does for a 3D artist in 3ds Max, in two or three sentences.> A plug-in for 3ds Max,
written in C++ with the 3ds Max SDK. Part of the [Open3D](https://github.com/carlesllavata/Open3D)
suite of open-source tools for 3D work.

**Version 0.1.0** – what changed in each version is in [`CHANGELOG.md`](CHANGELOG.md).

## Install

OpenExample needs **3ds Max 2027** on Windows, the version it is built for.

The easy way: install the [Open3D Launcher](https://github.com/carlesllavata/Open3D/releases/latest)
and click **Install** next to OpenExample, with 3ds Max closed. It puts OpenExample in
`%APPDATA%\Autodesk\ApplicationPlugins` as a plug-in bundle, tells you when a new version is out and
updates it. To uninstall, choose **Uninstall…** in OpenExample's menu in the launcher.

By hand: each release of this repository has the bundle as a zip,
`openexample-<version>-windows-x64-3dsmax.zip`. With 3ds Max closed, unzip it, copy the
`OpenExample.bundle` folder into `%APPDATA%\Autodesk\ApplicationPlugins\` and start 3ds Max. To remove
it, delete that folder with 3ds Max closed.

## Use

<How to use it, step by step.>

In the MAXScript Listener, `OpenExample.describe()` says where OpenExample keeps its files.

The interface is in English only.

## Building

Windows 10 or 11, 64-bit, with Visual Studio 2022 or its Build Tools and the C++ workload (MSVC, CMake
and Ninja come with it):

```
powershell -ExecutionPolicy Bypass -File build.ps1           # the library and its tests
powershell -ExecutionPolicy Bypass -File build.ps1 -Plugin   # also the plug-in and its bundle
```

`-Plugin` needs the 3ds Max 2027 SDK, which Autodesk publishes for developers: its `maxsdk` folder
comes from `-MaxSdk`, or from the `ADSK_3DSMAX_SDK_2027` variable its installer sets.
The bundle is put together in `build\release\bundle\OpenExample.bundle`.

`tools\check.ps1` runs every check: it needs Python 3.11 or newer and the ruff that `ruff.toml`
names. `python -I tools/repo.py package` runs them, builds the plug-in and writes the release zip to
`dist/`.

## License

Copyright (C) 2026 <Author> and OpenExample contributors.

OpenExample is free software under the GNU General Public License, version 3 or later
(`GPL-3.0-or-later`). See [`LICENSE`](LICENSE).
