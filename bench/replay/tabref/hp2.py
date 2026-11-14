"""hp2 (WebVoyager ArXiv--27): which categories Economics includes on arXiv, with their abbreviations.
Path: the category taxonomy page -> expand the Economics group (the page shows groups collapsed) -> find econ.*."""
import re

PAIR = re.compile(r"(econ\.[A-Z]{2})\W{0,4}\(?([A-Z][A-Za-z ,-]+?)\)?\s*(?:$|[\"\n])", re.M)


def run(t):
    out = t("open", "https://arxiv.org/category_taxonomy")
    pairs = dict(PAIR.findall(out))
    if len(pairs) < 2:
        t("click", t.find_ref(out, r"button\W+economics"))
        t.tolerate = True
        pairs = dict(PAIR.findall(t("find", "econ.")))
        if len(pairs) < 2:
            pairs = dict(PAIR.findall(t("read")))
        t.tolerate = False
    return "DONE " + "; ".join(f"{code} ({name.strip()})" for code, name in pairs.items())
