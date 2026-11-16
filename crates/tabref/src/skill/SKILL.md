# tabref: read and drive Chrome from the shell

Start: `tabref open <url>` loads the page and prints it (first call: `tabref skill && tabref open <url>`).
Chrome is already connected. Output is plain text sized to read whole: do not pipe it through grep, head
or sed, and do not sleep; every command waits for the page itself.
Downloads, waits, saved logins, the user's own Chrome, apps built on Chrome: `tabref skill more`.
Any command's options: `tabref CMD --help`.

## Reading
One line per block in reading order; `#` lines are headings. Controls sit in brackets with their ref:
`[e5 button Save]`, `[e7 Edit -> /admin/product/7/edit]` (open the address or click the ref),
`[e9 select Size = "M" options: S | M | L]`.
- Repeated blocks are numbered `(1/3) … (3/3)`; lists say `N items:`; tables say `table N rows: head | …`,
  one row per line. To count, use these numbers across the whole page, not the first block you see.
- The last line says what was cut and the exact command for it (`tabref snap --rows 21-`,
  `tabref read --section "<heading>"`). Run it before answering about cut content.
- `tabref read` prints the main text. `tabref find <text>` searches all text, hidden too
  (a hidden match names the ref to click that shows it).
- A `dialog` line at the top is a popup: click one of its buttons first (decline cookie banners).

## Acting
`click T`, `fill T VALUE`, `type T TEXT` (adds), `select T LABEL|VALUE`, `check T`, `uncheck T`,
`press KEY [T]`, `hover T`, `scroll down|T`, `double-click T`, `focus T`, `upload T FILE`.
T is a ref (`e4`) or what the page shows: `"Save"`, `button "Save"`, `"Edit" nth 2`,
`"Edit" in "<row text>"`, `"Reply" after "<text>"`. Several or no matches fail with candidates.
Each action prints what changed (new address, `{3 -> 4}`, `"67.00" -> "62.00"`, new controls), so do not
snapshot again to check. Dates, times and numbers take plain values (`fill "Due date" 2027-07-31`).

## Many steps: one call
Use `tabref do` with a quoted heredoc, not shell loops. It stops at the first failing step.

    tabref do <<'EOF'
    for size price in XS 47.00 L 49.00
      open /admin/products?search=Hoodie
      click "Edit" in "Hoodie-{size}-Red"
      fill "Price" {price}
      click "Save"
      expect url ~saved=1
    end
    EOF

Steps: any action, `open URL`, `expect text|gone|url X`, `read [T]`, `find TEXT`, `js "EXPR"`, `snap`.
`tabref js "EXPR"` returns a page value as JSON (e.g. every link or row at once).

## Errors (one line, code first)
`ELEMENT_NOT_FOUND` lists close names; `AMBIGUOUS_TARGET`: add `nth N`, `in` or `after`; `STALE_REF`: use
the fresh ref it gives. Tabs: `tabref tabs`, `tabref tab N`, `tabref open --new-tab URL`.
