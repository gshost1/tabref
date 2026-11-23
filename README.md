# tabref

[![npm](https://img.shields.io/npm/v/@gshost1/tabref)](https://www.npmjs.com/package/@gshost1/tabref)

A Chrome DevTools CLI that lets AI agents read and drive browser tabs from the shell.

```bash
npm i -g @gshost1/tabref
```

Installs the `tabref` command.

## What it is

An agent opens a page with `tabref open` and reads a plain-text snapshot where each control has a short ref (`e1`, `e2`). It acts on a ref or visible label with commands like `click`, `fill`, and `select`, or runs several steps in one `tabref do` call. Each action waits for the page and prints what changed, and the agent can verify state with `snap`, `find`, `read`, or `wait`. `tabref` is a single Rust binary that talks to Chrome, Chromium, or Edge over the Chrome DevTools Protocol on Linux, macOS, and Windows. It launches its own headless browser with `--launch`, attaches to a DevTools port with `--cdp`, or connects to your running browser with `connect --user-chrome`.

## Why use it

- **Fewer tokens per page:** Compact reading-order views and per-action change reports keep context small. In a 2026-11-22 Wikipedia demo (5 runs each, Claude Sonnet 5.5), `tabref` 0.2.0 used a median of 9.9k tokens vs 25.7k for Playwright MCP 0.0.86 (2.6× fewer) and finished in 7.8 s vs 9.9 s.
- **Fast repeated reads:** A fresh capture is reused for up to 5 s while the page stays unchanged. On an RFC page (10 release runs, PR #122), `open` + `snap` + `find` + `find` dropped from 4.9 s to 2.2 s, and `find` after `open` dropped from 1,196 ms to 66.6 ms (9/10 reused).
- **Short guide and safer actions:** `tabref skill` prints the full agent guide in 39 lines, `tabref do` batches steps into one shell call (10 s default per step, 120 s overall), and an action that was sent is never reported as safe to retry (PR #118, PR #120).

[▶ Watch the demo (21 s)](https://github.com/gshost1/tabref/releases/download/v0.2.0/tabref-race-v3.mp4)

## Results

### Single-task demo (2026-11-22)

This is one task, so it is a demo, not a benchmark. Task: Wikipedia lookup for the Apollo 11 launch date and crew, 5 runs each, Claude Sonnet 5.5.

| Tool | Pass | Median wall | Median tokens |
| --- | --- | --- | --- |
| `tabref` 0.2.0 | 5/5 | 7.8 s | 9.9k (2.6× fewer) |
| Playwright MCP 0.0.86 | 5/5 | 9.9 s | 25.7k |

On an RFC page (10 release runs, PR #122), capture reuse across cross-origin stylesheets cut `open` + `snap` + `find` + `find` from 4.9 s to 2.2 s (and `find` after `open` from 1,196 ms to 66.6 ms).

## Install

Via npm (from v0.2.0; installs the `tabref` command):

```bash
npm i -g @gshost1/tabref
```

Or download a release archive from [GitHub Releases](https://github.com/gshost1/tabref/releases), unpack it, and put `tabref` (or `tabref.exe` on Windows) on your `PATH`. Prebuilt archives are published with `SHA256SUMS` for:

- Linux x86_64 (`x86_64-unknown-linux-gnu`, `.tar.gz`)
- macOS Apple Silicon (`aarch64-apple-darwin`, `.tar.gz`)
- Windows x86_64 (`x86_64-pc-windows-msvc`, `.zip`)

To build from source (Rust 1.89+):

```bash
cargo build --locked --release -p tabref
```

`tabref` needs Chrome, Chromium, or Edge installed locally, or `TABREF_CHROME` set to the browser binary path.

## Quick start

```bash
# Print the short agent guide
tabref skill

# Launch headless Chrome, open a page, and print a snapshot with refs
tabref open https://example.com --launch

# Click a ref (or visible label) and print what changed
tabref click e1

# Search the loaded page (including hidden text)
tabref find "RFC 2606"

# Quit the browser that --launch started
tabref close --browser
```

Run `tabref --help` or `tabref skill more` for multi-step `do` batches, waits, downloads, tabs, and `connect --user-chrome`.

## Limits

- **Chromium-based browsers only:** Works with Chrome, Chromium, and Edge over CDP. Does not drive Firefox, Safari, or native desktop UI.
- **Embedded-Chrome (Electron) apps:** When attached via `--cdp`, `open`, `open --new-tab`, and `close` are refused because the windows are the app itself. Only reading and acting on existing windows work.
- **No background daemon on Windows:** Commands run in-process on Windows (and under `TABREF_NO_DAEMON=1`). Without the daemon, a JavaScript dialog opened after a command ends blocks the page until answered in the browser.
- **No bot-check bypass:** Bot checks (Cloudflare, reCAPTCHA, hCaptcha) are reported in the page view rather than solved or bypassed.
- **Opt-in user browser and downloads:** `tabref` does not touch your logged-in browser unless you run `tabref connect --user-chrome`. In your own Chrome, default download behavior is restored after each command, so downloads started after a command ends go to Chrome's own folder and are not reported.
- **Fail-closed capture reuse and batch scope:** Capture reuse skips pages with running animations, unsupported CSS features (`:has()`, container queries), or cross-origin stylesheets over 256 KiB or of unknown size. `tabref do` does not include `upload` steps.
- **Release targets not met yet:** On sealed set 0.4 (PR #125), 0.2.0 still misses all four targets (45/50 pass, 95.3 s wall, 9.4 s tool time, 356k tokens).

## Release benchmark details

Claude Sonnet 5.5 on 10 web tasks × 5 runs each, run once on a clean VM on a sealed task set that nobody tuning `tabref` had seen. Values are sums of per-task medians.

| Run | Pass | Wall | Tool time | Tokens | Turns |
| --- | --- | --- | --- | --- | --- |
| Target | 50/50 | ≤ 90 s | ≤ 5 s | ≤ 250k | — |
| 0.2.0 (`5be61e7`), sealed set 0.4 | 45/50 | 95.3 s | 9.4 s | 356k | 52 |
| `2acbe8c`, sealed set 0.4 (control rerun) | 45/50 | 97.3 s | 9.4 s | 363k | 54 |
| `2acbe8c`, sealed set 0.3 | 48/50 | 98.7 s | 13.9 s | 371k | 58 |
| 0.2.0 (`5be61e7`), dev set | 50/50 | 68.3 s | 11.6 s | 272k | 42 |

All four targets are still missed on sealed set 0.4 (4 of 0.2.0's 5 failures were on one task). Set 0.4 is harder than set 0.3: the earlier `2acbe8c` build rerun on set 0.4 does no better than 0.2.0, which matches or slightly beats it on every metric.
