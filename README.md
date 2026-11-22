# Open3D-SDK

The guidelines and the starting point for a new Open3D app.

Open3D is a suite of free, open-source tools for 3D artists who use 3ds Max: a launcher that installs and
updates the apps, programs of their own (OpenView, OpenRef, OpenFarm, OpenGaussian) and 3ds Max plug-ins
(OpenTools, OpenManager, OpenScatter). Each app lives in a public repository of its own and works on its
own. They share one way of being built, checked, packaged, installed and set up, so that the suite feels
like one thing to the people who use it. The launcher and the list of apps are in
[Open3D](https://github.com/carlesllavata/Open3D).

This repository is a GitHub template. *Use this template* makes a new app's repository from it, with:

- the files every Open3D repository shares: the editor, Git, format and lint settings, the licence, and the
  common tool in `tools/` (`check.ps1`, `repo.py` and its parts), which checks, versions, packages and
  releases the app;
- the CI, which runs the same checks on GitHub on every push and pull request;
- three starters in `starters/`, one per kind of app, each a full root of a new app's repository: a program
  in Rust, a 3ds Max plug-in in Python, a 3ds Max plug-in in C++. Each one carries models of the README,
  the CHANGELOG and the `CLAUDE.md`, and the code every app shares for its language and its folders;
- these guides, in `docs/`.

A repository made from it passes `tools\check.ps1` from its first commit.

> Status: early. A new app starts from `main`; what changes here: [CHANGELOG.md](CHANGELOG.md).

## Start a new app

1. Agree the app's name with the Open3D maintainer: Open + a name, such as OpenRef.
2. Make its repository with *Use this template* and clone it.
3. Copy the starter of your kind of app to the root, then remove `starters/`, `docs/` and this README.
4. Replace the placeholder name `OpenExample` (`openexample`) and the values each app makes its own, such
   as the upgrade code of a program's installer.
5. Run `tools\check.ps1` until it passes, commit on a branch and open the first pull request.

Each step is in [docs/start-a-new-app.md](docs/start-a-new-app.md). Read the guides before you start:
they say what an app must do to enter the launcher.

## The guides

- [Start a new app](docs/start-a-new-app.md): from *Use this template* to the first pull request.
- [Principles](docs/principles.md): who the apps are for and what every app has in common.
- [Development](docs/development.md): the layout, the checks, formats, tests, commits and versions.
- [Settings and folders](docs/settings-and-folders.md): the shared language and where an app keeps its
  files.
- [Security](docs/security.md): the rules every app follows, and the review before the launcher.
- [Distribution](docs/distribution.md): the release package, the release and entering the launcher.
- [Testing](docs/testing.md): unit tests and the real test as a user, in 3ds Max too.
- [Working with Claude Code](docs/working-with-claude-code.md): the model `CLAUDE.md` and the working
  rules.

## Licence

Open3D-SDK is free software under the GNU General Public License, version 3 or later
(`GPL-3.0-or-later`). See [`LICENSE`](LICENSE). An app made from it carries the same licence; its copyright
is its author's and its contributors'.
