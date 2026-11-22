# Settings and folders

Every Open3D app shares one language preference and keeps its files in the same two kinds of folder. The
code that does it is the same in every app, one implementation per technology, and every starter carries
its copy:

| | The file | In the starter |
|---|---|---|
| Rust | `open3d_settings.rs`, with `open3d_settings_cases.txt` beside it | `starters/rust-program/src/` |
| Python 3.11 | `open3d_settings.py`, standard library only | `starters/python-plugin/OpenExample.bundle/Contents/python/openexample/` |
| C++17 | `open3d_settings.h` and `open3d_settings.cpp`, Windows API only | `starters/cpp-plugin/src/` |

The three answer the same on the same input: their tests read the same table of cases. They are common
files: never edit them in the app's repository ([Development](development.md#the-common-files)). Use them;
never build these paths by hand.

The same names in the three: `language`, `read_language`, `write_language`, `windows_language`,
`settings_dir` and `data_dir`, `app_settings_dir(app)`, `app_data_dir(app)` and `carry_over(old, new)`. For
given values instead of the real variables, Rust and C++ have `settings_dir_from` and `data_dir_from`, and
the Python functions take an `environ` argument.

## The shared language

- It lives in `%APPDATA%\Open3D\settings.json`, a JSON object in UTF-8. Its key `"language"` is `"en"` or
  `"es"`. Other keys may come later.
- **Reading never fails and never writes.** No file, no key, a file that cannot be read and any other value
  all mean *no preference yet*. Then the language of Windows decides: Spanish when Windows' interface is
  Spanish, of any country; English otherwise.
- **Writing happens only when a person changes the language**, from the launcher or from an app's own
  settings. No app writes the file just because it started. Every other key is kept. Taking the preference
  out (an *Automatic* choice) removes the key, and Windows decides again.
- The file is written whole into a new file beside it, which then takes its place, so no app ever reads half
  a file. A failure to write is told to the person; it never ends the program.
- The app where the language changes shows it at once. The others take it at their next start. No app
  watches the file.
- **3ds Max plug-ins** are in English only and do not read the language ([Principles](principles.md)).

## The folders of each app

- **Settings** (what the person chose: options, window places, recent files) go to
  `%APPDATA%\Open3D\<App>` (`app_settings_dir`).
- **Everything else the app writes** (caches, logs, recovery files, web view data, databases) goes to
  `%LOCALAPPDATA%\Open3D\<App>` (`app_data_dir`).
- `<App>` is the app's name as the launcher shows it: `OpenRef`, `OpenTools`.
- Only the app that writes into its folders creates them.
- **Never write beside the exe or inside the bundle.** A program in `C:\Program Files\Open3D\<App>` cannot
  write there, and an update replaces that folder.
- A 3ds Max plug-in keeps one set of settings for every 3ds Max version, not one per version.

*Why:* it is the Windows custom. With roaming profiles on a network, settings travel with the person, while
caches and databases stay on the computer.

## The variables that move them

- `OPEN3D_SETTINGS_DIR`, when it is set and not empty, takes the place of `%APPDATA%\Open3D`: for the shared
  `settings.json` and for every app's settings.
- `OPEN3D_DATA_DIR`, when it is set and not empty, takes the place of `%LOCALAPPDATA%\Open3D`.
- A portable launcher keeps everything in its own folder and gives both variables to the programs it opens,
  so they keep their files there too. This is why an app always asks the shared code for its folders.
- Tests set both variables to a scratch folder, so they never read or write the person's real folders. In
  Rust, where setting a variable inside a test needs `unsafe`, test `settings_dir_from` and `data_dir_from`
  with given values, or start the test program again with the variables set.
- With neither the variable nor `APPDATA` (or `LOCALAPPDATA`) there is no folder, and the app decides what
  to do; it still never writes beside its exe.

## Carrying an old folder over

A new app has no old folders. When a later version moves one, it calls `carry_over(old, new)` once for each
old folder, at start, before it reads its settings or writes anything:

- It copies only when nothing named `new` exists yet and `old` is a folder. An existing `new` is never
  touched.
- It copies everything into a temporary folder beside `new`, which takes the name `new` only once the copy
  is complete, so a copy cut half way is never taken for a finished one.
- `old` is never changed, moved or deleted. Removing old folders is for later, and asks first.
- Links and junctions inside `old` are not followed. A file that cannot be read is left out and named; the
  rest is still copied.
- It tells the caller what happened (**nothing** to do, **copied** N files with what was left out, or
  **failed** with why) and never ends the program. Say in the app what was left out.
- When it fails, the app uses its old folder for that run, so that it does not create `new` and lose the
  copy; the next start tries again.

*Why:* nobody has to set anything up again after an update, and if something goes wrong, or an older
version is used again, nothing is lost.

## When the app is removed

Removing a program with the launcher keeps both folders, unless the person ticks *Also delete its settings
and data*; removing a 3ds Max plug-in keeps them. Say in the README which folders the app uses, so that a
person who wants them gone can delete them.
