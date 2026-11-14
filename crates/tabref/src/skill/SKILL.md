---
name: tabref
description: Read and drive Chrome from the shell. Placeholder guide; the full skill is written later.
---

# tabref (placeholder guide)

tabref reads a Chrome tab into one line per element, each actionable element
with a ref such as `@s8f3k2p9:e4`, and acts on those refs.

    tabref open example.com --launch   # start headless Chrome if none is reachable, load, snapshot
    tabref snap                        # controls and headings; snap -a adds all text
    tabref click @s8f3k2p9:e4
    tabref fill @s8f3k2p9:e5 "ada@example.com"
    tabref press Enter @s8f3k2p9:e5
    tabref find "Total"                # search all text, prints the ref to act on
    tabref read                        # page text

Each snapshot gives new refs. An action that navigates prints the new page.
`STALE_REF` means the page changed: run `tabref snap` and use the new ref.
Add `--json` for the `{version, ok, command, data|error}` envelope.
