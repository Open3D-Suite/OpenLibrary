# Working with Claude Code

The Open3D apps are written with Claude Code. Each starter carries a model `CLAUDE.md`, and the rules below
are the ones every Open3D repository works by. "The author" is the person who owns the app and works with
Claude on it.

## The model `CLAUDE.md`

Claude Code reads `CLAUDE.md` at the start of every session. In an Open3D repository it opens with a line
or two on what the app is, then:

- **How we work:** the working rules below, so that Claude follows them in this repository too.
- **Layout:** where things are, one line per folder or file that matters.
- **Build and check:** how it is built, what `tools\check.ps1` runs and what `-Full` adds, and CI.
- **Version and release:** where the version lives, the CHANGELOG, what `package` and `release` make, and
  what the launcher needs from the package.
- **Rules of this code:** the rules a change must keep, the test rule first, then those of the app (where
  its files go, how texts are translated, what is never done).
- **Traps:** what has gone wrong before and is not obvious from the code.

The rules for the file itself:

- **Only what can be checked in the code.** Every line names a file, a function or a command that says the
  same.
- **Updated in the same commit** as the change that makes a line of it old.
- **No history and no dated notes.** What changed goes in the CHANGELOG; reports of work go in documents.
- **Short.** Facts, not explanations: the existing apps keep the sections after *How we work* to about 60
  lines.
- **Its placeholders filled in.** The model marks in angle brackets (`<...>`) what each app writes for
  itself: what the app does, its own rules, its traps. Replace them as soon as there is something true to
  say, and remove a line that has nothing yet.
- **In English**, like everything in the repository, and published with it.
- **Nothing of the author's computer**: no paths of it, user names, computer names or addresses. Those go
  in a `CLAUDE.md` in a folder above the repository, which Claude Code reads too, or in Claude's memory;
  never in the repository.

*Why:* instruction files written from memory drift from the code, contradict each other and grow with notes
of past sessions until nobody trusts them. A file that states only what the code says, and changes with it,
stays true.

The guides of this SDK are not copied into the app's repository. Point Claude to them when a task touches
what they cover (a new setting, a network feature, a release).

## The working rules

- **The author decides.** A working or design criterion is never decided by Claude alone. Claude asks in
  blocks, one at a time: two to four options, its recommendation and why, and what each costs. Each decision
  is written down with its reason.
- **What is common to the suite is decided by the Open3D maintainer.** A change to these guides or to the
  common files is proposed there, not made in the app.
- **The code is the truth.** A claim is checked in the code before it is relied on, whatever a document
  says.
- **Plan first.** Before a large change, Claude shows a plan and waits for the author's confirmation.
- **Nothing goes out without the author's explicit OK, each time:** no push, no tag, no release. A release
  is left on GitHub as a draft; the Open3D maintainer publishes it. Tags are annotated, `vX.Y.Z`, and pushed
  one at a time, never with `--tags`.
- **Nothing is deleted without asking first, each time.** That holds for files, branches, folders and data.
- **A test for every fixed bug**, which fails without the fix; new logic comes with its tests.
- **After a change**, the app builds, `tools\check.ps1` passes and the app starts.
- **Small commits**, one step each, with one English sentence in the imperative. The CHANGELOG entry goes in
  the same commit as a change a user can notice.
- **Pull requests.** Every change reaches `main` through one: a branch, CI passes, someone else reviews it,
  then it is merged.
- **One subagent per area** for a task that touches several (the engine and the plug-in, or two parts of a
  program that do not share code): each works in its own part, and the main session checks what each one
  did.
- **Nothing of the author's computer in a published file.** Keep the private list of
  [Security](security.md#nothing-of-anybodys-computer-in-a-published-file) up to date, so the check catches
  what slips through.
- **Talk in the author's language; write in English.** The conversation can be in any language; code,
  comments, commits and documents are in English.

## Testing in 3ds Max with Claude Code

- Claude starts its own 3ds Max for a test and closes it at the end without saving. It never closes or
  touches a 3ds Max it did not start; if the author has one open, it asks.
- It never opens or saves a scene of the author's: every test uses a new test scene.
- An MCP server for 3ds Max stays in its safe mode. Claude does not work around what it refuses.
- Panels are driven with real clicks; the MCP builds the scene and reads the result.
- Claude never sends the Escape key to 3ds Max.

The full method is in [Testing](testing.md).
