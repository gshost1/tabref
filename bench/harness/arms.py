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


def web_system_prompt(arm, cdp_port):
    if arm == "tabref":
        return ("You operate Google Chrome only through the `tabref` CLI (on PATH), which drives Chrome over CDP. "
                f"Chrome is already running with a DevTools endpoint on port {cdp_port}, and tabref is already pointed "
                f"at it (TABREF_CDP={cdp_port} is set; `--cdp {cdp_port}` does the same). Learn it with `tabref skill` "
                "before you start." + WEB_COMMON)
    return _ab.web_system_prompt(arm, cdp_port)


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
