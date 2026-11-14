"""Addendum arm `ab` (agent-browser 0.42.0) on top of the frozen common_linux.py, which stays unchanged.

Prompt: the browser bench's `cdp` channel text (feat/unified-browser run.py channel_prompt), with the Linux
WEB_COMMON line the other web arms use. Every other arm is passed through untouched."""
import common_linux as _c
from common_linux import *  # noqa: F401,F403

WEB_ARMS = {**_c.WEB_ARMS, "ab": "cdp"}
WEB_TOOL = {**_c.WEB_TOOL, "ab": "agent-browser"}
ARM_VERSIONS = {**_c.ARM_VERSIONS,
                "ab": "agent-browser 0.42.0 (vercel-labs/agent-browser, npm)"}


def web_system_prompt(arm, cdp_port):
    if arm == "ab":
        return ("You operate Google Chrome only through the `agent-browser` CLI (on PATH), which drives Chrome over CDP. "
                f"Chrome is already running with a DevTools endpoint on port {cdp_port}; pass `--cdp {cdp_port}` on every "
                "agent-browser command. Learn it with `agent-browser skills get core` before you start." + _c.WEB_COMMON)
    return _c.web_system_prompt(arm, cdp_port)


def web_allowed_tools(arm):
    base = ["Bash(jq:*)", "Bash(grep:*)", "Bash(head:*)", "Bash(tail:*)", "Bash(sleep:*)", "Bash(wc:*)"]
    return base + ["Bash(cat:*)", f"Bash({WEB_TOOL[arm]}:*)"]
