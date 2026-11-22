# OpenExample

<What OpenExample does for a 3D artist in 3ds Max, in one or two sentences.> One 3ds Max plug-in, written
in C++ with the 3ds Max SDK: a library with no 3ds Max header, tested on its own, and the plug-in around
it.

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
- **After a change** the plug-in must build (`build.ps1 -Plugin`), pass `tools\check.ps1` and load in
  3ds Max.
- **Testing in 3ds Max.** A version is not ready until it has been used in 3ds Max as a person would:
  real clicks on its panels, on a new test scene, reading the MAXScript Listener afterwards. Start a
  3ds Max of your own for the test and close it yourself when you finish, without saving. Never close
  or touch a 3ds Max you did not start: if the author has one open, ask. Never open or save one of the
  author's scenes.
- **Keep this file true.** It states only what can be checked in the code and is updated in the same
  commit as the change that makes it old. No history, no dated notes.
- **Nothing of the author's computer in a published file:** no paths, user names, computer names or
  network addresses. What belongs to one computer goes in the ignored `build.local.ps1`.

## Layout

- `src/`: the library `openexample_core`, plain C++20 with no 3ds Max header. `folders.h` and
  `folders.cpp`: where the plug-in keeps its files. `open3d_settings.h` and `open3d_settings.cpp`: what
  every Open3D app shares (the language and the folders), a copy of a common file; the plug-in takes only
  its folders from it.
- `plugin/`: the plug-in, `OpenExample.gup`, with the 3ds Max SDK. `plugin.cpp`: the entry points, the
  global utility plug-in 3ds Max starts with itself, and the MAXScript interface `OpenExample`
  (`OpenExample.describe()`). `version.rc.in`: its file properties.
- `tests/`: the tests of the library, `openexample_tests`, without a framework.
- `bundle/OpenExample.bundle/PackageContents.xml`: what 3ds Max reads to load the bundle.

## Build and check

- Build: `powershell -ExecutionPolicy Bypass -File build.ps1` (MSVC, CMake, Ninja; warnings are errors):
  the library and its tests, which it runs. `-Config Debug`, `-BuildDir <dir>`, `-NoTests`, `-Clean`.
- `-Plugin` also builds `OpenExample.gup` and puts `OpenExample.bundle` together in `-Bundle`, by default
  `<build>\bundle\OpenExample.bundle`. It needs the 3ds Max 2027 SDK and refuses `-Config Debug`. Nothing
  is installed.
- The build goes to `build\<config>`, or under `$BuildRoot` when the ignored local file `build.local.ps1`
  sets it. The SDK comes from `-MaxSdk`, else `$MaxSdk` in that file, else `ADSK_3DSMAX_SDK_2027`.
- Checks: `tools\check.ps1` runs the quick ones, `-Full` all; all must pass. It needs Python 3.11 or newer
  and the ruff `ruff.toml` names. Its own steps (the version agrees, every source has its licence line, no
  file holds a path of somebody's computer), then those of `tools/repo.toml`:
  - `clang-format --dry-run -Werror` on the `.cpp` and `.h` files; `.clang-format-ignore` leaves out the
    copies of the common files. There is no C++ lint.
  - `ruff format --check .` and `ruff check .`, for the Python in `tools/`
  - `build.ps1`, which runs `openexample_tests`
  - only with `-Full`: `build.ps1 -Plugin`, which needs the 3ds Max 2027 SDK. The checks marked
    `full = true` must pass before each release.
- CI: `.github/workflows/check.yml` runs `tools\check.ps1`, without `-Full`, on every push and pull request
  to `main`, on GitHub's Windows runner with Python 3.14: the library and its tests only, with the
  runner's clang-format and Visual Studio, since the runner has no 3ds Max SDK.
- `.editorconfig`, `.gitattributes`, `ruff.toml`, `LICENSE`, `tools/check.ps1`, `tools/repo.py`,
  `tools/repo_package.py`, `tools/repo_release.py`, `tools/repo_msi.py`, `src/open3d_settings.h` and
  `src/open3d_settings.cpp` come from Open3D's common files and are the same in every Open3D repo. Do not
  change them here: a change to them is proposed to Open3D's maintainers.

## Version and release

- The version is `project(OpenExample VERSION X.Y.Z)` in `CMakeLists.txt`, which stamps the plug-in;
  `tools/repo.toml` lists its copies (`PackageContents.xml` twice and the README's `**Version X.Y.Z**`
  line), and `python -I tools/repo.py version set X.Y.Z` writes them all. `build.ps1 -Plugin` also refuses
  a mismatch with `PackageContents.xml`.
- Changes a user can notice go into `CHANGELOG.md` under `## In development`, in the same commit.
- `python -I tools/repo.py package` makes the release package as the `[package]` table of
  `tools/repo.toml` says: it needs the version's `## X.Y.Z` section in `CHANGELOG.md`, runs the quick
  checks, then `build.ps1 -Plugin -Bundle dist/OpenExample.bundle`, and writes
  `dist/openexample-<version>-windows-x64-3dsmax.zip`, with that folder at its root and `LICENSE` inside
  it, and `dist/SHA256SUMS.txt`. It refuses a file that names this computer and a bundle whose
  `AppVersion` is not the version.
- `python -I tools/repo.py release X.Y.Z` turns `## In development` into `## X.Y.Z — <date>`, sets the
  version, commits `Release X.Y.Z`, packages, tags `vX.Y.Z` and prints the pushes to approve; run again
  once they are done, it makes the GitHub release as a draft. It never pushes. `--dry-run` tries it all
  and writes only to `dist/`.
- The Open3D launcher needs `OpenExample.bundle` at the root of the zip, with the release's version in
  its `PackageContents.xml`.

## Rules of this code

- A bug that is fixed comes with a test that fails without the fix, and new logic comes with its tests.
  The plug-in is tried by hand in 3ds Max before a release.
- What can be tested lives in `src/`, with no 3ds Max header; `plugin/` only connects it to 3ds Max.
- The interface is in English only: its texts are written straight in the code. Open3D's 3ds Max
  plug-ins do not follow the language the programs and the launcher share in `settings.json`.
- Its files: settings in `open3d_settings::app_settings_dir(kApp)` (`%APPDATA%\Open3D\OpenExample`),
  everything else it writes in `app_data_dir(kApp)` (`%LOCALAPPDATA%\Open3D\OpenExample`);
  `OPEN3D_SETTINGS_DIR` and `OPEN3D_DATA_DIR` move them. Only the plug-in creates them, when it first
  writes there. `open3d_settings` gives it these folders and, for a plug-in that kept its files
  elsewhere before, `carry_over`, which copies the old folder into the new one once, at start; never a
  language.
- The plug-in's `Class_ID` and `Interface_ID` (top of `plugin/plugin.cpp`) and the bundle's `ProductCode`
  and `UpgradeCode` are made once for OpenExample and never change: scenes and 3ds Max know the plug-in
  by them.
- `.clang-format`: Microsoft style, 120 columns, include order left alone, `Type* name` and `{ a, b }`
  kept.
- <The rules of this app's own code.>

## Traps

- A plug-in built with the SDK of one 3ds Max loads only in that version: `PackageContents.xml` names
  2027 alone, and a new version of 3ds Max needs its SDK and a component of its own.
- 3ds Max only loads plug-ins built against the release C runtime: `-Plugin` refuses a debug build.
- <What surprised you here and would surprise the next person.>
