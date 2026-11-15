#!/usr/bin/env python3
"""The sealed fresh task set: 10 web tasks nobody tuning tabref has seen, kept outside the repo (never committed).

The folder (--fresh-dir or BENCH_FRESH_DIR) holds tasks_fresh.py (the tasks_heldout.py interface: WEB_TASKS,
web_seed, reset_payload, web_expectations, verify_web), web/server.py and a SHA256SUMS file. The seal is the
sha256 of SHA256SUMS; `verify` checks it, every listed file's hash, and that nothing else is in the folder (a
__pycache__ or an edited file breaks it). The seal is checked before every use and again after a pass.
Nothing here prints task content: callers report counts and file names only.

  python3 -I -B bench/harness/fresh.py verify <dir>     exit 0 when the seal holds, else 1 with the reason
"""
import hashlib
import importlib.util
import os
import sys
from pathlib import Path

SEAL = "1067387e3e7631ff2a1d28675cb2a2391c99324a57f2fc7f5e45dbdd2fd5468a"
PORT = 8793
BASE = f"http://127.0.0.1:{PORT}"


class SealBroken(Exception):
    pass


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fresh_dir(arg=None):
    d = arg or os.environ.get("BENCH_FRESH_DIR")
    if not d:
        raise SealBroken("no fresh folder: pass --fresh-dir or set BENCH_FRESH_DIR")
    return Path(d).resolve()


def listed(root):
    out = {}
    for line in (root / "SHA256SUMS").read_text().splitlines():
        if not line.strip():
            continue
        digest, _, name = line.partition("  ")
        name = name.lstrip("*")
        if not name or Path(name).is_absolute() or ".." in Path(name).parts:
            raise SealBroken("SHA256SUMS has a bad entry")
        out[name] = digest
    return out


def verify(root):
    """The folder's file count when the seal holds; raises SealBroken (with a content-free reason) otherwise."""
    root = Path(root)
    if not (root / "SHA256SUMS").is_file() or (root / "SHA256SUMS").is_symlink():
        raise SealBroken(f"no SHA256SUMS in {root}")
    if sha256(root / "SHA256SUMS") != SEAL:
        raise SealBroken("SHA256SUMS does not match the seal")
    want = listed(root)
    present, dirs = set(), set()
    for p in root.rglob("*"):
        rel = p.relative_to(root).as_posix()
        if p.is_symlink():
            raise SealBroken(f"symlink in the sealed folder: {rel}")
        if p.is_dir():
            dirs.add(rel)
        elif p.is_file():
            present.add(rel)
        else:
            raise SealBroken(f"special file in the sealed folder: {rel}")
    extra = present - set(want) - {"SHA256SUMS"}
    missing = set(want) - present
    if extra or missing:
        raise SealBroken(f"{len(extra)} unlisted and {len(missing)} missing files")
    parents = {Path(n).parent.as_posix() for n in want} - {"."}
    parents |= {q.as_posix() for n in parents for q in Path(n).parents if q.as_posix() != "."}
    if dirs - parents:
        raise SealBroken(f"{len(dirs - parents)} unlisted folders (e.g. __pycache__)")
    bad = [n for n, d in want.items() if sha256(root / n) != d]
    if bad:
        raise SealBroken(f"{len(bad)} files differ from SHA256SUMS")
    return len(want)


def load(root):
    """tasks_fresh from a verified folder, imported without writing bytecode into it."""
    root = Path(root)
    verify(root)
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("tasks_fresh", root / "tasks_fresh.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules["tasks_fresh"] = mod
    spec.loader.exec_module(mod)
    return mod


def server_script(root):
    return Path(root) / "web" / "server.py"


def main():
    if len(sys.argv) != 3 or sys.argv[1] != "verify":
        raise SystemExit("usage: fresh.py verify <dir>")
    try:
        n = verify(Path(sys.argv[2]).resolve())
    except (SealBroken, OSError) as e:
        print(f"fresh seal BROKEN: {e}", file=sys.stderr)
        raise SystemExit(1)
    print(f"fresh seal ok: {n} files")


if __name__ == "__main__":
    main()
