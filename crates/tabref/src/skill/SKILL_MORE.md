# tabref: rare commands

Downloads: an action or `open` that downloads prints the saved file's path. `tabref wait --download` waits
for one still running; `open --download-dir DIR URL` picks the folder (default for a browser tabref launched:
its state folder). In your own Chrome, tabref restores Chrome's default download behaviour after each
command; a download that starts after the command goes to Chrome's own folder and is not reported.

Browsers: `tabref open URL --launch` starts tabref's own Chrome when none is connected; add
`--profile DIR` to keep logins between runs (a profile of tabref's own, never yours). `tabref close --browser`
quits the browser `--launch` started (never one tabref did not start).
Only when the user asks for their own browser: `tabref connect --user-chrome` (opt-in; it drives their
logged-in Chrome and its sessions); `tabref connect --off` stops.

Apps built on Chrome (Electron, reached with `--cdp PORT`): `open`, `open --new-tab` and `close` are refused,
as the windows are the app; read and act with snap, click and fill (js to read).

Screenshots: `tabref screenshot [T] [--out FILE]` saves a PNG of the page, or of element T, and prints its path.

Waits: `tabref wait --text X`, `--gone X`, `--url TEXT` (the address contains it), `--idle`,
`--ref T --state enabled|visible|hidden|gone|value=X`. Every command takes `--timeout MS`
and `--json` (the `{version, ok, command, data|error}` envelope). Each command's options: `tabref CMD --help`.
