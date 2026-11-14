"""Linux stand-in for bench/tasks.py (gshost1/agent-desktop perf/speed-cost ae93c615), the four names
heldout/tasks_heldout.py imports from it. BENCH_DIR, FORBIDDEN and Task are copied verbatim; macctl is
macOS-only (AppKit helper) and the Linux port never calls it."""
from pathlib import Path

BENCH_DIR = Path.home() / "agent-desktop-bench"
FORBIDDEN = {"open", "osascript", "mkdir", "touch", "cp", "mv", "defaults", "sw_vers", "sysctl",
             "system_profiler", "textutil", "screencapture", "shortcuts", "bc", "python", "python3",
             "ls", "cat", "find", "stat", "mdfind", "rm", "say", "uname"}


class Task:
    key = ""
    app = ""

    def __init__(self, run_id, real_bin):
        self.run_id = run_id
        self.real_bin = real_bin

    def setup(self):
        pass

    def prompt(self):
        raise NotImplementedError

    def verify(self, answer):
        raise NotImplementedError

    def cleanup(self):
        pass


def macctl(*args, timeout=20):
    raise RuntimeError("macctl is macOS-only; the Linux port does not call it")
