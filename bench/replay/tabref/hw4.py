"""hw4 (WebArena 404): upvote the newest post in the books forum.
Path: forum -> books -> sort New -> upvote the first post."""


def run(t):
    out = t("open", f"{t.base}/forum/")
    out = t("click", t.find_ref(out, r"link\W+books\b"))
    out = t("click", t.find_ref(out, r"link\W+new\b"))
    t("click", t.find_ref(out, r"upvote|▲"))
    return "DONE"
