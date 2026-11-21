# tabref: read and drive Chrome from the shell

Start with `tabref open <url>`. Read the whole output.
If no browser is connected, use `tabref open <url> --launch`.
Commands wait for the page. Don't add sleeps.
See `tabref skill more` for waits, `connect` to your own Chrome, downloads, dialogs, logins, tabs, and Chrome-based apps.

## Read
`tabref snap`: one line per block; controls show refs: `[e5 button Save]`,
`[e9 select Size = "M" options: S | M | L]`, links their address: `[e7 Next -> <address>]`.
Count `N items:`, `table N rows:` and repeats `(1/3)` across the whole page, not the first block.
If what you need is not shown, use the outline's `--from N`, `tabref read --section "<heading>"`
or `tabref find TEXT` (hidden text too). A `dialog` line at the top: click one of its buttons.

## Act
`click T`, `fill T VALUE` (replaces), `type T TEXT` (appends), `select T LABEL`, `check T`, `uncheck T`,
`press KEY`, `hover T`, `scroll down|T`, `upload T FILE`. T is a ref (`e4`) or what the page shows: `"Save"`,
`button "Save"`, `"Edit" nth 2`, `"Edit" in "<row text>"`, `"Reply" after "<text>"`.
Each action prints what it changed: read that instead of taking a snapshot.
`STALE_REF`: use the fresh ref it printed.

## Many steps: one call
Put all known steps in one `tabref do`; each step prints its result. Snap only if the output
did not show what you need. It stops at the first failing step.

    tabref do <<'EOF'
    fill "Name" Ada
    fill "Email" a@b.c
    check "I agree"
    click "Submit"
    EOF

`for name qty in Pen 2 Cup 5` ... `end` runs the steps per pair as `{name}` and `{qty}`.
Pages: loop the Next or 2 link's address with the page number as `{n}`:
`for n in 2 3 4`, `open "<address with {n}>"`, `find TEXT`, `end`.
Lazy list: repeat `scroll down` in a `for` loop (`for n in 1 2 3 4 5 6`), then `find TEXT`.
Steps: any action but `upload`, `open URL`, `expect text|gone|url X`,
`expect T is visible|enabled|hidden|gone|value=X`, `read [T]`, `find TEXT`, `js "EXPR"`, `snap`.
