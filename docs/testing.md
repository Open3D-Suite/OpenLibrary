# Testing

Two kinds of test, and a version needs both: the automatic tests, run by `tools\check.ps1` on every change,
and the real test as a user, done by a person (or by Claude Code with real clicks) before every release.

## Automatic tests

- **A bug that is fixed comes with a test that fails without the fix**, and new logic comes with its tests.
  No coverage target.
- `tools/repo.toml` lists them as checks, so `tools\check.ps1` and CI run them: `cargo test` for Rust, a test
  runner of the repository for Python, the engine's tests for C++.
- Quick tests need nothing but the repository: no 3ds Max, no network, no other program running. A test that
  needs 3ds Max (`3dsmaxbatch`), the network or a long time is marked `full = true` and runs only with
  `tools\check.ps1 -Full`, which must pass before every release.
- Tests never read or write the real folders: they set `OPEN3D_SETTINGS_DIR` and `OPEN3D_DATA_DIR`, and any
  variable of the app's own, to a scratch folder ([Settings and folders](settings-and-folders.md)).
- Test the logic without the host. A Python plug-in keeps what is tested in modules that import neither
  `pymxs` nor Qt when they load, and a small stand-in for 3ds Max gives the rest what it needs. A C++
  plug-in tests its engine, which has no 3ds Max header. A Rust program tests its model apart from its
  window.
- Make the check fail when the test fails. `3dsmaxbatch` can end well even when the script inside failed:
  let the test write its verdict to a file, and have the check read it.

## The real test as a user

Before every release, and before an app enters the launcher, someone uses the version to be released as a
person would, on the package the release will carry:

- open every window, panel and dialog, and check that every button, control and text is there, in each
  language the app has;
- press every button and run every tool, and check the result: in the scene, in the files written, in the
  window;
- try what goes wrong for a person: a missing file, a cancelled dialog, a file the app cannot read;
- read the errors and the log (for a plug-in, the MAXScript Listener) after each action;
- take screenshots of the interface.

At the end, write a short report: what was tested, what works, what fails or is missing, with the
screenshots. A version is not ready until it passes.

*Why:* even if it is slower, it catches what a test behind the scenes does not see.

For a program, start the exe from the release zip, not from the build folder, and check that it writes
nothing beside itself.

## Testing a 3ds Max plug-in

### The rules

- **Start your own 3ds Max for the test, and close it yourself when you finish, without saving.**
- **Never close or touch a 3ds Max you did not start.** If someone else's is open, ask them first.
- **Never open or save someone else's scene.** Always build a new test scene, with what the test needs:
  objects and instances, lights, materials with one map present and one missing, say.
- **Point the folders at a scratch folder.** Start 3ds Max with `OPEN3D_SETTINGS_DIR`, `OPEN3D_DATA_DIR`
  and any test variable of the plug-in set to it, so the test reads and writes none of your real settings.
- **Real clicks** on the plug-in's panels, menus and windows. A script or an MCP server for 3ds Max may build
  the scene and read the result after each click, but the clicks are real.
- **Read the MAXScript Listener** after each action. Nothing but the plug-in's own messages and the
  expected errors should be there.
- Test on each 3ds Max version the README will call tested, with the renderers it names.

When Claude Code drives the test:

- Keep an MCP server for 3ds Max in its safe mode, and do not work around what it refuses.
- **Never send the Escape key to 3ds Max.** A synthetic Escape press once closed 3ds Max with an
  application error, in the middle of a call from Python to MAXScript.
- Note which 3ds Max it started (its process id), so that it closes only that one.

### Loading a plug-in without installing it

While working, load the plug-in from the repository or from `dist/`:

- **A Python plug-in, from the repository:** a small development bundle in
  `%APPDATA%\Autodesk\ApplicationPlugins` whose start-up script puts the bundle's `Contents\python` in the
  repository on `sys.path` and starts the package from there. Every 3ds Max and every `3dsmaxbatch` you start
  then runs the repository's code, so set the scratch variables for those too. Never install the
  development bundle and the release bundle together: only the first to start runs. The Python starter
  carries no script for it; OpenTools' `dev/install_dev.ps1` is one to follow.
- **A built bundle in `dist/`:** in a 3ds Max you started, run the bundle's start-up script with `fileIn`.
- **A C++ plug-in:** `build.ps1 -Plugin` puts the bundle together in the build folder, without installing
  it. In a 3ds Max you started, `loadDllsFromDir` on a copy of its plug-in file (the starter's
  `OpenExample.gup`, a `.dlo` for an object) in a scratch folder: 3ds Max holds the file until it closes, so
  a copy keeps the build folder free. Automatic tests run in `3dsmaxbatch`, each with a private copy of the
  bundle.

Once before a release, try the installed package as well: with any development bundle removed, unzip the
release zip's bundle into `ApplicationPlugins`, start 3ds Max and check that the plug-in loads and runs. A
copy loaded from a folder does not show everything an installed one does.
