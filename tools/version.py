# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""TreeDock release versions.

Rules (RELEASING.md):
- Versions are MAJOR.MINOR.PATCH, starting at 0.1.0. Tags are `v` + version.
- PATCH is a single digit: after x.y.9 comes x.(y+1).0, never x.y.10.
- Each release must be newer than every earlier release tag.
- The version in the app (.csproj) and the engine (Cargo.toml/Cargo.lock) must match the tag.

Usage:
  python tools/version.py current            print the version in the source files
  python tools/version.py next [--minor]     print the next version after the latest tag
  python tools/version.py set X.Y.Z          write X.Y.Z into the source files
  python tools/version.py check vX.Y.Z       validate a release tag (used by the release workflow)
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CSPROJ = ROOT / "app" / "TreeDock.App" / "TreeDock.App.csproj"
CARGO = ROOT / "engine" / "Cargo.toml"
LOCK = ROOT / "engine" / "Cargo.lock"
FIRST = (0, 1, 0)
VERSION_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(\d)$")


def parse(text):
    m = VERSION_RE.match(text)
    if not m:
        raise ValueError(f"'{text}' is not MAJOR.MINOR.PATCH with a single-digit PATCH (0-9)")
    v = tuple(int(x) for x in m.groups())
    if v < FIRST:
        raise ValueError(f"'{text}' is below the first release, 0.1.0")
    return v


def fmt(v):
    return ".".join(map(str, v))


def bump(v, minor=False):
    major, mi, patch = v
    if minor or patch == 9:
        return (major, mi + 1, 0)
    return (major, mi, patch + 1)


def tags():
    out = subprocess.run(["git", "tag", "--list", "v*"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    found = []
    for t in out.split():
        try:
            found.append((parse(t[1:]), t))
        except ValueError:
            pass
    return sorted(found)


def read_files():
    cs = re.search(r"<Version>([^<]+)</Version>", CSPROJ.read_text(encoding="utf-8"))
    cargo = re.search(r'\[workspace\.package\]\s*\nversion = "([^"]+)"', CARGO.read_text(encoding="utf-8"))
    lock = set(re.findall(r'name = "treedock-[a-z]+"\nversion = "([^"]+)"', LOCK.read_text(encoding="utf-8")))
    return {"csproj": cs and cs.group(1), "Cargo.toml": cargo and cargo.group(1), "Cargo.lock": ",".join(sorted(lock))}


def sub_file(path, pattern, repl, count):
    text = path.read_bytes().decode("utf-8")  # keep the file's own line endings
    new, n = re.subn(pattern, repl, text)
    if n != count and count >= 0:
        raise SystemExit(f"{path.name}: expected {count} version line(s), found {n}")
    path.write_bytes(new.encode("utf-8"))


def cmd_set(text):
    v = fmt(parse(text))
    sub_file(CSPROJ, r"<Version>[^<]+</Version>", f"<Version>{v}</Version>", 1)
    sub_file(CARGO, r'(\[workspace\.package\]\s*version = ")[^"]+(")', rf"\g<1>{v}\g<2>", 1)
    sub_file(LOCK, r'(name = "treedock-[a-z]+"\r?\nversion = ")[^"]+(")', rf"\g<1>{v}\g<2>", -1)
    print(v)


def cmd_check(tag):
    if not tag.startswith("v"):
        raise SystemExit(f"tag '{tag}' must start with 'v', e.g. v0.1.0")
    try:
        v = parse(tag[1:])
    except ValueError as e:
        raise SystemExit(f"tag '{tag}': {e}")
    earlier = [(tv, t) for tv, t in tags() if t != tag]
    if earlier and v <= earlier[-1][0]:
        raise SystemExit(f"tag '{tag}' is not newer than the latest release {earlier[-1][1]}")
    expected = fmt(bump(earlier[-1][0])) if earlier else fmt(FIRST)
    files = read_files()
    bad = {k: x for k, x in files.items() if x != fmt(v)}
    if bad:
        raise SystemExit(f"tag '{tag}' does not match the source versions {bad}; run: python tools/version.py set {fmt(v)}")
    note = "" if fmt(v) == expected else f" (note: the next patch release would have been {expected})"
    print(f"{tag} OK{note}")


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return
    c, rest = argv[0], argv[1:]
    if c == "current":
        files = read_files()
        if len(set(files.values())) != 1:
            raise SystemExit(f"source versions disagree: {files}")
        print(files["csproj"])
    elif c == "next":
        t = tags()
        print(fmt(bump(t[-1][0], minor="--minor" in rest)) if t else fmt(FIRST))
    elif c == "set" and len(rest) == 1:
        cmd_set(rest[0])
    elif c == "check" and len(rest) == 1:
        cmd_check(rest[0])
    elif c == "selftest":
        assert fmt(bump((0, 1, 8))) == "0.1.9"
        assert fmt(bump((0, 1, 9))) == "0.2.0"
        assert fmt(bump((0, 2, 3), minor=True)) == "0.3.0"
        assert fmt(bump((1, 9, 9))) == "1.10.0"
        for bad in ("0.1.10", "0.0.9", "1.2", "v0.1.0", "01.1.0", "0.1.0-beta"):
            try:
                parse(bad)
            except ValueError:
                continue
            raise AssertionError(bad)
        print("selftest OK")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
