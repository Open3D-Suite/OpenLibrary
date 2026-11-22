# OpenExample

<What OpenExample does for a 3D artist in 3ds Max, in one or two sentences.> One 3ds Max plug-in,
written in Python (Qt, pymxs) and MAXScript; nothing is compiled.

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
- **After a change** the plug-in must pass `tools\check.ps1` and load in 3ds Max.
- **Testing in 3ds Max.** A version is not ready until it has been used in 3ds Max as a person would:
  real clicks on its windows, on a new test scene, reading the MAXScript Listener afterwards. Start a
  3ds Max of your own for the test and close it yourself when you finish, without saving. Never close
  or touch a 3ds Max you did not start: if the author has one open, ask. Never open or save one of the
  author's scenes.
- **Keep this file true.** It states only what can be checked in the code and is updated in the same
  commit as the change that makes it old. No history, no dated notes.
- **Nothing of the author's computer in a published file:** no paths, user names, computer names or
  network addresses.

## Layout

- `OpenExample.bundle/`: the package 3ds Max loads. `PackageContents.xml` describes it;
  `Contents/scripts/openexample_startup.ms` runs once 3ds Max has started, puts `Contents/python` on
  `sys.path` and calls `openexample.start()`.
- `Contents/python/openexample/`: the plug-in. `__init__.py`: its name, its version and `start()`.
  `folders.py`: where it keeps its files. `open3d_settings.py`: what every Open3D app shares (the
  language and the folders), a copy of a common file; the plug-in takes only its folders from it.
- `tests/`: the tests that need no 3ds Max; `tests/run.py` runs them.

## Build and check

- No build step. `tools\check.ps1` runs every check, and all must pass; it needs Python 3.11 or newer and
  the ruff that `ruff.toml` names. Built in: the version agrees in its four places, each `.py`, `.ms` and
  `.ps1` has its licence line, no file holds a path of somebody's computer. Then, from `tools/repo.toml`:
  `ruff format --check .`, `ruff check .` and `python -I tests/run.py` (without 3ds Max or Qt).
- `tools\check.ps1 -Full` also runs the checks marked `full = true` in `tools/repo.toml`, those that need
  3ds Max (`3dsmaxbatch` or the Python of 3ds Max), the network or a long time; there are none yet. They
  must pass before each release.
- CI: `.github/workflows/check.yml` runs `tools\check.ps1`, without `-Full`, on every push and pull request
  to `main`, on GitHub's Windows runner, once with Python 3.11 and once with 3.13.
- `.editorconfig`, `.gitattributes`, `ruff.toml`, `LICENSE`, `tools/check.ps1`, `tools/repo.py`,
  `tools/repo_package.py`, `tools/repo_release.py`, `tools/repo_msi.py` and
  `openexample/open3d_settings.py` come from Open3D's common files and are the same in every Open3D repo.
  Do not change them here: a change to them is proposed to Open3D's maintainers. Only the end of
  `.gitattributes` and `ruff.toml`, after the line `# Specific to this repository.`, may be this repo's.

## Version and release

- The version is `AppVersion` in `PackageContents.xml`. The `ComponentEntry` `Version` there, `__version__`
  in `openexample/__init__.py` and the README's status line repeat it: `tools\check.ps1` fails if one
  differs, and `python -I tools/repo.py version set X.Y.Z` writes all four.
- Changes a user can notice go into `CHANGELOG.md` under `## In development`, in the same commit.
- `python -I tools/repo.py package` makes the release package as the `[package]` table of
  `tools/repo.toml` says: it needs the version's `## X.Y.Z` section in `CHANGELOG.md`, runs the quick
  checks and writes `dist/openexample-<version>-windows-x64-3dsmax.zip`, with the files Git has in
  `OpenExample.bundle/` at its root (never `__pycache__`) and `LICENSE` inside the bundle, and
  `dist/SHA256SUMS.txt`. It refuses a bundle whose `AppVersion` is not the version. Nothing is built.
- `python -I tools/repo.py release X.Y.Z` turns `## In development` into `## X.Y.Z — <date>`, sets the
  version, commits `Release X.Y.Z`, packages, tags `vX.Y.Z` and prints the pushes to approve; run again
  once they are done, it makes the GitHub release as a draft. It never pushes. `--dry-run` tries it all
  and writes only to `dist/`.
- The Open3D launcher needs `OpenExample.bundle` at the root of the zip, with the release's version in
  its `PackageContents.xml`.

## Rules of this code

- A bug that is fixed comes with a test that fails without the fix, and new logic comes with its tests.
  The interface is tried by hand in 3ds Max before a release.
- The code runs inside 3ds Max's own Python: 3.11 in 3ds Max 2025, the oldest the bundle accepts
  (`SeriesMin` 2025, `SeriesMax` 2099). `pymxs` and `qtmax` exist only inside 3ds Max; PySide6 comes with
  it.
- Only the modules that make windows import Qt and pymxs at module level; the others import them inside
  functions, so that the tests can load them. What is tested lives in modules without those imports.
- The interface is in English only: its texts are written straight in the code. Open3D's 3ds Max
  plug-ins do not follow the language the programs and the launcher share in `settings.json`.
- Its files (`folders.py`): settings in `%APPDATA%\Open3D\OpenExample`, everything else it writes in
  `%LOCALAPPDATA%\Open3D\OpenExample`; `OPEN3D_SETTINGS_DIR` and `OPEN3D_DATA_DIR` move them. Only the
  plug-in creates them, when it first writes there. `open3d_settings` gives it these folders and, for a
  plug-in that kept its files elsewhere before, `carry_over`, which copies the old folder into the new
  one once, at start; never a language.
- `PackageContents.xml`: `ProductCode` and `UpgradeCode` are made once for OpenExample and never change;
  `tests/test_bundle.py` fails while they are not GUIDs.
- MAXScript files: 4 spaces, LF, UTF-8 with a byte order mark. Their globals and structs start with
  `<PREFIX>_`, a short prefix of OpenExample's own.
- <The rules of this app's own code.>

## Traps

- In 3ds Max 2027 `pymxs.undo` swallows every exception raised in its block, Python and MAXScript alike:
  catch it inside the block, keep it, and raise it again once the block has closed, or it is lost.
- A `.ms` file that uses a global made by a file it loads with `fileIn` declares it `global` before its
  block; otherwise, on the first run in a session, the name compiles as an empty local.
- In a session without an interface (`3dsmaxbatch`, render nodes) no Qt window can be shown: what runs at
  start must work there too.
- ruff does not find the package inside the bundle, so it sorts `openexample` among `pymxs` and
  `PySide6`. To keep it apart, add `[lint.isort]` with `known-first-party = ["openexample"]` at the end of
  `ruff.toml`, after the line `# Specific to this repository.`.
- <What surprised you here and would surprise the next person.>
