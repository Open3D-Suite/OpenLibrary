# Development

How an Open3D app's repository is laid out, checked and changed. The same in every app, so that anyone can
work in any of them.

## The layout of a repository

At the root, the files every Open3D repository shares (see [The common files](#the-common-files)):

- `.editorconfig`, `.gitattributes`, `LICENSE`, `ruff.toml`, and for Rust `rustfmt.toml` and `clippy.toml`;
- `tools/check.ps1`, `tools/repo.py` and its parts `tools/repo_package.py`, `tools/repo_release.py` and
  `tools/repo_msi.py`: the common tool.

And the app's own:

- `README.md`: what the app does, how to install it and use it, the 3ds Max versions it was tested on, how
  to build it, its licence. Its status line holds the version.
- `CHANGELOG.md`: what changed in each version, for the people who use the app.
- `CLAUDE.md`: facts about the repository for Claude Code ([Working with Claude Code](working-with-claude-code.md)).
- `tools/repo.toml`: what the common tool needs to know about this repository (below).
- `.github/workflows/check.yml`: the CI.
- `.gitignore`, the sources, the tests and the assets (the icon and what the package needs).
- The copy of the code every app shares for its language and its folders
  ([Settings and folders](settings-and-folders.md)), beside the app's sources.

Each starter in `starters/` is this layout for its kind of app, and `starters/README.md` lists the files to
adapt in each.

## `tools\check.ps1`

One command runs every check, on any computer and in CI:

```
tools\check.ps1
```

If PowerShell refuses to run the script: `powershell -ExecutionPolicy Bypass -File tools\check.ps1`. It
needs Python 3.11 or newer and the ruff version that `ruff.toml` pins. All its checks must pass.

First the checks built into `tools/repo.py`:

- the version is the same in every place that holds it;
- every source file starts with its licence line, `SPDX-License-Identifier: GPL-3.0-or-later`
  (`.rs`, `.py`, `.ps1`, `.ms`, `.mcr`, `.c`, `.h`, `.cpp`, `.hpp`, `.js`, `.css`, `.html`);
- a Rust project's `Cargo.toml` has the common lints;
- no file holds the folder of a Windows user, or a word of the private list
  ([Security](security.md#nothing-of-anybodys-computer-in-a-published-file)).

Then the checks that `tools/repo.toml` lists: format, lints, the build and the tests.

`tools\check.ps1 -Full` adds the checks that need 3ds Max, the network or a long time. They stay out of CI
and must pass before every release.

## `tools/repo.toml`

The one file the common tool reads to know the repository. It is the app's own, not a copy. Its tables:

- `[version]`: `source`, the file and pattern where the version lives; `places`, every other file and
  pattern that repeats it (the README's status line among them); `after_set`, commands to run once a new
  version is written (`cargo update --workspace --offline`, for Rust).
- `[[check]]`, one per check, in the order they run: its `name`, its command as a list in `run`, and
  `full = true` for one that only `-Full` runs. `files` adds the tracked files that match some names to the
  command (`"*.cpp"`, for clang-format).
- `[licence_header]`: `exclude`, folders whose sources need no licence line (a third-party library).
- `[private_data]`: `allow`, what may stay although it looks like private data, each with its reason (the
  made-up user of a test).
- `[package]` and `[package.msi]`: what goes in the release package ([Distribution](distribution.md)).

The heading of `tools/repo.py` lists every key and the programs a command may name.

## CI

`.github/workflows/check.yml` runs `tools\check.ps1`, without `-Full`, on GitHub's Windows runner, on every
push and every pull request to `main`. A pull request is merged only when it passes.

- A Python plug-in runs it twice, on Python 3.11 and 3.13: the Pythons of 3ds Max 2025 and 2027.
- A Rust program adds rustfmt and clippy to the runner's Rust, and runs on Python 3.14.
- A C++ plug-in builds and tests the library that needs no 3ds Max, on Python 3.14; the plug-in itself,
  which needs the 3ds Max SDK, is built only with `-Full`.

The workflow reads only (`permissions: contents: read`), and its actions are pinned to a commit.

## Formats and lints

Every text file: UTF-8, LF line endings, 4 spaces, a final newline. JSON and YAML use 2 spaces. Only `.ps1`,
`.bat` and `.cmd` use CRLF. `.ps1`, `.ms` and `.mcr` files keep a UTF-8 byte order mark, as `.editorconfig`
says: Windows PowerShell and 3ds Max read a file without one as ANSI, and any character beyond plain ASCII
then breaks.

- **Rust:** edition 2024. `cargo fmt` with the common `rustfmt.toml` (lines of up to 150). `cargo clippy
  --workspace --all-targets --locked -- -D warnings`: every warning is an error. The common lints in
  `Cargo.toml` (`cognitive_complexity`, `too_many_arguments` and `wildcard_imports` denied, `dbg_macro` and
  `todo` warned). `unsafe` is denied, and allowed by name, with its reason beside it, only where Windows or
  the graphics card is called. `Cargo.lock` is committed and every command runs with `--locked`.
- **Python:** `ruff format` and `ruff check` with the common `ruff.toml`: lines of up to 120, Python 3.11 as
  the target, the rule families E, F, I, B, BLE and S. A block that guards a call catches only what that call
  raises; a broad `except Exception` stays only where an error must not stop the rest (what a button runs, a
  callback of 3ds Max, a loop over many items), writes the error where the person sees it (the MAXScript
  Listener, for a plug-in) and carries `# noqa: BLE001` with its reason on that line.
- **MAXScript:** 4 spaces. Every global and struct starts with the app's own prefix, so that two plug-ins
  never share a name. A script that uses a global made by another file declares it `global` before its
  block; otherwise, on the first run, the name becomes an empty local.
- **C++:** C++20 with MSVC. clang-format with the starter's `.clang-format` (Microsoft style, lines of up to
  120), checked with `--dry-run -Werror`. The compiler's warnings are errors (`/W4 /WX`). There is no C++
  lint.
- **HTML, CSS and JavaScript:** no formatter yet; it comes with the design.

A commit that only formats carries no other change.

## Tests

- **A bug that is fixed comes with a test that fails without the fix.** Check that it fails on the old code
  before you commit the fix.
- **New logic comes with its tests.**
- **The interface is covered by the real test as a user** before every release ([Testing](testing.md)).
- No coverage target.

Keep the logic apart from what needs the host, so that it can be tested without it: a Python plug-in keeps
what is tested in modules that do not import `pymxs`, `qtmax` or PySide6 when they load; a C++ plug-in keeps
its engine free of 3ds Max headers; a Rust program keeps its model apart from its window.

## English in everything in the repository

Code, comments, names, commit messages, the README, the CHANGELOG, documents and release notes are in
English. The interface is the one exception: a program's texts are in English and Spanish, through its
translation mechanism. Talking about the work can be in any language; what is written in the repository is
in English.

## Commits

- Small, one step each, so that each can be read, reviewed and undone alone.
- The message is one English sentence in the imperative: "Add the export dialog", with the area in front
  when it helps: "Export: keep the file's date". The release step writes "Release X.Y.Z".
- A change a user can notice brings its CHANGELOG entry in the same commit.

## Pull requests

Every change reaches `main` through a pull request:

1. Work on a short-lived branch.
2. Push it and open a pull request to `main`. CI runs `tools\check.ps1` on it.
3. Someone other than its author reviews it.
4. Once CI passes and the review is done, merge it into `main`.

*Why:* two people work on a new app, and a second pair of eyes before `main` catches what one misses.

## The CHANGELOG

- The newest heading is `## In development`. Each change a user can notice goes under it, in the same
  commit as the change.
- The release step turns it into `## X.Y.Z — <date>`, and that section becomes the release's notes.
- Write for the people who use the app: what they can now do, or what no longer goes wrong.

*Why:* written with the change, nothing is forgotten when the release comes.

## Versions

- `X.Y.Z`, defined in one place, the file the technology already uses: `Cargo.toml` for Rust, the
  bundle's `PackageContents.xml` (`AppVersion`) for a Python plug-in, `CMakeLists.txt`
  (`project(... VERSION X.Y.Z)`) for C++.
- `python -I tools/repo.py version set X.Y.Z` writes it into every other place (`Cargo.lock`, the README's
  status line, `__version__` in Python, the bundle's `ComponentEntry`); `tools\check.ps1` fails if they
  disagree. `python -I tools/repo.py version` prints it.
- A new app starts at 0.1.0.
- While an app is at 0.x, the middle number goes up when something stops being compatible or a feature is
  added (0.3.0 to 0.4.0), and the last one when only faults are fixed (0.4.0 to 0.4.1).

*Why:* the launcher reads the version from the release's tag and checks it inside the package; one source
and a check keep them from drifting.

## The common files

`.editorconfig`, `.gitattributes`, `LICENSE`, `ruff.toml`, `rustfmt.toml`, `clippy.toml`, `tools/check.ps1`,
`tools/repo.py`, `tools/repo_package.py`, `tools/repo_release.py`, `tools/repo_msi.py` and the shared
settings code (`open3d_settings.*`, with `open3d_settings_cases.txt` for Rust) are the same in every Open3D
repository.

- They come from Open3D's maintainers, who keep the originals. **Never edit them in the app's repository.**
- Updates arrive as a commit or a pull request from the maintainers. Take them as they come.
- In `.gitattributes`, `ruff.toml` and `clippy.toml` a repository may add lines of its own after the common
  block, below the line `# Specific to this repository.`; they are kept.
- If a common file is in the way, ask the Open3D maintainer: the change is made in the original, for every
  app at once.

*Why:* one original for every repository; a copy changed in one place drifts from the rest and is lost at
the next update.
