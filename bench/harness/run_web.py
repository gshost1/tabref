#!/usr/bin/env python3
"""Held-out web pass with a model: the Linux run's run_web_ab_linux.py, made portable and given the tabref arm.

Unchanged from the Linux pass: the 10 web tasks, prompts, seeds (tasks_heldout.web_seed), local site
(heldout/web/server.py), verifiers (tasks_heldout.verify_web), the claude -p command line (--safe-mode, Bash only,
the same allow-list), the 600 s cap, a fresh Chrome profile per run, the stray-save and leftover-dialog checks,
per-round task rotation, and the row fields. Arms rotate per round too, like orchestrate.py's blocks.
Changed: paths (core.py), a clean environment and a bench-only HOME for claude and the tools, the display mode,
Claude Code 2.1.322 from $BENCH_TOOLS (override: BENCH_CLAUDE), and the key: ANTHROPIC_API_KEY must already be in
this process's environment; the runner never looks for one and never writes it anywhere.
Isolation: a model pass runs only on a VM or as the dedicated unprivileged user (core.isolation_problem;
bench/tools/tabbench.sh sets that user up on a laptop and hands it the key on stdin).
Spend: Claude Code's total_cost_usd per run; a run that hit the cap has no result event, so it is charged the
largest per-run cost seen so far in the pass (at least --budget/10). It is a valid failure, not an invalid row.

One command per pass (spend stops at --budget, from Claude Code's total_cost_usd per run):
  VM:     ANTHROPIC_API_KEY=... python3 -I bench/harness/run_web.py --arms tabref --rounds 0-4 --model claude-sonnet-5-5
  laptop: <key on stdin> | bench/tools/tabbench.sh run --arms tabref --rounds 0-4   (as the tabbench user)
  [--display xvfb|headless|host] [--tasks hw1,hp3] [--budget 5] [--tabref-checkout <dir>]
Rows go to $BENCH_RUNS/results-web.jsonl (default bench/runs/); report: python3 -I bench/report.py models.
"""
import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import core  # noqa: E402
import arms  # noqa: E402

sys.path.insert(0, str(core.HELDOUT))
sys.path.insert(0, str(core.BENCH))
import tasks_heldout as th  # noqa: E402

ORDER = ["hw1", "hw2", "hw3", "hw4", "hw5", "hw6", "hw7", "hp1", "hp2", "hp3"]
TASKS = {t["id"]: t for t in th.WEB_TASKS}
TIMEOUT_S = 600


def rot(n, items):
    k = n % len(items)
    return items[k:] + items[:k]


def rounds(spec):
    out = []
    for part in spec.split(","):
        lo, _, hi = part.partition("-")
        out += list(range(int(lo), int(hi or lo) + 1))
    return out


def version(cmd, env=None):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=20, env=env).stdout.strip().splitlines()[0]
    except (OSError, IndexError, subprocess.SubprocessError):
        return None


def bench_dialogs(display, env):
    """run_web_linux.bench_dialogs: Chrome dialogs on AT-SPI, read by agent-desktop 1.4.0 (needs a display). It uses
    its own agent-desktop home, so the check never touches the ad-14 arm's state."""
    if display.mode == "headless" or not core.ad_bin().exists():
        return []
    check_env = dict(env, **display.env, AGENT_DESKTOP_HOME=core.short_dir("dialog-check-ad-home"))
    check_env.pop("ANTHROPIC_API_KEY", None)
    try:
        out = subprocess.run([str(core.ad_bin()), "list-surfaces", "--app", "chrome"], capture_output=True, text=True,
                             timeout=15, env=check_env).stdout
        surfaces = json.loads(out).get("data", {}).get("surfaces", [])
    except Exception:
        return []
    return [s.get("title") or s.get("type") for s in surfaces if s.get("type") in ("alert", "sheet", "dialog")]


def collect_strays(since, work, dest):
    """browser bench strays.py: fresh files named ':*' in the run's work folder are moved out. The folders behind
    the work folder's ad-home and arm-home links are scanned too."""
    found = []
    for p in (x for d in core.real_dirs(work) for x in d.rglob(":*")):
        try:
            if p.is_file() and p.lstat().st_mtime >= since:
                found.append(p)
        except OSError:
            continue
    moved = []
    for p in found:
        Path(dest).mkdir(parents=True, exist_ok=True)
        shutil.move(str(p), str(Path(dest) / p.name))
        moved.append(f"{p.parent.name}/{p.name[-60:]}")
    return moved


def assistant_usage(lines):
    """Usage summed over the assistant messages of a stream (one count per message id): what a run that never
    reached its result event consumed."""
    seen, total = set(), {}
    for line in lines:
        try:
            ev = json.loads(line)
        except Exception:
            continue
        msg = ev.get("message") if isinstance(ev, dict) and ev.get("type") == "assistant" else None
        if not isinstance(msg, dict) or msg.get("id") in seen:
            continue
        seen.add(msg.get("id"))
        for k, v in (msg.get("usage") or {}).items():
            if isinstance(v, (int, float)):
                total[k] = total.get(k, 0) + v
    return total


def summarize_stream(lines):
    """browser bench run.py summarize_stream, unchanged."""
    turns, tool_calls, tool_chars, snap_chars, snap_calls = 0, 0, 0, 0, 0
    pending = {}
    final = {}
    for line in lines:
        try:
            ev = json.loads(line)
        except Exception:
            continue
        if not isinstance(ev, dict) or not isinstance(ev.get("message", {}), dict):
            continue
        if ev.get("type") == "assistant":
            turns += 1
            for c in ev.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    tool_calls += 1
                    pending[c["id"]] = c.get("input", {}).get("command", "")
        elif ev.get("type") == "user":
            for c in ev.get("message", {}).get("content", []) if isinstance(ev.get("message", {}).get("content"), list) else []:
                if c.get("type") == "tool_result":
                    content = c.get("content")
                    text = content if isinstance(content, str) else "".join(x.get("text", "") for x in content or [])
                    tool_chars += len(text)
                    cmd = pending.get(c.get("tool_use_id"), "")
                    if " snapshot" in cmd or " find" in cmd:
                        snap_chars += len(text)
                        snap_calls += 1
        elif ev.get("type") == "result":
            final = ev
    return {"assistant_messages": turns, "tool_calls": tool_calls, "tool_result_tokens_est": tool_chars // 4,
            "snapshot_tokens_est": snap_chars // 4, "snapshot_calls": snap_calls, "final": final}


class Runner:
    def __init__(self, a, display, site, key):
        self.a, self.display, self.site, self.key = a, display, site, key
        self.runs = Path(a.out)
        self.home = self.runs / "home"
        self.home.mkdir(parents=True, exist_ok=True)
        self.tabref = core.tabref_bin(a.tabref_checkout)
        self.spent, self.max_cost = 0.0, 0.0
        self.versions = {"claude": version([str(core.claude_bin()), "--version"]),
                         "chrome": version([a.chrome, "--version"]),
                         "tabref": version([str(self.tabref), "--version"]) if self.tabref.exists() else None}

    def write_row(self, record):
        with open(self.runs / "results-web.jsonl", "a") as fh:
            fh.write(json.dumps(record) + "\n")

    def run_one(self, arm, tid, rnd):
        a = self.a
        task = TASKS[tid]
        seed = th.web_seed(tid, rnd)
        channel = arms.WEB_ARMS[arm]
        base_row = {"suite": "web", "arm": arm, "arm_version": arms.arm_version(arm, self.versions["tabref"]), "task": tid,
                    "round": rnd, "model": a.model, "channel": channel, "seed": seed, "host": a.host,
                    "vm": core.host_label(), "display": self.display.mode, "claude_version": self.versions["claude"],
                    "chrome_version": self.versions["chrome"]}
        exp = self.site.reset(seed)
        try:
            exp.update(th.web_expectations(tid))
        except Exception as e:
            why = f"expected value unavailable: {e}"
            self.write_row({**base_row, "success": False, "verify": why, "valid": False, "invalid_reason": why})
            print(f"{tid} {arm} INVALID {why}", flush=True)
            return
        run_id = f"{tid}-{channel}-{rnd}"
        out = self.runs / "web" / f"{arm}-{a.model}"
        work = out / "work" / run_id
        shutil.rmtree(work, ignore_errors=True)
        downloads = work / "downloads"
        downloads.mkdir(parents=True)
        profile = work / "profile"
        cdp_port = core.free_port() if arm in arms.CDP_ARMS else None
        env = core.arm_env(arm, work, cdp_port, self.display, self.home, self.tabref)
        chrome = None
        try:
            chrome = core.launch_chrome_ad(env["AGENT_DESKTOP_HOME"], profile, downloads, self.display, env) if arm == "ad-14" else \
                core.launch_chrome(profile, cdp_port, downloads, self.display, env, a.chrome)
            time.sleep(1.0)
            dialogs = bench_dialogs(self.display, env)
            if dialogs:
                raise RuntimeError(f"dialog on screen after launch: {dialogs}")
        except RuntimeError as e:
            core.stop_chrome(chrome, profile)
            why = f"bench launch failed: {e}"
            self.write_row({**base_row, "success": False, "verify": why, "valid": False, "invalid_reason": why})
            print(f"{tid} {arm} INVALID {why}", flush=True)
            return
        if arm == "ad-14":
            cdp_port = core.ad_endpoint_port(env["AGENT_DESKTOP_HOME"])
            env["HO_CDP_PORT"] = str(cdp_port or "")
        if cdp_port:
            (self.runs / "web-current-port").write_text(str(cdp_port))
        ctx = {"base": a.base, "work": str(work), "downloads": str(downloads), "exp": exp, "ad": str(core.ad_bin()),
               "cdp_port": cdp_port}
        prompt = task["prompt"].format(**ctx)
        env.update({"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_AUTOUPDATER": "1"})
        ab_sock = None
        if arm == "ab":
            ab_sock = tempfile.mkdtemp(prefix="ab-sock-")
            env["AGENT_BROWSER_SOCKET_DIR"] = ab_sock
        cmd = [str(core.claude_bin()), "-p", prompt, "--safe-mode", "--tools", "Bash", "--allowedTools",
               *arms.web_allowed_tools(arm), "--append-system-prompt", arms.web_system_prompt(arm, cdp_port),
               "--model", a.model, "--output-format", "stream-json", "--verbose", "--no-session-persistence"]
        t0 = time.time()
        since = t0 - 1
        proc = subprocess.Popen(cmd, cwd=str(work), env=dict(env, ANTHROPIC_API_KEY=self.key), stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, text=True, start_new_session=True)
        try:
            stdout, stderr = proc.communicate(timeout=a.timeout)
            timed_out = False
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            stdout, stderr = proc.communicate()
            timed_out = True
        wall = time.time() - t0
        raw = out / "raw" / f"{run_id}.jsonl"
        raw.parent.mkdir(parents=True, exist_ok=True)
        raw.write_text(stdout or "")
        (work / "stderr.log").write_text((stderr or "")[-20000:])
        s = summarize_stream((stdout or "").splitlines())
        final = s.pop("final")
        answer = final.get("result", "") or ""
        state = self.site.state()
        try:
            ok, why = th.verify_web(task, ctx, answer, state)
        except Exception as exc:
            ok, why = False, f"verify error {exc}"
        strays = collect_strays(since, work, out / "strays" / run_id)
        if strays:
            ok, why = False, f"stray saves moved: {strays}; " + why
        left = bench_dialogs(self.display, env)
        if left:
            ok, why = False, f"dialog left on screen: {left}; " + why
        if timed_out:
            ok, why = False, f"timed out at the {a.timeout} s cap; {why}"
        chrome_alive = bool(core.profile_processes(profile))
        core.stop_chrome(chrome, profile)
        core.stop_arm_daemons(arm, env)
        if ab_sock:
            shutil.rmtree(ab_sock, ignore_errors=True)
        shutil.rmtree(profile, ignore_errors=True)
        u = final.get("usage") or assistant_usage((stdout or "").splitlines())
        calls, seconds = core.tool_log_totals(work / "tool.log")
        record = {**base_row, "success": bool(ok), "verify": why, "wall_s": round(wall, 1), "cli_s": seconds,
                  "cli_calls": calls, "num_turns": final.get("num_turns"),
                  "input_total": sum(u.get(k) or 0 for k in ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens")),
                  "output_tokens": u.get("output_tokens"), "input_tokens": u.get("input_tokens"),
                  "cache_read": u.get("cache_read_input_tokens"), "cache_creation": u.get("cache_creation_input_tokens"),
                  "tool_calls": s["tool_calls"], "snapshot_tokens_est": s["snapshot_tokens_est"],
                  "tool_result_tokens_est": s["tool_result_tokens_est"],
                  "cost_usd_list": final.get("total_cost_usd"), "duration_api_ms": final.get("duration_api_ms"),
                  "timed_out": timed_out, "chrome_alive_at_end": chrome_alive, "answer": answer[:500], "strays": strays,
                  "valid": True, "disk_free_gb": round(shutil.disk_usage(str(self.runs)).free / 1024 ** 3, 2)}
        if not final and not timed_out:
            record.update({"valid": False, "invalid_reason": f"no result event from claude: {(stderr or '')[-300:]}"})
        if record["disk_free_gb"] < 2 or "No space left on device" in (stderr or ""):
            record.update({"valid": False, "invalid_reason": "low disk or ENOSPC"})
        cost = record["cost_usd_list"]
        if cost is None:
            cost = max(self.max_cost, self.a.budget / 10)
            record["cost_charged_estimate"] = round(cost, 4)
        self.max_cost = max(self.max_cost, record["cost_usd_list"] or 0.0)
        self.spent += cost
        self.write_row(record)
        print(f"{run_id} {arm:6} ok={ok!s:5} wall={wall:6.1f}s cli={seconds:5.1f}s turns={final.get('num_turns')} "
              f"in={record['input_total']} ${record['cost_usd_list']} | {why[:160]}", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arms", default="tabref", help="comma list of " + ",".join(arms.ARMS))
    ap.add_argument("--rounds", default="0", help="e.g. 0-4 or 0,2")
    ap.add_argument("--tasks", default="")
    ap.add_argument("--model", default="claude-sonnet-5-5")
    ap.add_argument("--base", default="http://127.0.0.1:8792")
    ap.add_argument("--timeout", type=int, default=TIMEOUT_S)
    ap.add_argument("--host", default=None, help="host label for rows (default: vm or laptop)")
    ap.add_argument("--display", default="xvfb", choices=["xvfb", "headless", "host"])
    ap.add_argument("--chrome", default="/usr/bin/google-chrome")
    ap.add_argument("--tabref-checkout", default=None)
    ap.add_argument("--budget", type=float, default=5.0, help="stop starting runs once this many USD are spent")
    ap.add_argument("--out", default=str(core.runs_dir()))
    a = ap.parse_args()
    a.host = a.host or core.host_label()
    core.check_frozen()
    core.exit_on_sigterm()
    why = core.isolation_problem(a.display)
    if why:
        raise SystemExit(f"refusing a model pass: {why}")
    key = os.environ.pop("ANTHROPIC_API_KEY", "")
    if not key:
        raise SystemExit("no API key: ANTHROPIC_API_KEY must be in this process's environment")
    if not core.claude_bin().exists():
        raise SystemExit(f"claude not found at {core.claude_bin()}: run bench/tools/install.sh or set BENCH_CLAUDE")
    want = a.arms.split(",")
    ready = []
    for arm in want:
        why = core.arm_available(arm, core.tabref_bin(a.tabref_checkout))
        if why is None and arm == "ad-14" and a.display == "headless":
            why = "ad-14 needs --display xvfb or host"
        print(f"SKIP {why}" if why else f"arm {arm} ready", flush=True)
        if why is None:
            ready.append(arm)
    Path(a.out).mkdir(parents=True, exist_ok=True)
    with core.Display(a.display, Path(a.out) / "display") as display, \
            core.Site(a.base, Path(a.out) / "server.log") as site:
        r = Runner(a, display, site, key)
        for rnd in rounds(a.rounds):
            for arm in rot(rnd, ready):
                for tid in (a.tasks.split(",") if a.tasks else rot(rnd, ORDER)):
                    if r.spent >= a.budget:
                        print(f"budget stop: ${r.spent:.2f} spent of ${a.budget:.2f}", flush=True)
                        return
                    r.run_one(arm, tid, rnd)
        print(f"pass done: ${r.spent:.2f} spent", flush=True)


if __name__ == "__main__":
    main()
