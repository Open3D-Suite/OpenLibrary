# Start a new app

From *Use this template* to the first passing `tools\check.ps1` and the first pull request, then how the app
enters the launcher. The commands are for PowerShell, from the root of the new repository.

## 1. Agree the name

An Open3D app is called **Open + a name**, written as one word: OpenRef, OpenFarm, OpenTools. The Open3D
maintainer approves the name before anything is made with it. Below, `Open<Name>` stands for it.

- The **name** is what the launcher shows, the app's folders (`Open3D\Open<Name>`), its exe
  (`Open<Name>.exe`) or its bundle (`Open<Name>.bundle`), and its repository.
- The **id** is the name in lower case (`open<name>`): the package names, the Rust crate, the Python
  package.

*Why:* the launcher and the folders of every app already follow it.

Agree also the kind of app, a program or a 3ds Max plug-in, and its technology
([Principles](principles.md#technologies)).

## 2. Make the repository

1. On this repository's page on GitHub, press *Use this template* and *Create a new repository*. Name it
   as the app, make it public, and include only the default branch, `main`.
2. Clone it.

What you need on the computer:

- Windows 10 or 11, 64-bit, Git, and Python 3.11 or newer for the common tool.
- ruff, at the version `ruff.toml` pins in `required-version`.
- For a Rust program: Rust (stable, MSVC) with rustfmt and clippy, and the Visual Studio Build Tools with the
  C++ workload. WiX 5 for its MSI, when you package it.
- For a C++ plug-in: the Visual Studio Build Tools with the C++ workload (with the CMake and Ninja that come
  with it), clang-format, and for the plug-in itself the 3ds Max SDK of the version it is built for (2027 in
  the starter, found through `ADSK_3DSMAX_SDK_2027`). A `build.local.ps1` beside `build.ps1`, which Git
  ignores, can name a build folder and the SDK on your computer.
- For a 3ds Max plug-in: 3ds Max, for the real test.
- `gh`, signed in, for the release draft.

## 3. Copy the starter of your kind

`starters/` holds one full root of an app's repository per kind:

| Starter | For |
|---|---|
| `starters/rust-program/` | a program of its own, in Rust |
| `starters/python-plugin/` | a 3ds Max plug-in in Python, with Qt and MAXScript |
| `starters/cpp-plugin/` | a 3ds Max plug-in in C++, with the 3ds Max SDK |

Copy the whole starter over the root of the repository, replacing the files of the same name, then remove
`starters/` and `docs/`. For a Rust program:

```
robocopy starters\rust-program . /E
git rm -r -q starters docs
```

(robocopy's exit code is 1 when it copied files: that is success.) Every starter has its own `README.md`,
`CHANGELOG.md`, `CLAUDE.md`, `.gitignore`, `.github/workflows/check.yml` and `tools/repo.toml`, which take the
place of the SDK's: the SDK's README goes that way. The common files at the root (`.editorconfig`,
`.gitattributes`, `LICENSE`, `ruff.toml` and the common tool in `tools/`) stay as they are.

The guides stay here, in Open3D-SDK; the app's repository does not carry them.

## 4. Replace the placeholder name

Every starter is an app called `OpenExample`, with the id `openexample`. Replace them with your name and
your id, keeping the case wherever they appear (`OpenExample`, `openexample`, `OPENEXAMPLE`), in the content
of every file and in the names of files and folders (`OpenExample.bundle`, the Python package
`openexample/`). Then nothing may be left:

```
git grep -n -i openexample
git ls-files | Select-String -Pattern openexample
```

Both must print nothing. For a Rust program, run `cargo update --workspace --offline` afterwards, so that
`Cargo.lock` takes the new crate name.

## 5. Make the app's own values

`starters/README.md` lists every file to adapt in each starter. Among them, the values that must be the
app's own and never copied from another app:

- **A program's upgrade code:** `upgrade_code` in the `[package.msi]` table of `tools/repo.toml`. Make a new
  GUID once (`python -c "import uuid; print(str(uuid.uuid4()).upper())"`) and **never change it** after the
  first release: with another one, Windows would install a new version beside the old one instead of in its
  place. The packager refuses to make the package until it is written.
- **A plug-in's codes:** `ProductCode` and `UpgradeCode` in the bundle's `PackageContents.xml`, each a new
  GUID in braces, made once and never changed. The Python starter has it in `OpenExample.bundle/`, the C++
  one in `bundle/OpenExample.bundle/`. Until they are written, the Python starter's tests fail and the C++
  starter's bundle is refused.
- **A C++ plug-in's ids:** `OPENEXAMPLE_CLASS_ID` and `OPENEXAMPLE_INTERFACE_ID` in `plugin/plugin.cpp`, and
  one more for each class the plug-in adds: two random 32-bit numbers each, made with the 3ds Max SDK's
  `gencid.exe`. They are stored in scenes: never reuse another plug-in's, and never change one once a
  scene may hold it. Until they are written the plug-in does not build.
- **A MAXScript prefix** for every global and struct of the plug-in, short and the app's own.
- **The texts:** every text the starters leave for you is in angle brackets, such as `<Author>` in the
  copyright lines or `<What OpenExample does for a 3D artist ...>` at the top of the README and
  `CLAUDE.md`. Write the description of a bundle's `PackageContents.xml` too, and for a plug-in the 3ds Max
  versions it is tested on ([Principles](principles.md#the-3ds-max-versions-to-state)).
- **The icon** in `assets/`, from which the launcher's copy is made later.

The version starts at 0.1.0; leave it.

## 6. The first passing check

First write your private list ([Security](security.md#nothing-of-anybodys-computer-in-a-published-file)),
so that the check knows the names of your computer. Then:

```
powershell -ExecutionPolicy Bypass -File tools\check.ps1
```

It checks the version, the licence line of every source, the common lints and that no file holds private
data, then runs the format, lints, build and tests of the starter ([Development](development.md)). Fix what
it reports until it passes.

Build the app and start it once, to see the starter run; a plug-in, in a 3ds Max you started yourself
([Testing](testing.md#loading-a-plug-in-without-installing-it)).

## 7. The first commit and the first pull request

Every change reaches `main` through a pull request, the first one too:

```
git switch -c start
git add -A
git commit -m "Start Open<Name> from the Open3D-SDK template"
git push -u origin start
```

Then open a pull request from `start` to `main`, on GitHub or with `gh pr create`. CI runs
`tools\check.ps1` on it. Once it passes, someone other than you reviews it, and it is merged into `main`.

On GitHub, in the repository's settings, also switch on **Dependabot alerts**, and leave its automatic fix
requests off ([Security](security.md#dependencies)).

## 8. Build the app

From here on, each change is a branch, small commits and a pull request
([Development](development.md#pull-requests)), with its tests and its CHANGELOG entry. Keep the starter's
`CLAUDE.md` true as the code grows ([Working with Claude Code](working-with-claude-code.md)).

## 9. Entering the launcher

When the app does what its first version should:

1. `tools\check.ps1` and `tools\check.ps1 -Full` pass, and CI is green on `main`.
2. Tell the Open3D maintainer. Open3D's maintainers review the code, its security
   ([Security](security.md#the-review-before-the-launcher)) and do the real test as a user
   ([Testing](testing.md)). What they find is fixed first.
3. Make the first release, 0.1.0: a pull request that prepares it, then `python -I tools/repo.py release
   0.1.0` on `main` ([Distribution](distribution.md#the-release)). It stays a draft until the Open3D
   maintainer publishes it.
4. Give the maintainers the texts and the icon for the launcher. They add the app to the signed catalog,
   and every launcher shows it at its next start.

Each step is in [Distribution](distribution.md#entering-the-launcher).
