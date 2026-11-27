# OpenLibrary

A library of 3D models, materials and textures for 3ds Max (Corona and V-Ray), with a small bridge inside 3ds Max. One Windows program, written in Rust
with eframe/egui.

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
- `src/lib.rs`: the name, the version and `run`, which opens the window.
- `src/app.rs`: the window (eframe/egui): what it holds, and what its threads send. **The window's thread never asks the
  disk about the library** (it may be on a slow server): cards use `peek` of the caches, and `Job` reads on threads.
  Each tab is kept as it was (`kept_models`, `kept_surfaces`).
- `src/app/frame.rs` (header, status bar), `src/app/gallery.rs` (title, filters, cards, their menu),
  `src/app/inspector.rs` (the panel of the selected model), `src/app/merge.rs` (the menu after a drop, the questions,
  models one after another).
- `src/badges.rs`: what is drawn over the images: renderers, warning, maps, shape, the material ball.
- `src/kit.rs`: the look (colours of both modes, fonts of Windows, icons) and the widgets: `button`, `chip`, `tab`...
- `src/app/work.rs`: the window's batches (previews, model data) and what opens the two windows below.
- `src/app/live.rs`: what is asked of an open 3ds Max: the preview of its selection, lost paths of the scene or of
  `.max` files.
- `src/ui_setup.rs`: the welcome of the first time (`olib_welcome_done`): workspace, bridge with its button, shortcuts.
  `src/shortcuts.rs`: the shortcuts, made by Windows' PowerShell with the paths in environment variables.
- `src/ui_settings.rs`: the settings window (a draft until it is accepted, but the bridge). `src/quicklook.rs`: the
  image of a card, large.
- `src/ui_catalog.rs`, `src/ui_missing.rs`: the windows to catalogue a download and for the missing files.
- `src/max_session.rs`: the window's side of 3ds Max: `MaxSession` (discovery every 3 s, the bridge prompt,
  `install`, `request` on a thread, the 3ds Max in front) and `prepare`, which builds a merge (it reads the disk).
- `src/thumbs.rs`: `ThumbLoader`, thumbnails on threads, last asked first, cached as JPEG in `thumbs` of the
  data folder (`cache_file`); a `.max` without preview gives its viewport capture.
- `src/platform.rs`: every call to Windows but one.
- `src/maxfile.rs`: reads a `.max` without 3ds Max (compound file): its renderer (`detect_engine`), the viewport
  capture 3ds Max saved in it (`embedded_thumbnail`) and its external files (`read_assets`); `EngineCache`
  keeps each file's renderer by date and size in `engines-2.json` of the data folder (scenes saved with "Compress on Save" have gzip streams: `unpacked`).
- `src/library.rs`: the index of a library (`LibraryIndex`): assets by engine suffix (`split_engine_suffix`), their
  previews, a cache per set of roots (`cache_name`) and a scan that lists again only the folders whose date changed;
  `hidden_folders` for the folders not shown.
- `src/settings.rs`: OpenLibrary's `settings.json`, kept as JSON values over `defaults()` so unknown keys survive;
  `save` merges with what another copy saved and keeps a `.bak`; `carry_over_old_settings` copies the Python
  version's file once.
- `src/texts.rs`: every text of the interface as a `Text`, English and Spanish side by side (`Text::get`), with
  `{name}` places filled by `fill`; its test fails on a text without Spanish or with other places.
- `src/bridge_install.rs`: the bridge's files travel in the exe (`FILES`, `STARTUP`) and go to each 3ds Max
  of the user (`find_targets`): `ENU\scripts\startup\open3d_openlibrary.ms` and `ENU\scripts\Open3D\OpenLibrary`
  with `app.json` (exe, toolbar, `token_dpapi`). `outdated` compares every file; `pending` tells what is
  missing or old.
- `src/maxlink.rs`: the client: `bridge_token` (made once, kept encrypted with DPAPI in the data folder), `call`
  and `discover` on 127.0.0.1:47820-47829, and `choose_file`, `relink_dirs`, `proxy_dir` for a merge.
- `bridge/`: what runs inside 3ds Max (Python and MAXScript, English only): `olib_bridge/server.py` (refuses a
  request without the token and closes on a line that is not a JSON object; only `COMMANDS`), `ops.py`,
  `toolbar.py` (the button, written into the `.cuix` of 3ds Max beside OpenManager's, with a backup, and put back at each start while it is wanted: a 3ds Max that closes saves its interface, and one opened before the button was put saves it without it), `olib_core.ms` and `olib_materials.ms` (every global starts with `olib_`), `config.py` (decrypts the token). It lives beside the
  Python version's bridge (`scripts\OpenLibrary`, ports 47800-47809, prefix `olb_`) without touching it.
  `bridge/tests` run without 3ds Max, with stand-ins for pymxs and Qt.
- `src/maxbatch.rs`: jobs of a 3ds Max in the background (`3dsmaxbatch.exe`): `Job` is a function of
  `bridge/olib_batch.ms` and a line per file; `run` reads its log, gives up a file that stalls and goes on with the
  rest. It sets `OPENLIBRARY_BATCH`, with which an installed bridge does not start. `olib_batch.ms` travels in the
  exe and is never installed in 3ds Max.
- `src/modelstats.rs`: faces, vertices, displacement and animation of a model, kept in `model_stats.json`.
- `src/missing.rs`: the external files a `.max` does not find (`externals`), and the job that changes or removes
  their paths without opening it. `CheckCache` keeps what was found in `external_files.json`.
- `src/archives.rs`: opens a download with the 7-Zip, NanaZip or UnRAR installed, or the `tar` of Windows; its parts
  (`volumes`). `src/catalog.rs`: the proposed share (`guess`: the `.max`, the maps of `MAP_EXTS`, nothing else), the names and the copy (`Plan`).
- `src/surfaces.rs`: materials and textures: `parse_name` (channel, resolution), `real_size`, `build_materials`,
  `build_textures`, and `SurfaceIndex` with its cache (`surf_*.json`), which also keeps the pixels of the colour maps.
- `src/interiors.rs`: the wParallax and ShadersBOX rooms (`build_interiors`).
- `src/surface_prefs.rs`: the size and the mapping remembered of each material (keys relative to the root folder,
  `stored_key`), the options of those tabs, and the arguments of the bridge's commands.
- `src/ui_surfaces.rs`: `SurfacePanel`, the tabs of materials, textures and interiors; a drop asks `check_drop` first.
- `src/foldertree.rs`: the tree of folders: `SideTree` (the tabs, with favourites, counts, icons and its menu) and
  `show` (the catalogue).
- `src/settings/workspace.rs`: the team's workspace: the `SHARED` keys go to `workspace.json` too, mixed on saving
  with what others wrote (`ENTRIES`, entry by entry); a file that cannot be read is never written over. The format
  is the Python version's, and both may use one folder. The window looks at the file's date on a thread.
- `src/favorites.rs`: the favourites of each person (`rel_key`) and what a tab shows of them (`tab_view`).
- `src/foldericons.rs`: the icon of a folder by its name (`guess`) and those the person gives (`folder_marks`).
  The drawings are SVG paths drawn as lines: the fonts of egui lack most signs, and a test in `texts.rs` fails on
  a text with a sign they lack.
- `src/task.rs`: `Task`, work on a thread of its own; dropping it cancels it and waits.
- `src/open3d_settings.rs`: what every Open3D app shares, the language and the folders of each app, with
  the cases its tests read (`src/open3d_settings_cases.txt`).
- `docs/security.md`: what the app reads and writes and what it checks; a change to any of it updates that page.
  `assets/icon-256.png` is the icon of the window and for the launcher's catalog. `assets/engine_*.png` are the logos of
  the renderers, trademarks of their owners (README, *License*).
- `build.rs`: the exe's icon (`assets/icon.ico`) and file properties.

## Build and check

- Build: `cargo build --release --locked`. `.cargo/config.toml` links the Visual C++ runtime into the exe
  (`+crt-static`), so `OpenLibrary.exe` needs no `VCRUNTIME140.dll`; it reads the ignored
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
  dependencies in `THIRD-PARTY-LICENSES.txt`, then `dist/openlibrary-<version>-windows-x64.zip`
  (`OpenLibrary.exe`, README, LICENSE, CHANGELOG) and `dist/SHA256SUMS.txt`. It refuses an exe that holds
  a path of the computer that built it, or whose file version or `--version` is not the version.
- From the `[package.msi]` table it also makes `dist/openlibrary-<version>-windows-x64.msi` with WiX 5
  (`wix` on PATH or as a .NET global tool): the same files, for everyone on the computer, into
  `C:\Program Files\Open3D\OpenLibrary`, with the Start menu shortcut `OpenLibrary` and `assets/icon.ico`
  in Installed apps. It is checked before it is written and never installed.
- `python -I tools/repo.py release X.Y.Z` turns `## In development` into `## X.Y.Z — <date>`, sets the
  version, commits `Release X.Y.Z`, packages, tags `vX.Y.Z` and prints the pushes to approve; run again
  once they are done, it makes the GitHub release as a draft, with the zip, the MSI and
  `SHA256SUMS.txt`. It never pushes. `--dry-run` tries it all and writes only to `dist/`.
- The Open3D launcher needs `OpenLibrary.exe` at the root of the zip, with the release's version as its
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
- Its files: settings in `open3d_settings::app_settings_dir(APP)` (`%APPDATA%\Open3D\OpenLibrary`),
  everything else it writes in `app_data_dir(APP)` (`%LOCALAPPDATA%\Open3D\OpenLibrary`);
  `OPEN3D_SETTINGS_DIR` and `OPEN3D_DATA_DIR` move them. Only the program creates them, when it first
  writes there.
- `unsafe` is denied. It is allowed by name, with its reason, only where Windows is called:
  `src/platform.rs` and `user_interface_language` in `src/open3d_settings.rs`.

## Traps

- The `upgrade_code` of `[package.msi]` never changes once a release has it: with another one, Windows
  would install a new version beside the old one instead of in its place.
- When the package is renamed in `Cargo.toml`, `cargo update --workspace --offline` brings `Cargo.lock`
  in line; until then `--locked` refuses to build.
