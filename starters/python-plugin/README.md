# OpenExample

<What OpenExample does for a 3D artist in 3ds Max, in two or three sentences.> A plug-in that lives
inside **3ds Max**, written in Python (the Python and Qt that come with 3ds Max) and MAXScript: no
separate program and no account. Part of the [Open3D](https://github.com/carlesllavata/Open3D) suite
of open-source tools for 3D work.

> Status: early (v0.1.0).
> What each version adds: [CHANGELOG.md](CHANGELOG.md).

**3ds Max:** 2025 or newer, on Windows. <The versions and renderers it has been tested with.>

## Install

The easy way: install the [Open3D Launcher](https://github.com/carlesllavata/Open3D/releases/latest)
and click **Install** next to OpenExample, with 3ds Max closed. It puts OpenExample in
`%APPDATA%\Autodesk\ApplicationPlugins` as a plug-in bundle, tells you when a new version is out and
updates it. To uninstall, choose **Uninstall…** in OpenExample's menu in the launcher.

By hand: each release of this repository has the bundle as a zip,
`openexample-<version>-windows-x64-3dsmax.zip`. With 3ds Max closed, unzip it, copy the
`OpenExample.bundle` folder into `%APPDATA%\Autodesk\ApplicationPlugins\` and start 3ds Max. To remove
it, delete that folder with 3ds Max closed.

OpenExample keeps its settings in `%APPDATA%\Open3D\OpenExample` and everything else it writes in
`%LOCALAPPDATA%\Open3D\OpenExample`; delete those folders too for a clean removal.

## Use

<How to use it, step by step.>

The interface is in English only.

## Development

- `OpenExample.bundle/` is the package 3ds Max loads; the Python code is in
  `OpenExample.bundle/Contents/python/openexample/`.
- `tools\check.ps1` runs every check: it needs Python 3.11 or newer and the ruff that `ruff.toml`
  names. The tests need no 3ds Max: `python -I tests/run.py`.
- `python -I tools/repo.py package` runs the checks and writes the release zip to `dist/`.

## License

Copyright (C) 2026 <Author> and OpenExample contributors.

OpenExample is free software under the GNU General Public License, version 3 or later
(`GPL-3.0-or-later`). See [`LICENSE`](LICENSE).
