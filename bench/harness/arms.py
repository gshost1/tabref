"""Web arms: the Linux run's frozen prompt code (heldout/linux/common_linux.py and common_ab_linux.py, byte-identical,
sha256 checked by core.check_frozen) for ad-14, bh, pw and ab, plus the tabref arm.

Only the install folder changes (common_linux.SKILLS is pointed at $BENCH_TOOLS/skills, which shows up in pw's
`cat .../SKILL.md` line); every other word of the four prompts is the frozen text. The tabref paragraph follows
ab's: the harness launched Chrome, the arm is told the DevTools port, and the model reads the tool's own guide.
"""
import sys
from pathlib import Path

from core import HELDOUT, tools_dir

sys.path.insert(0, str(HELDOUT / "linux"))
import common_ab_linux as _ab  # noqa: E402
import common_linux as _c  # noqa: E402

_c.TOOLS = tools_dir()
_c.SKILLS = _c.TOOLS / "skills"

WEB_COMMON = _c.WEB_COMMON
ARMS = ["ad-14", "bh", "pw", "ab", "tabref"]
WEB_ARMS = {**{a: _ab.WEB_ARMS[a] for a in ("ad-14", "bh", "pw", "ab")}, "tabref": "cdp"}
WEB_TOOL = {**{a: _ab.WEB_TOOL[a] for a in ("ad-14", "bh", "pw", "ab")}, "tabref": "tabref"}
ARM_VERSIONS = {**{a: _ab.ARM_VERSIONS[a] for a in ("ad-14", "bh", "pw", "ab")}}
CDP_ARMS = ("bh", "pw", "ab", "tabref")


def arm_version(arm, tabref_version=None):
    if arm == "tabref":
        return f"tabref {tabref_version or 'unknown'}"
    return ARM_VERSIONS[arm]


PROMPT_VERSION = "chained-2026-11-15"
FROZEN_PROMPT_VERSION = "frozen-2026-11-10"
START_LINE = " Read the guide and open the first page in one call, for example: "
START_COMMANDS = {
    "tabref": "`tabref skill && tabref open <url>`",
    "ab": "`agent-browser skills get core && agent-browser --cdp {port} open <url>`",
    "bh": "`browser-harness skill && browser-harness <<'PY'\nnew_tab(\"<url>\"); wait_for_load(); print(page_info())\nPY`",
    "pw": ("`cat {skills}/playwright-cli/SKILL.md && playwright-cli attach --cdp=http://127.0.0.1:{port} "
           "&& playwright-cli goto <url>`"),
    "ad-14": "`agent-desktop skills get web && agent-desktop open-url <url> --app \"Google Chrome\"`",
}


def start_line(arm, cdp_port):
    """The same kind of line for every arm (decided 2026-11-15): the tool's own guide read chained with its first
    open, so no arm pays a separate turn for its guide because only some prompts showed a command shape."""
    command = START_COMMANDS[arm].replace("{port}", str(cdp_port)).replace("{skills}", str(skills_dir()))
    return START_LINE + command + "."


def frozen_system_prompt(arm, cdp_port):
    if arm == "tabref":
        return ("You operate Google Chrome only through the `tabref` CLI (on PATH), which drives Chrome over CDP. "
                f"Chrome is already running with a DevTools endpoint on port {cdp_port}, and tabref is already pointed "
                f"at it (TABREF_CDP={cdp_port} is set; `--cdp {cdp_port}` does the same). Learn it with `tabref skill` "
                "before you start." + WEB_COMMON)
    return _ab.web_system_prompt(arm, cdp_port)


def web_system_prompt(arm, cdp_port):
    return frozen_system_prompt(arm, cdp_port) + start_line(arm, cdp_port)


def web_allowed_tools(arm):
    base = ["Bash(jq:*)", "Bash(grep:*)", "Bash(head:*)", "Bash(tail:*)", "Bash(sleep:*)", "Bash(wc:*)"]
    return base + ["Bash(cat:*)", f"Bash({WEB_TOOL[arm]}:*)"]


def web_extra_args(arm):
    """pw's guide is a file outside the run's folder. Claude Code refuses `cat` there even with Bash(cat:*) allowed
    ("requires approval"), so every pw run of the earlier passes worked without its guide. Adding the skills folder
    as a working directory lets it read the guide the prompt names; the other arms print theirs from their CLI."""
    return ["--add-dir", str(skills_dir())] if arm == "pw" else []


def skills_dir():
    return Path(_c.SKILLS)
