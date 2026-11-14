"""hp3 (WebVoyager Cambridge Dictionary--17): how many meanings Cambridge Dictionary gives for "unblemished".
Path: the entry page -> decline the cookie banner if one shows -> count the distinct definitions.
The count uses `tabref js` over the definition blocks, as bh's runs did; a tuning track may swap it for read/find."""
import json

COUNT = ("[...new Set([...document.querySelectorAll('.def.ddef_d')].map(d => d.innerText.trim().replace(/:$/, '')))]"
         ".length")


def run(t):
    out = t("open", "https://dictionary.cambridge.org/dictionary/english/unblemished")
    for pattern in (r"reject all", r"decline", r"necessary only"):
        if t.has(out, pattern):
            try:
                t("click", t.ref(out, pattern))
                break
            except LookupError:
                pass
    raw = t("js", COUNT).strip().splitlines()
    value = json.loads(raw[-1]) if raw else None
    return f"DONE {value}"
