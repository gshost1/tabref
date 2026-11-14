# tabref bench: arms and modes

The held-out web bench (10 tasks: hw1–hw7 on the local site, hp1–hp3 on public pages) from the agent-desktop fork
(`origin/perf/speed-cost:bench/heldout`) and its Linux run (Sonnet 5.5, n=5, boxd VM, 2026-11-11), made
self-contained, with a `tabref` arm beside the four it compared.

## Frozen inputs (sha256 checked by `harness/core.py` before every model pass and replay)

| file | sha256 | from |
|---|---|---|
| `heldout/tasks_heldout.py` | `55a13cb1…ac7c8` | TASKS_FROZEN.md |
| `heldout/web/server.py` | `b6b60b53…f8f0f` | TASKS_FROZEN.md |
| `tasks.py` (Linux stand-in) | `3cc9144b…c0b` | TASKS_LINUX.md |
| `heldout/linux/common_linux.py` | `0594ba1e…1cd` | TASKS_LINUX.md (prompts of ad-14, bh, pw) |
| `heldout/linux/common_ab_linux.py` | `6c9f1372…ed4` | TASKS_LINUX_AB.md (prompt of ab) |
| `heldout/linux/wrap.sh` | `5ee33b1b…8a9` | TASKS_LINUX.md (tool-time log) |

`heldout/tests/` are the fork's verifier tests at 7faca3235 (30, all pass on Linux); `tests/` covers the bench
plumbing and the scoring rule. `heldout/report.py` is the fork's 7faca3235 report, which cannot reward failure
(ranking by pass rate, then per-success wall time; pass medians apart from all-run medians); `heldout/report_v1.py`
is the earlier 5e612b37c one, kept only to compare with past reports.

## Arms

| arm | tool, version | install (`tools/install.sh`, into `$BENCH_TOOLS`, default `<main checkout>/.bench-tools`) | check |
|---|---|---|---|
| ad-14 | agent-desktop 1.4.0 | `gh release download v1.4.0 --repo gshost1/agent-desktop` (curl if gh is signed out) | tarball sha256 = checksums.txt (`f0e8e0ec…`) |
| bh | browser-harness 0.1.13 | venv (uv 0.13.7's CPython 3.12.16) from `pip download --only-binary=:all:` wheels, installed `--no-index` | every wheel's sha256 = PyPI's; browser_harness `2491459e…` |
| pw | @playwright/cli 0.1.24 | `npm ci --ignore-scripts --no-audit --no-fund` from `tools/locks/` (whole tree pinned) | lockfile integrity |
| ab | agent-browser 0.42.0 | same, from `tools/locks/` | packaged linux-x64 binary `b69e0f5e…` = v0.42.0 release digest |
| tabref | tabref under test | `TABREF_BIN`, else `<checkout>/target/release/tabref` (`--tabref-checkout`, default the main checkout) | rows are skipped while the binary is missing |

Claude Code for model passes: 2.1.322 (the Linux pass's version), `npm ci` from `tools/locks/`, platform binary
called directly (`BENCH_CLAUDE` overrides). Node is v26.13.0 from nodejs.org (SHASUMS256 checked) inside
`$BENCH_TOOLS`; uv 0.13.7 (release .sha256 checked) provides CPython 3.12.16. Nothing is taken from PATH.

Wrappers (`harness/bin/<arm>/`) are the Linux run's (03-ho.sh, ab-install.sh) with only install paths changed;
every call goes through `heldout/linux/wrap.sh`, which logs t0, t1, rc, bytes and args per call.
Telemetry and updates off as in the fork's ARMS.md: bh `BH_TELEMETRY=0 BH_UPDATE_CHECK=0 ANONYMIZED_TELEMETRY=false`;
pw `NO_UPDATE_NOTIFIER=1 PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`; ab `AGENT_BROWSER_NO_VERSION_CHECK=1`, skills dir
pinned, `chat|upgrade|install|doctor|dashboard|plugin(s)|-p|--provider|--auto-connect` refused, AI keys unset;
tabref `TABREF_CDP=<port>`, per-run `TABREF_HOME`, agent-desktop bridge pointed nowhere, `ANTHROPIC_API_KEY` unset.
Cleanup after a run stops only processes whose environment holds that run's `HO_ARM_HOME` or `TABREF_HOME`
(tabref: `tabref daemon stop` first).

Prompts: ad-14, bh, pw and ab are the frozen Linux text (`tests/test_bench.py` checks it); the only change is the
guide folder in pw's `cat …/SKILL.md`. tabref: "You operate Google Chrome only through the `tabref` CLI … Chrome is
already running with a DevTools endpoint on port P, and tabref is already pointed at it (TABREF_CDP=P is set;
`--cdp P` does the same). Learn it with `tabref skill` before you start." + the common line.

## Environment differences from the Linux pass

- Runs use a clean environment: bench-only HOME, private XDG_RUNTIME_DIR and D-Bus (no service activation), PATH =
  arm wrappers + node/npm/npx/jq/claude + /usr/bin:/bin. Nothing of the operator's session reaches the arms.
- `--display xvfb` (default in every mode, for every arm alike): private Xvfb 1920×1080 + openbox + AT-SPI, headed
  Chrome with `--force-renderer-accessibility`, as the VM's GNOME/Xvfb session did. `headless`: `--headless=new`
  (ad-14 is skipped: it cannot run headless). `host`: the session's own display, refused for model passes off a VM.
- Model passes are isolated: `run_web.py` refuses to start unless it runs on a VM or as the unprivileged user
  `tabbench` (no admin groups, no other home readable). `tools/tabbench.sh` sets that user up with the operator's
  own sudo (no sudoers change) and passes the key over stdin into that process's environment only.
- A run that hits the 600 s cap is a valid failure; its usage is summed from the assistant events, and the budget
  is charged the largest per-run cost seen so far (at least budget/10) because Claude Code reports no cost for it.
- The local site must start on a free port (a taken port is an error, never a shared site).
- Results carry a generic host label (`laptop`, `vm`, `linux` for the old pass) and no operator paths.
- Chrome on this laptop is 156.0.8078.79 (the VM had 157.0.8103.50).

## Modes (one command each, from the repo root)

```bash
bench/tools/install.sh                                   # arms into .bench-tools/ (idempotent)
python3 -I bench/latency/run.py                          # per-command latency, all arms -> bench/runs/latency/
python3 -I bench/replay/run.py                           # model-free replay, all arms -> bench/runs/replay/
sudo bench/tools/tabbench.sh setup                       # laptop, once: the tabbench user + its own tool install
<key on stdin> | bench/tools/tabbench.sh run --arms tabref --rounds 0-4 --budget 5   # laptop model pass
ANTHROPIC_API_KEY=… python3 -I bench/harness/run_web.py --arms tabref --rounds 0-4   # VM model pass
python3 -I bench/report.py all                           # tables (models: add --baseline for the Linux rows)
bench/vm/boxd.sh latency|replay|models …                 # the same on a throwaway boxd VM (signed-in boxd CLI)
```
