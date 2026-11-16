# tabref: read and drive Chrome from the shell

Start: `tabref open <url>` loads the page and prints it (first call: `tabref skill && tabref open <url>`).
Chrome is already connected (TABREF_CDP). Output is plain text sized to read whole: do not pipe it through
grep, head or sed, and do not sleep; every command waits for the page itself.
Only when the user asks for their own browser: `tabref connect --user-chrome` (opt-in; it drives their
logged-in Chrome and its sessions; `tabref connect --off` stops).
To keep logins between runs: `tabref open <url> --launch --profile DIR` (a profile of tabref's own, not yours).

## Reading
One line per block in reading order; `#` lines are headings. Controls sit in brackets with their ref:
`[e5 button Save]`, `[e7 Edit -> /admin/product/7/edit]` (open the address or click the ref),
`[e9 select Size = "M" options: S | M | L]`.
- Repeated blocks are numbered `(1/3) … (3/3)`; lists say `N items:`; tables say `table N rows: head | …`,
  one row per line. To count, use these numbers across the whole page, not the first block you see.
- The last line says what was cut and the exact command for it (`tabref snap --rows 21-`,
  `tabref read --section "<heading>"`). Run it before answering about cut content.
- `tabref read` prints the main text. `tabref find <text>` searches all text, hidden too: each match with its
  section, its ref, and `(hidden; expand [e8 …])` when a click shows it.
- A `dialog` line at the top is a popup: click one of its buttons first (decline cookie banners).

## Acting
`click T`, `fill T VALUE`, `select T OPTION`, `check T`, `uncheck T`, `press KEY [T]`, `hover T`, `scroll down|T`.
`type T TEXT` adds TEXT after what the field holds (`fill` replaces it); `double-click T` clicks twice.
JavaScript dialogs are accepted and reported; `--dialog dismiss` (or `accept=TEXT` for a prompt) answers the
first confirm, prompt or leave-page dialog of the whole command, `do` included.
Downloads: an action or `open` that downloads prints the saved file's path; `tabref wait --download` waits for
one still running (`open --download-dir DIR URL` picks the folder).
In your own Chrome, tabref restores Chrome's default download behaviour after each command, and a download
that starts after the command goes to Chrome's own folder and is not reported; one left running is named by
the next `wait --download`.
T is a ref (`e4`) or what the page shows: `"Save"`, `button "Save"`, `"Edit" nth 2`,
`"Edit" in "<row text>"`, `"Reply" after "<text>"` (or `before`). Several or no matches fail with candidates;
nothing is guessed. Each action prints what changed (new address, `{3 -> 4}`, field `"67.00" -> "62.00"`,
new controls with refs, or the next page), so do not snapshot again to check. Dates, times and numbers take
plain values (`fill "Due date" 2027-07-31`); `select` takes a label or value.

## Many steps: one call
Use `tabref do` with a quoted heredoc, not shell loops. It stops at the first failing step and shows the page.

    tabref do <<'EOF'
    for size price in XS 47.00 L 49.00
      open /admin/products?search=Hoodie
      click "Edit" in "Hoodie-{size}-Red"
      fill "Price" {price}
      click "Save"
      expect url ~saved=1
    end
    EOF

Steps: any action above, `open URL`, `expect text|gone|url X`, `read [T]`, `find TEXT`, `js "EXPR"`, `snap`.
`tabref js "EXPR"` returns a page value as JSON (e.g. every link or row at once).

## Errors (one line, code first)
`ELEMENT_NOT_FOUND` lists the closest names; `AMBIGUOUS_TARGET`: add `nth N`, `in` or `after`;
`STALE_REF`: use the fresh ref it gives; `TIMEOUT`; `INVALID_ARGS`. Tabs: `tabref tabs`, `tabref tab N`,
`tabref open --new-tab URL` (relative too; the new tab becomes current).
In an app built on Chrome (Electron, reached with `--cdp PORT`), `open` and `open --new-tab` are refused, as its
windows are the app; read and act on them with snap, click and fill (js to read them).
