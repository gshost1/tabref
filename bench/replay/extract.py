#!/usr/bin/env python3
"""Extract the recorded tool-call sequence of every Linux held-out web run (Sonnet 5.5 pass, 2026-11-11) into
bench/replay/recorded/<arm>.jsonl, one line per run, for replay.py.

Per run: arm, task, round, seed, the recorded verdict, answer and tool time, and every Bash call the model made in
order: the command, whether Claude Code refused it before it ran (those are not replayed), the size of what the model
saw, and the ids the output carried that a replay must map to its own (agent-desktop snapshot ids, playwright-cli
snapshot file names, the run's DevTools port).

Usage: python3 -I bench/replay/extract.py [--src <linux run results/claude-sonnet-5-5>] [--arms ad-14,bh,pw,ab]
"""
import argparse
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_SRC = Path.home() / ".t3/scratch/2026-11-11-you-own-a-linux-run-9e607cbc/linux-bench/results/claude-sonnet-5-5"
OUT = HERE / "recorded"
AD_SNAP = re.compile(r"@?(s[0-9a-z]{10,16})(?=:e\d)|\"snapshot_id\":\s*\"(s[0-9a-z]{10,16})\"")
PW_FILE = re.compile(r"\.playwright-cli/((?:page|console|network)-[0-9TZ:.-]+\.(?:yml|log|txt))")
PORT = re.compile(r"--cdp[= ](?:http://127\.0\.0\.1:)?(\d{4,5})|127\.0\.0\.1:(\d{4,5})")


LABELLED = [re.compile(r'^\s*-\s*([A-Za-z]+)(?:\s+"((?:[^"\\]|\\.)*)")?[^\n]*?\bref=([a-z0-9]+)', re.M),
            re.compile(r'(@s[0-9a-z]{10,16}:e\d+) (\w+)(?: \\"((?:[^"\\]|\\[^"])*)\\")?')]


def ref_labels(text):
    """[ref, key] for every ref an output shows next to a role and name (agent-browser and playwright-cli tree lines,
    agent-desktop --lines). key = role|name|n, n counting repeats of that role and name within the output, so a
    replay can find the same element under the ref its own run gave it."""
    out, seen = [], {}
    for m in LABELLED[0].finditer(text):
        role, name, ref = m.group(1), m.group(2) or "", m.group(3)
        out.append((ref, role, name))
    for m in LABELLED[1].finditer(text):
        out.append((m.group(1), m.group(2), m.group(3) or ""))
    keyed = []
    for ref, role, name in out:
        base = f"{role}|{name}"
        seen[base] = seen.get(base, 0) + 1
        keyed.append([ref, f"{base}|{seen[base]}"])
    return keyed


def result_text(content):
    return content if isinstance(content, str) else "".join(x.get("text", "") for x in content or [] if isinstance(x, dict))


def ordered(pattern, text):
    seen = []
    for m in pattern.finditer(text):
        v = next(g for g in m.groups() if g)
        if v not in seen:
            seen.append(v)
    return seen


def parse_transcript(path):
    steps, pending, final = [], {}, {}
    for line in path.read_text(errors="replace").splitlines():
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        if ev.get("type") == "assistant":
            for c in ev.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    pending[c["id"]] = len(steps)
                    steps.append({"cmd": c.get("input", {}).get("command", ""), "tool": c.get("name"),
                                  "background": bool(c.get("input", {}).get("run_in_background"))})
        elif ev.get("type") == "user" and isinstance(ev.get("message", {}).get("content"), list):
            for c in ev["message"]["content"]:
                if c.get("type") != "tool_result" or c.get("tool_use_id") not in pending:
                    continue
                step = steps[pending[c["tool_use_id"]]]
                text = result_text(c.get("content"))
                err = bool(c.get("is_error"))
                step.update({"rec_chars": len(text), "rec_error": err,
                             "denied": err and not text.startswith("Exit code"),
                             "ad_snaps": ordered(AD_SNAP, text), "pw_files": ordered(PW_FILE, text),
                             "refs": ref_labels(text)})
        elif ev.get("type") == "result":
            final = ev
    return steps, final


def run_rows(src, arms):
    rows = {}
    for f in sorted(src.glob("*/r*-web-*/ho-runs/results-web.jsonl")):
        for line in f.read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                if r.get("arm") in arms and r.get("valid") is not False:
                    rows[(r["arm"], r["task"], r["round"])] = (r, f.parent)
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", default=str(DEFAULT_SRC))
    ap.add_argument("--arms", default="ad-14,bh,pw,ab")
    a = ap.parse_args()
    arms = a.arms.split(",")
    OUT.mkdir(parents=True, exist_ok=True)
    by_arm = {arm: [] for arm in arms}
    for (arm, task, rnd), (row, runs) in sorted(run_rows(Path(a.src), arms).items()):
        run_id = f"{task}-{row['channel']}-{rnd}"
        raw = runs / "web" / f"{arm}-{row['model']}" / "raw" / f"{run_id}.jsonl"
        if not raw.exists():
            continue
        steps, final = parse_transcript(raw)
        cmds = "\n".join(s["cmd"] for s in steps)
        port = next((p for m in PORT.finditer(cmds) for p in m.groups() if p and p != "8792"), None)
        by_arm[arm].append({
            "arm": arm, "task": task, "round": rnd, "seed": row["seed"], "channel": row["channel"],
            "model": row["model"], "rec_success": row["success"], "rec_verify": row.get("verify", "")[:300],
            "rec_cli_s": row.get("cli_s"), "rec_cli_calls": row.get("cli_calls"), "rec_wall_s": row.get("wall_s"),
            "rec_turns": row.get("num_turns"), "rec_tokens": (row.get("input_total") or 0) + (row.get("output_tokens") or 0),
            "rec_work": f"/var/tmp/ho-runs/web/{arm}-{row['model']}/work/{run_id}", "rec_port": port,
            "answer": final.get("result", "") or "", "steps": steps})
    for arm, runs in by_arm.items():
        (OUT / f"{arm}.jsonl").write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in runs))
        print(f"{arm}: {len(runs)} runs, {sum(len(r['steps']) for r in runs)} calls "
              f"({sum(s.get('denied', False) for r in runs for s in r['steps'])} refused by Claude Code, not replayed)")


if __name__ == "__main__":
    main()
