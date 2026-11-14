"""hw2 (WebArena 47): how many Complete orders from the window start through today, and their total.
Path: shop -> My Account -> My Orders -> read order pages 1-3."""
import datetime
import re

ROW = re.compile(r"(\d{9})\W+(\d{1,2})/(\d{1,2})/(\d{2})\b(.*?)\$([\d,]+\.\d{2})\W+(Complete|Processing|Canceled|Pending|"
                 r"Closed|On Hold)", re.S)


def day(long):
    return datetime.datetime.strptime(long, "%B %d, %Y").date()


def run(t):
    start, today = day(t.exp["hw2_start_long"]), day(t.exp["hw2_today_long"])
    out = t("open", f"{t.base}/shop/")
    out = t("click", t.find_ref(out, r"link\W+my account"))
    t("click", t.find_ref(out, r"link\W+my orders"))
    text = t("read")
    for page in ("2", "3"):
        try:
            t("click", t.find_ref(t.outs[-1], rf'link\W+{page}\W*$'))
        except LookupError:
            break
        text += "\n" + t("read")
    orders = {}
    for number, m, d, y, _, total, status in ROW.findall(text):
        orders[number] = (datetime.date(2000 + int(y), int(m), int(d)), float(total.replace(",", "")), status)
    hits = [v for v in orders.values() if v[2] == "Complete" and start <= v[0] <= today]
    return f"{len(hits)} | {sum(v[1] for v in hits):.2f}"
