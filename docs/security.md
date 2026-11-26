# Security review

What OpenLibrary reads that it did not make, what it writes outside its own folders, and what it does not protect
against. It follows the checklist of the Open3D-SDK; a change to any of these updates this page in the same commit.

## What it reads

| What | What is checked |
|---|---|
| `.max` files (`src/maxfile.rs`) | Read without 3ds Max, as data. Every chain of sectors is bounded by the size of the file, tables and streams by a fixed size; a tampered file gives nothing. |
| Paths written in a `.max` (`src/missing.rs`) | Shown, and looked for on this computer. A network path is looked at only on a computer the person already uses (that of a library, of the `.max` or of a mapped drive), so that a file cannot make Windows sign in to another. |
| Images: previews, maps, clipboard | Decoded in Rust (`image`), with its memory limits; one that breaks its decoder has no thumbnail. |
| Downloads: zip, rar, 7z (`src/archives.rs`) | Extracted by the 7-Zip, NanaZip or UnRAR installed, or the `tar` of Windows, run by their whole path with their arguments apart, into a new folder of the app; at most 60 GB. Links are left out. Names that become files are cleaned (`catalog::clean_name`), and what exists is replaced only after asking. |
| `settings.json` and caches | JSON; a value of the wrong kind is ignored. |
| The team's `workspace.json` (`src/settings/workspace.rs`) | Only six keys are taken. Patterns of hidden folders take a time bounded by their length. One that cannot be read is never written over. |
| `openlibrary_medidas.json`, `medidas\*.json` | Numbers, in cm. |
| What 3ds Max answers, and its logs | JSON and lines of text, shown; never run. |
| Command-line arguments | `--version`, or nothing. |

## The local server in 3ds Max

- It listens only on `127.0.0.1`. Every request carries a token of 32 random bytes, which is kept encrypted with
  DPAPI for the Windows user, in the app's data folder and in the bridge's `app.json`. It is compared in constant time.
- A request without it, or a line that is not a JSON object (what a web page would send), is refused and the connection
  closed. A silent one closes after a minute.
- Only the commands of `COMMANDS` run. Their arguments are checked: files must exist and be of the kind expected, and
  engines, channels, targets and sizes come from fixed lists.
- A model brought in is marked in its objects' user properties with a hash of its place in the library (letters and
  digits only), so that it can be found again.
- No code is built from text. `bridge/tests` lists every place that could run a text or a file, and fails on a new one.
  The scripts of a 3ds Max in the background have only paths of the app's own folder and numbers in them; the files to
  work on go in a list, as data.

## What it writes outside its own folders

- In a library, and only when asked: previews beside their model, catalogued downloads, proxies (`_proxies`), and the
  paths, layers or Corona version of a `.max`.
- When the person asks for them in the welcome: the shortcuts `OpenLibrary.lnk` on the desktop and in the Start menu
  of the user, made by Windows' own PowerShell (`WScript.Shell`), with the paths in environment variables and never in
  its script.
- In the folder the team chooses: `workspace.json`, `README.txt` and `medidas`.
- In each 3ds Max where the person installs the bridge: `scripts\Open3D\OpenLibrary` and
  `scripts\startup\open3d_openlibrary.ms`, of the user's settings folder.
  Once, unless the person said no, a button in its interface file (`.cuix`, backed up first in the data folder) and
  its icon in `usericons`.

It never asks for administrator rights, never uses the internet and downloads nothing.

## What it does not protect against

- **Opening a `.max` in 3ds Max.** The 3ds Max in the background (previews, model data, *Corona version*, *layer 0*) is
  always started with *Safe Scene Script Execution* on, and does nothing without it: the scripts of a scene may run,
  but not commands of the system, Python, .NET or what writes scripts. Merging a model into your own 3ds Max is as safe
  as its *Preferences > Security*: keep it on, and do not open files you do not trust. The same goes for `.mat`
  libraries and for the OSL shaders of a library.
- **Programs of the same Windows user.** They can read what that user can, the token included.
- **Files of a download.** A double click in the catalogue opens only images and texts; anything else is shown in its
  folder, not opened.
- **The password of an archive** is given to the unpacker on its command line, where other programs of the same user can
  see it while it extracts.
- **Who can write in the shared workspace** can change the tabs, sizes and hidden folders of the team.
