# Distribution

How an app is packaged and released, what the launcher needs from a release, and how a new app enters the
launcher. The common tool does all of it from `tools/repo.toml`: a new app needs no packaging or release
script of its own, only its build.

## The release package

```
python -I tools/repo.py package
```

It writes everything to `dist/`, in this order, and stops at the first problem:

1. The version agrees in every place, and `CHANGELOG.md` has its `## X.Y.Z` section.
2. The quick checks of `tools\check.ps1`.
3. The build, as `build` in `[package]` says.
4. For a Rust program (`third_party = "cargo"`), the licence of every library linked in is checked to be
   compatible, and their texts go into `THIRD-PARTY-LICENSES.txt`.
5. The files `[package]` names, never `__pycache__`; `LICENSE` always goes in.
6. No file in it holds a folder of the building computer, the folder of a Windows user or a word of the
   private list ([Security](security.md#nothing-of-anybodys-computer-in-a-published-file)), and the version
   inside the package is the release's.
7. A program's MSI, when `[package.msi]` is there.
8. The zip and `SHA256SUMS.txt`, with a line for each file made. The same files always give the same zip,
   byte for byte.

### A program

- `dist/<id>-<version>-windows-x64.zip`, with `<App>.exe` at its root, and the README, `LICENSE`, the
  CHANGELOG, `THIRD-PARTY-LICENSES.txt` and `NOTICE` if the app has one.
- The exe's file version is the release's, and `<App>.exe --version` answers `<App> X.Y.Z`
  (`version_answer`). A Rust exe carries the Visual C++ runtime inside it, so nothing has to be installed
  beside it.
- `dist/<id>-<version>-windows-x64.msi`, made with WiX 5 (`wix` on PATH, or the .NET global tool `wix`
  5.0.2). It installs the same files as the zip, for everyone on the computer, into
  `C:\Program Files\Open3D\<App>`, with one shortcut in the Start menu for all users, none on the desktop,
  and the icon in Windows' *Installed apps*. Its publisher is "Open3D". A newer version replaces the one
  installed; an older one stops with a message. It has no dialog, no custom action and no service.
- Its `upgrade_code` is a GUID made once for the app and **never changed**: with another one, Windows would
  install a new version beside the old one instead of in its place.
- The MSI is checked before it is written: Windows Installer's own validation (the ICEs), its tables read
  with `msi.dll`, and its files compared with the zip's, byte for byte. The common tool never installs it.
- The MSIs are not code-signed yet, so Windows' prompt says "Unknown publisher". Say so in the README.

The `[package]` table of a program names `name`, `build`, `third_party = "cargo"`, `files` (with
`{cargo_target}` for cargo's target folder), `exe` and `version_answer`; `[package.msi]` names
`upgrade_code`, `shortcut` and `icon`.

### A 3ds Max plug-in

- `dist/<id>-<version>-windows-x64-3dsmax.zip`, with the `<App>.bundle` folder at its root and `LICENSE`
  inside the bundle.
- The bundle's `PackageContents.xml` says the release's version as `AppVersion`; the packager refuses it
  otherwise. Its `RuntimeRequirements` give `SeriesMin` (2025 at the oldest) and a `SeriesMax`: 3ds Max 2027
  ignores a bundle without one, so the other plug-ins write 2099. Its `ProductCode` and `UpgradeCode` are
  made once for the app and never changed.
- The launcher puts the bundle in `%APPDATA%\Autodesk\ApplicationPlugins`, for the user, with no
  permission prompt. By hand, it is copied there with 3ds Max closed.

The `[package]` table of a plug-in names `name`, `bundle` (`"<App>.bundle"`), `build` when something is
built (a C++ plug-in, a generated `PackageContents.xml`) and `files`.

## The release

A release reaches `main` through a pull request too, like every change. It takes two steps.

**1. Prepare it, in a pull request.** On a branch:

```
git switch -c release-X.Y.Z
python -I tools/repo.py version set X.Y.Z
```

Then, in `CHANGELOG.md`, turn `## In development` into `## X.Y.Z — <date>`, dated the way the last heading
is. Commit `Release X.Y.Z`, open the pull request, and merge it once CI passes and it is reviewed.

**2. Tag it and make the draft, on `main`.** Switch to `main`, pull, and run:

```
python -I tools/repo.py release X.Y.Z --dry-run
python -I tools/repo.py release X.Y.Z
```

`--dry-run` tries everything and writes only to `dist/`. Without it, on a clean `main` that already has the
version and its CHANGELOG section, the release step:

1. Commits nothing: it finds the version ready.
2. Makes the package from `main`'s commit.
3. Tags that commit with the annotated tag `vX.Y.Z`.
4. Prints the pushes to approve and stops; it never pushes. `main` is already on GitHub, so only the tag
   goes: `git push origin vX.Y.Z`, never `--tags`.
5. Run again once the tag is on GitHub, it packages again from the tagged commit and makes the GitHub
   release **as a draft**, titled `<App> X.Y.Z`, with that version's CHANGELOG section as its notes and the
   zip, the MSI of a program and `SHA256SUMS.txt` as its files. It needs `gh`, signed in.
6. **The Open3D maintainer publishes the draft.** Nobody else does.

*Why:* every change is reviewed before it reaches `main`, the release too; and the launcher installs a
published version on every computer at once, so nothing reaches it without the maintainer's approval.

Before every release:

- `tools\check.ps1 -Full` passes.
- The Dependabot alerts are reviewed.
- The real test as a user passed on the commit to be released ([Testing](testing.md)).

## What the launcher needs from a release

- A published release whose tag is `vX.Y.Z`.
- Files named exactly `<id>-<version>-windows-x64-3dsmax.zip` (a plug-in), or
  `<id>-<version>-windows-x64.msi` and `<id>-<version>-windows-x64.zip` (a program; a portable launcher, and
  a release without an MSI, use the zip).
- A SHA-256 for every file, from GitHub or from `SHA256SUMS.txt`. Without one the launcher installs
  nothing.
- The same version inside the package: the bundle's `AppVersion`, or the exe's file version.
- For a program, `<App>.exe` at the zip's root, or where the catalog's `exe` says for a program that needs
  the folders around it.

What the launcher then does, so that the app does not do it itself: it never changes a plug-in while 3ds
Max runs, nor a program while it runs; it puts the previous version back when an update fails; and it
removes an app on request, keeping its settings ([Settings and folders](settings-and-folders.md)).

## Entering the launcher

The launcher lists the apps in a catalog that the Open3D maintainer signs; installed launchers read it at
every start and accept it only with a valid signature. A new app enters it in these steps:

1. **Checks and CI green.** `tools\check.ps1` and `-Full` pass, and CI is green on `main`.
2. **A review by Open3D's maintainers**: the code, the security
   ([Security](security.md#the-review-before-the-launcher)) and the real test as a user
   ([Testing](testing.md)), on the package of the version to be released. Each problem found is fixed, with
   its test, before the next step.
3. **A first release**, as a draft, with its package as above. The Open3D maintainer publishes it.
4. **The catalog.** The maintainers add the app to the signed catalog. For that, give them:
   - the name, the id (the name in lower case) and the repository;
   - whether it is a program or a 3ds Max plug-in, and for a program where its exe is if not at the zip's
     root;
   - a tagline, a headline, an about text of a paragraph or two and a few features (a title and a line
     each; the existing apps have four), in English and in Spanish;
   - the platforms line: Windows, and for a plug-in the 3ds Max versions tested and those that should work
     ([Principles](principles.md#the-3ds-max-versions-to-state));
   - the icon as a PNG of 256 by 256 pixels, and its main colour.

Once the maintainer has signed and pushed the catalog, every launcher shows the app at its next start.
Each later release of the app reaches them once its draft is published.

*Why:* the people who install the apps trust the maintainer's signature on the catalog, so an app is
added only after it has been checked as the others were.
