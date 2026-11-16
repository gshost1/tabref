#!/usr/bin/env python3
"""Scripted solver for the dev tasks over raw CDP (cdp.py, not tabref): proves every task can be done in a real
Chrome from what its pages show, and that the verifier accepts the result.

Each solver reads the page the way an agent would (the prompt's values from the /__reset reply, everything else from
the DOM), acts with mouse clicks, keyboard input and value-plus-events on date, time and select inputs, and returns
an answer. The harness's own Site starts the dev server; verify_web then judges the site's recorded state.

  ~/bin/chrome-slot python3 -I -B bench/dev/tests/solve_cdp.py [--runs 0-4] [--tasks dw1,dp2] [--base URL]

Public tasks run on run 0 only (their pages do not depend on the seed; --public-every-run to repeat them). Chrome
runs headless on a fresh profile under <main checkout>/.cache/dev-solver/ (never /tmp), removed at the end.
Exit 0 only when every attempted task passes; a public task whose truth cannot be fetched counts as invalid.
"""
import argparse
import datetime
import json
import re
import shutil
import sys
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEV = HERE.parent
BENCH = DEV.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(DEV))
sys.path.insert(0, str(BENCH / "harness"))
import cdp  # noqa: E402
import core  # noqa: E402
import tasks_dev as td  # noqa: E402


def q(sel, root="document"):
    return f"{root}.querySelector({json.dumps(sel)})"


def by_text(sel, text, root="document"):
    return f"Array.from({root}.querySelectorAll({json.dumps(sel)})).find(e => e.textContent.trim() === {json.dumps(text)})"


def to24(clock):
    return datetime.datetime.strptime(clock, "%I:%M %p").strftime("%H:%M")


def iso(long_text):
    return datetime.datetime.strptime(long_text, "%B %d, %Y").date().isoformat()


def submit(pg, el):
    pg.mark()
    pg.click(el)
    pg.settle()


def dw1(pg, base, exp):
    pg.goto(base + "/clinic/")
    submit(pg, by_text("a", "Book an appointment"))
    requested = to24(exp["dw1_time"])
    pg.type_into(q("input[name=patient]"), exp["dw1_patient"])
    pg.set_value(q("input[name=dob]"), iso(exp["dw1_dob_long"]))
    pg.type_into(q("input[name=phone]"), exp["dw1_phone"])
    pg.set_value(q("#department"), exp["dw1_dept"])
    doctor = pg.js(f"({by_text('#doctor option', exp['dw1_doctor'])}).value")
    pg.set_value(q("#doctor"), doctor)
    pg.set_value(q("input[name=date]"), iso(exp["dw1_day_long"]))
    pg.set_value(q("input[name=time]"), requested)
    pg.type_into(q("textarea[name=reason]"), exp["dw1_reason"])
    pg.click(q("input[name=consent]"))
    for _ in range(3):
        submit(pg, by_text("button", "Book appointment"))
        if pg.js("location.pathname").startswith("/clinic/booked/"):
            return pg.js("document.querySelector('main p').textContent")
        errors = pg.js("Array.from(document.querySelectorAll('.errors li')).map(e => e.textContent)") or []
        for err in errors:
            if "Phone number" in err:
                digits = re.sub(r"\D", "", exp["dw1_phone"])
                pg.type_into(q("input[name=phone]"), f"{digits[:3]}-{digits[3:6]}-{digits[6:]}")
            elif "Free times that day:" in err:
                free = [to24(x.strip()) for x in err.split("Free times that day:")[1].strip(" .").split(",")]
                pg.set_value(q("input[name=time]"), next(t for t in free if t > requested))
            else:
                raise cdp.CDPError(f"unexpected form error: {err}")
    raise cdp.CDPError("booking never went through")


def dw2(pg, base, exp):
    pg.goto(base + "/ops/shipments")
    city = exp["dw2_city"]
    pg.type_into(q("#filter"), city)
    pg.set_value(q("#status"), "In transit")
    for _ in range(2):
        pg.click(q("th[data-key=weight] button"))
    pg.wait("document.querySelector('th[data-key=weight]').getAttribute('aria-sort') === 'descending'")
    rows = pg.js("Array.from(document.querySelectorAll('#grid tbody tr')).map(tr => Array.from(tr.cells).map(c => c.textContent))")
    hits = [r[0] for r in rows if r[2] == city and r[3] == "In transit"]
    return ", ".join(hits[:3])


def dw3(pg, base, exp):
    pg.goto(base + "/console/tokens")
    cutoff = iso(exp["dw3_cutoff_long"])
    rows = pg.js("""Array.from(document.querySelectorAll('tbody tr')).map(tr => ({id: tr.dataset.id,
        name: tr.cells[0].textContent.trim(), scopes: Array.from(tr.cells[1].querySelectorAll('code')).map(c => c.textContent),
        used: tr.cells[3].textContent.trim()}))""")
    revoked = []
    for r in rows:
        used = None if r["used"] == "Never" else datetime.datetime.strptime(r["used"], "%b %d, %Y").date().isoformat()
        if "write:packages" not in r["scopes"] or (used and used >= cutoff):
            continue
        pg.click(q(f"button.revoke[data-id='{r['id']}']"))
        pg.wait("!document.getElementById('modal').hidden", what="modal")
        pg.type_into(q("#mconfirm"), r["name"])
        pg.wait("!document.getElementById('mgo').disabled", what="confirm enabled")
        pg.click(q("#mgo"))
        pg.wait(f"!document.querySelector(\"tr[data-id='{r['id']}']\") && document.getElementById('modal').hidden",
                what="row removed")
        revoked.append(r["name"])
    return "revoked " + ", ".join(revoked)


def dw4(pg, base, exp):
    pg.goto(base + "/console/notifications")
    root = "document.querySelector('notif-matrix').shadowRoot"
    pg.wait(f"{root} && {root}.querySelector('#save')", what="shadow widget")
    want = {exp["dw4_a"]: {c: c in exp["dw4_want"].split(" and ") for c in td.CHANNELS},
            exp["dw4_b"]: {c: exp["dw4_onoff"] == "on" for c in td.CHANNELS}}
    for ev, cells in want.items():
        for ch, on in cells.items():
            el = q(f"button[data-ev='{ev}'][data-ch='{ch}']", root)
            if (pg.js(f"{el}.getAttribute('aria-checked')") == "true") != on:
                pg.click(el)
    pg.click(q("#save", root))
    pg.wait(f"{root}.getElementById('msg').textContent === 'Preferences saved.'", what="saved")
    return "saved"


def dw5(pg, base, exp):
    pg.goto(base + "/desk/")
    pg.type_into(q("input[name=q]"), exp["dw5_requester"])
    submit(pg, by_text("button", "Search"))
    find = (f"Array.from(document.querySelectorAll('tr')).find(tr => tr.cells[1] && tr.cells[1].textContent === "
            f"{json.dumps(exp['dw5_subject'])} && tr.cells[2].textContent === {json.dumps(exp['dw5_requester'])})")
    while not pg.js(f"!!({find})"):
        nxt = pg.js("(function(){var b=document.querySelector('nav[aria-label=Pages] b');"
                    "var a=b&&b.nextElementSibling;return a&&a.tagName==='A'?a.href:null;})()")
        if not nxt:
            raise cdp.CDPError("ticket not found")
        pg.goto(nxt)
    submit(pg, f"({find}).querySelector('a')")
    doc = "document.querySelector('iframe').contentDocument"
    pg.wait(f"{doc} && {doc}.readyState === 'complete' && {doc}.querySelector('textarea')", what="composer")
    pg.click(q("input[value=internal]", doc))
    pg.type_into(q("textarea[name=body]", doc), exp["dw5_note"])
    pg.set_value(q("select[name=status]", doc), "On-hold")
    pg.click(by_text("button", "Submit", doc))
    pg.wait(f"{doc} && {doc}.querySelector('[role=status]') && "
            f"{doc}.querySelector('[role=status]').textContent.indexOf('On-hold') >= 0", what="composer sent")
    return "note added"


def dw6(pg, base, exp):
    pg.goto(base + "/people/")
    name, dept = exp["dw6_name"], exp["dw6_dept"]
    cards = f"Array.from(document.querySelectorAll('#list .card')).filter(c => c.querySelector('a').textContent === {json.dumps(name)})"
    for _ in range(40):
        hit = pg.js(f"(function(){{var c=({cards}).find(c => c.querySelector('span').textContent.split(' · ')[1] === "
                    f"{json.dumps(dept)});return c ? c.querySelector('a').href : null;}})()")
        if hit:
            break
        if pg.js("document.getElementById('more').textContent") == "End of directory.":
            raise cdp.CDPError("person not in the directory")
        n = pg.js("document.querySelectorAll('#list .card').length")
        pg.call("Input.dispatchMouseEvent", type="mouseWheel", x=640, y=450, deltaX=0, deltaY=4000)
        try:
            pg.wait(f"document.querySelectorAll('#list .card').length > {n} || "
                    "document.getElementById('more').textContent === 'End of directory.'", 5)
        except cdp.CDPError:
            pass
    else:
        raise cdp.CDPError("gave up scrolling")
    submit(pg, f"document.querySelector('a[href=\"' + new URL({json.dumps(hit)}).pathname + '\"]')")
    row = "Array.from(document.querySelectorAll('.card th')).find(th => th.textContent === {}).nextElementSibling"
    boss = pg.js(f"({row.format(json.dumps('Reports to'))}).textContent")
    submit(pg, f"({row.format(json.dumps('Reports to'))}).querySelector('a')")
    ext = pg.js(f"({row.format(json.dumps('Desk ext.'))}).textContent")
    return f"{boss} | {ext}"


def dw7(pg, base, exp):
    pg.goto(base + "/cms/")
    link = by_text("td a", exp["dw7_title"])
    while not pg.js(f"!!({link})"):
        nxt = pg.js("(function(){var b=document.querySelector('nav[aria-label=Pages] b');"
                    "var a=b&&b.nextElementSibling;return a&&a.tagName==='A'?a.href:null;})()")
        if not nxt:
            raise cdp.CDPError("post not found")
        pg.goto(nxt)
    submit(pg, link)
    pg.type_into(q("#catbox"), exp["dw7_category"])
    pg.click(f"Array.from(document.querySelectorAll('#catlist [role=option]')).find(o => !o.hidden && "
             f"o.dataset.value === {json.dumps(exp['dw7_category'])})")
    pg.wait(f"document.getElementById('catval').value === {json.dumps(exp['dw7_category'])}", what="category picked")
    pg.set_value(q("input[name=publish_at]"), f"{iso(exp['dw7_day_long'])}T{to24(exp['dw7_time'])}")
    pg.set_value(q("select[name=status]"), "Scheduled")
    pg.click(q(f"button[aria-label='Remove tag {exp['dw7_drop']}']"))
    submit(pg, by_text("button", "Save post"))
    if not pg.js("!!document.querySelector('[role=status]')"):
        raise cdp.CDPError(f"not saved: {pg.js('document.body.innerText')[:300]}")
    return "saved"


def dp1(pg, base, exp):
    pg.goto("https://pypi.org/project/requests/")
    pg.click(q("a[href='#history']"))
    card = f"document.querySelector('a[href=\"/project/requests/{td.PYPI_VERSION}/\"]').closest('.release')"
    pg.wait(card, what="release card")
    return f"Version {td.PYPI_VERSION} was released on " + pg.js(f"{card}.querySelector('time').textContent.trim()")


def dp2(pg, base, exp):
    pg.goto("https://docs.python.org/3/")
    submit(pg, "Array.from(document.querySelectorAll('a')).find(a => a.textContent.trim().startsWith('Library reference'))")
    submit(pg, "Array.from(document.querySelectorAll('a')).find(a => a.textContent.indexOf('itertools') === 0)")
    notes = pg.js("Array.from(document.getElementById('itertools.batched').parentNode.querySelectorAll('.versionmodified'))"
                  ".map(s => s.parentNode.textContent)")
    note = next(n for n in notes if "strict" in n)
    return "Python " + re.search(r"version (\d+\.\d+)", note).group(1)


def dp3(pg, base, exp):
    pg.goto("https://www.rfc-editor.org/")
    pg.goto("https://www.rfc-editor.org/rfc/rfc9110.html")
    head = pg.js("Array.from(document.querySelectorAll('h4')).map(h => h.textContent.trim()).find(t => t.indexOf('418 (Unused)') >= 0)")
    return "Section " + head.split()[0].rstrip(".")


SOLVERS = {"dw1": dw1, "dw2": dw2, "dw3": dw3, "dw4": dw4, "dw5": dw5, "dw6": dw6, "dw7": dw7,
           "dp1": dp1, "dp2": dp2, "dp3": dp3}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", default="0-4")
    ap.add_argument("--tasks", default="")
    ap.add_argument("--base", default="http://127.0.0.1:18794")
    ap.add_argument("--public-every-run", action="store_true")
    ap.add_argument("--profile", default=str(core.main_checkout() / ".cache" / "dev-solver" / "profile"))
    a = ap.parse_args()
    lo, _, hi = a.runs.partition("-")
    runs = list(range(int(lo), int(hi or lo) + 1))
    tasks = a.tasks.split(",") if a.tasks else td.ORDER
    by_id = {t["id"]: t for t in td.WEB_TASKS}
    results = []
    chrome = cdp.Chrome(a.profile)
    try:
        with core.Site(a.base, None, DEV / "web" / "server.py") as site:
            for run in runs:
                for tid in tasks:
                    if tid.startswith("dp") and run and not a.public_every_run:
                        continue
                    exp = site.reset(td.web_seed(tid, run))
                    try:
                        exp.update(td.web_expectations(tid))
                    except Exception as err:
                        results.append((tid, run, None, f"invalid: truth unavailable: {err}"))
                        print(f"{tid} run {run} INVALID truth unavailable: {err}", flush=True)
                        continue
                    pg, target = chrome.new_page()
                    t0 = time.time()
                    try:
                        answer = SOLVERS[tid](pg, a.base, exp)
                    except Exception:
                        answer = ""
                        print(traceback.format_exc(limit=3), flush=True)
                    finally:
                        pg.close()
                        chrome.close_page(target)
                    ok, why = td.verify_web(by_id[tid], {"exp": exp}, answer, site.state())
                    results.append((tid, run, ok, why))
                    print(f"{tid} run {run} ok={ok} {time.time() - t0:5.1f}s answer={answer[:80]!r} | {why[:140]}",
                          flush=True)
    finally:
        chrome.stop()
        shutil.rmtree(Path(a.profile).parent, ignore_errors=True)
    passed = sum(1 for r in results if r[2])
    invalid = sum(1 for r in results if r[2] is None)
    print(f"solver: {passed}/{len(results)} passed, {invalid} invalid", flush=True)
    raise SystemExit(0 if passed == len(results) else 1)


if __name__ == "__main__":
    main()
