#!/usr/bin/env python3
"""Bench report, Markdown on stdout.

  models   model runs: the old held-out table (heldout/report.py, unchanged: sum of per-task medians of CLI/tool s,
           wall s, tokens, turns, and pass), then per-task min/median/max. Reads $BENCH_RUNS/results-web.jsonl and,
           with --baseline, the Linux Sonnet 5.5 pass (results/linux-sonnet-5-5/, arms ad-14 bh pw ab) beside it.
  latency  per-command latency (latency/run.py): cold open and first snapshot, warm min/median/max ms, output tokens
  replay   model-free replay (replay/run.py): per arm and task the median tool s, output tokens and passes, the
           recorded run's numbers beside them, and which kinds of call carry the output tokens
  all      the three

Usage: python3 -I bench/report.py models [--baseline] [--model claude-sonnet-5-5] [--results f.jsonl ...]
       python3 -I bench/report.py latency [file.json]
       python3 -I bench/report.py replay [file.jsonl ...]
"""
import argparse
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "harness"))
sys.path.insert(0, str(HERE / "heldout"))
import core  # noqa: E402
import report as heldout_report  # noqa: E402  (heldout/report.py, unchanged)

BASELINE = HERE / "results" / "linux-sonnet-5-5" / "results-web.jsonl"
ARMS = ["ad-14", "bh", "pw", "ab", "tabref"]
TASKS = ["hw1", "hw2", "hw3", "hw4", "hw5", "hw6", "hw7", "hp1", "hp2", "hp3"]
OPS = ["open", "snapshot", "click", "fill", "text"]
heldout_report.ARM_ORDER = ARMS + [a for a in heldout_report.ARM_ORDER if a not in ARMS]


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def models(a):
    paths = a.results or [str(core.runs_dir() / "results-web.jsonl")]
    if a.baseline:
        paths.append(str(BASELINE))
    rows, skipped = heldout_report.load(paths, a.model, None)
    rows = [r for r in rows if r["arm"] in (a.arms.split(",") if a.arms else ARMS)]
    if not rows:
        return ["## Model runs", "", f"No valid rows in {paths}.", ""]
    lines = ["## Model runs", "", f"{len(rows)} valid runs ({skipped} invalid skipped); models: "
             f"{', '.join(sorted({r['model'] for r in rows}))}. Sources: {', '.join(paths)}.",
             "CLI s is tool time (the arm wrapper's log); tokens are input (uncached + cache read + cache write) plus "
             "output.", ""]
    lines += heldout_report.summary_table(rows, "web") + [""]
    lines += ["### web per task", ""] + heldout_report.task_tables(rows, "web")
    return lines


def latest(pattern):
    files = sorted((core.runs_dir()).glob(pattern), key=lambda p: p.stat().st_mtime)
    return files[-1] if files else None


def ms(x):
    return "–" if x is None else f"{x * 1000:.0f}"


def latency(a):
    path = Path(a.files[0]) if a.files else latest("latency/*.json")
    if not path:
        return ["## Per-command latency", "", "No latency results yet (python3 -I bench/latency/run.py).", ""]
    d = json.loads(path.read_text())
    lines = ["## Per-command latency", "",
             f"{path.name}: host {d['host']}, display {d['display']} (ad-14 always in a private Xvfb session), "
             f"{d['chrome']}, warm n={d['n']} after one discarded warm-up, cold n={d['cold_n']} fresh Chrome + tool "
             f"each, Chrome idle {d.get('settle_s', 0)} s before the first call; load average "
             f"{d.get('load1_start', 0):.1f} at start, {d.get('load1_end', 0):.1f} at end. "
             "Times in ms from the arm wrapper's log, as min / **median** / max. tok = median output chars/4.", ""]
    skipped = [r for r in d["rows"] if r.get("skipped")]
    for page in ("simple", "heavy"):
        rs = [r for r in d["rows"] if r.get("page") == page]
        if not rs:
            continue
        lines += [f"### {page} page", "", "| arm | cold open | cold 1st snapshot | " + " | ".join(f"warm {o}" for o in OPS) +
                  " | snapshot tok |", "|---|---|---|" + "---|" * len(OPS) + "---|"]
        for r in sorted(rs, key=lambda r: ARMS.index(r["arm"]) if r["arm"] in ARMS else 99):
            if r.get("error"):
                lines.append(f"| {r['arm']} | failed: {r['error'][:80]} |")
                continue
            cold = r["cold"]
            co = [c["open_s"] for c in cold if c["open_ok"]]
            cs = [c["snapshot_s"] for c in cold if c["snapshot_ok"]]
            cells = [f"{ms(min(co))} / **{ms(med(co))}** / {ms(max(co))}" if co else "–",
                     f"**{ms(med(cs))}**" if cs else "–"]
            for op in OPS:
                w = r["warm"][op]
                s = w["samples"]
                cell = f"{ms(min(s))} / **{ms(med(s))}** / {ms(max(s))}" if s else "fail"
                if w["fails"]:
                    cell += f" ({w['fails']} failed)"
                cells.append(cell)
            cells.append(str(r["warm"]["snapshot"]["tokens"]))
            lines.append(f"| {r['arm']} | " + " | ".join(cells) + " |")
        lines.append("")
    for r in skipped:
        lines.append(f"- skipped: {r['skipped']}")
    return lines + [""]


def load_replay(files):
    paths = [Path(f) for f in files] if files else sorted(core.runs_dir().glob("replay/*.jsonl"))
    rows = []
    for p in paths:
        for line in p.read_text().splitlines():
            if line.strip():
                rows.append(json.loads(line))
    return rows, paths


def replay(a):
    rows, paths = load_replay(a.files)
    rows = [r for r in rows if r.get("valid")]
    if not rows:
        return ["## Replay", "", "No replay rows yet (python3 -I bench/replay/run.py).", ""]
    by = defaultdict(list)
    for r in rows:
        by[(r["arm"], r["task"])].append(r)
    arms = [x for x in ARMS if any(k[0] == x for k in by)]
    lines = ["## Replay (no model)", "",
             f"{len(rows)} replayed runs from {', '.join(p.name for p in paths)}. ad-14/bh/pw/ab re-run the recorded Linux "
             "Sonnet 5.5 calls (answer = the model's recorded answer, state = this replay's site); tabref runs its "
             "scripted sequence (replay/tabref/). Medians over rounds. tool s = arm wrapper log; out tok = chars/4 of "
             "what the Bash tool shows. rec = the recorded run on the boxd VM.", "",
             "### Sum of per-task medians", "",
             "| arm | tool s | rec tool s | out tok | rec out tok | calls | pass | rec pass | tasks |",
             "|---|---|---|---|---|---|---|---|---|"]
    for arm in arms:
        t = [k[1] for k in by if k[0] == arm]
        rs = [by[(arm, x)] for x in t]
        s = lambda key: sum(med([r.get(key) for r in g]) or 0 for g in rs)  # noqa: E731
        passes = sum(r["success"] for g in rs for r in g)
        rec_pass = sum(bool(r.get("rec_success")) for g in rs for r in g if "rec_success" in r)
        n = sum(len(g) for g in rs)
        rec_cell = f"{rec_pass}/{n}" if arm != "tabref" else "–"
        lines.append(f"| {arm} | {s('tool_s'):.1f} | {s('rec_tool_s'):.1f} | {s('out_tokens') / 1000:.1f}k | "
                     f"{s('rec_out_tokens') / 1000:.1f}k | {s('tool_calls'):.0f} | {passes}/{n} | {rec_cell} | {len(t)}/10 |")
    lines += ["", "### Per task: tool s / out tok / pass (median over rounds)", "",
              "| task | " + " | ".join(arms) + " |", "|---|" + "---|" * len(arms)]
    for task in TASKS:
        cells = []
        for arm in arms:
            g = by.get((arm, task))
            if not g:
                cells.append("–")
                continue
            cells.append(f"{med([r['tool_s'] for r in g]):.2f} s / {med([r['out_tokens'] for r in g]):.0f} / "
                         f"{sum(r['success'] for r in g)}/{len(g)}")
        lines.append(f"| {task} | " + " | ".join(cells) + " |")
    lines += ["", "### Where the output tokens go (share of all replayed output tokens, by call kind)", ""]
    for arm in arms:
        kinds = defaultdict(int)
        for r in rows:
            if r["arm"] == arm:
                for st in r.get("steps", []):
                    kinds[st["kind"]] += st["chars"]
        total = sum(kinds.values()) or 1
        top = sorted(kinds.items(), key=lambda kv: -kv[1])[:6]
        lines.append(f"- {arm}: " + ", ".join(f"{k} {v / total:.0%}" for k, v in top))
    div = [r for r in rows if r.get("diverged_calls")]
    flips = [r for r in rows if "rec_success" in r and bool(r["rec_success"]) != bool(r["success"])]
    lines += ["", f"- replays whose call results differ from the recording (a call failed here but not there, or the "
              f"reverse): {len(div)} of {len(rows)}",
              f"- verdict differs from the recorded run: {len(flips)}" +
              (": " + ", ".join(f"{r['arm']} {r['task']} r{r['round']} ({'pass' if r['success'] else 'fail'} here)"
                                for r in flips[:12]) if flips else "")]
    errs = [r for r in rows if r.get("error")]
    for r in errs[:12]:
        lines.append(f"- {r['arm']} {r['task']} r{r['round']}: {r['error'][:160]}")
    return lines + [""]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("what", choices=["models", "latency", "replay", "all"])
    ap.add_argument("files", nargs="*")
    ap.add_argument("--results", nargs="*")
    ap.add_argument("--baseline", action="store_true")
    ap.add_argument("--model", default=None)
    ap.add_argument("--arms", default=None)
    a = ap.parse_args()
    out = []
    if a.what in ("models", "all"):
        out += models(a)
    if a.what in ("latency", "all"):
        out += latency(argparse.Namespace(files=a.files if a.what == "latency" else []))
    if a.what in ("replay", "all"):
        out += replay(argparse.Namespace(files=a.files if a.what == "replay" else []))
    print("\n".join(out))


if __name__ == "__main__":
    main()
