"""hw7 (WebArena 269): leave the browser on the Women Shoes listing filtered to products under $25.
Path: shop -> Women Shoes -> price "to" field = 24.99 -> Apply."""


def run(t):
    out = t("open", f"{t.base}/shop/")
    out = t("click", t.find_ref(out, r"link\W+women shoes"))
    t("fill", t.find_ref(out, r"textbox|textfield|input", nth=2), "24.99")
    t("click", t.find_ref(out, r"button\W+apply"))
    return "DONE"
