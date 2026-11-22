# The guides

How a new Open3D app is made, checked, released and brought into the launcher. They hold for every app
of the suite; what is specific to one app goes in that app's own README and `CLAUDE.md`.

Read them in this order the first time:

1. [Start a new app](start-a-new-app.md): from *Use this template* to the first passing `tools\check.ps1`
   and the first pull request, then how the app enters the launcher.
2. [Principles](principles.md): who the apps are for, a suite where each app works on its own, Windows
   only, no accounts, English and Spanish, the 3ds Max versions to state, the technologies, the licence and
   the look.
3. [Development](development.md): the layout of a repository, `tools\check.ps1`, `tools/repo.toml`, CI,
   formats and lints, tests, commits, pull requests, the CHANGELOG, versions, and the common files.
4. [Settings and folders](settings-and-folders.md): the language every app shares in `settings.json`, the
   two folders of each app, the variables that move them, and carrying an old folder over.
5. [Security](security.md): the rules every app follows, each with its reason, and the review before an
   app enters the launcher.
6. [Distribution](distribution.md): the release package, the release as a draft, what the launcher needs
   from it, and entering the launcher.
7. [Testing](testing.md): unit tests, the real test as a user before every release, and how a 3ds Max
   plug-in is tested.
8. [Working with Claude Code](working-with-claude-code.md): the model `CLAUDE.md` of each starter and the
   working rules.

A rule here that is wrong, missing or in the way is changed here, by the Open3D maintainer, for every app
at once: open an issue or a pull request in this repository rather than working around it in an app.
