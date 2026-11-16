"""Unit tests of the dev task set: every verifier accepts the right outcome and rejects wrong ones, driven through the
dev server's handler (no socket) or fixture snippets of the public pages (no network); plus the --taskset dev wiring.

Run: python3 -I -B -m unittest discover -s bench/dev/tests -p 'test_*.py'
"""
import argparse
import datetime
import importlib.util
import io
import json
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path
from urllib.parse import urlencode

HERE = Path(__file__).resolve().parent
DEV = HERE.parent
BENCH = DEV.parent
FIX = HERE / "fixtures"
sys.dont_write_bytecode = True
sys.path.insert(0, str(DEV))
sys.path.insert(0, str(DEV / "web"))
import server  # noqa: E402
import tasks_dev as td  # noqa: E402

TASK = {t["id"]: t for t in td.WEB_TASKS}


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class FakeRequest(server.Handler):
    def __init__(self, path, body=b"", ctype="application/x-www-form-urlencoded"):
        self.path = path
        self.headers = {"Content-Length": str(len(body)), "Content-Type": ctype}
        self.rfile = io.BytesIO(body)
        self.sent = []

    def send(self, status, body, ctype="text/html; charset=utf-8", extra=None):
        self.sent.append((status, body if isinstance(body, str) else body.decode(), extra or {}))


def get(path):
    req = FakeRequest(path)
    req.do_GET()
    return req.sent[-1]


def post(path, form):
    req = FakeRequest(path, urlencode(form, doseq=True).encode())
    req.do_POST()
    return req.sent[-1]


def post_json(path, value):
    req = FakeRequest(path, json.dumps(value).encode(), "application/json")
    req.do_POST()
    return req.sent[-1]


def state():
    return json.loads(get("/__state")[1])


class Base(unittest.TestCase):
    tid = None

    def setUp(self):
        self.seed = td.web_seed(self.tid or "dw1", 1)
        self.exp = json.loads(get(f"/__reset?seed={self.seed}")[1])
        self.w = td.world(self.seed)

    def verdict(self, answer=""):
        return td.verify_web(TASK[self.tid], {"exp": self.exp}, answer, state())[0]


class WorldAndSeeds(unittest.TestCase):
    def test_world_is_deterministic_and_seed_dependent(self):
        self.assertEqual(td.world(td.web_seed("dw1", 0)), td.world(td.web_seed("dw1", 0)))
        self.assertNotEqual(td.world(td.web_seed("dw1", 0))["dw2"], td.world(td.web_seed("dw1", 1))["dw2"])

    def test_seeds_are_disjoint_from_heldout_and_ids_are_new(self):
        sys.path.insert(0, str(BENCH / "heldout"))
        import tasks_heldout as th
        ho = {th.web_seed(t["id"], r) for t in th.WEB_TASKS for r in range(50)}
        dev = {td.web_seed(t, r) for t in td.ORDER for r in range(50)}
        self.assertFalse(ho & dev)
        self.assertEqual(len(dev), 500)
        self.assertFalse({t["id"] for t in th.WEB_TASKS} & set(td.ORDER))

    def test_prompts_format_for_every_run_and_name_only_dev_pages(self):
        for run in range(5):
            for t in td.WEB_TASKS:
                text = t["prompt"].format(base="http://127.0.0.1:8794", exp=dict(td.reset_payload(td.web_seed(t["id"], run))))
                self.assertNotIn("{", text)
                for path in ("/shop/", "/forum/", "/git/", "/admin/"):
                    self.assertNotIn(path, text)

    def test_every_local_page_a_prompt_names_is_served(self):
        get(f"/__reset?seed={td.web_seed('dw1', 0)}")
        payload = td.reset_payload(td.web_seed("dw1", 0))
        for t in td.WEB_TASKS:
            for url in re.findall(r"http://127\.0\.0\.1:8794(\S*)", t["prompt"].format(base="http://127.0.0.1:8794", exp=payload)):
                self.assertEqual(get(url.rstrip(".,"))[0], 200, url)

    def test_hard_parts_are_seeded_in(self):
        for run in range(5):
            w = td.world(td.web_seed("dw6", run))
            names = [p["name"] for p in w["dw6"]["people"]]
            self.assertEqual(names.count(w["dw6"]["name"]), 2)
            self.assertGreaterEqual(min(i for i, n in enumerate(names) if n == w["dw6"]["name"]), 30)
            c = w["dw1"]
            self.assertIn(c["requested"], c["booked"])
            self.assertGreater(c["expected_time"], c["requested"])
            desk = w["dw5"]["tickets"]
            self.assertGreaterEqual([t["id"] for t in desk].index(w["dw5"]["target"]), 15)


class Clinic(Base):
    tid = "dw1"

    def form(self, **over):
        c = self.w["dw1"]
        f = {"patient": c["patient"], "dob": c["dob"], "phone": f"{c['phone'][:3]}-{c['phone'][3:6]}-{c['phone'][6:]}",
             "department": c["doctor"]["dept"], "doctor": c["doctor"]["id"], "date": c["day"], "time": c["expected_time"],
             "reason": c["reason"], "consent": "yes"}
        f.update(over)
        return f

    def test_the_prompt_as_written_hits_both_validation_errors(self):
        status, body, _ = post("/clinic/book", self.form(phone=self.exp["dw1_phone"], time=self.w["dw1"]["requested"]))
        self.assertEqual(status, 200)
        self.assertIn("Phone number must be written", body)
        self.assertIn("Free times that day", body)
        self.assertEqual(state()["appointments"], [])
        self.assertFalse(self.verdict())

    def test_right_booking_passes(self):
        self.assertEqual(post("/clinic/book", self.form())[0], 303)
        self.assertTrue(self.verdict())

    def test_an_earlier_free_time_fails(self):
        c = self.w["dw1"]
        earlier = next(s for s in td.quarter_slots() if s not in c["booked"] and s < c["requested"])
        self.assertEqual(post("/clinic/book", self.form(time=earlier))[0], 303)
        self.assertFalse(self.verdict())

    def test_wrong_doctor_or_a_second_booking_fails(self):
        other = next(d for d in self.w["dw1"]["doctors"] if d["dept"] == self.w["dw1"]["doctor"]["dept"]
                     and d["id"] != self.w["dw1"]["doctor"]["id"])
        post("/clinic/book", self.form(doctor=other["id"]))
        self.assertFalse(self.verdict())
        get(f"/__reset?seed={self.seed}")
        post("/clinic/book", self.form())
        later = [s for s in td.quarter_slots() if s > self.w["dw1"]["expected_time"] and s not in self.w["dw1"]["booked"]]
        post("/clinic/book", self.form(time=later[0]))
        self.assertFalse(self.verdict())


class Shipments(Base):
    tid = "dw2"

    def test_right_order_passes_and_wrong_ones_fail(self):
        d = self.w["dw2"]
        self.assertTrue(self.verdict(", ".join(d["expected"])))
        self.assertFalse(self.verdict(", ".join(reversed(d["expected"]))))
        city_other_status = max((r for r in d["rows"] if r["dest"] == d["city"] and r["status"] != "In transit"),
                                key=lambda r: r["weight"])
        self.assertGreater(city_other_status["weight"], max(r["weight"] for r in d["rows"] if r["tracking"] in d["expected"]))
        self.assertFalse(self.verdict(", ".join([city_other_status["tracking"]] + d["expected"][:2])))
        self.assertFalse(self.verdict(", ".join(d["expected"][:2])))


class Tokens(Base):
    tid = "dw3"

    def revoke(self, tid):
        tok = next(t for t in self.w["dw3"]["tokens"] if t["id"] == tid)
        return post_json("/console/tokens/revoke", {"id": tid, "confirm": tok["name"]})[0]

    def test_right_set_passes(self):
        for t in self.w["dw3"]["expected"]:
            self.assertEqual(self.revoke(t), 200)
        self.assertTrue(self.verdict())

    def test_too_few_or_too_many_fail(self):
        self.revoke(self.w["dw3"]["expected"][0])
        self.assertFalse(self.verdict())
        for t in self.w["dw3"]["expected"][1:]:
            self.revoke(t)
        decoy = next(t for t in self.w["dw3"]["tokens"] if t["id"] not in self.w["dw3"]["expected"])
        self.revoke(decoy["id"])
        self.assertFalse(self.verdict())

    def test_a_wrong_confirmation_revokes_nothing(self):
        t = self.w["dw3"]["expected"][0]
        self.assertEqual(post_json("/console/tokens/revoke", {"id": t, "confirm": "nope"})[0], 400)
        self.assertEqual(state()["revoked"], [])


class Prefs(Base):
    tid = "dw4"

    def test_saving_the_wanted_matrix_passes(self):
        self.assertEqual(post_json("/console/notifications/save", {"prefs": self.w["dw4"]["expected"]})[0], 200)
        self.assertTrue(self.verdict())

    def test_unsaved_untouched_or_collateral_changes_fail(self):
        self.assertFalse(self.verdict())
        post_json("/console/notifications/save", {"prefs": self.w["dw4"]["initial"]})
        self.assertFalse(self.verdict())
        extra = json.loads(json.dumps(self.w["dw4"]["expected"]))
        other = next(x for x in td.EVENTS if x not in (self.w["dw4"]["a"], self.w["dw4"]["b"]))
        extra[other]["Email"] = not extra[other]["Email"]
        post_json("/console/notifications/save", {"prefs": extra})
        self.assertFalse(self.verdict())


class Desk(Base):
    tid = "dw5"

    def note(self, ticket, kind="internal", status="On-hold", body=None):
        return post(f"/desk/ticket/{ticket}/composer", {"kind": kind, "body": body or self.w["dw5"]["note"],
                                                         "status": status})[0]

    def test_internal_note_and_on_hold_passes(self):
        self.assertEqual(self.note(self.w["dw5"]["target"]), 303)
        self.assertTrue(self.verdict())

    def test_public_reply_wrong_ticket_or_no_status_fail(self):
        self.note(self.w["dw5"]["target"], kind="public")
        self.assertFalse(self.verdict())
        get(f"/__reset?seed={self.seed}")
        d = self.w["dw5"]
        twin = next(t for t in d["tickets"] if t["subject"] == d["subject"] and t["requester"] != d["requester"])
        self.note(twin["id"])
        self.assertFalse(self.verdict())
        get(f"/__reset?seed={self.seed}")
        current = next(t for t in d["tickets"] if t["id"] == d["target"])["status"]
        self.note(d["target"], status=current)
        self.assertFalse(self.verdict())

    def test_the_composer_is_its_own_page_inside_the_ticket(self):
        body = get(f"/desk/ticket/{self.w['dw5']['target']}")[1]
        self.assertIn(f"<iframe src=/desk/ticket/{self.w['dw5']['target']}/composer", body)
        self.assertNotIn("<textarea", body)


class Directory(Base):
    tid = "dw6"

    def test_manager_and_extension_pass_and_look_alikes_fail(self):
        d = self.w["dw6"]
        self.assertTrue(self.verdict(f"{d['manager']} | {d['manager_ext']}"))
        self.assertFalse(self.verdict(f"{d['twin_manager']} | {d['twin_manager_ext']}"))
        self.assertFalse(self.verdict(f"{d['manager']} | {d['own_ext']}"))
        self.assertFalse(self.verdict(f"{d['manager']} {d['manager_ext']}"))

    def test_the_list_is_paged_by_the_api_and_the_profile_names_the_manager(self):
        d = self.w["dw6"]
        first = json.loads(get("/people/api/list?offset=0&limit=30")[1])
        self.assertEqual(first["total"], len(d["people"]))
        self.assertNotIn(d["name"], [p["name"] for p in first["people"]])
        body = get(f"/people/{d['target']}")[1]
        self.assertIn(d["manager"], body)
        self.assertNotIn(str(d["manager_ext"]), body)


class Cms(Base):
    tid = "dw7"

    def form(self, pid=None, **over):
        d = self.w["dw7"]
        p = next(x for x in d["posts"] if x["id"] == (pid or d["target"]))
        f = {"title": p["title"], "slug": p["slug"], "category": d["category"], "status": "Scheduled",
             "publish_at": f"{d['day']}T{d['time']}", "tags": d["keep"]}
        f.update(over)
        return f

    def test_right_edit_passes(self):
        self.assertEqual(post(f"/cms/posts/{self.w['dw7']['target']}/edit", self.form())[0], 303)
        self.assertTrue(self.verdict())

    def test_lost_tags_wrong_status_or_an_edited_look_alike_fail(self):
        t = self.w["dw7"]["target"]
        post(f"/cms/posts/{t}/edit", self.form(tags=[]))
        self.assertFalse(self.verdict())
        post(f"/cms/posts/{t}/edit", self.form(status="Draft"))
        self.assertFalse(self.verdict())
        post(f"/cms/posts/{t}/edit", self.form())
        twin = next(x for x in self.w["dw7"]["posts"] if x["title"] == self.w["dw7"]["title"] + " (copy)")
        post(f"/cms/posts/{twin['id']}/edit", self.form(twin["id"]))
        self.assertFalse(self.verdict())

    def test_a_category_not_picked_from_the_list_is_refused(self):
        status, body, _ = post(f"/cms/posts/{self.w['dw7']['target']}/edit", self.form(category="Security Adv"))
        self.assertEqual(status, 200)
        self.assertIn("Choose a category from the list.", body)
        self.assertFalse(self.verdict())


class Public(unittest.TestCase):
    def check(self, tid, exp, good, bad):
        self.assertTrue(td.verify_web(TASK[tid], {"exp": exp}, good, {})[0], good)
        for answer in bad:
            self.assertFalse(td.verify_web(TASK[tid], {"exp": exp}, answer, {})[0], answer)

    def test_pypi_release_date(self):
        dates = td.parse_pypi_release_dates((FIX / "pypi_requests_subset.json").read_text(), td.PYPI_VERSION)
        self.assertEqual(dates, ["2017-01-24"])
        self.check("dp1", {"dp1_dates": dates}, "Version 2.13.0 was released on Jan 24, 2017.",
                   ["January 23, 2017", "2016-12-20", "It was released in 2017", ""])

    def test_batched_strict_version(self):
        v = td.parse_batched_strict((FIX / "itertools_batched_snippet.html").read_text())
        self.assertEqual(v, "3.13")
        self.check("dp2", {"dp2_version": v}, "Python 3.13", ["Python 3.12", "batched came in 3.12, strict in 3.13", ""])

    def test_rfc_section(self):
        s = td.parse_rfc_section((FIX / "rfc9110_418_snippet.html").read_text())
        self.assertEqual(s, "15.5.19")
        self.check("dp3", {"dp3_section": s}, "Section 15.5.19 (418 Unused)",
                   ["Section 15.5.18", "Section 15.5", "15.5.19 or 15.5.20", "RFC 9110"])

    def test_parsers_refuse_pages_without_the_fact(self):
        for fn, page in ((td.parse_batched_strict, "<html></html>"), (td.parse_rfc_section, "<h4 id=x></h4>")):
            with self.assertRaises(ValueError):
                fn(page)
        with self.assertRaises(ValueError):
            td.parse_pypi_release_dates('{"releases": {}}', "2.13.0")


class Wiring(unittest.TestCase):
    def test_run_web_loads_dev_unsealed_on_its_own_port(self):
        run_web = load("run_web_dev", BENCH / "harness" / "run_web.py")
        ts = run_web.load_taskset("dev")
        self.assertEqual((ts.name, ts.sealed, ts.order), ("dev", False, td.ORDER))
        self.assertEqual(ts.script, DEV / "web" / "server.py")
        self.assertEqual(run_web.DEV_BASE, "http://127.0.0.1:8794")
        self.assertEqual(run_web.load_taskset("heldout").name, "heldout")

    def test_report_shows_dev_per_task_with_reasons(self):
        report = load("bench_report_dev", BENCH / "report.py")
        rows = [{"suite": "web", "arm": "tabref", "task": "dw1", "model": "m", "success": False, "cli_s": 1, "wall_s": 5,
                 "input_total": 100, "output_tokens": 0, "num_turns": 3, "verify": "dev reason", "host": "laptop",
                 "valid": True, "cost_usd_list": 0.1, "taskset": "dev"},
                {"suite": "web", "arm": "tabref", "task": "t1", "model": "m", "success": False, "cli_s": 1, "wall_s": 5,
                 "input_total": 100, "output_tokens": 0, "num_turns": 3, "verify": "fresh reason", "host": "laptop",
                 "valid": True, "cost_usd_list": 0.1, "taskset": "fresh"}]
        f = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False)
        f.write("".join(json.dumps(r) + "\n" for r in rows))
        f.close()
        self.addCleanup(os.unlink, f.name)
        text = "\n".join(report.models(argparse.Namespace(results=[f.name], baseline=False, model=None, arms=None,
                                                          taskset="dev")))
        self.assertIn("## Model runs: dev tasks", text)
        self.assertIn("#### dw1", text)
        self.assertIn("dev reason", text)
        self.assertNotIn("fresh reason", text)
        fresh = "\n".join(report.models(argparse.Namespace(results=[f.name], baseline=False, model=None, arms=None,
                                                           taskset="fresh")))
        self.assertNotIn("fresh reason", fresh)
        self.assertNotIn("#### t1", fresh)


if __name__ == "__main__":
    unittest.main()
