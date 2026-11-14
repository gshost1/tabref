"""hw1 (WebArena 21): every reviewer who says the product's ear cups are too small.
Path: shop -> search the product -> product page -> read review pages 1-3."""
import re


def complainers(text):
    names = []
    for name, body in re.findall(r"Review by\s+([A-Z][\w'-]+ [A-Z][\w'-]+)\W*(.*?)(?=Review by|Page:|$)", text, re.S):
        body = body.lower()
        if re.search(r"\bcups?\b|earcups", body) and re.search(r"\b(small|tiny)\b", body) and name not in names:
            names.append(name)
    return names


def run(t):
    product = t.exp["hw1_product"]
    out = t("open", f"{t.base}/shop/")
    t("fill", t.find_ref(out, r"search entire store"), product)
    out = t("press", "Enter")
    t("click", t.find_ref(out, rf"link\W+{re.escape(product)}"))
    text = t("read")
    for page in ("2", "3"):
        try:
            t("click", t.find_ref(t.outs[-1], rf'link\W+{page}\W*$'))
        except LookupError:
            break
        text += "\n" + t("read")
    return ", ".join(complainers(text))
