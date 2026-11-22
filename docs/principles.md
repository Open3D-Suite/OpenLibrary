# Principles

What every Open3D app has in common, whatever it does. Each rule has its reason.

## Who the apps are for

3D artists and small studios that use 3ds Max, without a technical profile. So:

- **An installer, not a setup.** The launcher installs and updates every app with a click; an app needs no
  account and nothing else installed beside it.
- **Plain texts.** Say what happened and what to do next, in the words of a 3D artist, not of a developer.
- **Nothing is lost.** Settings survive updates, a move of an app's folders copies the old ones over, and
  removing an app keeps them.
- **Familiar terms.** Inside 3ds Max, use 3ds Max's own names for its windows, menus, objects and
  renderers.

## A coherent suite, where each app works on its own

- Every app is installed, set up and translated the same way, and shares the same language preference and
  the same kind of folders ([Settings and folders](settings-and-folders.md)).
- Each app works alone: installed with the launcher or by hand, with no other Open3D app present, and with
  no launcher running. It never needs another app to start.
- A specific integration between two apps (one app sending work to another, say) is a feature of its own,
  proposed to the Open3D maintainer later, never a requirement.

*Why:* people install only the apps they need, and an app that depends on another breaks when the other
changes.

## Windows only

- Windows 10 or 11, 64-bit. Every package is `windows-x64`.
- No code for macOS or Linux.

*Why:* 3ds Max runs only on Windows, and the suite is made for the people who use it.

## No accounts, no sign-in, no telemetry

- An app never asks for an account, a licence key or a sign-in, and never sends anything about its use.

*Why:* the apps are free tools for a person's own work, installed without an account, as the existing ones
are.

## English and Spanish

- **Programs** (and the launcher) have their whole interface in English and in Spanish. English is the
  default.
- They follow the **language every Open3D app shares**, in `settings.json`
  ([Settings and folders](settings-and-folders.md)): with no preference yet, Spanish when Windows is in
  Spanish (of any country) and English otherwise. The person changes it from the launcher or from any app;
  the change holds for all of them. The app where it is changed switches at once, the others at their next
  start.
- One translation mechanism per technology, and a test that fails when a text of the interface has no
  Spanish.
- **Plug-ins that only work inside 3ds Max** are in English only, and do not read the shared language. So
  are the names of 3ds Max's own windows, menus and renderers wherever any app mentions them.
- A plug-in for another program that has Spanish (Blender, say) follows that program's language, through
  its own translation system, not the shared preference.

*Why the plug-ins stay in English:* 3ds Max has no Spanish, and the people who use it are used to its
terms and its work in English. A plug-in follows the program it lives in.

## The 3ds Max versions to state

- Say in the README, and in the launcher's text for the app, which 3ds Max versions the app was **tested**
  on and which **should work, but are not tested**. For example: "tested on 3ds Max 2027; 2025 and 2026
  should work, but are not tested".
- A version counts as tested only when the real test as a user passed on it ([Testing](testing.md)).
- 3ds Max 2025 is the oldest version the suite supports. A Python plug-in runs on its Python, 3.11.
- A C++ plug-in is built against the SDK of one 3ds Max version and says that version only.
- A program that does not run inside 3ds Max says nothing about 3ds Max versions.

*Why:* only the versions someone actually used can be promised; the others are a fair expectation, said as
such.

## Technologies

Only those the suite already uses:

- **A program:** Rust, with a window of eframe/egui, or a tao + wry (WebView2) window over a local server.
- **A 3ds Max plug-in:** Python with Qt (the PySide6 that comes with 3ds Max) and MAXScript, or C++ with the
  3ds Max SDK.

Each has its starter in this repository. Anything else needs the Open3D maintainer's agreement first.

*Why:* the common checks, the packager, the installer and the shared settings already work for these, and
nothing else.

## Licence

- Every app is free software under the **GNU General Public License, version 3 or later**
  (`GPL-3.0-or-later`), with the `LICENSE` text every repository shares.
- Every source file starts with the line `SPDX-License-Identifier: GPL-3.0-or-later`; `tools\check.ps1`
  fails on one that lacks it.
- The copyright is the app's author's and its contributors'.
- A library or code taken from another project must have a licence compatible with it; the packager checks
  the licence of every Rust library it links in. Code ported from another project keeps its credit in
  `NOTICE` and its licence text in `LICENSES/`.

*Why:* the suite has one licence, and the common checks and the packager expect it.

## The look

The design of the suite is not decided yet: it is a later phase, with a design system for every app. Until
then:

- Follow the existing apps. A program looks like the other programs and the launcher. A 3ds Max plug-in
  looks like the other plug-ins; OpenTools, for one, takes its colours from 3ds Max's own theme.
- Nothing about the design is decided before that phase: no design system of an app's own, and no
  formatter for HTML, CSS or JavaScript, which comes with the design.

## How an app is installed

- A **program** is installed by its own Windows Installer package (MSI), for everyone on the computer, into
  `C:\Program Files\Open3D\<App>`, with one shortcut in the Start menu. Windows asks for permission at each
  install and update. There is a zip too, which runs from any folder.
- A **3ds Max plug-in** is a `<App>.bundle` folder in `%APPDATA%\Autodesk\ApplicationPlugins`, for the
  user, without asking for permission.
- No app installs a Windows service, adds a firewall rule, asks for administrator rights by itself or
  writes beside its own exe.
- Removing an app keeps its settings and data. For a program, the launcher offers a box to delete them too.

The launcher does the installing; the details are in [Distribution](distribution.md).
