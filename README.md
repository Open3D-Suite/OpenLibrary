# OpenLibrary

OpenLibrary is a library of 3D models, materials and textures for 3ds Max, with Corona and V-Ray: browse
your own folders with previews, drag a model or a material into the scene, and catalogue new downloads in a
click. A single Windows program, nothing else to install, no account. Part of the
[Open3D](https://github.com/Open3D-Suite) suite of open-source tools for 3D work.

**Version 0.1.1** – what changed in each version is in [`CHANGELOG.md`](CHANGELOG.md).

## Install

Windows 10 or 11, 64-bit. Nothing else is needed. Two ways:

- **With the [Open3D launcher](https://github.com/Open3D-Suite/Open3D-Launcher/releases/latest)**: press *Install* on
  OpenLibrary's card. It is installed for everyone on the computer in
  `C:\Program Files\Open3D\OpenLibrary`, with a shortcut in the Start menu, and Windows asks for
  permission at each install and update. The launcher updates it and removes it.
- **By hand**, from the releases page of this repository: `openlibrary-<version>-windows-x64.msi`
  installs the same, in the same place. Or take `openlibrary-<version>-windows-x64.zip`, unzip it
  anywhere and run `OpenLibrary.exe`: nothing is installed.

The packages are not signed yet, so Windows' permission prompt says "Unknown publisher", and one
downloaded by hand may first bring "Windows protected your PC": choose *More info* and then *Run
anyway*. `SHA256SUMS.txt` on the release page lets you check the download.

To remove OpenLibrary, use the launcher, or Windows' *Settings ▸ Apps ▸ Installed apps* for the MSI;
for the zip, delete its folder. Its settings stay in `%APPDATA%\Open3D\OpenLibrary` and everything
else it wrote in `%LOCALAPPDATA%\Open3D\OpenLibrary`; delete those folders too if you want them gone.

## Use

Open *Settings*, add the folders of your library to its tabs and install the bridge in your 3ds Max (2025 or
later); restart 3ds Max. Then:

- **Models**: drag one or several cards onto 3ds Max and choose how they come in: placed with the mouse, as a Corona or
  V-Ray proxy, or in their original place. A model the scene has already can come in as an instance.
- **Materials, textures and interiors**: drag a card onto an object or onto the Slate editor.
- **Catalogue** opens a download and leaves it in the library under one name.

A library is only folders of yours: OpenLibrary keeps no database and changes nothing in them unless you ask.
OpenLibrary does not use the internet.

The version before, in Python, is
[OpenLibrary-python](https://github.com/daniellopezcastellanos/OpenLibrary-python); both can be installed.

3ds Max 2026 is the version it is written with; 2025 and 2027 should work, but are not tested. The renderers are Corona
and V-Ray.

## Privacy and security

OpenLibrary does not use the internet: no account, no downloads, nothing sent. It talks to 3ds Max on your own computer
(`127.0.0.1`), with a token that only your Windows user can read, and reads and writes your library and, if your team
uses one, its shared folder. Opening a model in 3ds Max is as safe as 3ds Max's own *Safe Scene Script Execution*: keep
it on. [docs/security.md](https://github.com/Open3D-Suite/OpenLibrary/blob/main/docs/security.md) lists what it reads and writes, and what it does not protect against.

## Language

OpenLibrary shows itself in the language of the Open3D launcher and the other Open3D apps, English or
Spanish. Until one is chosen there, it follows Windows: Spanish if Windows is in Spanish, English
otherwise.

## Build

Rust (stable) on Windows 10 or 11, 64-bit:

```
cargo build --release
```

`tools\check.ps1` runs every check: it needs Python 3.11 or newer and the ruff that `ruff.toml` names.
`python -I tools/repo.py package` runs them, builds and writes the release zip and the Windows
installer (`.msi`, made with [WiX](https://wixtoolset.org) 5) to `dist/`.

## License

Copyright (C) 2026 daniellopezcastellanos and OpenLibrary contributors.

OpenLibrary is free software under the GNU General Public License, version 3 or later
(`GPL-3.0-or-later`). See [`LICENSE`](LICENSE). The libraries it contains and their licences are
listed in `THIRD-PARTY-LICENSES.txt` inside the release zip.

The logos of Corona, V-Ray and FStorm (`assets/engine_*.png`) are trademarks of their owners. They are not under that
licence, and are used only to say which renderer a model is made for. OpenLibrary is not made or endorsed by them.
