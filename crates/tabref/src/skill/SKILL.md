# tabref: read and drive Chrome from the shell

Start with `tabref skill && tabref open <url>`; Chrome is connected. Output is sized to read whole
(no grep or head) and commands wait for the page (never sleep). Downloads, waits, JS dialogs, logins,
the user's own Chrome, Chrome-based apps: `tabref skill more`. Options: `tabref CMD --help`.

## Reading
One line per block; `#` lines are headings. Controls show their ref: `[e5 button Save]`,
`[e7 Edit -> /admin/7/edit]` (open the address or click the ref). Lists say `N items:`, tables
`table N rows:`, repeats `(1/3)`: count with these. A cut ends with the command for the rest
(`tabref snap --rows 21-`); run it before answering about it. `tabref find TEXT` searches all text,
hidden too; `tabref read` prints the main text. A `dialog` line at the top: click one of its buttons (decline cookies).

## Acting
`click T`, `fill T VALUE`, `type T TEXT` (adds), `select T LABEL|VALUE`, `check T`, `uncheck T`,
`press KEY [T]`, `focus T`, `hover T`, `scroll down|T`, `double-click T`, `upload T FILE`.
T is a ref (`e4`) or what the page shows: `"Save"`, `button "Save"`, `"Edit" nth 2`,
`"Edit" in "<row text>"`, `"Reply" after "<text>"`; names survive a reload, refs do not.
Dates and times take plain values (`2027-07-31`, `14:30`). Each action prints what it changed (new address,
`"67.00" -> "62.00"`, new rows and controls): read that instead of taking a snapshot.

## Many steps: one call
Put every step you already know, such as a whole form and its submit, in one `tabref do`.
It stops at the first failing step; the last step prints the full change.

    tabref do <<'EOF'
    for name price in Cap 12.00 Mug 9.50
      click "Edit" in "{name}"
      fill "Price" {price}
    end
    click "Save"
    EOF

Steps: any action but `upload`, `open URL`, `expect text|gone|url X`, `expect T is visible|gone|value=X`,
`read [T]`, `find TEXT`, `js "EXPR"`, `snap`. To load more of a list, repeat `scroll down` in a `for`
loop, then `find`: each scroll waits for what it loads. `tabref js "EXPR"` returns a page value as JSON.
Tabs: `tabref tabs`, `tabref tab N`, `tabref open --new-tab URL`.
