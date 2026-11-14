"""hw5 (WebArena 409): reply to the site manager's (MOD) comment with the given text.
Path: thread -> the Reply toggle under the MOD comment -> its text box -> Post."""

REPLY = "thanks! I am a big fan of your website."


def run(t):
    out = t("open", f"{t.base}/forum/f/pics/{t.exp['hw5_post']}")
    if not t.has(out, r"\bMOD\b"):
        out = t("snap")
    out = t("click", t.ref(out, r"reply", after=r"\bMOD\b"))
    try:
        box = t.ref(out, r"textbox|textarea")
    except LookupError:
        out = t("snap")
        box = t.ref(out, r"textbox|textarea", after=r"\bMOD\b")
    t("fill", box, REPLY)
    t("click", t.ref(out, r"button\W+post", after=r"\bMOD\b") if t.has(out, r"\bMOD\b") else t.find_ref(out, r"button\W+post"))
    return "DONE"
