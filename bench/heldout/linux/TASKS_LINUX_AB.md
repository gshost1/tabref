# Addendum: web arm ab (agent-browser 0.42.0), Linux

Approved by George on 2026-11-11 (Mac thread "Benchmark Against agent-browser"). Frozen before any counted ab run.
`TASKS_LINUX.md` and every file it hashes are unchanged; ab lives in the new files below.

- Scope: the 10 web tasks, n = 5, Sonnet 5.5, the same per-round task rotation, seeds, 600 s cap and fresh Chrome
  profile as the finished Sonnet pass. Other arms are not re-run. Rows: arm `ab`, channel `cdp`.
- Tool: agent-browser 0.42.0 from npm (`--ignore-scripts`, no browser download), in /opt/bench-ab only. Its
  packaged linux-x64 binary has sha256 b69e0f5ea76dcb51f8f75da6bcb87c7f4d81f79b5738f1f08bd121b51cb3dc57, equal to
  the v0.42.0 GitHub release asset digest. It attaches to the harness-launched Chrome over CDP, like bh and pw.
- Wrapper: the Mac's `bench/heldout/arms/bin/ab/agent-browser` (chore/heldout-ab 1a867924d, PR #136) in bash,
  with Linux paths: AGENT_BROWSER_NO_VERSION_CHECK=1, AGENT_BROWSER_SKILLS_DIR=<package>/skill-data; refuses
  chat, upgrade, install, doctor, dashboard, plugin/plugins, -p/--provider, --auto-connect; unsets the AI gateway and Anthropic
  keys; logs through wrap.sh. 0.42.0 has no telemetry code.
- Prompt: run.py's own `cdp` text ("… pass `--cdp <port>` on every agent-browser command. Learn it with
  `agent-browser skills get core` …") plus the Linux common line the other web arms use. Allowed tools: the web
  base list, Bash(cat:*), Bash(agent-browser:*).
- Runner: `run_web_ab_linux.py` is `run_web_linux.py` plus: import common_ab_linux; ab joins bh/pw for the CDP
  port; agent-browser daemons are killed after each run (pkill, not through the logged wrapper); a per-run
  empty AGENT_BROWSER_SOCKET_DIR (mkdtemp, removed after the run), as run.py does on the Mac. The Linux runner has no
  `agent-browser close --all` cleanup, so the Mac's tool-log cut is not needed.
- Checkpoint `golden-ab` = `golden` + /opt/bench-ab + ho-bin/ab + the two files. One VM (adbench-linux) runs
  rounds 0–4 in order; a $5 spend stop is set in orchestrate.py.
- Not counted: the smoke run (hw1, pass, $0.026) and a first start at 09:06 PST stopped after 2 runs (hw1, hw2,
  both pass, $0.055) to add plugin/plugins and the per-run socket dir from the Mac's 1a867924d.

## Hashes

- 6c9f13724762675581fed01e697e6f2e8000a2e046cdcaea73a904c75accfed4  common_ab_linux.py
- 3d618f7402c4a16b412d836e31057dc2ad3d499f0ea9b18f7444845d04aa64d3  run_web_ab_linux.py
- 3ecea62e222cb251b09ae9c529ed029b1e428a08307903523c9626fd18b1d607  ab-install.sh
