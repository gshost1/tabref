#!/usr/bin/env python3
"""Per-command latency of each arm's tool, no model: open/navigate, snapshot, click, fill and a text read, on a simple
page and on a heavy page with four iframes (latency/fixture.py), against a harness-launched Chrome.

Every call goes through the arm's bench wrapper (harness/bin/<arm>/), and its time is the wrapper's own log line
(t1 - t0, the same number a model run sums as tool time). Each command is the form the tool's guide teaches:

  op        ad-14 (agent-desktop 1.4.0)      bh (browser-harness)            pw (playwright-cli)  ab (agent-browser)        tabref
  open      open-url URL --app ...           goto_url + wait_for_load        goto URL             open URL                  open URL
  snapshot  snapshot --surface web -i --lines  AX tree read (skill's recipe)  snapshot             snapshot -i               snap
  click     click @ref (Count button)        AX node -> box -> click_at_xy   click eN             click @eN                 click @ref
  fill      set-value @ref hello             fill_input('#name', ...)        fill eN hello        fill @eN hello            fill @ref hello
  text      find --text "amber falcon"       js(... innerText ...)           find "amber falcon"  find text "amber falcon"  find "amber falcon"

cold   the first call(s) against a fresh Chrome (idle for --settle s after launch, as Chrome is by the time a model
       makes its first call) with no tool daemon: the page open (pw: attach + goto, summed) and the
       first snapshot after it; --cold sessions (default 7), each a new Chrome
warm   one session per page: each op once as warm-up (discarded), then --n timed calls (default 7)
Refs for click/fill come from a fresh snapshot before each timed call (untimed), the way an agent acts.
Output size (chars/4) is recorded too.

A sample counts only when the call exits 0 and its output shows the op worked (Adapter.ok: the page title after
open, the Count button in a snapshot, the tool's success marker after click/fill, the marker text after a read).
Usage: python3 -I bench/latency/run.py [--arms ad-14,bh,pw,ab,tabref] [--display xvfb|headless|host] [--n 7]
       [--cold 7] [--tabref-checkout <dir>]
Every arm runs in the same display mode (default: headed Chrome in a private Xvfb; ad-14 cannot run headless).
Results: $BENCH_RUNS/latency/<stamp>.json; table: python3 -I bench/report.py latency
"""
import argparse
import os
import json
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "harness"))
import core  # noqa: E402

NAME = "hello bench"
MARK = "amber falcon"
OPS = ["open", "snapshot", "click", "fill", "text"]
SETTLE = [5.0]
CHROME = ["/usr/bin/google-chrome"]
BH_AX = ("nodes = cdp('Accessibility.getFullAXTree')['nodes']\n"
         "for n in nodes:\n"
         "    name = (n.get('name') or {}).get('value')\n"
         "    if name and not n.get('ignored'):\n"
         "        print((n.get('role') or {}).get('value'), repr(name), n.get('backendDOMNodeId'))\n")
BH_CLICK = ("nodes = cdp('Accessibility.getFullAXTree')['nodes']\n"
            "n = next(n for n in nodes if (n.get('role') or {}).get('value') == 'button' and (n.get('name') or {}).get('value') == 'Count')\n"
            "q = cdp('DOM.getBoxModel', backendNodeId=n['backendDOMNodeId'])['model']['content']\n"
            "click_at_xy(sum(q[0::2]) / 4, sum(q[1::2]) / 4)\n"
            "print(js(\"document.getElementById('out').textContent\"))\n")


PAGE = re.compile(r"(Simple|Heavy) page|Latency fixture")


class Adapter:
    """Per tool: the command for each op and how to tell from its output that the op worked. `done` holds the
    marker each tool prints for a successful click and fill."""
    tool = ""
    needs_ref = False
    done = {}

    def __init__(self, port):
        self.port = port

    def ok(self, op, out):
        if op == "open":
            return PAGE.search(out) is not None
        if op == "snapshot":
            return "Count" in out
        if op == "text":
            return MARK in out
        marker = self.done.get(op)
        return marker is None or re.search(marker, out) is not None

    def cold(self, url):
        return [self.open(url)]

    def parse_refs(self, out, work):
        return {}


class AD(Adapter):
    tool = "agent-desktop"
    needs_ref = True
    done = {"click": r'"ok":\s*true', "fill": r'"ok":\s*true'}
    APP = ["--app", "Google Chrome"]

    def open(self, url):
        return ["open-url", url, *self.APP], None

    def snapshot(self, refs):
        return ["snapshot", *self.APP, "--surface", "web", "-i", "--lines"], None

    def click(self, refs):
        return ["click", refs["count"]], None

    def fill(self, refs):
        return ["set-value", refs["name"], NAME], None

    def text(self, refs):
        return ["find", *self.APP, "--surface", "web", "--text", MARK], None

    def parse_refs(self, out, work):
        refs = {}
        for key, pat in (("count", r'button \\"Count\\"'), ("name", r'textfield \\"Name\\"')):
            m = re.search(r'(@s[0-9a-z]+:e\d+) ' + pat, out)
            if m:
                refs[key] = m.group(1)
        return refs


class BH(Adapter):
    tool = "browser-harness"
    done = {"click": r"clicked [1-9]", "fill": re.escape(NAME)}

    def cold(self, url):
        return [([], f"new_tab({url!r})\nwait_for_load()\nprint(page_info())\n")]

    def open(self, url):
        return [], f"goto_url({url!r})\nwait_for_load()\nprint(page_info())\n"

    def snapshot(self, refs):
        return [], BH_AX

    def click(self, refs):
        return [], BH_CLICK

    def fill(self, refs):
        return [], f"fill_input('#name', {NAME!r})\nprint(js(\"document.getElementById('name').value\"))\n"

    def text(self, refs):
        return [], (f"print(js(\"[...document.querySelectorAll('p')].map(p => p.innerText)"
                    f".find(t => t.includes('{MARK}'))\"))\n")


class PW(Adapter):
    tool = "playwright-cli"
    needs_ref = True
    done = {"click": r"\.click\(", "fill": r"\.fill\("}

    def cold(self, url):
        return [([f"attach", f"--cdp=http://127.0.0.1:{self.port}"], None), self.open(url)]

    def open(self, url):
        return ["goto", url], None

    def snapshot(self, refs):
        return ["snapshot"], None

    def click(self, refs):
        return ["click", refs["count"]], None

    def fill(self, refs):
        return ["fill", refs["name"], NAME], None

    def text(self, refs):
        return ["find", MARK], None

    def parse_refs(self, out, work):
        refs = {}
        for key, pat in (("count", r'button "Count"(?: \[[a-z]+\])* \[ref=([a-z0-9]+)\]'), ("name", r'textbox "Name"(?: \[[a-z]+\])* \[ref=([a-z0-9]+)\]')):
            m = re.search(pat, out)
            if m:
                refs[key] = m.group(1)
        return refs


class AB(Adapter):
    tool = "agent-browser"
    needs_ref = True
    done = {"click": "✓", "fill": "✓"}

    def _c(self, *args):
        return ["--cdp", str(self.port), *args], None

    def open(self, url):
        return self._c("open", url)

    def snapshot(self, refs):
        return self._c("snapshot", "-i")

    def click(self, refs):
        return self._c("click", refs["count"])

    def fill(self, refs):
        return self._c("fill", refs["name"], NAME)

    def text(self, refs):
        return self._c("find", "text", MARK, "text")

    def parse_refs(self, out, work):
        refs = {}
        for key, pat in (("count", r'button "Count" \[ref=([a-z0-9]+)\]'), ("name", r'textbox "Name" \[ref=([a-z0-9]+)\]')):
            m = re.search(pat, out)
            if m:
                refs[key] = "@" + m.group(1)
        return refs


class TR(Adapter):
    """tabref per .plan/DESIGN.md's command surface; the tuning tracks adjust this if the CLI changes."""
    tool = "tabref"
    needs_ref = True

    def open(self, url):
        return ["open", url], None

    def snapshot(self, refs):
        return ["snap"], None

    def click(self, refs):
        return ["click", refs["count"]], None

    def fill(self, refs):
        return ["fill", refs["name"], NAME], None

    def text(self, refs):
        return ["find", MARK], None

    def parse_refs(self, out, work):
        refs = {}
        for key, word in (("count", "Count"), ("name", "Name")):
            for line in out.splitlines():
                m = re.search(r"(@[A-Za-z0-9]+(?::e\d+)?)", line)
                if m and re.search(rf"\b{word}\b", line) and not re.search(r"Latency fixture", line):
                    refs[key] = m.group(1)
                    break
        return refs


ADAPTERS = {"ad-14": AD, "bh": BH, "pw": PW, "ab": AB, "tabref": TR}


class Session:
    def __init__(self, arm, work, display, home, tabref, url):
        self.arm, self.work, self.display = arm, Path(work), display
        shutil.rmtree(self.work, ignore_errors=True)
        self.work.mkdir(parents=True)
        self.port = core.free_port() if arm != "ad-14" else None
        self.env = core.arm_env(arm, self.work, self.port, display, home, tabref)
        profile, downloads = self.work / "profile", self.work / "downloads"
        downloads.mkdir()
        if arm == "ad-14":
            self.chrome = core.launch_chrome_ad(self.env["AGENT_DESKTOP_HOME"], profile, downloads, display, self.env)
            self.port = core.ad_endpoint_port(self.env["AGENT_DESKTOP_HOME"])
            self.env["HO_CDP_PORT"] = str(self.port or "")
        else:
            self.chrome = core.launch_chrome(profile, self.port, downloads, display, self.env, CHROME[0])
        time.sleep(SETTLE[0])
        self.adapter = ADAPTERS[arm](self.port)
        self.wrapper = str(core.BIN / arm / self.adapter.tool)
        self.log = self.work / "tool.log"

    def call(self, argv_stdin):
        """One tool call in its own session. Its time is the wrapper's log line; a call killed at the timeout never
        logs one, so it counts as rc 124 at its wall time."""
        argv, stdin = argv_stdin
        before = len(core.tool_log_lines(self.log))
        rc, out, err, wall, timed_out = core.run_in_session([self.wrapper, *argv], self.work, self.env, 120, stdin)
        lines = core.tool_log_lines(self.log)[before:]
        secs = wall if timed_out else (sum(x["t1"] - x["t0"] for x in lines) if lines else None)
        out = out + err
        return {"s": secs, "rc": rc, "chars": len(out), "out": out}

    def refs(self):
        r = self.call(self.adapter.snapshot({}))
        out = r["out"]
        if self.arm == "pw" and "[ref=" not in out:
            files = sorted((self.work / ".playwright-cli").glob("page-*.yml"), key=lambda p: p.stat().st_mtime)
            out = files[-1].read_text() if files else out
        return self.adapter.parse_refs(out, self.work)

    def close(self):
        core.stop_chrome(self.chrome, self.work / "profile")
        core.stop_arm_daemons(self.arm, self.env)


def cold_samples(arm, page_url, display, home, tabref, root, n):
    out = []
    for i in range(n):
        s = Session(arm, root / f"cold-{i}", display, home, tabref, page_url)
        try:
            calls = [s.call(c) for c in s.adapter.cold(page_url)]
            first_snap = s.call(s.adapter.snapshot({}))
            out.append({"open_s": sum(c["s"] or 0 for c in calls),
                        "open_ok": all(c["rc"] == 0 for c in calls) and s.adapter.ok("open", calls[-1]["out"]),
                        "snapshot_s": first_snap["s"],
                        "snapshot_ok": first_snap["rc"] == 0 and s.adapter.ok("snapshot", first_snap["out"])})
        finally:
            s.close()
    return out


def warm_samples(arm, page_url, display, home, tabref, root, n):
    s = Session(arm, root / "warm", display, home, tabref, page_url)
    res = {}
    try:
        for c in s.adapter.cold(page_url):
            s.call(c)
        for op in OPS:
            samples, fails, chars, err = [], 0, [], ""
            for i in range(n + 1):
                refs = s.refs() if s.adapter.needs_ref and op in ("click", "fill") else {}
                if s.adapter.needs_ref and op in ("click", "fill") and not refs.get("count" if op == "click" else "name"):
                    fails += 1
                    err = f"no ref for {op} in snapshot"
                    continue
                cmd = s.adapter.open(page_url) if op == "open" else getattr(s.adapter, op)(refs)
                r = s.call(cmd)
                if i == 0:
                    continue
                if r["rc"] != 0 or r["s"] is None or not s.adapter.ok(op, r["out"]):
                    fails += 1
                    err = ("" if r["rc"] else "output shows no success: ") + r["out"][-300:]
                    continue
                samples.append(r["s"])
                chars.append(r["chars"])
            res[op] = {"samples": samples, "fails": fails, "tokens": int(statistics.median(chars) // 4) if chars else None,
                       "error": err if fails else ""}
    finally:
        s.close()
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arms", default="ad-14,bh,pw,ab,tabref")
    ap.add_argument("--pages", default="simple,heavy")
    ap.add_argument("--display", default="xvfb", choices=["headless", "xvfb", "host"],
                    help="xvfb (default): every arm headed in a private Xvfb; headless skips ad-14")
    ap.add_argument("--n", type=int, default=7)
    ap.add_argument("--cold", type=int, default=7)
    ap.add_argument("--tabref-checkout", default=None)
    ap.add_argument("--settle", type=float, default=5.0, help="seconds Chrome idles after launch before the first call")
    ap.add_argument("--chrome", default="/usr/bin/google-chrome")
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    SETTLE[0], CHROME[0] = a.settle, a.chrome
    core.check_frozen()
    core.exit_on_sigterm()
    runs = core.runs_dir() / "latency"
    stamp = time.strftime("%Y%m%d-%H%M%S")
    root = runs / f"work-{stamp}"
    home = runs / "home"
    tabref = core.tabref_bin(a.tabref_checkout)
    result = {"stamp": stamp, "host": core.host_label(), "display": a.display, "n": a.n, "cold_n": a.cold, "settle_s": a.settle,
              "load1_start": os.getloadavg()[0],
              "chrome": subprocess.run([a.chrome, "--version"], capture_output=True, text=True).stdout.strip(),
              "rows": []}
    fx_port = core.free_port()
    fx = subprocess.Popen([sys.executable, "-I", str(HERE / "fixture.py"), str(fx_port)], stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL)
    time.sleep(0.5)
    displays = {}
    try:
        for arm in a.arms.split(","):
            why = core.arm_available(arm, tabref)
            if why:
                print(f"SKIP {why}", flush=True)
                result["rows"].append({"arm": arm, "skipped": why})
                continue
            mode = a.display
            if arm == "ad-14" and mode == "headless":
                print("SKIP ad-14: needs a display (use --display xvfb)", flush=True)
                result["rows"].append({"arm": arm, "skipped": "ad-14 needs a display; run with --display xvfb"})
                continue
            if mode not in displays:
                displays[mode] = core.Display(mode, runs / f"display-{stamp}-{mode}").start()
            for page in a.pages.split(","):
                url = f"http://127.0.0.1:{fx_port}/{page}"
                t0 = time.time()
                try:
                    cold = cold_samples(arm, url, displays[mode], home, tabref, root / arm / page, a.cold)
                    warm = warm_samples(arm, url, displays[mode], home, tabref, root / arm / page, a.n)
                except Exception as e:
                    print(f"{arm} {page}: FAILED {e}", flush=True)
                    result["rows"].append({"arm": arm, "page": page, "display": mode, "error": str(e)[:500]})
                    continue
                result["rows"].append({"arm": arm, "page": page, "display": mode, "cold": cold, "warm": warm})
                med = {op: (round(statistics.median(v["samples"]) * 1000) if v["samples"] else None) for op, v in warm.items()}
                print(f"{arm:6} {page:6} {mode:8} cold open {statistics.median(c['open_s'] for c in cold) * 1000:.0f} ms "
                      f"| warm ms {med} ({time.time() - t0:.0f}s)", flush=True)
    finally:
        for d in displays.values():
            d.stop()
        fx.terminate()
    result["load1_end"] = os.getloadavg()[0]
    out = Path(a.out) if a.out else runs / f"{stamp}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(core.scrub(json.dumps(result, indent=1)))
    shutil.rmtree(root, ignore_errors=True)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
