#!/usr/bin/env python3
"""Bench report, Markdown on stdout.

  models   model runs, scored by heldout/report.py (fork 7faca3235), which cannot reward failure: ranking by pass
           rate, then per-success wall time (a run's cost over the passes, so a failure only adds cost); per-task
           pass median / all median / p90 / max kept apart; losses against arms at least as reliable. The v1 sum of
           all-run medians follows as a secondary view, to compare with past reports (heldout/report_v1.py is the
           old script). Reads $BENCH_RUNS/results-web.jsonl and, with --baseline, the Linux Sonnet 5.5 pass
           (results/linux-sonnet-5-5/, arms ad-14 bh pw ab) beside it; the host column says where each arm ran.
           One task set at a time (rows carry `taskset`; rows without one are heldout): heldout by
           default; --taskset fresh prints only the aggregate ranking (no per-task tables, no
           verifier text). Ranking: pass rate, then USD per success (the control rule).
  latency  per-command latency (latency/run.py): cold open and first snapshot, warm min/median/max ms, output tokens
  replay   model-free replay (replay/run.py): per arm and task the median tool s, output tokens and passes, the
           recorded run's numbers beside them, and which kinds of call carry the output tokens
  all      the three

Usage: python3 -I bench/report.py models [--baseline] [--model claude-sonnet-5-5] [--taskset heldout|fresh]
                                          [--results f.jsonl ...]
       python3 -I bench/report.py latency [file.json]
       python3 -I bench/report.py replay [file.jsonl ...]
"""
import argparse
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "harness"))
sys.path.insert(0, str(HERE / "heldout"))
import core  # noqa: E402
import report as heldout_report  # noqa: E402  (heldout/report.py = fork 7faca3235, unchanged)

BASELINE = HERE / "results" / "linux-sonnet-5-5" / "results-web.jsonl"
ARMS = ["ad-14", "bh", "pw", "ab", "tabref"]
TASKS = ["hw1", "hw2", "hw3", "hw4", "hw5", "hw6", "hw7", "hp1", "hp2", "hp3"]
TASKSETS = ["heldout", "fresh"]
OPS = ["open", "snapshot", "click", "fill", "text"]
heldout_report.ARM_ORDER = ARMS + [a for a in heldout_report.ARM_ORDER if a not in ARMS]


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def rel(path):
    try:
        return str(Path(path).resolve().relative_to(HERE.parent))
    except ValueError:
        return Path(path).name


def taskset_of(r):
    return r.get("taskset") or "heldout"


def row_cost(r):
    """USD a run cost: Claude Code's total_cost_usd, else what the runner charged for a capped run."""
    c = r.get("cost_usd_list")
    return c if c is not None else r.get("cost_charged_estimate")


def load_by_taskset(paths, model, want):
    """Valid rows per task set (rows without one are heldout), normalized as heldout/report.py does plus the run's
    cost, with the hosts per arm and the invalid counts. Fresh rows never carry verifier text (the runner seals it);
    `why` is blanked here as well, so no report prints any."""
    rows, hosts, skipped = defaultdict(list), defaultdict(lambda: defaultdict(set)), defaultdict(int)
    for path in paths:
        if not Path(path).exists():
            continue
        for line in Path(path).read_text().splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            if model and r.get("model") != model or r.get("arm") not in want:
                continue
            ts = taskset_of(r)
            if r.get("valid") is False:
                skipped[ts] += 1
                continue
            n = heldout_report.normalize(r)
            n["cost"] = row_cost(r)
            if ts != "heldout":
                n["why"] = "sealed"
            rows[ts].append(n)
            hosts[ts][r.get("arm")].add(r.get("host", "mac"))
    return rows, hosts, skipped


def cost_per_success(rows, arm):
    """USD per passed run: every valid run's cost over the arm's passes (inf without a pass, None without costs)."""
    rs = [r for r in rows if r["arm"] == arm]
    costs = [r["cost"] for r in rs if r.get("cost") is not None]
    if not costs:
        return None
    passes = sum(r["ok"] for r in rs)
    return sum(costs) / passes if passes else math.inf


def ranked_by_cost(rows):
    """heldout/report.py's ranking, re-ordered by the control rule: pass rate first, then cost per success (USD per
    passed run); wall per success stays in the table."""
    ranked, common, tasks = heldout_report.ranking(rows, "web")
    for x in ranked:
        x["usd"] = cost_per_success(rows, x["arm"])
    ranked.sort(key=lambda x: (-x["passes"] / max(x["runs"], 1), math.inf if x["usd"] is None else x["usd"],
                               x["per"]["wall"]))
    return ranked, common, tasks


def usd(x):
    return "–" if x is None else "∞" if x == math.inf else f"{x:.3f}"


def models(a):
    paths = a.results or [str(core.runs_dir() / "results-web.jsonl")]
    if a.baseline:
        paths.append(str(BASELINE))
    want = set(a.arms.split(",")) if a.arms else set(ARMS)
    by_set, hosts, skipped = load_by_taskset(paths, a.model, want)
    ts = getattr(a, "taskset", None) or "heldout"
    if not by_set.get(ts):
        return [f"## Model runs: {ts} tasks", "", f"No valid {ts} rows in {paths}.", ""]
    return models_section(ts, by_set[ts], hosts[ts], skipped[ts], paths)


def models_section(ts, rows, hosts, skipped, paths):
    ranked, common, tasks = ranked_by_cost(rows)
    lines = [f"## Model runs: {ts} tasks", "", f"{len(rows)} valid runs ({skipped} invalid skipped); models: "
             f"{', '.join(sorted({r['model'] for r in rows}))}. Sources: {', '.join(rel(x) for x in paths)}.",
             "CLI s is tool time (the arm wrapper's log); tokens are input (uncached + cache read + cache write) plus "
             "output. USD is Claude Code's total_cost_usd (a capped run: the runner's charge). A run that hit the cap "
             "is a valid failure.", ""]
    if ts == "fresh":
        lines += ["Sealed set: the aggregate ranking only (no per-task tables, no verifier text or answers).", ""]
    lines += [f"### web ({ts}): ranking (cannot reward failing)", "",
              "Order: pass rate, then cost per success (USD over every valid run / passes). The other per-success "
              f"sums cover the {len(common)}/{len(tasks)} tasks where every arm passes at least once.", "",
              "| rank | arm | host | pass | rate | tasks with 0 passes | USD / success | CLI s / success | "
              "wall s / success | tokens / success | turns / success |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    for i, x in enumerate(ranked, 1):
        per = " | ".join(heldout_report.fmt(x["per"][m], m) for m, _ in heldout_report.METRICS)
        lines.append(f"| {i} | {x['arm']} | {', '.join(sorted(hosts.get(x['arm'], {'?'})))} | {x['passes']}/{x['runs']} | "
                     f"{x['passes'] / max(x['runs'], 1):.0%} | {x['zero_tasks']} | {usd(x['usd'])} | {per} |")
    if ts != "heldout":
        return lines + [""]
    rest = heldout_report.summary_tables(rows, "web")
    second = next(i for i, line in enumerate(rest) if i > 0 and line.startswith("### "))
    lines += [""] + [line.replace("### web:", f"### web ({ts}):", 1) for line in rest[second:]] + [""]
    focus = "tabref" if any(r["arm"] == "tabref" for r in rows) else "ad-14"
    lines += [f"### Where {focus} loses on {ts} (pass rate, then pass medians against arms at least as reliable)", ""]
    lines += heldout_report.losses(rows, focus) + [""]
    lines += [f"### web ({ts}) per task", ""] + heldout_report.task_tables(rows, "web")
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


def replay_kind(arm, cmd):
    import importlib.util
    if "mod" not in _REPLAY:
        spec = importlib.util.spec_from_file_location("replay_run", HERE / "replay" / "run.py")
        _REPLAY["mod"] = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(_REPLAY["mod"])
    return _REPLAY["mod"].kind(arm, cmd)


_REPLAY = {}


def recorded_cmds():
    """(arm, task, round) -> the replayed commands in order (refused ones dropped), from replay/recorded/."""
    out = {}
    for f in (HERE / "replay" / "recorded").glob("*.jsonl"):
        for line in f.read_text().splitlines():
            if line.strip():
                rec = json.loads(line)
                out[(rec["arm"], rec["task"], rec["round"])] = [s["cmd"] for s in rec["steps"]
                                                                if not s.get("denied") and s.get("tool") == "Bash"]
    return out


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
             "### Recorded runs replayed: sum of per-task medians", "",
             "| arm | tool s | rec tool s | out tok | rec out tok | calls | pass | rec pass | tasks |",
             "|---|---|---|---|---|---|---|---|---|"]
    sums = {}
    for arm in arms:
        t = [k[1] for k in by if k[0] == arm]
        rs = [by[(arm, x)] for x in t]
        sums[arm] = lambda key, rs=rs: sum(med([r.get(key) for r in g]) or 0 for g in rs)  # noqa: E731
        n = sum(len(g) for g in rs)
        passes = sum(r["success"] for g in rs for r in g)
        if arm == "tabref":
            continue
        s = sums[arm]
        rec_pass = sum(bool(r.get("rec_success")) for g in rs for r in g)
        lines.append(f"| {arm} | {s('tool_s'):.1f} | {s('rec_tool_s'):.1f} | {s('out_tokens') / 1000:.1f}k | "
                     f"{s('rec_out_tokens') / 1000:.1f}k | {s('tool_calls'):.0f} | {passes}/{n} | {rec_pass}/{n} | {len(t)}/10 |")
    if "tabref" in arms:
        t = [k[1] for k in by if k[0] == "tabref"]
        rs = [by[("tabref", x)] for x in t]
        s = sums["tabref"]
        n = sum(len(g) for g in rs)
        lines += ["", "### tabref, hand-written sequences (replay/tabref/; not a model's calls, so not comparable "
                  "row for row with the recorded arms)", "",
                  "| arm | tool s | out tok | calls | pass | tasks |", "|---|---|---|---|---|---|",
                  f"| tabref | {s('tool_s'):.1f} | {s('out_tokens') / 1000:.1f}k | {s('tool_calls'):.0f} | "
                  f"{sum(r['success'] for g in rs for r in g)}/{n} | {len(t)}/10 |", "",
                  "Per task (tool s / out tok / pass): " + "; ".join(
                      f"{task} {med([r['tool_s'] for r in by[('tabref', task)]]):.2f} s / "
                      f"{med([r['out_tokens'] for r in by[('tabref', task)]]):.0f} / "
                      f"{sum(r['success'] for r in by[('tabref', task)])}/{len(by[('tabref', task)])}"
                      for task in TASKS if ("tabref", task) in by)]
    recorded_arms = [x for x in arms if x != "tabref"]
    lines += ["", "### Recorded runs per task: tool s / out tok / pass (median over rounds)", "",
              "| task | " + " | ".join(recorded_arms) + " |", "|---|" + "---|" * len(recorded_arms)]
    for task in TASKS:
        cells = []
        for arm in recorded_arms:
            g = by.get((arm, task))
            if not g:
                cells.append("–")
                continue
            cells.append(f"{med([r['tool_s'] for r in g]):.2f} s / {med([r['out_tokens'] for r in g]):.0f} / "
                         f"{sum(r['success'] for r in g)}/{len(g)}")
        lines.append(f"| {task} | " + " | ".join(cells) + " |")
    lines += ["", "### Where the output tokens go (share of all replayed output tokens, by what the call printed)", ""]
    cmds = recorded_cmds()
    for arm in arms:
        kinds = defaultdict(int)
        for r in rows:
            if r["arm"] == arm:
                rec = cmds.get((arm, r["task"], r["round"]))
                for i, st in enumerate(r.get("steps", [])):
                    k = replay_kind(arm, rec[i]) if rec and r["mode"] == "recorded" and i < len(rec) else st["kind"]
                    kinds[k] += st["chars"]
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
    ap.add_argument("--taskset", default=None, choices=TASKSETS, help="models: which task set (default heldout; fresh shows the aggregate ranking only)")
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
