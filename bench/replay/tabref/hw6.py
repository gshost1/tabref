"""hw6 (WebArena 658): create an issue in a11yproject.com with a title, an assignee and a due date (end of a month).
Path: git -> a11yproject.com -> Issues -> New issue -> fill title, pick assignee, set due date -> Create issue."""
import calendar
import datetime
import re

TITLE = "401 bad gateway"


def run(t):
    month, year = t.exp["hw6_due_text"].split()
    m = list(calendar.month_name).index(month)
    due = datetime.date(int(year), m, calendar.monthrange(int(year), m)[1]).isoformat()
    out = t("open", f"{t.base}/git/")
    out = t("click", t.find_ref(out, r"link\W+a11yproject\W+a11yproject\.com"))
    out = t("click", t.find_ref(out, r"link\W+issues\b"))
    out = t("click", t.find_ref(out, r"link\W+new issue"))
    t("fill", t.find_ref(out, r"\btitle\b"), TITLE)
    name = t.exp["hw6_assignee"]
    option = re.search(rf"{re.escape(name)} \(@[\w.-]+\)", t.text())
    try:
        select = t.find_ref(out, r"assignee")
    except LookupError:
        select = t.ref(t.outs[-1], r"combobox|listbox|select")
    t("select", select, option.group(0) if option else name)
    t("fill", t.find_ref(out, r"due date"), due)
    t("click", t.find_ref(out, r"button\W+create issue"))
    return "DONE"
