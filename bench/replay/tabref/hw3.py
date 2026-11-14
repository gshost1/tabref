"""hw3 (WebArena 777): lower the price of every size of one colour of one product family by $5.
Path: admin -> Catalog > Products -> search the family -> for each matching row: edit page, set price, Save."""
import re


def run(t):
    family, color = t.exp["hw3_family"], t.exp["hw3_color"]
    out = t("open", f"{t.base}/admin/")
    out = t("click", t.find_ref(out, r"link\W+catalog"))
    t("fill", t.find_ref(out, r"search by keyword"), family)
    t("press", "Enter")
    grid = t("read")
    rows = re.findall(rf"(\d{{3,5}})\W+{re.escape(family)}-([A-Z]{{1,3}})-{color}\b.*?\$([\d,]+\.\d{{2}})", grid)
    for pid, _size, price in rows:
        out = t("open", f"{t.base}/admin/catalog/product/{pid}/edit")
        t("fill", t.find_ref(out, r"price"), f"{float(price.replace(',', '')) - 5:.2f}")
        t("click", t.find_ref(out, r"button\W+save"))
    return "DONE"
