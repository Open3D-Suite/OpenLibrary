# Starters

Each folder here is the root of a new Open3D app's repository, of one kind, with the placeholder name
`OpenExample`. A new app copies the whole content of one of them over the root of its repository: its
`README.md`, `CHANGELOG.md`, `CLAUDE.md`, `.gitignore`, `tools/repo.toml` and
`.github/workflows/check.yml` take the place of the SDK's own. The common files at the SDK's root
(`.editorconfig`, `.gitattributes`, `LICENSE`, `ruff.toml`, `tools/check.ps1` and `tools/repo*.py`)
stay as they are. Then the `starters/` folder goes. `check_starter.py` does the same in a temporary
folder and runs the checks of the result: `tools\check.ps1` runs it for each starter.

- **`rust-program/`**: a Windows program in Rust, like OpenRef and OpenView. It shows in a message box,
  in the shared language, where it keeps its files, and answers `--version`. The exe is stamped with its
  icon and its version by `build.rs`, the Visual C++ runtime is linked in, and the package is a zip and a
  per-machine MSI.
- **`python-plugin/`**: a 3ds Max plug-in in Python and MAXScript, like OpenTools and OpenManager. Its
  bundle loads in 3ds Max 2025 and newer, and at start it writes one line in the MAXScript Listener
  saying where it keeps its files. Its tests need no 3ds Max and run on Python 3.11 and 3.13.
- **`cpp-plugin/`**: a 3ds Max plug-in in C++ with the 3ds Max 2027 SDK, like OpenScatter: a library with
  no 3ds Max header and its tests, which the CI builds and runs, and a global utility plug-in,
  `OpenExample.gup`, which gives MAXScript `OpenExample.describe()`, the folders where it keeps its
  files. The plug-in needs the 3ds Max SDK to build.

The program follows the language the launcher and the programs share; the plug-ins are in English only
and take only their folders from the shared settings.

## What each app makes its own

In every starter:

- **The name.** Every `OpenExample` becomes the app's name (Open + a name, approved by Open3D's
  maintainers), every `openexample` its lower-case id, and in the C++ plug-in every `OPENEXAMPLE` its
  upper-case id: in the files and in the names of files and folders. `git grep -n -i openexample` lists
  them all.
- **The texts in angle brackets**, such as `<What OpenExample does for a 3D artist, ...>`, and
  `<Author>` in the copyright lines.
- **The codes.** Each `REPLACE-WITH-A-NEW-GUID` becomes a GUID made for the app (two places never share
  one), and each `REPLACE_WITH_TWO_NEW_NUMBERS` two random 32-bit numbers. They are made once and never
  change. What uses them refuses the placeholder, as said below.

### `rust-program/`

| File | What to make the app's own |
|---|---|
| `Cargo.toml` | `name = "openexample"`, the `[lib]` name, the `[[bin]]` name `OpenExample` (the exe's name), `description` |
| `Cargo.lock` | the `openexample` entry: `cargo update --workspace --offline` after the rename |
| `build.rs` | `ProductName`, `FileDescription`, `<Author>` in `LegalCopyright` |
| `src/lib.rs` | `APP = "OpenExample"`, the first line of its documentation, the names in its tests |
| `src/main.rs` | the crate's name (`openexample::`) and the program's name in its message |
| `tools/repo.toml` | the `openexample` of the `Cargo.lock` pattern; `name`, the exe in `files`, `exe` and `version_answer` of `[package]`; `upgrade_code` and `shortcut` of `[package.msi]`. `upgrade_code = "REPLACE-WITH-A-NEW-GUID"`: the packager refuses to make the package until it is a GUID |
| `assets/icon.ico` | the app's icon (16, 32, 48 and 256 pixels); build.rs puts it in the exe and the MSI shows it |
| `.cargo/config.toml` | the exe's name in a comment |
| `README.md`, `CHANGELOG.md`, `CLAUDE.md` | the name, the texts in angle brackets, `<Author>` |

### `python-plugin/`

| File | What to make the app's own |
|---|---|
| `OpenExample.bundle/` | the folder's name |
| `OpenExample.bundle/PackageContents.xml` | `Name`, `Description`, `CompanyDetails`, `AppName`, `ModuleName`; `ProductCode` and `UpgradeCode`, each `{REPLACE-WITH-A-NEW-GUID}` and each a different GUID in braces: `tests/test_bundle.py` fails until they are |
| `Contents/python/openexample/` | the package's name |
| `Contents/python/openexample/__init__.py` | `APP = "OpenExample"`, the first line of its documentation |
| `Contents/python/openexample/folders.py` | the name in its documentation |
| `Contents/scripts/openexample_startup.ms` | the file's name, and the package it imports |
| `tests/run.py`, `tests/test_bundle.py`, `tests/test_folders.py` | the bundle's folder, the package and the names they expect |
| `tools/repo.toml` | the paths of the four version places, and `name`, `bundle` and `files` of `[package]` |
| `README.md`, `CHANGELOG.md`, `CLAUDE.md` | the name, the texts in angle brackets, `<Author>` |

### `cpp-plugin/`

| File | What to make the app's own |
|---|---|
| `CMakeLists.txt` | `project(OpenExample ...)`, the `OPENEXAMPLE_*` options, the targets `openexample_core` and `openexample_tests`, the description in its heading |
| `plugin/plugin.cpp` | `OPENEXAMPLE_CLASS_ID` and `OPENEXAMPLE_INTERFACE_ID`, each `REPLACE_WITH_TWO_NEW_NUMBERS`: two random 32-bit numbers each (the SDK's `gencid.exe` makes them); the plug-in does not compile until they are. The class and interface names |
| `plugin/CMakeLists.txt` | the target `OpenExample`, `MODULE_NAME`, the `OPENEXAMPLE_*` names |
| `plugin/OpenExample.def` | the file's name and its `LIBRARY` line |
| `plugin/version.rc.in` | the names, `<Author>` in `LegalCopyright` |
| `src/folders.h`, `src/folders.cpp` | `kApp = L"OpenExample"`, the namespace `openexample` |
| `tests/test_folders.cpp` | the namespace and the names it expects |
| `bundle/OpenExample.bundle/` | the folder's name |
| `bundle/OpenExample.bundle/PackageContents.xml` | `Name`, `Description`, `CompanyDetails`, `AppName`, `ModuleName`; `ProductCode` and `UpgradeCode`, each `{REPLACE-WITH-A-NEW-GUID}` and each a different GUID in braces: `build.ps1 -Plugin` refuses to put the bundle together until they are |
| `build.ps1` | the project, bundle, plug-in and test names |
| `tools/repo.toml` | the project in the version pattern, the bundle's path, and `name`, `bundle`, `build` and `files` of `[package]` |
| `README.md`, `CHANGELOG.md`, `CLAUDE.md` | the name, the texts in angle brackets, `<Author>` |

A GUID can be made with `python -c "import uuid; print(str(uuid.uuid4()).upper())"` or, in PowerShell,
`[guid]::NewGuid()`; two random 32-bit numbers with
`python -c "import secrets; print(hex(secrets.randbits(32)), hex(secrets.randbits(32)))"`.
