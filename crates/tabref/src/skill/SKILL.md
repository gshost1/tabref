# tabref: drive Chrome from the shell

Start with `tabref open <url>`. Read the whole output.
If no browser is connected, use `tabref open <url> --launch`.
Commands wait for the page. Don't add sleeps.
`tabref skill more` for waits, `connect` to your own Chrome, downloads, dialogs, logins, tabs, apps.

## Read
`tabref snap`: a line per block; controls show refs: `[e5 button Save]`,
`[e9 select Size = "M" options: S | M | L]`, links their address: `[e7 Next -> <address>]`.
Count `N items:`, `table N rows:`, repeats `(1/3)` across the whole page, not the first block.
A cut view ends with an outline of the rest; if what you need is not shown, use its `--from N`,
`tabref read --section "<heading>"` or `tabref find TEXT` (hidden text too). A `dialog` line on top: click a button.
`tabref js "EXPR"` returns a page value as JSON.

## Act
`click T`, `fill T VALUE` (replaces), `type T TEXT` (appends), `select T LABEL`, `check T`, `uncheck T`,
`press KEY`, `hover T`, `scroll down|T`, `upload T FILE`. T is a ref (`e4`) or what the page shows: `"Save"`,
`button "Save"`, `"Edit" nth 2`, `"Edit" in "<row text>"`, `"Reply" after "<text>"`.
Each action prints its changes: read that instead of taking a snapshot.
`STALE_REF`: use the fresh ref it printed.

## Many steps: one call
Put known steps in one `tabref do`; each step prints its result. Snap only if the output
did not show what you need. It stops at a failure.

    tabref do <<'EOF'
    fill "Name" Ada
    fill "Email" a@b.c
    check "I agree"
    click "Submit"
    EOF

`for name qty in Pen 2 Cup 5` ... `end` runs the steps per pair as `{name}`, `{qty}`.
Pages: loop the `Next` or `2` link's address, page number as `{n}`:
`for n in 2 3 4`, `open "<address with {n}>"`, `find TEXT`, `end`; no address: `click "Next"` in the loop.
Lazy list: `scroll down --until "TEXT"` or `scroll down --to-end`; if it says more may load, run it again.
Steps: any action but `upload`, `open URL`, `expect text|gone|url X`,
`expect T is visible|enabled|hidden|gone|value=X`, `read [T]`, `find TEXT`, `js "EXPR"`, `snap`.
