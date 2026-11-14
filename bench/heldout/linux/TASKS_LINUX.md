# Held-out tasks, Linux run (frozen 2026-11-11 00:06 PST, before any counted run)

The Mac set (`../TASKS_FROZEN.md`, `tasks_heldout.py` sha256 `55a13cb1…`) on an Ubuntu GNOME boxd VM. The 10 web
tasks run unchanged. The 8 native tasks go back to their OSWorld originals: LibreOffice Writer, GIMP and
read-only GNOME Settings. Each keeps its intent, seed content and its independent verifier (the frozen judge
functions in `tasks_heldout.py` are called unchanged; only the file readers are new).

Smoke runs during the port (Sonnet 5.5, $0.96 in total, plus $0.08 for the earlier harness smoke) are not counted and are not in the results.

## Environment

boxd VM, Ubuntu 24.04.5 x86_64, 4 vCPU / 16 GB, no GPU. GNOME Shell 46 (X11) on Xvfb :99 at 1920×1080,
a user D-Bus via systemd linger, AT-SPI on. User `bench` (no sudo, cut off from the boxd private network
except DNS). Chrome 157.0.8103.50, LibreOffice 24.2.7.2, GIMP 2.10.36. Claude Code 2.1.322 on the bench API
key. Time zone Europe/Berlin, Screen Blank 15 minutes, an idle inhibitor running so the screen never blanks.
Golden checkpoint `golden` restored before every arm block.

## Arms

| Arm | Suite | Tool |
|---|---|---|
| ad-14 | native, web (`ad-web`) | agent-desktop 1.4.0, release `v1.4.0` x86_64-unknown-linux-gnu (sha256 f0e8e0ec… = checksums.txt) |
| ad-up | native, web (`ax`) | agent-desktop upstream 8e8dd7cbe, built in the VM with Rust 1.89.0 (reports 1.2.0) |
| cua | native | Cua Driver 0.46.1 linux-x86_64 (checksums.txt OK), the Mac's pinned version; guides SKILL, LINUX, WORKFLOW, RUNTIME, README at the tag |
| bh | web (`cdp`) | browser-harness 0.1.13 (PyPI) |
| pw | web (`cdp`) | @playwright/cli 0.1.24 |

Peekaboo is macOS-only and has no Linux arm.

## Native tasks (Linux)

| Id | OSWorld original | Linux task | Verifier |
|---|---|---|---|
| hn1 | 0b17a146 libreoffice_writer: subscript the 2 in H2O | Writer, a bench .docx with the same 4 paragraphs (two H2O, one CO2) | docx read (`read_docx`): text unchanged, both H2O 2s subscript, nothing else (`judge_subscript`) |
| hn2 | 72b810ef libreoffice_writer: strike the last paragraph | Writer, the same 5-paragraph outline | `judge_strike` |
| hn3 | 3ef2b351 libreoffice_writer: center the heading | Writer, the same heading + 3 paragraphs | `judge_center` (paragraph `w:jc`, through styles) |
| hn4 | e528b65e libreoffice_writer: capitalise each word | Writer, the same 3 lines | `judge_caps` |
| hn5 | 72f83cdc gimp: mirror horizontally | GIMP, the same 600×400 PNG; overwrite it as PNG | PIL grid read (`read_png_grid`), `judge_flip` |
| hn6 | d16c99dc gimp: height 512, keep aspect | GIMP, the same PNG | `judge_resize` |
| hn7 | b6781586 os: set the time zone to UTC+0 | Read-only: report the time zone in Settings > System > Date & Time | `/etc/localtime` unchanged; answer names the zone (IANA id, city, abbreviation, UTC offset or country) |
| hn8 | bedcedc4 os: turn "Dim screen when inactive" off | Read-only: report Screen Blank in Settings > Power | `idle-delay` unchanged; minutes (or Never) equal it (`minutes_said`) |

## Every change from the Mac version

1. Native apps: TextEdit → LibreOffice Writer (hn1–hn4), Preview → GIMP (hn5–hn6), System Settings → GNOME Settings (hn7–hn8), i.e. back to the OSWorld apps.
2. Native files: RTF → .docx (OSWorld's format), written by `write_docx` (Normal + Heading 1 styles). Same text, same headings.
3. Native verifiers: `ho_probe rtf` → `read_docx` (paragraph alignment, runs with vertAlign/position, strike/dstrike, through character and paragraph styles); `ho_probe pixels` → PIL. Both return the probe's JSON shape, so the frozen judges run unchanged. Checked with LibreOffice-written oracle files: all four Writer judges pass a correct file and fail a wrong one.
4. Native prompts: "TextEdit/Preview has … open" → "LibreOffice Writer/GIMP has … open". The save sentence says "under its current name and format (Word .docx)" / "(overwrite the PNG file)", because Writer and GIMP offer another format on save and TextEdit/Preview did not.
5. hn7: path "General > Date & Time" → "System > Date & Time"; accepted names come from zoneinfo + iso3166 (the Mac's from NSTimeZone).
6. hn8: macOS "display turns off on power adapter (Lock Screen)", read from `pmset` → GNOME "Screen Blank" (Power panel), read from `org.gnome.desktop.session idle-delay`. Same intent (a read-only display-idle setting), same `minutes_said` judge.
7. hn1–hn4 `saved()`: the seed's mtime is set 120 s in the past before the app opens it, so a save in the same second still counts.
8. Native system prompt: "this Mac's GUI … accessibility granted" → "this Ubuntu (GNOME) desktop's GUI … accessibility enabled"; the example forbidden programs (open, osascript, defaults) → (xdg-open, xdotool, gsettings). Screenshots work for every arm on Linux, so the Mac's "Screen Recording is off, so screenshots fail" line is left out, as `aws_setup.md` plans for a host with screenshots.
9. Forbidden programs: the Mac list plus their Linux counterparts (xdg-open, gio, xdotool, wmctrl, gsettings, dconf, timedatectl, busctl, gdbus, dbus-send, soffice, libreoffice, gimp, convert, unzip, zip, xclip, …; full set in `tasks_linux.FORBIDDEN`).
10. Cua guide: MACOS.md → LINUX.md in the guide list (same tag, cua-driver-rs-v0.46.1). The wrapper keeps the Mac rules: telemetry off, private home, update/autostart/telemetry/channel refused, `call get_desktop_state` refused.
11. Web prompts: "this Mac" → "this computer"; ad-up's "macOS accessibility tree" → "Linux accessibility tree (AT-SPI)"; the forbidden examples (curl, osascript, open, python) → (curl, xdg-open, xdotool, python). Task prompts, seeds, server and verifiers unchanged.
12. Web Chrome: `/usr/bin/google-chrome` with `--force-renderer-accessibility` for every arm (Chrome on Linux exposes no AT-SPI tree without it); `--use-mock-keychain` dropped (macOS-only). ad-14 launches Chrome through `agent-desktop launch google-chrome --cdp`, as on the Mac.
13. Web runner: the browser bench's `run.py` (feat/unified-browser 4cb19985, the pushed copy; the Mac's local worktree may be newer) and `run_web.py` folded into `run_web_linux.py`; the macOS-only `lsappinfo front` field is dropped; rows also record `chrome_alive_at_end`.
14. Tool-time wrapper: `logwrap.zsh` → `wrap.sh` (bash, same log line format, `date +%s.%N`).
15. Driver: `summarize()`/`validity()` copied from bench/driver.py (perf/speed-cost ae93c615); the disk check reads `/`.
16. No desktop lock (the VM runs one thing at a time). Golden restore per arm block, as `aws_setup.md` plans; arms and tasks still rotate per round.
17. Model loop: the bench Anthropic API key (`ANTHROPIC_API_KEY` from a 600 file), not the subscription; Claude Code 2.1.322, not 2.1.321. `cost_usd_list` is logged per run.
18. Pass order per George: Sonnet 5.5 first. Opus 5.5 is not run until George decides.

## Known issue found while porting

Chrome 157 on this Xvfb session segfaulted on an agent-desktop XTEST key press (both 1.4.0 and upstream) in six
reproductions in a row, then stopped reproducing and could not be brought back (xdotool's keys never crashed it).
Cause not found. Every web row records `chrome_alive_at_end`, and the report counts them per arm.

## Frozen hashes

- tasks_heldout.py sha256 `55a13cb15cec84e348630caf2d89dc254a6b6beaafc5ea0405b6ae56ecdac7c8` (unchanged)
- web/server.py sha256 `b6b60b534ed17aea189fc004973436514128874c3b831551b0f28507f2be8f0f` (unchanged)
- bench/tasks.py (Linux stand-in) sha256 `3cc9144bac48ab09fed83b1b14fb40beabc87fbf75ce087f9d7ec9718b8ddc0b`
- linux/tasks_linux.py sha256 `ccc8bf3ca0095d5a2ea0d05598b24a987632d827fc948fda634901eeec8c89b0`
- linux/common_linux.py sha256 `0594ba1ee4ee1f9fbe0b88d61460bd5a55513050c5aea84149c7a6ed0c43c1cd`
- linux/run_native_linux.py sha256 `930192838a2fd47609abb27d93eebe90378201352b8a7ae068426f76a31ec54b`
- linux/run_web_linux.py sha256 `332a8143dfa3b2c789a72a9b418f86a6db94f9dc33c5ae6e1e4f238f6924c12e`
- linux/driver_lite.py sha256 `d7d911b1fab98a6f99662c1cdc1db607a6fed2c4913b0e14c1e24b6e1a467c5e`
- linux/wrap.sh sha256 `5ee33b1bbfdafc0228a28a02d1878c3b9b40f40c3f5a2d528c5175bddf3ee8a9`
- 03-ho.sh (VM install + wrappers) sha256 `ba106162781b15059161b390bd7926dc596f26214bf3b60e17430410788c2f1d`
