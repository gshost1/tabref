#!/usr/bin/env python3
"""The sealed fresh task set: 10 web tasks nobody tuning tabref has seen, kept outside the repo (never committed).

The folder (--fresh-dir or BENCH_FRESH_DIR) holds tasks_fresh.py (the tasks_heldout.py interface: WEB_TASKS,
web_seed, reset_payload, web_expectations, verify_web), web/server.py and a SHA256SUMS file. The seal is the
sha256 of SHA256SUMS; `verify` checks it, every listed file's hash, and that nothing else is in the folder (a
__pycache__ or an edited file breaks it). The seal is checked before every use and again after a pass.
Nothing here prints task content: callers report counts and file names only.

  python3 -I -B bench/harness/fresh.py verify <dir>     exit 0 when the seal holds, else 1 with the reason
  python3 -I -B bench/harness/fresh.py commit|discard <runs> <pass id>
                                                        staged rows of a pass into results-web.jsonl, or dropped
"""
import hashlib
import importlib.util
import json
import os
import re
import sys
import threading
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


def check_pass_id(pass_id):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,80}", pass_id or ""):
        raise SystemExit("bad pass id")
    return pass_id


class Vault:
    """One fresh pass's output. Rows (numbers and flags only) are staged as they come, outside results-web.jsonl.
    Everything with task text (answers, verifier reasons, transcripts, stderr, the server log, tracebacks) is held in
    memory while models run and written to <stage>/sealed/ only by flush(), after the last run, so no run can read
    an earlier one's. Staged rows reach results-web.jsonl only through commit(), once the seal has held after the
    pass; discard() drops them."""

    def __init__(self, runs, pass_id):
        self.runs = Path(runs)
        self.dir = stage_dir(runs, check_pass_id(pass_id))
        self.held, self.lock = {}, threading.Lock()

    def hold(self, rel, text):
        with self.lock:
            self.held[rel] = self.held.get(rel, "") + text

    def stage_row(self, row):
        self.dir.mkdir(parents=True, exist_ok=True)
        with open(self.dir / "rows.jsonl", "a") as fh:
            fh.write(json.dumps(row) + "\n")

    def flush(self):
        with self.lock:
            held, self.held = self.held, {}
        for rel, text in held.items():
            p = self.dir / "sealed" / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            with open(p, "a") as fh:
                fh.write(text)


SEALED_FIELDS = ("verify", "answer", "strays", "invalid_reason")


def seal_record(record, vault):
    """A fresh row: free text (answer, verifier reason, stray names, invalid reason) goes to the vault's detail; the
    row keeps numbers, flags, the stray count and the invalid reason's generic prefix."""
    detail = {k: record[k] for k in SEALED_FIELDS if k in record}
    detail.update({k: record.get(k) for k in ("pass_id", "task", "arm", "round", "seed", "success")})
    vault.hold("detail.jsonl", json.dumps(detail) + "\n")
    out = dict(record, verify="sealed")
    out.pop("answer", None)
    if "strays" in record:
        out["strays"] = len(record["strays"])
    if record.get("invalid_reason"):
        out["invalid_reason"] = str(record["invalid_reason"]).split(":", 1)[0]
    return out


def stage_dir(runs, pass_id):
    return Path(runs) / "fresh-staged" / pass_id


def commit(runs, pass_id):
    """Append a pass's staged rows to results-web.jsonl; returns how many."""
    rows = stage_dir(runs, check_pass_id(pass_id)) / "rows.jsonl"
    if not rows.exists():
        return 0
    lines = [x for x in rows.read_text().splitlines() if x.strip()]
    with open(Path(runs) / "results-web.jsonl", "a") as fh:
        fh.write("".join(x + "\n" for x in lines))
    rows.unlink()
    return len(lines)


def discard(runs, pass_id):
    rows = stage_dir(runs, check_pass_id(pass_id)) / "rows.jsonl"
    n = len([x for x in rows.read_text().splitlines() if x.strip()]) if rows.exists() else 0
    rows.unlink(missing_ok=True)
    return n


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "verify" and len(sys.argv) == 3:
        try:
            n = verify(Path(sys.argv[2]).resolve())
        except (SealBroken, OSError) as e:
            print(f"fresh seal BROKEN: {e}", file=sys.stderr)
            raise SystemExit(1)
        print(f"fresh seal ok: {n} files")
    elif cmd in ("commit", "discard") and len(sys.argv) == 4:
        n = (commit if cmd == "commit" else discard)(Path(sys.argv[2]), sys.argv[3])
        print(f"fresh rows {'committed' if cmd == 'commit' else 'discarded'}: {n}")
    else:
        raise SystemExit("usage: fresh.py verify <dir> | commit <runs> <pass id> | discard <runs> <pass id>")


if __name__ == "__main__":
    main()
