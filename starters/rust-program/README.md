# OpenExample

<What OpenExample does for a 3D artist, in two or three sentences.> A single Windows program, nothing
else to install, no account. Part of the [Open3D](https://github.com/carlesllavata/Open3D) suite of
open-source tools for 3D work.

**Version 0.1.0** – what changed in each version is in [`CHANGELOG.md`](CHANGELOG.md).

## Install

Windows 10 or 11, 64-bit. Nothing else is needed. Two ways:

- **With the [Open3D launcher](https://github.com/carlesllavata/Open3D)**: press *Install* on
  OpenExample's card. It is installed for everyone on the computer in
  `C:\Program Files\Open3D\OpenExample`, with a shortcut in the Start menu, and Windows asks for
  permission at each install and update. The launcher updates it and removes it.
- **By hand**, from the releases page of this repository: `openexample-<version>-windows-x64.msi`
  installs the same, in the same place. Or take `openexample-<version>-windows-x64.zip`, unzip it
  anywhere and run `OpenExample.exe`: nothing is installed.

The packages are not signed yet, so Windows' permission prompt says "Unknown publisher", and one
downloaded by hand may first bring "Windows protected your PC": choose *More info* and then *Run
anyway*. `SHA256SUMS.txt` on the release page lets you check the download.

To remove OpenExample, use the launcher, or Windows' *Settings ▸ Apps ▸ Installed apps* for the MSI;
for the zip, delete its folder. Its settings stay in `%APPDATA%\Open3D\OpenExample` and everything
else it wrote in `%LOCALAPPDATA%\Open3D\OpenExample`; delete those folders too if you want them gone.

## Use

<How to use it, step by step.>

OpenExample shows itself in the language of the Open3D launcher and the other Open3D apps, English or
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

Copyright (C) 2026 <Author> and OpenExample contributors.

OpenExample is free software under the GNU General Public License, version 3 or later
(`GPL-3.0-or-later`). See [`LICENSE`](LICENSE). The libraries it contains and their licences are
listed in `THIRD-PARTY-LICENSES.txt` inside the release zip.
