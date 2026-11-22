# SPDX-License-Identifier: GPL-3.0-or-later
"""Makes the repository of a new app from one starter, as its author would, and runs that repository's checks.

    python -I starters/check_starter.py rust-program | python-plugin | cpp-plugin

In a new temporary folder it puts:

1. the common files a new app's repository holds, from the root of this one (COMMON);
2. every file of the starter, at the root, as Git lists them (tracked, or new and not ignored), and the
   starter's ignored local settings in LOCAL when this computer has them, so that a build can reuse
   its folder;
3. the codes the author makes first: each REPLACE-WITH-A-NEW-GUID becomes a GUID of its own, and each
   REPLACE_WITH_TWO_NEW_NUMBERS two random 32-bit numbers.

Then it makes a Git repository there, with nothing committed, and runs "tools/repo.py check" in it with
the Python that runs this file. Its exit code is the check's. The temporary folder is removed afterwards.
"""

import os
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

SDK = Path(__file__).absolute().parent.parent
STARTERS = ("rust-program", "python-plugin", "cpp-plugin")
# What the root of this repository gives every new app: the common files it holds at the same place.
COMMON = (
    ".editorconfig",
    ".gitattributes",
    "LICENSE",
    "ruff.toml",
    "tools/check.ps1",
    "tools/repo.py",
    "tools/repo_package.py",
    "tools/repo_release.py",
    "tools/repo_msi.py",
)
# What every starter holds, besides its code.
REQUIRED = (
    "README.md",
    "CHANGELOG.md",
    "CLAUDE.md",
    ".gitignore",
    ".github/workflows/check.yml",
    "tools/repo.toml",
)
# Ignored files of one computer that the copy may take: cargo's build folder on a local disk. A CMake
# build folder is not shared, since CMake refuses a build folder made from another source folder.
LOCAL = (".cargo/config.local.toml",)
GUID = re.compile(rb"REPLACE-WITH-A-NEW-GUID")
NUMBERS = re.compile(rb"REPLACE_WITH_TWO_NEW_NUMBERS")


def git():
    found = shutil.which("git")
    if not found:
        raise SystemExit("check_starter.py: git was not found")
    return found


def starter_files(name):
    """The starter's files as Git lists them, relative to the starter's folder, with forward slashes."""
    listing = [git(), "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", f"starters/{name}"]
    # S603: git, found above, with fixed arguments.
    done = subprocess.run(listing, cwd=SDK, capture_output=True, check=True)  # noqa: S603
    prefix = f"starters/{name}/"
    names = [entry[len(prefix) :] for entry in done.stdout.decode("utf-8").split("\0") if entry.startswith(prefix)]
    return sorted(name for name in names if (SDK / prefix / name).is_file())


def made_codes(data):
    """The text with the codes made: a GUID of its own for each placeholder, two numbers for each pair."""
    data = GUID.sub(lambda _: str(uuid.uuid4()).upper().encode(), data)
    return NUMBERS.sub(lambda _: b"0x%08x, 0x%08x" % (secrets.randbits(32), secrets.randbits(32)), data)


def make_repository(name, folder):
    """The new app's repository in `folder`, as its author would make it from the starter."""
    files = starter_files(name)
    missing = [required for required in REQUIRED if required not in files]
    if missing:
        raise SystemExit(f"check_starter.py: starters/{name} has no {', '.join(missing)}")
    starter = SDK / "starters" / name
    sources = {path: SDK / path for path in COMMON}
    sources.update({path: starter / path for path in files})
    sources.update({path: starter / path for path in LOCAL if (starter / path).is_file()})
    for path, source in sources.items():
        target = folder / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(made_codes(source.read_bytes()))
    # S603: git, found above, with fixed arguments.
    subprocess.run([git(), "-c", "init.defaultBranch=main", "init", "-q"], cwd=folder, check=True)  # noqa: S603
    return len(sources)


def writable(function, path, _error):
    """Git keeps its objects read-only, and Windows does not remove a read-only file."""
    os.chmod(path, stat.S_IWRITE)
    function(path)


def remove(folder):
    if sys.version_info >= (3, 12):
        shutil.rmtree(folder, onexc=writable)
    else:
        shutil.rmtree(folder, onerror=writable)


def main(arguments):
    if len(arguments) != 1 or arguments[0] not in STARTERS:
        print(__doc__.strip())
        return 2
    name = arguments[0]
    folder = Path(tempfile.mkdtemp(prefix=f"open3d-{name}-"))
    try:
        count = make_repository(name, folder)
        print(f"A new app's repository from starters/{name}: {count} files in {folder}", flush=True)
        # S603: the Python that runs this file, on the common tool just copied, with fixed arguments.
        done = subprocess.run([sys.executable, "-I", "tools/repo.py", "check"], cwd=folder)  # noqa: S603
    finally:
        remove(folder)
    return done.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
