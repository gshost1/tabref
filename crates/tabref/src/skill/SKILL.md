# tabref: read and drive Chrome from the shell

Start with `tabref open <url>`. Read the whole output.
If no browser is connected, use `tabref open <url> --launch`.
Commands wait for the page. Don't add sleeps. Errors print their fix.
See `tabref skill more` for waits, `connect` to your own Chrome, downloads, dialogs, logins, tabs, and Chrome-based apps.

## Read
`tabref snap`: one line per block, `#` for headings; controls show refs: `[e5 button Save]`,
`[e9 select Size = "M" options: S | M | L]`. Count `N items:`, `table N rows:` across the whole page, not the first block.
A cut ends with the command for the rest: run it.
`tabref find TEXT` searches all text; `tabref read --section "<heading>"` reads one section.
A `dialog` line at the top: click one of its buttons.

## Act
`click T`, `fill T VALUE`, `type T TEXT`, `select T LABEL`, `check T`, `uncheck T`, `press KEY`,
`hover T`, `scroll down|T`, `upload T FILE`. T is a ref (`e4`) or what the page shows: `"Save"`,
`button "Save"`, `"Edit" nth 2`, `"Edit" in "<row text>"` (row text from snap).
Each action prints what it changed: read that instead of taking a snapshot.

## Many steps: one call
Put all known steps in one `tabref do`; each step prints its result, so don't `snap` between
or after them unless unsure. It stops at the first failing step.

    tabref do <<'EOF'
    fill "Name" Ada
    fill "Email" a@b.c
    select "Plan" Pro
    check "I agree"
    click "Submit"
    EOF

Pages: `tabref do 'for p in 2 3 4; open ?p={p}; find "TEXT"; end'`.
Lazy list: repeat `scroll down` in a `for` loop (`for n in 1 2 3 4 5 6`), then `find`.
Steps: any action but `upload`, `open URL`, `expect text|gone|url X`,
`expect T is visible|enabled|hidden|gone|value=X`, `read [T]`, `find TEXT`, `js "EXPR"`, `snap`.
