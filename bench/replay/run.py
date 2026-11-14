#!/usr/bin/env python3
"""Model-free replay of held-out web runs: tool time and output size for a fixed tool-call sequence, checked by the
task's own verifier (tasks_heldout.verify_web against the frozen site's recorded state).

recorded  (ad-14, bh, pw, ab) re-runs every Bash command the model ran in a Linux Sonnet 5.5 run (extract.py),
          in order, through the same arm wrapper, against the same seed. Commands Claude Code refused are skipped.
          Run-specific values are mapped to this replay's: the site URL, the DevTools port, the work folder, the
          guide folder, agent-desktop snapshot ids and playwright-cli snapshot file names (paired in order of
          appearance in the recorded and the replayed output). The answer is the model's recorded final answer.
scripted  (tabref) runs replay/tabref/<task>.py: a hand-written, minimal sequence of tabref calls whose run(t)
          returns the answer, computed from tabref's output only (a stand-in for the model's reading). Tuning
          tracks edit these files as the CLI changes; a missing binary skips the tabref rows.

Per run: tool s (the wrapper's log, as a model run counts it), calls, output tokens (chars/4 of what the Bash tool
would show the model: stdout + stderr, capped at Claude Code's 30000 chars), the recorded counterparts, and pass.

Usage: python3 -I bench/replay/run.py [--arms ad-14,bh,pw,ab,tabref] [--tasks hw1,...] [--rounds 0-4]
       [--display headless|xvfb|host] [--tabref-checkout <dir>]
Rows: $BENCH_RUNS/replay/<stamp>.jsonl; table: python3 -I bench/report.py replay
"""
import argparse
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "harness"))
import core  # noqa: E402
import arms  # noqa: E402

sys.path.insert(0, str(core.HELDOUT))
sys.path.insert(0, str(core.BENCH))
import tasks_heldout as th  # noqa: E402

sys.path.insert(0, str(HERE))
from extract import AD_SNAP, PW_FILE, ordered, ref_labels  # noqa: E402

ORDER = ["hw1", "hw2", "hw3", "hw4", "hw5", "hw6", "hw7", "hp1", "hp2", "hp3"]
TASKS = {t["id"]: t for t in th.WEB_TASKS}
MAX_CHARS = 30000
STEP_TIMEOUT = 120
REC_SKILLS = "/opt/bench/ho-tools/skills"
REC_RUNS = "/var/tmp/ho-runs"


def rounds(spec):
    out = []
    for part in spec.split(","):
        lo, _, hi = part.partition("-")
        out += list(range(int(lo), int(hi or lo) + 1))
    return out


REF_TOKEN = re.compile(r"@s[0-9a-z]{10,16}:e\d+|(?<![\w@./=:-])@?(?:f\d+)?e\d+\b")


def remap_refs(cmd, rec_label, new_ref):
    """Swap each recorded ref in a command for the ref this replay's output gave the same element (same role, name
    and repeat number). Refs whose element was never shown, or not shown again here, stay as recorded."""
    def swap(m):
        tok = m.group(0)
        at = tok.startswith("@") and not tok.startswith("@s")
        key = rec_label.get(tok[1:] if at else tok)
        new = new_ref.get(key) if key else None
        return ("@" + new if at and not new.startswith("@") else new) if new else tok
    return REF_TOKEN.sub(swap, cmd)


def kind(arm, cmd):
    """What a call was for, by the tool's first subcommand (for the token breakdown)."""
    tool = arms.WEB_TOOL[arm]
    if arm == "bh":
        if re.search(r"browser-harness\s+skill", cmd):
            return "guide"
        return "script"
    m = re.search(rf"{re.escape(tool)}((?:\s+--?[\w-]+(?:[= ](?!-)\S+)?)*)\s+([a-z][\w-]*)", cmd)
    if not m:
        return "other" if tool not in cmd else "tool"
    sub = m.group(2)
    if sub in ("skills", "skill"):
        return "guide"
    return sub


class Replay:
    def __init__(self, a, display, site, tabref):
        self.a, self.display, self.site, self.tabref = a, display, site, tabref
        self.runs = core.runs_dir() / "replay"
        self.home = self.runs / "home"
        self.rows_path = Path(a.out)
        self.rows_path.parent.mkdir(parents=True, exist_ok=True)

    def start_run(self, arm, tid, rnd):
        seed = th.web_seed(tid, rnd)
        exp = self.site.reset(seed)
        exp.update(th.web_expectations(tid))
        work = self.runs / "work" / f"{arm}-{tid}-{rnd}"
        shutil.rmtree(work, ignore_errors=True)
        work.mkdir(parents=True)
        (work / "downloads").mkdir()
        port = core.free_port() if arm in arms.CDP_ARMS else None
        env = core.arm_env(arm, work, port, self.display, self.home, self.tabref)
        if arm == "ad-14":
            chrome = core.launch_chrome_ad(env["AGENT_DESKTOP_HOME"], work / "profile", work / "downloads", self.display, env)
            port = core.ad_endpoint_port(env["AGENT_DESKTOP_HOME"])
            env["HO_CDP_PORT"] = str(port or "")
        else:
            chrome = core.launch_chrome(work / "profile", port, work / "downloads", self.display, env, self.a.chrome)
        time.sleep(self.a.settle)
        ctx = {"base": self.site.base, "work": str(work), "downloads": str(work / "downloads"), "exp": exp,
               "ad": str(core.ad_bin()), "cdp_port": port}
        return seed, exp, work, port, env, chrome, ctx

    def finish(self, arm, work, env, chrome):
        core.stop_chrome(chrome, work / "profile")
        core.stop_arm_daemons(arm, env)
        shutil.rmtree(work / "profile", ignore_errors=True)

    def bash(self, cmd, work, env):
        t0 = time.time()
        try:
            p = subprocess.run(["bash", "-c", cmd], cwd=work, env=env, capture_output=True, text=True, timeout=STEP_TIMEOUT,
                               stdin=subprocess.DEVNULL)
            rc, out = p.returncode, (p.stdout or "") + (p.stderr or "")
        except subprocess.TimeoutExpired as e:
            rc, out = 124, f"{e.stdout or ''}{e.stderr or ''}\nCommand timed out after {STEP_TIMEOUT} s"
            out = out if isinstance(out, str) else out.decode(errors="replace")
        if rc != 0:
            out = f"Exit code {rc}\n{out}"
        return rc, out[:MAX_CHARS], time.time() - t0

    def verify(self, tid, ctx, answer):
        try:
            return th.verify_web(TASKS[tid], ctx, answer, self.site.state())
        except Exception as e:
            return False, f"verify error {e}"

    def write(self, row):
        with open(self.rows_path, "a") as fh:
            fh.write(json.dumps(row) + "\n")
        print(f"{row['arm']:6} {row['task']} r{row['round']} {row['mode']:8} ok={row['success']!s:5} "
              f"tool={row.get('tool_s')}s calls={row.get('tool_calls')} out_tok={row.get('out_tokens')} "
              f"(rec {row.get('rec_tool_s')}s, {row.get('rec_out_tokens')} tok, ok={row.get('rec_success')}) "
              f"{(row.get('error') or row.get('verify') or '')[:120]}", flush=True)

    def recorded(self, rec):
        arm, tid, rnd = rec["arm"], rec["task"], rec["round"]
        row = {"arm": arm, "task": tid, "round": rnd, "mode": "recorded", "seed": rec["seed"], "display": self.display.mode,
               "host": os.uname().nodename, "load1": os.getloadavg()[0],
               "rec_success": rec["rec_success"], "rec_tool_s": rec["rec_cli_s"], "rec_tool_calls": rec["rec_cli_calls"],
               "rec_wall_s": rec["rec_wall_s"], "rec_turns": rec["rec_turns"], "rec_tokens": rec["rec_tokens"],
               "rec_out_tokens": sum(s.get("rec_chars", 0) for s in rec["steps"]) // 4}
        try:
            seed, exp, work, port, env, chrome, ctx = self.start_run(arm, tid, rnd)
        except Exception as e:
            self.write({**row, "success": False, "valid": False, "error": f"setup: {e}"})
            return
        maps = {}
        if rec.get("rec_port") and port:
            maps[rec["rec_port"]] = str(port)
        fixed = [("http://127.0.0.1:8792", self.site.base), ("127.0.0.1:8792", self.site.base.split("//")[1]),
                 (rec["rec_work"], str(work)), (REC_SKILLS, str(arms.skills_dir())), (REC_RUNS, str(core.runs_dir()))]
        steps, diverged, rec_label, new_ref = [], 0, {}, {}
        try:
            for s in rec["steps"]:
                if s.get("denied") or s.get("tool") != "Bash":
                    continue
                if steps:
                    time.sleep(self.a.gap)
                cmd = s["cmd"]
                for old, new in fixed:
                    cmd = cmd.replace(old, new)
                cmd = remap_refs(cmd, rec_label, new_ref)
                for old, new in maps.items():
                    if old == rec.get("rec_port"):
                        cmd = re.sub(rf"(?<=--cdp[= ]){old}\b|(?<=--cdp=http://127\.0\.0\.1:){old}\b|(?<=127\.0\.0\.1:){old}\b",
                                     new, cmd)
                    else:
                        cmd = cmd.replace(old, new)
                rc, out, secs = self.bash(cmd, work, env)
                for pat, key in ((AD_SNAP, "ad_snaps"), (PW_FILE, "pw_files")):
                    for old, new in zip(s.get(key) or [], ordered(pat, out)):
                        maps.setdefault(old, new)
                rec_label.update({ref: key for ref, key in s.get("refs") or []})
                new_ref.update({key: ref for ref, key in ref_labels(out)})
                if (rc != 0) != bool(s.get("rec_error")):
                    diverged += 1
                steps.append({"kind": kind(arm, s["cmd"]), "rc": rc, "chars": len(out), "rec_chars": s.get("rec_chars", 0),
                              "s": round(secs, 3)})
            ok, why = self.verify(tid, ctx, rec["answer"])
        finally:
            calls, secs = core.tool_log_totals(work / "tool.log")
            self.finish(arm, work, env, chrome)
        self.write({**row, "success": bool(ok), "verify": why, "valid": True, "tool_s": secs, "tool_calls": calls,
                    "bash_calls": len(steps), "out_tokens": sum(x["chars"] for x in steps) // 4,
                    "replay_wall_s": round(sum(x["s"] for x in steps), 2), "diverged_calls": diverged, "steps": steps})

    def scripted(self, tid, rnd):
        arm = "tabref"
        row = {"arm": arm, "task": tid, "round": rnd, "mode": "scripted", "seed": th.web_seed(tid, rnd),
               "display": self.display.mode, "host": os.uname().nodename, "load1": os.getloadavg()[0]}
        path = HERE / "tabref" / f"{tid}.py"
        spec = importlib.util.spec_from_file_location(f"tabref_{tid}", path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        try:
            seed, exp, work, port, env, chrome, ctx = self.start_run(arm, tid, rnd)
        except Exception as e:
            self.write({**row, "success": False, "valid": False, "error": f"setup: {e}"})
            return
        t = Tabref(self, work, env, ctx)
        answer, err = "", ""
        try:
            try:
                answer = mod.run(t) or ""
            except Exception as e:
                err = f"script: {type(e).__name__}: {e} @ {traceback.extract_tb(e.__traceback__)[-1].lineno}"
            ok, why = self.verify(tid, ctx, answer)
        finally:
            calls, secs = core.tool_log_totals(work / "tool.log")
            self.finish(arm, work, env, chrome)
        self.write({**row, "success": bool(ok) and not err, "verify": why, "valid": True, "error": err, "answer": answer[:300],
                    "tool_s": secs, "tool_calls": calls, "bash_calls": len(t.steps),
                    "out_tokens": sum(x["chars"] for x in t.steps) // 4, "replay_wall_s": round(sum(x["s"] for x in t.steps), 2),
                    "steps": t.steps})


class Tabref:
    """What a scripted sequence sees: t(*args) runs one `tabref ...` call through the arm wrapper and returns what
    the model would read (stdout + stderr, "Exit code N" first on failure); t.ref() picks a ref out of an output."""
    REF = re.compile(r"@[A-Za-z0-9]+(?::e\d+)?")

    def __init__(self, replay, work, env, ctx):
        self.replay, self.work, self.env, self.ctx = replay, work, env, ctx
        self.base, self.exp, self.steps, self.outs = ctx["base"], ctx["exp"], [], []

    def __call__(self, *args):
        import shlex
        cmd = "tabref " + " ".join(shlex.quote(str(x)) for x in args)
        rc, out, secs = self.replay.bash(cmd, self.work, self.env)
        self.steps.append({"kind": str(args[0]) if args else "tool", "rc": rc, "chars": len(out), "s": round(secs, 3),
                           "cmd": cmd[:300]})
        self.outs.append(out)
        keep = Path(self.work) / "outputs"
        keep.mkdir(exist_ok=True)
        (keep / f"{len(self.outs):02d}.txt").write_text(f"$ {cmd}\n{out}")
        if rc != 0 and not getattr(self, "tolerate", False):
            raise RuntimeError(f"{cmd[:120]} failed: {out[:300]}")
        return out

    def lines(self, out, after=None):
        lines = out.splitlines()
        if after:
            idx = next((i for i, x in enumerate(lines) if re.search(after, x, re.I)), None)
            if idx is None:
                raise LookupError(f"no line matching {after!r}")
            lines = lines[idx + 1:]
        return lines

    def ref(self, out, pattern, after=None, nth=1):
        """The first ref on the nth line that matches `pattern` (case-insensitive), optionally only after the first
        line matching `after`."""
        seen = 0
        for line in self.lines(out, after):
            if re.search(pattern, line, re.I):
                m = self.REF.search(line)
                if m:
                    seen += 1
                    if seen == nth:
                        return m.group(0)
        raise LookupError(f"no ref on a line matching {pattern!r}")

    def find_ref(self, out, pattern, after=None, nth=1):
        """t.ref() on `out` (an action's output), else on a fresh `tabref snap` (one more call)."""
        try:
            return self.ref(out, pattern, after, nth)
        except LookupError:
            return self.ref(self("snap"), pattern, after, nth)

    def has(self, out, pattern):
        return re.search(pattern, out, re.I) is not None

    def text(self):
        return "\n".join(self.outs)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arms", default="ad-14,bh,pw,ab,tabref")
    ap.add_argument("--tasks", default=",".join(ORDER))
    ap.add_argument("--rounds", default="0-4")
    ap.add_argument("--display", default="headless", choices=["headless", "xvfb", "host"])
    ap.add_argument("--chrome", default="/usr/bin/google-chrome")
    ap.add_argument("--tabref-checkout", default=None)
    ap.add_argument("--settle", type=float, default=5.0,
                    help="seconds Chrome idles after launch before the first call (a model's first call comes after "
                         "Claude Code's start and one model turn)")
    ap.add_argument("--gap", type=float, default=0.5, help="seconds between replayed calls (the model's turn)")
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    core.check_frozen()
    core.exit_on_sigterm()
    tabref = core.tabref_bin(a.tabref_checkout)
    want_tasks, want_rounds = a.tasks.split(","), set(rounds(a.rounds))
    displays = {}
    runs = core.runs_dir() / "replay"
    a.out = a.out or str(runs / f"{time.strftime('%Y%m%d-%H%M%S')}.jsonl")
    with core.Site(f"http://127.0.0.1:{core.free_port()}", runs / "server.log") as site:
        try:
            for arm in a.arms.split(","):
                why = core.arm_available(arm, tabref)
                if why:
                    print(f"SKIP {why}", flush=True)
                    continue
                mode = "xvfb" if arm == "ad-14" and a.display == "headless" else a.display
                if mode not in displays:
                    displays[mode] = core.Display(mode, runs / f"display-{mode}").start()
                r = Replay(a, displays[mode], site, tabref)
                if arm == "tabref":
                    for rnd in sorted(want_rounds):
                        for tid in want_tasks:
                            r.scripted(tid, rnd)
                    continue
                recs = [json.loads(x) for x in (HERE / "recorded" / f"{arm}.jsonl").read_text().splitlines() if x.strip()]
                for rec in recs:
                    if rec["task"] in want_tasks and rec["round"] in want_rounds:
                        r.recorded(rec)
        finally:
            for d in displays.values():
                d.stop()


if __name__ == "__main__":
    main()
