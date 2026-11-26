# Changelog

## Unreleased

### Your own browser asks less
- In your own browser, the daemon keeps the one connection you allowed in its "Allow remote debugging?" prompt, and `connect --user-chrome` makes it there, so the first command after `connect` no longer asks again.
- A command that times out on the prompt leaves its connection waiting in the daemon: the prompt on screen stays the one that counts, and a retry waits on it instead of opening another prompt. The error says so (`details.prompt_kept`), and without the daemon it says the prompt no longer counts and that retrying adds prompts.
- Commands that come together in your browser take turns on that one connection instead of each opening a prompt.
- A prompt answered with Cancel during the upgrade is reported as declined (`cdp_connection_rejected`).
- `close` prints how many tabs are left and the current one's index and id, not every tab's title and URL (in your own browser those are your tabs); `close --list` lists them.

## 0.2.0 (unreleased)

### Safer actions
- An action that was sent is never reported as safe to retry. A failure after delivery (a busy tab, a failed settle or attach, a read of the page that follows) is reported as delivered with a note and a `tabref snap` hint, so an agent does not click, order or delete twice (#118, #120).
- `open`, and a `do` batch that ends in `open`, succeed with a note when the page loaded but its view could not be read (#120).
- No-daemon commands fail fast with a clear hint when a late JavaScript dialog blocks the page (#110).

### Fewer turns and tokens
- `scroll --until TEXT` and `scroll --to-end` reach text in lazy lists in one command (#103).
- `snap` and `read` take `--to LINE`; long views say where they were cut, and short sections print inline in cut-view outlines (#70, #77, #102). `--rows` windows always print and are labelled by position (#117).
- Change reports drop repeated popup and option lines, collapse closed dialogs, summarize replaced table bodies, count distinct changes and keep refs in `do`-step reports (#57, #98, #109, #116).
- Copied table rows work as action scopes, including control details and literal pipes (#90, #92, #94). Single action commands accept shell-split qualifiers (#99).
- A shorter agent guide (#72, #105, #108), which points at `js` again (#117).
- `find` reaches text in `pre` blocks, rustdoc summaries and accessible SVG, and says where more matches may load (#71, #115).
- Bot-check interstitials are reported in the page view (#82).

### Faster
- A fresh capture is reused within 5 s when the page is unchanged, and click targets resolve from a verified-unchanged capture (#61, #104).
- Pages with cross-origin stylesheets can now reuse captures: on an RFC page, open + snap + find + find went from 4.9 s to 2.2 s (#122).
- `open` settles load-time work before printing, and waits stop as soon as reply bytes arrive (#67, #68, #88, #89).

### Fixes
- Changes inside shadow roots and frames are reported (#56); visible panes inside hidden wrappers show (#76); reordered views report redrawn rows (#60); fresh refs after same-address reloads (#79).
- `select` matches the label printed in the page view (#69). Upload finds hidden file inputs within the target (#58). Long `fill` values are previewed, not echoed (#66).
- Connect and launch: find Chromium past a stale port file, verify the browser before attaching or closing, suggest a WebSocket URL when discovery fails, shorter logged-in browser warning (#55, #62-#65, #75, #81, #86).
- Downloads: refuse profile aliases before creating folders; no-daemon leftover downloads stay within the size limit (#93, #101).
- Windows: tests check out text files with LF (#53).

### Benchmarks
Claude Sonnet 5.5 doing 10 web tasks, 5 runs each, on a sealed task set that nobody tuning tabref had seen (set 0.4, run once on a clean VM). Each number is the sum of per-task medians.

| | pass | wall | tool time | tokens | turns |
|---|---|---|---|---|---|
| target | 50/50 | <= 90 s | <= 5 s | <= 250k | |
| 0.2.0 on sealed set 0.4 | 45/50 | 95.3 s | 9.4 s | 356k | 52 |
| main 2acbe8c (2026-11-21) on sealed set 0.4, control rerun | 45/50 | 97.3 s | 9.4 s | 363k | 54 |
| main 2acbe8c (2026-11-21) on sealed set 0.3 | 48/50 | 98.7 s | 13.9 s | 371k | 58 |
| 0.2.0 on the dev set | 50/50 | 68.3 s | 11.6 s | 272k | 42 |

All four targets are still missed. Set 0.4 is harder than set 0.3: the build that scored set 0.3, rerun on set 0.4 as a control, does no better than 0.2.0 (so 0.2.0 is not tuned to its dev set). 0.2.0 matches or slightly beats it on every number; 4 of its 5 failures are on a single task.
