"""Dev tasks: 10 web tasks for tuning tabref beyond the held-out 10. NOT sealed: read them, tune on them, report
them per task. They are none of the held-out tasks and were written without looking at the sealed fresh set.

Seven run on the local site (dev/web/server.py, 127.0.0.1:8794); three read stable public pages. The interface is
tasks_heldout.py's (WEB_TASKS, web_seed, reset_payload, web_expectations, verify_web), so bench/harness/run_web.py
runs them with --taskset dev. Every local value comes from world(seed); every verifier judges the site's recorded
state, or truth it fetches itself, never the agent's claim alone. A public run whose truth cannot be fetched is
invalid (run_web.py), not failed.

Styles follow public benchmark lists (WebArena, WebVoyager, Mind2Web): one goal in plain words, real site patterns.
  dw1 clinic booking: dependent selects, date and time inputs, server-side validation errors to fix (phone format,
      taken slot: book the next free one)
  dw2 shipments: client-side sortable and filterable table with pages; pick the three heaviest in-transit ones
  dw3 API tokens: revoke the ones matching two conditions, each through a type-the-name confirmation modal
  dw4 notification matrix: toggles inside a shadow-DOM custom element, saved from inside the shadow root
  dw5 help desk: paginated ticket search, then an internal note and a status change in a same-origin iframe
  dw6 staff directory: infinite-scroll list, a duplicate name, then a busy profile page for a two-hop lookup
  dw7 CMS: multi-field edit (ARIA combobox, datetime-local, tag chips) beside near-duplicate posts
  dp1 pypi.org: a release date from a long release history (starts on the project page: pypi.org's search shows
      a client challenge to a HeadlessChrome user agent that never clears, the project page does not)
  dp2 docs.python.org: when a parameter was added, beside the function's own version note
  dp3 rfc-editor.org: a section number deep in a very long document
"""
import datetime
import html
import random
import re
import urllib.request

BENCH_UA = "tabref-bench-dev/1.0"
PUBLIC_RULES = (" This is a public website. Do not log in, do not submit or send anything, and decline or dismiss "
                "any cookie or consent banner.")
WEB_CHANNELS = ["ax", "cdp", "ad-web"]
ORDER = ["dw1", "dw2", "dw3", "dw4", "dw5", "dw6", "dw7", "dp1", "dp2", "dp3"]
SEED_BASE = 610007

SOURCES = {
    "dw1": ("Mind2Web style", "book an appointment", "a clinic booking form with server-side validation"),
    "dw2": ("WebArena style", "shopping_admin / ops tables", "rank filtered rows of a client-side data table"),
    "dw3": ("WebArena style", "gitlab settings", "revoke access tokens through a confirmation dialog"),
    "dw4": ("Mind2Web style", "account settings", "notification preferences in a web component"),
    "dw5": ("WebArena style", "help desk", "internal note and status change in an embedded editor"),
    "dw6": ("WebVoyager style", "people directory", "two-hop lookup in an infinite list and a busy profile"),
    "dw7": ("WebArena style", "CMS admin", "edit category, schedule and tags of one post among look-alikes"),
    "dp1": ("WebVoyager style", "pypi.org", "release date of an old version of a package"),
    "dp2": ("WebVoyager style", "docs.python.org", "version a parameter was added in"),
    "dp3": ("WebVoyager style", "rfc-editor.org", "section number of a status code in RFC 9110"),
}

FIRST = ["Amara", "Benedikt", "Carmen", "Dario", "Esther", "Felix", "Gwen", "Hamid", "Ilse", "Joaquin", "Keiko",
         "Leon", "Maren", "Nikolai", "Odile", "Pavel", "Quinn", "Rosa", "Sanjay", "Tamsin", "Ulrich", "Vera",
         "Wendell", "Ximena", "Yusuf", "Zora", "Anouk", "Bastian", "Celeste", "Dorian", "Elif", "Florin"]
LAST = ["Achterberg", "Bianchi", "Calloway", "Dragomir", "Eastwood", "Ferreira", "Gallagher", "Hartmann", "Iversen",
        "Jaramillo", "Kowalczyk", "Lindqvist", "Moreau", "Nakagawa", "Okonkwo", "Petrakis", "Quintero", "Rasmussen",
        "Sorensen", "Takahashi", "Underhill", "Valdivia", "Whitfield", "Yamamoto", "Zeller", "Abernathy", "Brennan",
        "Castellano", "Delacroix", "Esposito"]


def people(rng, count):
    pool = [f"{a} {b}" for a in FIRST for b in LAST]
    return rng.sample(pool, count)


def long_date(day):
    return f"{day.strftime('%B')} {day.day}, {day.year}"


def clock12(hhmm):
    h, m = map(int, hhmm.split(":"))
    return f"{(h - 1) % 12 + 1}:{m:02d} {'AM' if h < 12 else 'PM'}"


def quarter_slots():
    return [f"{h:02d}:{m:02d}" for h in range(8, 17) for m in (0, 15, 30, 45)]


DEPARTMENTS = ["Cardiology", "Dermatology", "Neurology", "Orthopedics", "Pediatrics", "Ophthalmology"]
REASONS = ["Follow-up on blood test results", "Recurring headaches since last month", "Annual check of a mole on the arm",
           "Knee pain after running", "Blurry vision when reading", "Second opinion on a prescription"]
CITY_PAIRS = [("Springfield", "Springdale"), ("Portland", "Portsmouth"), ("Newport", "Newark"),
              ("Kingston", "Kingsport"), ("Fairview", "Fairfield")]
CARRIERS = ["Northline", "Bluebird Freight", "Corvid Post", "Tern Express"]
STATUSES = ["In transit", "Delivered", "Delayed", "Out for delivery", "Returned"]
TOKEN_BASES = ["ci-deploy", "release-bot", "metrics-export", "backup-sync", "docs-preview", "pager-hook"]
SCOPES = ["read:repo", "write:repo", "read:packages", "write:packages", "admin:org", "read:user"]
EVENTS = ["Deploy failed", "Deploy succeeded", "New sign-in", "Billing receipt", "Weekly digest",
          "Mentioned in a comment", "Pull request review requested"]
CHANNELS = ["Email", "SMS", "Push"]
SUBJECTS = ["Invoice charged twice this month", "Cannot reset my password", "Export to CSV times out",
            "Wrong VAT number on receipts", "Mobile app crashes on login", "Need to change account owner",
            "Webhook deliveries failing with 502", "Refund for cancelled seat", "SSO setup with Okta",
            "Dashboard shows stale numbers", "Feature request: dark mode", "Data deletion request",
            "Shipping address cannot be saved", "API rate limit questions", "Calendar sync duplicates events"]
NOTES = ["Escalated to billing, waiting on finance to confirm the duplicate charge.",
         "Customer confirmed the issue persists after clearing cache; engineering ticket filed.",
         "Do not close: legal review of the deletion request is still open.",
         "Asked the customer for a HAR file; parking until they reply."]
DESK_STATUSES = ["Open", "Pending", "On-hold", "Solved"]
TEAMS = ["Finance", "Platform", "Design", "Legal", "Sales", "Support", "Research", "Security"]
TITLES = ["Analyst", "Engineer", "Senior Engineer", "Designer", "Counsel", "Account Executive", "Specialist", "Lead"]
OFFICES = ["Lisbon", "Toronto", "Osaka", "Nairobi", "Denver", "Tallinn"]
CATEGORIES = ["Announcements", "Engineering", "Engineering Culture", "Product", "Product Updates", "Design",
              "Security", "Security Advisories", "Community", "Events", "Release Notes", "Customer Stories",
              "Tutorials", "Research", "Hiring", "Company News", "Partners", "Infrastructure"]
CATEGORY_TWINS = [("Security", "Security Advisories"), ("Product", "Product Updates"),
                  ("Engineering", "Engineering Culture")]
TAGS = ["roadmap", "beta", "api", "mobile", "billing", "q3", "migration", "performance", "accessibility", "draft",
        "internal", "launch"]
POST_TITLES = ["Spring platform update", "How we cut build times in half", "Introducing audit logs",
               "Our accessibility checklist", "Moving the API to v3", "What we learned from the outage",
               "Pricing changes for teams", "Design tokens at scale", "Meet the new support portal",
               "Mobile offline mode is here", "Quarterly community roundup", "Hiring for the infra team"]


def world(seed):
    """Every value a local dev task shows or expects, from one seed."""
    rng = random.Random(f"dev-web:{seed}")
    names = people(rng, 400)
    w = {"seed": seed}
    w["dw1"] = clinic(rng, names[:40])
    w["dw2"] = shipments(rng)
    w["dw3"] = tokens(rng)
    w["dw4"] = notifications(rng)
    w["dw5"] = desk(rng, names[40:100])
    w["dw6"] = directory(rng, names[100:360])
    w["dw7"] = cms(rng, names[360:380])
    return w


def clinic(rng, names):
    doctors = []
    for i, dept in enumerate(DEPARTMENTS):
        for j in range(3):
            doctors.append({"id": f"d{i}{j}", "name": "Dr. " + names[3 * i + j], "dept": dept})
    doc = rng.choice(doctors)
    day = datetime.date(2026, 12, 1) + datetime.timedelta(days=rng.randint(0, 50))
    while day.weekday() >= 5:
        day += datetime.timedelta(days=1)
    slots = quarter_slots()
    i = rng.randint(4, 20)
    requested = slots[i]
    run = rng.randint(1, 3)
    booked = set(slots[i:i + 1 + run]) | set(rng.sample(slots[:i], min(i, rng.randint(1, 4))))
    booked |= set(rng.sample(slots[i + run + 2:], rng.randint(4, 8)))
    expected = next(s for s in slots[i + 1:] if s not in booked)
    digits = "".join(str(rng.randint(0, 9)) for _ in range(10))
    digits = str(rng.randint(2, 9)) + digits[1:]
    dob = datetime.date(rng.randint(1950, 2004), rng.randint(1, 12), rng.randint(1, 28))
    return {"doctors": doctors, "doctor": doc, "day": day.isoformat(), "requested": requested, "booked": sorted(booked),
            "expected_time": expected, "patient": names[30], "dob": dob.isoformat(), "phone": digits,
            "phone_display": f"({digits[:3]}) {digits[3:6]} {digits[6:]}", "reason": rng.choice(REASONS)}


def shipments(rng):
    city, twin = rng.choice(CITY_PAIRS)
    others = [c for pair in CITY_PAIRS for c in pair if c not in (city, twin)]
    weights = rng.sample(range(5, 800), 140)
    codes = rng.sample(range(100000, 999999), 140)
    rows = []
    for n, wt in enumerate(weights):
        if n < 6:
            dest, status = city, "In transit"
        elif n < 10:
            dest, status = city, rng.choice(["Delivered", "Delayed", "Out for delivery"])
        elif n < 16:
            dest, status = twin, "In transit"
        else:
            dest, status = rng.choice(others + [city, twin]), rng.choice(STATUSES)
        rows.append({"tracking": f"TRK-{codes[n]}", "carrier": rng.choice(CARRIERS), "dest": dest,
                     "status": status, "weight": wt / 10,
                     "shipped": (datetime.date(2026, 9, 1) + datetime.timedelta(days=rng.randint(0, 70))).isoformat()})
    top = sorted(rows[10:16], key=lambda r: -r["weight"])[:2] + sorted(rows[6:10], key=lambda r: -r["weight"])[:2]
    heavy = sorted(rng.sample(range(820, 990), 4))
    for r, wt in zip(top, heavy[::-1]):
        r["weight"] = wt / 10
    rng.shuffle(rows)
    hits = sorted((r for r in rows if r["dest"] == city and r["status"] == "In transit"), key=lambda r: -r["weight"])
    return {"rows": rows, "city": city, "expected": [r["tracking"] for r in hits[:3]],
            "others": [r["tracking"] for r in rows if r["tracking"] not in {h["tracking"] for h in hits[:3]}]}


def tokens(rng):
    cutoff = datetime.date(2026, 8, 1) + datetime.timedelta(days=rng.randint(0, 60))
    names = []
    for base in rng.sample(TOKEN_BASES, 4):
        names += [base, base + "-old", base + "-2"]
    rng.shuffle(names)
    k = rng.choice([2, 3])
    ids = rng.sample(range(3100, 9900), len(names))
    rows = []
    for n, name in enumerate(names):
        scopes = set(rng.sample([s for s in SCOPES if s != "write:packages"], rng.randint(1, 3)))
        kind = "target" if n < k else rng.choice(["recent", "noscope", "noscope-never", "other"])
        if kind in ("target", "recent"):
            scopes.add("write:packages")
        if kind == "target" and n == 0:
            used = None
        elif kind in ("target", "noscope"):
            used = cutoff - datetime.timedelta(days=rng.randint(1, 200))
        elif kind == "noscope-never":
            used = None
        else:
            used = cutoff + datetime.timedelta(days=rng.randint(1, 90))
        created = (used or cutoff) - datetime.timedelta(days=rng.randint(30, 400))
        rows.append({"id": ids[n], "name": name, "scopes": sorted(scopes), "created": created.isoformat(),
                     "used": used.isoformat() if used else None, "target": kind == "target"})
    rng.shuffle(rows)
    return {"tokens": rows, "cutoff": cutoff.isoformat(), "expected": sorted(t["id"] for t in rows if t["target"])}


def notifications(rng):
    while True:
        initial = {e: {c: rng.random() < 0.5 for c in CHANNELS} for e in EVENTS}
        ea, eb = rng.sample(EVENTS, 2)
        want = set(rng.sample(CHANNELS, rng.choice([1, 2])))
        turn_on = rng.random() < 0.4
        diff = sum(initial[ea][c] != (c in want) for c in CHANNELS)
        flips = sum(initial[eb][c] != turn_on for c in CHANNELS)
        if diff >= 2 and flips >= 2:
            break
    expected = {e: dict(v) for e, v in initial.items()}
    expected[ea] = {c: c in want for c in CHANNELS}
    expected[eb] = {c: turn_on for c in CHANNELS}
    return {"initial": initial, "a": ea, "want": [c for c in CHANNELS if c in want], "b": eb, "turn_on": turn_on,
            "expected": expected}


def desk(rng, names):
    target_who, twin_subject_who = names[0], names[1]
    subj, other_subj = rng.sample(SUBJECTS, 2)
    base = datetime.datetime(2026, 11, 2, 9, 0)
    ids = rng.sample(range(40000, 49999), 64)
    tickets = []
    for n in range(64):
        if n == 0:
            who, s = target_who, subj
        elif n == 1:
            who, s = target_who, other_subj
        elif n == 2:
            who, s = twin_subject_who, subj
        else:
            who, s = rng.choice(names[2:]), rng.choice(SUBJECTS)
        status = rng.choice(["Open", "Pending"]) if n < 3 else rng.choice(DESK_STATUSES)
        updated = base + datetime.timedelta(minutes=rng.randint(0, 60 * 24 * 12))
        tickets.append({"id": ids[n], "subject": s, "requester": who, "status": status,
                        "updated": updated.isoformat(timespec="minutes"),
                        "first": f"Hi, {s.lower()}. Can you help? Thanks, {who.split()[0]}"})
    target = tickets[0]["id"]
    tickets.sort(key=lambda t: t["updated"], reverse=True)
    pos = next(i for i, t in enumerate(tickets) if t["id"] == target)
    if pos < 15:
        t = tickets.pop(pos)
        tickets.insert(rng.randint(18, 50), t)
        t["updated"] = tickets[tickets.index(t) - 1]["updated"]
    return {"tickets": tickets, "target": target, "requester": target_who, "subject": subj, "note": rng.choice(NOTES)}


def directory(rng, names):
    people_rows = []
    exts = rng.sample(range(2000, 7999), len(names) + 1)
    ids = rng.sample(range(1000, 9999), len(names) + 1)
    hired = [datetime.date(2010, 1, 1) + datetime.timedelta(days=rng.randint(0, 5800)) for _ in names]
    for n, name in enumerate(names):
        people_rows.append({"id": ids[n], "name": name, "dept": rng.choice(TEAMS), "title": rng.choice(TITLES),
                            "office": rng.choice(OFFICES), "ext": exts[n], "hired": hired[n].isoformat(),
                            "mobile": f"+1 555 {rng.randint(100, 999)} {rng.randint(1000, 9999)}",
                            "room": f"{rng.randint(1, 9)}.{rng.randint(10, 99)}"})
    by_dept = {}
    for p in people_rows:
        by_dept.setdefault(p["dept"], []).append(p)
    heads = {d: rng.choice(ps) for d, ps in by_dept.items()}
    for p in people_rows:
        head = heads[p["dept"]]
        p["manager"] = None if p is head else head["id"]
    head_ids = {h["id"] for h in heads.values()}
    candidates = [p for p in people_rows if p["id"] not in head_ids]
    dept_a, dept_b = rng.sample(sorted({p["dept"] for p in candidates}), 2)
    target = rng.choice([p for p in candidates if p["dept"] == dept_a])
    twin = rng.choice([p for p in candidates if p["dept"] == dept_b])
    twin["name"] = target["name"]
    people_rows.sort(key=lambda p: p["hired"], reverse=True)
    for who in (target, twin):
        pos = people_rows.index(who)
        if pos < 80:
            people_rows.insert(rng.randint(90, len(people_rows) - 1), people_rows.pop(pos))
            at = people_rows.index(who)
            who["hired"] = people_rows[at - 1]["hired"]
    by_id = {p["id"]: p for p in people_rows}
    return {"people": people_rows, "target": target["id"], "twin": twin["id"], "name": target["name"],
            "dept": target["dept"], "manager": by_id[target["manager"]]["name"], "manager_ext": by_id[target["manager"]]["ext"],
            "twin_manager": by_id[twin["manager"]]["name"], "twin_manager_ext": by_id[twin["manager"]]["ext"],
            "own_ext": target["ext"]}


def cms(rng, names):
    title = rng.choice(POST_TITLES)
    twin_from, twin_to = rng.choice(CATEGORY_TWINS)
    to_long = rng.random() < 0.5
    new_cat, old_cat = (twin_to, twin_from) if to_long else (twin_from, twin_to)
    titles = [title, title + " (copy)", "Draft: " + title, title + " 2025"]
    titles += rng.sample([t for t in POST_TITLES if t != title], 8)
    titles += [t + " (archived)" for t in rng.sample([t for t in POST_TITLES if t != title], 6)]
    ids = rng.sample(range(700, 999), len(titles))
    posts = []
    for n, t in enumerate(titles):
        tags = rng.sample(TAGS, rng.randint(3, 4))
        cat = old_cat if n < 4 else rng.choice(CATEGORIES)
        posts.append({"id": ids[n], "title": t, "slug": re.sub(r"[^a-z0-9]+", "-", t.lower()).strip("-"),
                      "category": cat, "tags": tags, "status": "Draft", "publish_at": "",
                      "author": rng.choice(names)})
    target = posts[0]
    drop = rng.choice(target["tags"])
    day = datetime.date(2027, 1, 4) + datetime.timedelta(days=rng.randint(0, 80))
    at = f"{rng.randint(7, 18):02d}:{rng.choice(['00', '15', '30', '45'])}"
    rng.shuffle(posts)
    return {"posts": posts, "target": target["id"], "title": title, "category": new_cat, "drop": drop,
            "day": day.isoformat(), "time": at, "keep": sorted(t for t in target["tags"] if t != drop)}


def web_seed(task_id, run_idx):
    """Same seed for every arm on a given task and run index; disjoint from held-out's 7000-7700 range."""
    return SEED_BASE + 1009 * run_idx + 97 * ORDER.index(task_id)


def _norm(text):
    return re.sub(r"\s+", " ", str(text or "").replace("’", "'").replace("“", '"').replace("”", '"')).strip()


def _low(text):
    return _norm(text).lower()


def verify_dw1(w, answer, state):
    exp = w["dw1"]
    booked = state.get("appointments") or []
    want = {"patient": _low(exp["patient"]), "dob": exp["dob"], "phone": exp["phone"], "doctor": exp["doctor"]["id"],
            "date": exp["day"], "time": exp["expected_time"], "reason": _low(exp["reason"])}
    good = [a for a in booked if {"patient": _low(a.get("patient")), "dob": a.get("dob"),
                                  "phone": re.sub(r"\D", "", str(a.get("phone"))), "doctor": a.get("doctor"),
                                  "date": a.get("date"), "time": a.get("time"), "reason": _low(a.get("reason"))} == want]
    return len(good) == 1 and len(booked) == 1, f"want one booking {want}; booked {booked}"


def tracking_mentions(answer, known):
    """Known tracking numbers in the order the answer names them: TRK-123456, TRK 123456 or the bare 123456."""
    found = []
    for t in known:
        m = re.search(rf"(?<!\d){re.escape(t.split('-', 1)[1])}(?!\d)", answer)
        if m:
            found.append((m.start(), t))
    return sorted(found)


def verify_dw2(w, answer, state):
    exp = w["dw2"]
    found = [t for _, t in tracking_mentions(answer, exp["expected"] + exp["others"])]
    return found == exp["expected"], f"want {exp['expected']} in that order; answer names {found}"


def verify_dw3(w, answer, state):
    exp = w["dw3"]["expected"]
    revoked = sorted(int(x) for x in state.get("revoked") or [])
    return revoked == exp, f"want revoked {exp}; revoked {revoked}"


def verify_dw4(w, answer, state):
    exp = w["dw4"]["expected"]
    saved = state.get("prefs_saves") or 0
    prefs = state.get("prefs") or {}
    wrong = sorted(f"{e}/{c}" for e in EVENTS for c in CHANNELS if bool((prefs.get(e) or {}).get(c)) != exp[e][c])
    return saved > 0 and not wrong, f"{saved} saves; cells differing from the wanted matrix: {wrong}"


def verify_dw5(w, answer, state):
    exp = w["dw5"]
    posts = state.get("desk_posts") or []
    status = state.get("desk_status") or {}
    original = {t["id"]: t["status"] for t in exp["tickets"]}
    changed = sorted(int(k) for k, v in status.items() if int(k) != exp["target"] and v != original.get(int(k)))
    good = [p for p in posts if int(p.get("ticket") or 0) == exp["target"] and p.get("kind") == "internal"
            and _norm(p.get("body")) == _norm(exp["note"])]
    target_status = status.get(str(exp["target"]))
    ok = len(good) == 1 and len(posts) == 1 and target_status == "On-hold" and not changed
    return ok, (f"want one internal note on #{exp['target']} and status On-hold; posts {posts}; status "
                f"{target_status}; other tickets changed {changed}")


def verify_dw6(w, answer, state):
    exp = w["dw6"]
    parts = answer.split("|")
    if len(parts) < 2:
        return False, f"want NAME | EXTENSION = {exp['manager']} | {exp['manager_ext']}; answer has no '|'"
    name_ok = _low(exp["manager"]) in _low(parts[0])
    ext_ok = re.findall(r"(?<!\d)\d{4}(?!\d)", "|".join(parts[1:])) == [str(exp["manager_ext"])]
    twin = _low(exp["twin_manager"]) in _low(answer) and exp["twin_manager"] != exp["manager"]
    return name_ok and ext_ok and not twin, f"want {exp['manager']} | {exp['manager_ext']}"


def verify_dw7(w, answer, state):
    exp = w["dw7"]
    saved = {int(k): v for k, v in (state.get("posts") or {}).items()}
    original = {p["id"]: p for p in exp["posts"]}
    t = saved.get(exp["target"]) or {}
    want = dict(original[exp["target"]], category=exp["category"], status="Scheduled",
                publish_at=f"{exp['day']}T{exp['time']}", tags=exp["keep"])
    bad = sorted(k for k in want if (sorted(t.get(k) or []) if k == "tags" else t.get(k)) !=
                 (sorted(want[k]) if k == "tags" else want[k]))
    others = sorted(pid for pid, p in saved.items() if pid != exp["target"] and
                    {**p, "tags": sorted(p.get("tags") or [])} != {**original[pid], "tags": sorted(original[pid]["tags"])})
    return not bad and not others, f"target fields wrong: {bad} ({ {k: t.get(k) for k in bad} }); other posts changed {others}"


def _fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": BENCH_UA})
    with urllib.request.urlopen(request, timeout=20) as r:
        return r.read().decode("utf-8", errors="replace")


def _strip_tags(text):
    return re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", text))).strip()


PYPI_VERSION = "2.13.0"


def parse_pypi_release_dates(doc, version):
    """Every UTC date a version's files were uploaded on, from pypi.org's JSON API."""
    import json
    files = json.loads(doc).get("releases", {}).get(version) or []
    dates = sorted({f["upload_time_iso_8601"][:10] for f in files if f.get("upload_time_iso_8601")})
    if not dates:
        raise ValueError(f"no files for version {version}")
    return dates


def parse_batched_strict(page):
    start = page.find('id="itertools.batched"')
    if start < 0:
        raise ValueError("no itertools.batched entry")
    end = page.find('<dl class="py function">', start)
    block = page[start:end if end > 0 else start + 20000]
    found = re.search(r"(?:Changed|Added) in version (\d+\.\d+)\s*:?\s*(?:</span>)?\s*Added the\s*(?:<[^>]+>)*\s*strict",
                      block)
    if not found:
        raise ValueError("no version note for the strict option")
    return found.group(1)


def parse_rfc_section(page, anchor="name-418-unused"):
    found = re.search(rf'<h\d id="{anchor}">\s*<a href="#section-([\d.]+)"', page)
    if not found:
        raise ValueError(f"no heading {anchor}")
    return found.group(1)


def web_expectations(task_id):
    """Truth for the public tasks, fetched by the verifier side just before the run."""
    if task_id == "dp1":
        return {"dp1_dates": parse_pypi_release_dates(_fetch("https://pypi.org/pypi/requests/json"), PYPI_VERSION)}
    if task_id == "dp2":
        return {"dp2_version": parse_batched_strict(_fetch("https://docs.python.org/3/library/itertools.html"))}
    if task_id == "dp3":
        return {"dp3_section": parse_rfc_section(_fetch("https://www.rfc-editor.org/rfc/rfc9110.html"))}
    return {}


def date_mentions(answer):
    out = set()
    text = re.sub(r"(\d)(st|nd|rd|th)\b", r"\1", re.sub(r"\s+", " ", answer.replace(",", " ")))
    for fmt, rx in (("%d %b %Y", r"\b\d{1,2} [A-Z][a-z]{2,8}\.? \d{4}\b"), ("%b %d %Y", r"\b[A-Z][a-z]{2,8}\.? \d{1,2} \d{4}\b"),
                    ("%Y-%m-%d", r"\b\d{4}-\d{2}-\d{2}\b"), ("%m/%d/%Y", r"\b\d{1,2}/\d{1,2}/\d{4}\b"),
                    ("%Y/%m/%d", r"\b\d{4}/\d{1,2}/\d{1,2}\b")):
        for m in re.findall(rx, text):
            m = re.sub(r"\s+", " ", m.replace(".", ""))
            for f in (fmt, fmt.replace("%b", "%B")):
                try:
                    out.add(datetime.datetime.strptime(m, f).date().isoformat())
                except ValueError:
                    pass
    return out


VERSION_RX = r"(?<![\d.])3\.(\d{1,2})(?:\.\d+)?(?!\d)"


def strict_version_given(said, want, answer):
    """dp2 passes when the right minor version is the answer: the only version named, or, when another one (the
    batched-itself decoy) is named too, the only one in the clauses that mention "strict"."""
    if want not in said:
        return False
    if set(said) == {want}:
        return True
    clauses = re.split(r"[,;()]|\.(?!\d)|\b(?:and|but|while|whereas)\b", answer)
    near = {int(m) for c in clauses if re.search(r"strict", c, re.I) for m in re.findall(VERSION_RX, c)}
    return near == {want}


def verify_public(task_id, exp, answer):
    if task_id == "dp1":
        said = date_mentions(answer)
        return bool(said) and said <= set(exp["dp1_dates"]), f"want {exp['dp1_dates'][0]}; answer dates {sorted(said)}"
    if task_id == "dp2":
        want = exp["dp2_version"]
        said = [int(m) for m in re.findall(VERSION_RX, answer)]
        return strict_version_given(said, int(want.split(".")[1]), answer), f"want {want}; answer minors {said}"
    if task_id == "dp3":
        said = re.findall(r"(?<![\d.])\d+(?:\.\d+)+", answer)
        want = exp["dp3_section"]
        others = [x for x in said if x != want and x.count(".") >= 2]
        return want in said and not others, f"want section {want}; answer {said}"
    return False, "unknown public task"


LOCAL_VERIFIERS = {"dw1": verify_dw1, "dw2": verify_dw2, "dw3": verify_dw3, "dw4": verify_dw4, "dw5": verify_dw5,
                   "dw6": verify_dw6, "dw7": verify_dw7}


def verify_web(task, ctx, answer, state):
    """run_web.py's verify(task, ctx, answer, state)."""
    tid = task["id"]
    if tid.startswith("dp"):
        return verify_public(tid, ctx["exp"], answer or "")
    w = world(int(ctx["exp"]["seed"]))
    return LOCAL_VERIFIERS[tid](w, answer or "", state or {})


def web_prompt(tid):
    """Prompts with {base} and {exp[...]} placeholders, filled by run_web.py from the server's /__reset reply."""
    return {
        "dw1": ("In Google Chrome, open the clinic site at {base}/clinic/ and book an appointment for "
                "{exp[dw1_patient]} (date of birth {exp[dw1_dob_long]}, phone {exp[dw1_phone]}) with "
                "{exp[dw1_doctor]} in {exp[dw1_dept]} on {exp[dw1_day_long]} at {exp[dw1_time]}. If that time is "
                "taken, book the first free time after it on the same day. Reason for the visit: "
                "\"{exp[dw1_reason]}\"."),
        "dw2": ("In Google Chrome, open the shipments table at {base}/ops/shipments . Find the three heaviest "
                "shipments to {exp[dw2_city]} whose status is In transit. Reply with their tracking numbers, "
                "heaviest first, separated by commas."),
        "dw3": ("In Google Chrome, open the API tokens page at {base}/console/tokens . Revoke every token that has the "
                "write:packages scope and was last used before {exp[dw3_cutoff_long]} (a token that was never "
                "used counts too). Leave every other token as it is."),
        "dw4": ("In Google Chrome, open the notification settings at {base}/console/notifications . Set "
                "\"{exp[dw4_a]}\" to notify by {exp[dw4_want]} only, turn {exp[dw4_onoff]} every channel for "
                "\"{exp[dw4_b]}\", and save. Leave the other notification settings as they are."),
        "dw5": ("In Google Chrome, open the help desk at {base}/desk/ . Find the ticket that {exp[dw5_requester]} "
                "opened with the subject \"{exp[dw5_subject]}\", add an internal note (not a public reply) saying "
                "\"{exp[dw5_note]}\", and set the ticket's status to On-hold."),
        "dw6": ("In Google Chrome, open the staff directory at {base}/people/ . Find {exp[dw6_name]} who works in "
                "{exp[dw6_dept]}, and tell me the full name of the person they report to and that person's desk "
                "phone extension. Reply as: NAME | EXTENSION."),
        "dw7": ("In Google Chrome, open the blog CMS at {base}/cms/ and edit the post titled exactly "
                "\"{exp[dw7_title]}\": change its category to {exp[dw7_category]}, schedule it to publish on "
                "{exp[dw7_day_long]} at {exp[dw7_time]} (status Scheduled), and remove its \"{exp[dw7_drop]}\" tag "
                "while keeping its other tags. Save it, and do not change any other post."),
        "dp1": ("In Google Chrome, open the PyPI page of the Python package \"requests\" at "
                "https://pypi.org/project/requests/ and tell me on which date its version "
                f"{PYPI_VERSION} was released." + PUBLIC_RULES),
        "dp2": ("In Google Chrome, open https://docs.python.org/3/ , find the documentation of itertools.batched() "
                "and tell me in which Python version its strict parameter was added. Reply with the version."
                + PUBLIC_RULES),
        "dp3": ("In Google Chrome, open https://www.rfc-editor.org/ , find RFC 9110 (HTTP Semantics) and tell me the "
                "number of the section that covers status code 418. Reply with the section number." + PUBLIC_RULES),
    }[tid]


def reset_payload(seed):
    """What the server's /__reset returns: the seed plus the short values the prompts need."""
    w = world(seed)
    c, n, d = w["dw1"], w["dw4"], w["dw7"]
    want = n["want"]
    return {"seed": seed, "dw1_patient": c["patient"], "dw1_dob_long": long_date(datetime.date.fromisoformat(c["dob"])),
            "dw1_phone": c["phone_display"], "dw1_doctor": c["doctor"]["name"], "dw1_dept": c["doctor"]["dept"],
            "dw1_day_long": long_date(datetime.date.fromisoformat(c["day"])), "dw1_time": clock12(c["requested"]),
            "dw1_reason": c["reason"], "dw2_city": w["dw2"]["city"],
            "dw3_cutoff_long": long_date(datetime.date.fromisoformat(w["dw3"]["cutoff"])),
            "dw4_a": n["a"], "dw4_want": " and ".join(want), "dw4_onoff": "on" if n["turn_on"] else "off",
            "dw4_b": n["b"], "dw5_requester": w["dw5"]["requester"], "dw5_subject": w["dw5"]["subject"],
            "dw5_note": w["dw5"]["note"], "dw6_name": w["dw6"]["name"], "dw6_dept": w["dw6"]["dept"],
            "dw7_title": d["title"], "dw7_category": d["category"],
            "dw7_day_long": long_date(datetime.date.fromisoformat(d["day"])), "dw7_time": clock12(d["time"]),
            "dw7_drop": d["drop"]}


WEB_TASKS = [{"id": tid, "channels": WEB_CHANNELS, "prompt": web_prompt(tid)} for tid in ORDER]
