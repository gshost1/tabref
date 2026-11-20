"""Exercise the smoke command's EXIT cleanup with a truncated tar stream."""

import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


BENCH = Path(__file__).resolve().parents[1]


@unittest.skipUnless(sys.platform == "linux" and os.geteuid() != 0, "requires unprivileged Linux")
class TabbenchCleanup(unittest.TestCase):
    def test_interrupted_copy_removes_a_partly_extracted_read_only_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bench = root / "checkout" / "bench"
            for name in ("tools", "harness"):
                (bench / name).mkdir(parents=True)
            runner = bench / "tools/tabbench.sh"
            shutil.copyfile(BENCH / "tools/tabbench.sh", runner)

            sealed = root / "sealed"
            locked = sealed / "locked"
            locked.mkdir(parents=True)
            (locked / "first").write_text("copied before the archive ends")
            (locked / "large").write_bytes(b"x" * (1024 * 1024))
            sums = "".join(
                f"{hashlib.sha256(file.read_bytes()).hexdigest()}  locked/{file.name}\n"
                for file in sorted(locked.iterdir())
            ).encode()
            (sealed / "SHA256SUMS").write_bytes(sums)
            fresh = (BENCH / "harness/fresh.py").read_text()
            (bench / "harness/fresh.py").write_text(
                re.sub(r'^SEAL = ".*"$', f'SEAL = "{hashlib.sha256(sums).hexdigest()}"', fresh, flags=re.M)
            )
            locked.chmod(0o500)

            home = root / "tabbench"
            home.mkdir()
            bin_dir = root / "bin"
            bin_dir.mkdir()
            self.script(bin_dir / "pgrep", "exit 1\n")
            tar = shutil.which("tar")
            self.assertIsNotNone(tar)
            self.script(bin_dir / "tar", f"""if [[ $2 == "$TEST_SEALED" ]]; then
    set -o pipefail
    {tar} --sort=name "$@" | head -c 32768
    exit 1
fi
exec {tar} "$@"
""")
            # Run as a separate scratch home and record that extraction really
            # left a read-only directory before the EXIT trap starts.
            wrapper = bin_dir / "as-tabbench"
            self.script(wrapper, """shift 7
export HOME="$TEST_BENCH_HOME"
if [[ $# == 1 && $1 == true ]]; then exit 0; fi
if [[ $1 != bash || $2 != -c ]]; then exit 99; fi
case "${3-}" in
    'kill -9 -1') exit 0 ;;
    *'tar -C "$HOME/fresh.new"'*)
        "$@"
        status=$?
        if [[ -f "$HOME/fresh.new/locked/first" && ! -w "$HOME/fresh.new/locked" ]]; then
            touch "$HOME/partial-copy"
        fi
        exit "$status"
        ;;
    *'tar -C "$HOME/tabref.new"'*|*'rm -rf "$HOME/fresh" "$HOME/fresh.new"'*) exec "$@" ;;
    *) exit 99 ;;
esac
""")
            try:
                subprocess.run(["git", "init", "-q", bench.parent], check=True)
                subprocess.run(["git", "-C", bench.parent, "add", "bench"], check=True)
                result = subprocess.run(
                    ["bash", runner, "smoke", "--taskset", "fresh", "--fresh-dir", sealed],
                    stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=15,
                    env={
                        **os.environ,
                        "PATH": f"{bin_dir}:{os.environ['PATH']}",
                        "TABBENCH_AS": str(wrapper),
                        "TABBENCH_LOCK": str(root / "lock"),
                        "TABREF_BIN": str(root / "no-binary"),
                        "TEST_BENCH_HOME": str(home),
                        "TEST_SEALED": str(sealed),
                    },
                )
                output = result.stdout + result.stderr
                self.assertNotEqual(result.returncode, 0, output)
                self.assertTrue((home / "partial-copy").is_file(), output)
                self.assertFalse((home / "fresh.new").exists(), output)
                self.assertFalse((home / "fresh").exists(), output)
                self.assertEqual(locked.stat().st_mode & 0o777, 0o500)
            finally:
                for directory in (locked, home / "fresh.new/locked"):
                    if directory.exists():
                        directory.chmod(0o700)

    @staticmethod
    def script(path, body):
        path.write_text("#!/bin/bash\n" + body)
        path.chmod(0o700)
