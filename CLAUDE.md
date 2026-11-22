# Open3D-SDK

The template a new Open3D app starts from: the guidelines in `docs/`, the common files every Open3D
repository holds, and a starter for each kind of app (a Windows program in Rust, a 3ds Max plug-in in
Python, one in C++). GitHub's "Use this template" makes a new app's repository from it.

This file holds facts about this repo only, each one checkable in the code. Update it in the same commit
as a change that makes it old.

## Layout

- `docs/`: the guidelines for a new app's author.
- `starters/`: one folder per kind of app, each the root of a new app's repository with the placeholder
  name `OpenExample`: `rust-program/`, `python-plugin/`, `cpp-plugin/`. `starters/README.md` says what
  each holds and what a new app makes its own.
- `starters/check_starter.py`: makes a new app's repository from a starter in a temporary folder and runs
  that repository's checks there.
- At the root, the common files a new app's repository keeps (`.editorconfig`, `.gitattributes`,
  `LICENSE`, `ruff.toml`, `tools/check.ps1`, `tools/repo.py` and its parts), and this repository's own
  `README.md`, `CHANGELOG.md`, this file, `.gitignore`, `tools/repo.toml` and
  `.github/workflows/check.yml`, which a starter's own replace in a new app.

## Build and check

- Nothing is built here but the starters, in temporary copies. `tools\check.ps1` runs every check, and
  all must pass. It needs Python 3.11 or newer and the ruff that `ruff.toml` names; the starters need
  Rust with rustfmt and clippy, Visual Studio with the C++ tools (MSVC, CMake, Ninja) and clang-format.
  First what `tools/repo.py` has built in (every source, in the starters too, has its licence line; no
  file holds a path of somebody's computer), then the commands that `tools/repo.toml` lists:
  - `ruff format --check .` and `ruff check .`, the starters' Python included
  - `python -I starters/check_starter.py <starter>` for each starter: the common files and the starter
    at the root of a new Git repository with nothing committed, its codes made, and that repository's
    own `tools/repo.py check`. For the Rust program: the version, the lints, format, clippy and its
    tests. For the Python plug-in: the version, ruff and its tests. For the C++ plug-in: the version,
    clang-format, and the build and tests of its library; not the plug-in, which needs the 3ds Max SDK.
- CI: `.github/workflows/check.yml` runs `tools\check.ps1`, without `-Full`, on every push and pull request
  to `main`, on GitHub's Windows runner, once with Python 3.11 and once with 3.13.
- The common files come from the common folder of Open3D's maintainers, which copies them here and into
  every Open3D repository: `.editorconfig`, `.gitattributes`, `LICENSE`, `ruff.toml`, `tools/check.ps1`,
  `tools/repo.py`, `tools/repo_package.py`, `tools/repo_release.py`, `tools/repo_msi.py`, and in the
  starters `rust-program/rustfmt.toml`, `rust-program/clippy.toml`, `rust-program/src/open3d_settings.rs`
  with `open3d_settings_cases.txt`, `python-plugin/.../openexample/open3d_settings.py`, and
  `cpp-plugin/src/open3d_settings.h` with its `.cpp`. Do not edit them here: they are overwritten.

## Version and release

- Open3D-SDK has no version of its own and makes no release package: `tools/repo.toml` has no `[version]`
  nor `[package]`. A new app is made from `main`. What changes here goes into `CHANGELOG.md`, under
  `## In development`.
- Each starter is at version 0.1.0, and its CHANGELOG has only `## In development`: a new app's first
  release turns it into `## 0.1.0`.

## Rules of this code

- A bug that is fixed comes with a test that fails without the fix, and new logic comes with its tests.
- Every starter passes its own checks as it is, once its codes are made: a new app's repository passes
  them from its first commit.
- A starter follows the Open3D app of its kind: the Rust program follows OpenRef, the Python plug-in
  OpenTools and OpenManager, the C++ plug-in OpenScatter.
- The placeholder name is `OpenExample`; `openexample` in lower case (the crate, the Python package, the
  file names) and `OPENEXAMPLE` in CMake. A code each app makes for itself is written
  `REPLACE-WITH-A-NEW-GUID` or `REPLACE_WITH_TWO_NEW_NUMBERS`, and what uses it refuses it: the packager,
  `tests/test_bundle.py`, `build.ps1 -Plugin` or the compiler. `check_starter.py` makes them as an author
  does. `starters/README.md` lists every place a new app makes its own; it changes with them.
- A starter's `CLAUDE.md` is a model: the five sections of an Open3D app's `CLAUDE.md`, with the texts the
  author writes in angle brackets, and how we work.
- No file names a computer. The guides, the starters and this repository's own files name no numbers of
  the maintainers' decisions, whose record is not public; the common copies keep the ones their
  originals have.

## Traps

- GitHub runs only the root's workflow: a starter's `.github/workflows/check.yml` runs once the starter
  is at the root of a new app.
- Each starter's `.gitignore` applies here too, inside its folder: `rust-program/.cargo/config.local.toml`
  and `cpp-plugin/build.local.ps1` hold the build folders of one computer and stay out of Git.
- `check_starter.py` copies `.cargo/config.local.toml`, so that the Rust build reuses its folder, but
  not `build.local.ps1`: CMake refuses a build folder made from another source folder.
- The copies of the common C++ files are not formatted with `.clang-format`: the C++ starter's
  `.clang-format-ignore` leaves them out.
