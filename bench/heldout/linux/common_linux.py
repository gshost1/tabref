"""Arm definitions for the Linux run: heldout/arms/common.py with the Mac wording moved to Ubuntu GNOME.

Each arm is one tool at one pinned version, reached through /opt/bench/ho-bin/<arm>/<tool> (wrap.sh), which
logs every call's wall time. Every arm gets the same task prompt; only the tool paragraph differs."""
import re
from pathlib import Path

TOOLS = Path("/opt/bench/ho-tools")
SKILLS = TOOLS / "skills"
BIN = Path("/opt/bench/ho-bin")
VERIFY_AD = Path("/opt/arms/ad-1.4.0/agent-desktop")

TOOL_NAMES = {"agent-desktop", "peekaboo", "cua-driver", "browser-harness", "playwright-cli", "agent-browser"}

NATIVE_TOOL_TEXT = {
    "ad-up": ("You operate this Ubuntu (GNOME) desktop's GUI through the `agent-desktop` CLI (on PATH, accessibility enabled). "
              "Learn it with `agent-desktop skills get desktop` (and `agent-desktop skills get platform`) before you start."),
    "ad-14": ("You operate this Ubuntu (GNOME) desktop's GUI through the `agent-desktop` CLI (on PATH, accessibility enabled). "
              "Learn it with `agent-desktop skills get desktop` (and `agent-desktop skills get platform`) before you start."),
    "cua": ("You operate this Ubuntu (GNOME) desktop's GUI through the `cua-driver` CLI (on PATH; tools run as "
            "`cua-driver call <tool> '<json>'`). "
            f"Learn it from its skill before you start: `cat {SKILLS}/cua-driver/SKILL.md` (its other guides, such as "
            "LINUX.md and WORKFLOW.md, are in the same folder)."),
}
NATIVE_TOOL = {"ad-up": "agent-desktop", "ad-14": "agent-desktop", "cua": "cua-driver"}
NATIVE_GUIDE_DIR = {"cua": SKILLS / "cua-driver"}

WEB_ARMS = {"ad-up": "ax", "ad-14": "ad-web", "bh": "cdp", "pw": "cdp"}
WEB_TOOL = {"ad-up": "agent-desktop", "ad-14": "agent-desktop", "bh": "browser-harness", "pw": "playwright-cli"}

ARM_VERSIONS = {
    "ad-up": "agent-desktop upstream 8e8dd7cbe (built in the VM, x86_64-unknown-linux-gnu, Rust 1.89.0)",
    "ad-14": "agent-desktop 1.4.0 (fork release v1.4.0, x86_64-unknown-linux-gnu)",
    "cua": "Cua Driver 0.46.1 (trycua/cua cua-driver-rs-v0.46.1, linux-x86_64)",
    "bh": "browser-harness 0.1.13 (PyPI)",
    "pw": "@playwright/cli 0.1.24 (microsoft/playwright-cli)",
}


def native_system_prompt(arm):
    rules = ("Use Bash only to run the tool named above; piping its output through jq, grep or head is fine. Do not use "
             "any other program to do the task or to read the answer (no xdg-open, xdotool, gsettings, mkdir, ls, cat other "
             "than the tool's guide, python, or any other GUI tool). ")
    return NATIVE_TOOL_TEXT[arm] + " " + rules + "When the task is complete, reply with the final answer only."


TASK_PREFIX = "Task: "


def _segments(cmd):
    for segment in re.split(r"&&|\|\||[;|\n`]|\$\(|\(", cmd):
        words = [w for w in segment.strip().split() if not re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", w)]
        if words:
            yield words


def audit_native(cmd, arm, forbidden):
    """heldout/arms/common.py audit_native, unchanged apart from the Linux guide folders."""
    own = NATIVE_TOOL[arm]
    guide = NATIVE_GUIDE_DIR.get(arm)
    for words in _segments(cmd):
        prog = words[0].split("/")[-1]
        if prog == own:
            continue
        if prog == "cat":
            paths = [w.strip("'\"") for w in words[1:] if not w.startswith("-")]
            if guide and paths and all(str(Path(p).expanduser()).startswith(str(guide)) for p in paths):
                continue
            return "cat"
        if prog in TOOL_NAMES or prog in forbidden:
            return prog
    return None


# Web: the browser bench's channel_prompt (feat/unified-browser 4cb19985 run.py) and heldout/arms/run_web.py,
# with "this Mac" / macOS programs replaced by their Ubuntu counterparts.
WEB_COMMON = (" Google Chrome is already open. Do not use any other program (no curl, xdg-open, xdotool, python). "
              "When finished, reply with one line: DONE <answer if the task asks for one>.")


def web_system_prompt(arm, cdp_port):
    if arm == "bh":
        return ("You operate Google Chrome only through the `browser-harness` CLI (on PATH), which drives Chrome over "
                "CDP. Chrome is already running and browser-harness is already connected to it. Learn it with "
                "`browser-harness skill` before you start." + WEB_COMMON)
    if arm == "pw":
        return ("You operate Google Chrome only through the `playwright-cli` CLI (on PATH). Chrome is already running "
                f"with a DevTools endpoint at http://127.0.0.1:{cdp_port}; connect to it with "
                f"`playwright-cli attach --cdp=http://127.0.0.1:{cdp_port}` and do not open another browser. Learn it "
                f"from its skill before you start: `cat {SKILLS}/playwright-cli/SKILL.md` (its references are "
                "in the same folder)." + WEB_COMMON)
    if arm == "ad-up":
        return ("You operate this computer only through the `agent-desktop` CLI (on PATH). "
                "It reads the Linux accessibility tree (AT-SPI) and acts on refs. Learn it with `agent-desktop skills get desktop` "
                "(and `agent-desktop skills get platform`) before you start." + WEB_COMMON)
    return ("You operate this computer and its web pages only through the `agent-desktop` CLI (on PATH). "
            "For web pages, learn it with `agent-desktop skills get web` before you start; only if the task "
            "also needs a native app, read `agent-desktop skills get desktop` too. agent-desktop already launched "
            "Chrome with its DevTools endpoint, so do not launch it again." + WEB_COMMON)


def web_allowed_tools(arm):
    base = ["Bash(jq:*)", "Bash(grep:*)", "Bash(head:*)", "Bash(tail:*)", "Bash(sleep:*)", "Bash(wc:*)"]
    return base + ["Bash(cat:*)", f"Bash({WEB_TOOL[arm]}:*)"]
