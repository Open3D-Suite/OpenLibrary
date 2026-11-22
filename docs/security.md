# Security

The rules of every Open3D app, drawn from the security work done on the existing ones, as a checklist with
the reason for each. A new app follows all of those that apply to it, and its review before the launcher
goes through them (at the end of this page).

The people who use the apps open scenes and files from clients, colleagues and the internet, and install
the apps on every computer of a studio. An app must never let a file, a web page or another computer do
something the person did not ask for.

## What comes with a file

- [ ] **Nothing that came with a file runs or loads without asking.** Scripts, render presets and anything
  else in a scene or a document that can run code are shown first, in a review question that names them,
  and run only once the person says yes. The default answer, the one Enter gives, is no. A choice made on
  this computer (a preset picked in the app's own interface) is trusted at once.
  *Why:* a `.max` file, or any file from someone else, can carry code, and opening a file must never be
  enough to run it.
- [ ] **A name read from a file is never evaluated.** A class name, say, is checked to be a plain name
  (letters, digits, underscore), looked up as a class without running anything, and required to be of the
  kind expected. Otherwise it is ignored.
  *Why:* building code from a text read from a file lets the file run anything it likes.
- [ ] **A file sets only what the app's own interface sets.** The settings a file may carry are a fixed
  list: never a path into other objects (`node.controller.script`), never a name that holds code.
  *Why:* a property path can reach a script controller, and a harmless-looking setting becomes code.
- [ ] **Text never becomes code.** No `execute`, `eval` or `python.Execute` of a string built from data. Keep
  a test that lists every place where the app turns text into code, so that a new one fails it.
  *Why:* each such place is a door; a test keeps their number known.
- [ ] **What is refused is said.** It is skipped, counted in a message in the window, and named in the
  MAXScript Listener or the log.
  *Why:* the person must know that the file was not applied whole, and why.
- [ ] **Input from files is validated.** A file whose format version is newer than the app knows is refused,
  never guessed at. Sizes and counts are bounded. A path that comes from a file or from the network is used
  only inside the folders the person allowed; names that become file names are cleaned first.
  *Why:* a file is written by someone else, possibly on purpose to break the app.
- [ ] **Text files are valid UTF-8.** A test fails on a source that is not, or that holds broken characters.
  *Why:* a broken character in a warning hides what the warning says.

## What crosses a network

- [ ] **From another computer, only signed requests are obeyed.** Each machine has a key pair; its private
  key never leaves it, and the other side stores only the fingerprint of its public key. Every request is
  signed over its method, its path, the time, a single-use number and a digest of its body. A missing or
  wrong signature, a time far off and a repeated request are refused. An id stays bound to the key it first
  came with.
  *Why:* anyone on the network can send a request, and anyone can copy one they saw.
- [ ] **A local server listens only on `127.0.0.1`**: a server for the app's own window, or for other
  programs on the same computer, never on every address.
  *Why:* a server on every address is open to the whole network.
- [ ] **It needs a token and checks where a request comes from.** A request that changes something carries
  the token of the run, which only the app's own window knows. A request is refused when its `Host` is not
  `127.0.0.1`, `localhost` or `[::1]` with the server's own port, when it has an `Origin` that is not the
  server's own, or a `Sec-Fetch-Site` that is not `same-origin` or `none`.
  *Why:* any web page open in the browser can call a port on the computer, and a DNS rebinding attack makes
  it look as if it came from the computer itself.
- [ ] **Nothing is exposed to the internet.** An app that talks to other computers is for a local network
  until it has encryption and a reviewed authentication, and its README says so.
  *Why:* the internet reaches every open port, and an unencrypted protocol can be read and altered on the
  way.
- [ ] **No firewall rule from the app or its installer.** Windows asks the person the first time a program
  listens.
  *Why:* no rule of the system is made without the person seeing it.

## Secrets

- [ ] **Keys are encrypted at rest**, with Windows DPAPI for the user. No secret in clear in a settings file.
  *Why:* settings files are copied, backed up and shared with a colleague; a key in them travels too.
- [ ] **No secret in the repository**: no key, token or password that works anywhere. A test makes its own
  keys when it runs.
  *Why:* a public repository's history keeps every file for ever.

## Downloads

- [ ] **Every download is checked by its hash before it is used.** A file that does not match its published
  SHA-256 (or the digest the job names) is deleted and the task fails. A release without a hash is not
  installed.
  *Why:* a file can be altered on its way, or replaced on the server.

## Nothing of anybody's computer in a published file

- [ ] **No path, user name, computer name or network address of a real computer** in code, tests, examples,
  documents, commit messages or packages. Examples and tests use made-up names and addresses.
  *Why:* the repositories are public, and removing something from their history means rewriting it and
  changing every commit and tag.
- [ ] **Keep a private list.** Write the words of your own computer and network (your user name, the names
  of your computers, drives and shares) one per line in a file named `open3d-private-terms.txt`, in a folder
  above the repository, never inside it. `tools\check.ps1` and the packager look for it in every folder
  above the repository and fail on any file that holds one of its words, or the folder of a Windows user.
  Nothing else may copy that list.
  *Why:* the check catches what a person misses; the list itself must never be published.
- [ ] **No folder of the building computer inside a program.** The packager builds without the build's
  folders in the exe and refuses an exe that still names one.
  *Why:* a compiler writes source paths into a program unless it is told not to.
- [ ] **Facts of your computer stay out of `CLAUDE.md`.** They go in a file of your own outside the
  repository ([Working with Claude Code](working-with-claude-code.md)).

## Dependencies

- [ ] **Dependabot alerts are on** in the repository, and are reviewed before each release. Its automatic
  fix requests are off.
  *Why:* a library with a known problem must be seen before a release, and updated on purpose, not by a
  bot.
- [ ] **Libraries with compatible licences, at fixed versions.** `Cargo.lock` is committed and every
  cargo command runs with `--locked`; the CI's actions are pinned to a commit and its token reads only.
  *Why:* every library is code that runs on the person's computer.

## The code and the installation

- [ ] **Rust: `unsafe` is denied**, and allowed by name, with its reason, only where Windows or the graphics
  card is called.
  *Why:* the rest of the code then cannot break memory safety.
- [ ] **The app never asks for administrator rights by itself**, and never starts a hidden PowerShell or
  script with them. Windows asks once, when its installer runs.
  *Why:* antivirus programs take a hidden elevated script for malware, and a program that elevates itself is
  a risk for the whole computer.
- [ ] **A program's installer has no custom action, no service and no dialog**, and writes nothing outside
  its folder but its shortcut and the one registry value Windows Installer needs for it
  ([Distribution](distribution.md)). The packager checks it.
  *Why:* an installer runs with administrator rights.

## The review before the launcher

Before an app enters the launcher's signed catalog, Open3D's maintainers review it. The author prepares it
by going through this list:

- [ ] `tools\check.ps1` and `tools\check.ps1 -Full` pass, and CI is green on `main`.
- [ ] Every rule above that applies to the app holds, and each has its test where a test can show it.
- [ ] A list of every place where the app reads something it did not make: files and scenes, the clipboard,
  the network, a local server, downloads, command-line arguments. For each, what is checked.
- [ ] A list of what the app writes outside its own folders, if anything, and why.
- [ ] Dependabot alerts reviewed; no open alert without a reason.
- [ ] The package checked: no private data, the version inside it the release's.
- [ ] The README says what the app sends over a network, if anything, and what it does not protect against.

The maintainers then read the code, try to break it as an attacker would (a tampered file, a request without
a signature, a web page calling its local server, an altered download), and do the real test as a user
([Testing](testing.md)). Each problem they find is fixed with a test that fails without the fix, and tried
again for real. Only then is the app added to the catalog ([Distribution](distribution.md)).

*Why:* the Open3D maintainer signs the catalog, and the people who install the apps trust that signature.
