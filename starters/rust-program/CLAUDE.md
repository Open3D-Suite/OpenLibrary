# OpenExample

<What OpenExample does for a 3D artist, in one or two sentences.> One Windows program, written in Rust
<with eframe/egui, or with tao and wry>.

This file holds facts about this repo only, each one checkable in the code, and how we work on it.
Update it in the same commit as a change that makes it old.

## How we work

- **Language.** Talk to the author in their own language. Everything written in the repo is in English:
  code, comments, commits, documents, CHANGELOG, releases.
- **The author decides.** A working or design criterion is never decided alone: ask, one question at a
  time, with 2 to 4 options, your recommendation and why. What every Open3D app shares (the common files,
  the shared settings, the checks) is decided by Open3D's maintainers, and they approve what enters the
  Open3D launcher: once the checks and the CI pass, after a review of the code, of its security and a
  real test as a user.
- **The code is the truth.** Check a claim in the code before relying on it.
- **Plan first.** Before a large change, show a plan and wait for the author's OK.
- **Commits.** Small, one step each, with one English sentence in the imperative. A format commit carries
  no code change. A change a user can notice gets its entry in `CHANGELOG.md`, under `## In development`,
  in the same commit.
- **Pull requests.** Changes reach `main` through pull requests: work on a branch, the CI passes, someone
  reviews it, then it is merged.
- **Nothing goes out without the author's explicit OK:** no push, no tag, no release. A release stays a
  draft on GitHub, and Open3D's maintainer publishes it. Tags are annotated, `vX.Y.Z`, and pushed one at
  a time, never with `--tags`.
- **Version numbers.** While the app is at 0.x, the middle number goes up when something stops being
  compatible or a feature is added, the last one when only faults are fixed.
- **Nothing is deleted without asking the author first,** each time.
- **After a change** the program must build, pass `tools\check.ps1` and start.
- **Keep this file true.** It states only what can be checked in the code and is updated in the same
  commit as the change that makes it old. No history, no dated notes.
- **Nothing of the author's computer in a published file:** no paths, user names, computer names or
  network addresses. What belongs to one computer goes in an ignored file, such as
  `.cargo/config.local.toml`.

## Layout

- `src/main.rs`: reads the arguments: `--version`, or the program itself.
- `src/lib.rs`: what the program does, where the tests reach it: its name, its version, the text it shows.
- `src/platform.rs`: every call to Windows but one.
- `src/open3d_settings.rs`: what every Open3D app shares, the language and the folders of each app, with
  the cases its tests read (`src/open3d_settings_cases.txt`).
- `build.rs`: the exe's icon (`assets/icon.ico`) and file properties.

## Build and check

- Build: `cargo build --release --locked`. `.cargo/config.toml` links the Visual C++ runtime into the exe
  (`+crt-static`), so `OpenExample.exe` needs no `VCRUNTIME140.dll`; it reads the ignored
  `.cargo/config.local.toml` when there is one, for a build folder of one computer.
- Checks: `tools\check.ps1` runs them all, and all must pass. It needs Python 3.11 or newer and the
  ruff that `ruff.toml` names. First what `tools/repo.py` has built in, then the commands that
  `tools/repo.toml` lists:
  - the version agrees in `Cargo.toml`, `Cargo.lock` and the README; every source has its licence
    line; `Cargo.toml` has the common lints; no file holds a path of somebody's computer
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo test --workspace --locked` (none needs the network or another program)
  - `ruff format --check .` and `ruff check .`, for the Python in `tools/`
- `tools\check.ps1 -Full` also runs the checks marked `full = true` in `tools/repo.toml`, those that need
  3ds Max, the network or a long time; there are none yet. They must pass before each release.
- CI: `.github/workflows/check.yml` runs `tools\check.ps1`, without `-Full`, on every push and pull request
  to `main`, on GitHub's Windows runner with Python 3.14.
- `.editorconfig`, `.gitattributes`, `rustfmt.toml`, `clippy.toml`, `ruff.toml`, `LICENSE`,
  `tools/check.ps1`, `tools/repo.py`, `tools/repo_package.py`, `tools/repo_release.py`,
  `tools/repo_msi.py`, `src/open3d_settings.rs` and `src/open3d_settings_cases.txt` come from Open3D's
  common files and are the same in every Open3D repo. Do not change them here: a change to them is
  proposed to Open3D's maintainers.

## Version and release

- The version is in `Cargo.toml`. `python -I tools/repo.py version set X.Y.Z` writes it there, in
  `Cargo.lock` and in the README's `**Version X.Y.Z**` line; `tools\check.ps1` fails if they disagree.
- Changes a user can notice go into `CHANGELOG.md` under `## In development`, in the same commit.
- `python -I tools/repo.py package` makes the release package as the `[package]` table of
  `tools/repo.toml` says: the version and its `## X.Y.Z` section in the CHANGELOG, the quick checks,
  `cargo build --release --locked` without this computer's folders in the exe, the licences of the
  dependencies in `THIRD-PARTY-LICENSES.txt`, then `dist/openexample-<version>-windows-x64.zip`
  (`OpenExample.exe`, README, LICENSE, CHANGELOG) and `dist/SHA256SUMS.txt`. It refuses an exe that holds
  a path of the computer that built it, or whose file version or `--version` is not the version.
- From the `[package.msi]` table it also makes `dist/openexample-<version>-windows-x64.msi` with WiX 5
  (`wix` on PATH or as a .NET global tool): the same files, for everyone on the computer, into
  `C:\Program Files\Open3D\OpenExample`, with the Start menu shortcut `OpenExample` and `assets/icon.ico`
  in Installed apps. It is checked before it is written and never installed.
- `python -I tools/repo.py release X.Y.Z` turns `## In development` into `## X.Y.Z — <date>`, sets the
  version, commits `Release X.Y.Z`, packages, tags `vX.Y.Z` and prints the pushes to approve; run again
  once they are done, it makes the GitHub release as a draft, with the zip, the MSI and
  `SHA256SUMS.txt`. It never pushes. `--dry-run` tries it all and writes only to `dist/`.
- The Open3D launcher needs `OpenExample.exe` at the root of the zip, with the release's version as its
  file version.

## Rules of this code

- A bug that is fixed comes with a test that fails without the fix, and new logic comes with its tests.
  The interface is tried by hand before a release.
- What can be tested lives in the library (`src/lib.rs` and its modules), not in `main.rs` nor in the
  code that draws the window.
- The language is the one every Open3D app shares, in `settings.json` (`%APPDATA%\Open3D`, or the folder
  `OPEN3D_SETTINGS_DIR` names): `open3d_settings::language` reads it, and with no preference the language
  of Windows decides. Every text of the interface exists in English and in Spanish. Only a person's
  choice writes the file (`open3d_settings::write_language`).
- Its files: settings in `open3d_settings::app_settings_dir(APP)` (`%APPDATA%\Open3D\OpenExample`),
  everything else it writes in `app_data_dir(APP)` (`%LOCALAPPDATA%\Open3D\OpenExample`);
  `OPEN3D_SETTINGS_DIR` and `OPEN3D_DATA_DIR` move them. Only the program creates them, when it first
  writes there.
- `unsafe` is denied. It is allowed by name, with its reason, only where Windows is called:
  `src/platform.rs` and `user_interface_language` in `src/open3d_settings.rs`.
- <The rules of this app's own code.>

## Traps

- The `upgrade_code` of `[package.msi]` never changes once a release has it: with another one, Windows
  would install a new version beside the old one instead of in its place.
- When the package is renamed in `Cargo.toml`, `cargo update --workspace --offline` brings `Cargo.lock`
  in line; until then `--locked` refuses to build.
- <What surprised you here and would surprise the next person.>
