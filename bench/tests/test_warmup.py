"""The warm-up before each run's timer (harness/warmup.py): what it asks a browser's DevTools HTTP endpoint for, that
it never touches the task site, that a failed warm-up makes the run invalid, and that the report keeps warm and cold
rows apart. No Chrome, no model: a fake DevTools endpoint that, like Chrome, requests the new tab's page.

Run: python3 -I -m unittest discover -s bench/tests -p 'test_*.py'
"""
import importlib.util
import json
import sys
import tempfile
import threading
import time
import unittest
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH / "harness"))
import core  # noqa: E402
import warmup  # noqa: E402

_spec = importlib.util.spec_from_file_location("bench_report", BENCH / "report.py")
report = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(report)


class FakeDevTools:
    """A DevTools HTTP endpoint with one starting tab. PUT /json/new makes a tab and, unless `fetch` is off, requests
    its URL the way Chrome loads a new tab; its reply comes after `put_delay` s. A closed tab stays listed for
    `closing_polls` listings; with `closable` off it never goes."""

    def __init__(self, fetch=True, put_delay=0.0, closing_polls=0, closable=True):
        self.calls, self.closed, self.fetched = [], [], []
        self.tabs = [{"id": "START", "type": "page", "url": "about:blank", "title": "about:blank"}]
        self.closing, self.closable, self.listed_after_close = closing_polls, closable, 0
        fake = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def reply(self, body, status=200):
                data = json.dumps(body).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_PUT(self):
                fake.calls.append(("PUT", self.path))
                if not self.path.startswith("/json/new?"):
                    return self.reply({}, 404)
                url = self.path.split("?", 1)[1]
                tab = {"id": "WARM", "type": "page", "url": url, "title": ""}
                fake.tabs.append(tab)
                if fetch:
                    threading.Thread(target=fake.load, args=(url,), daemon=True).start()
                time.sleep(put_delay)
                try:
                    self.reply(tab)
                except OSError:
                    pass

            def do_GET(self):
                fake.calls.append(("GET", self.path))
                if self.path == "/json/list":
                    if fake.closable and "WARM" in fake.closed and any(t["id"] == "WARM" for t in fake.tabs):
                        fake.listed_after_close += 1
                        if fake.listed_after_close > fake.closing:
                            fake.tabs = [t for t in fake.tabs if t["id"] != "WARM"]
                    return self.reply(fake.tabs)
                if self.path.startswith("/json/close/"):
                    fake.closed.append(self.path.rsplit("/", 1)[1])
                    return self.reply("Target is closing")
                self.reply({}, 404)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def load(self, url):
        with urllib.request.urlopen(url, timeout=5) as r:
            self.fetched.append((url, r.status))

    def stop(self):
        self.server.shutdown()
        self.server.server_close()


def warm_with(fake, **kw):
    try:
        return warmup.warm_up(fake.port, poll=0.01, **kw)
    finally:
        fake.stop()


class WarmUp(unittest.TestCase):
    def test_one_tab_loads_the_harness_page_and_is_closed_over_http_only(self):
        fake = FakeDevTools()
        warm = warm_with(fake)
        self.assertEqual((warm["warmup"], warm["warmup_reached"], warm["warmup_tab_left"]),
                         (warmup.VERSION, True, False), warm)
        self.assertIsNone(warmup.verdict(warm))
        self.assertNotIn("warmup_error", warm)
        method, path = fake.calls[0]
        url = path.split("?", 1)[1]
        self.assertEqual(method, "PUT")
        self.assertTrue(url.startswith(f"{warmup.server().base}/warm/"), url)
        self.assertEqual(fake.fetched, [(url, 200)])
        self.assertEqual(fake.closed, ["WARM"])
        self.assertEqual([t["id"] for t in fake.tabs], ["START"], "the starting about:blank tab is left alone")
        used = {p.split("?")[0] if not p.startswith("/json/close/") else "/json/close" for _, p in fake.calls}
        self.assertEqual(used, {"/json/new", "/json/list", "/json/close"})

    def test_each_warm_up_asks_for_its_own_page(self):
        first, second = FakeDevTools(), FakeDevTools()
        warm_with(first)
        warm_with(second)
        self.assertNotEqual(first.calls[0][1], second.calls[0][1])

    def test_a_put_reply_later_than_its_timeout_still_closes_the_tab_by_url(self):
        fake = FakeDevTools(put_delay=2.0)
        warm = warm_with(fake, limit=0.5)
        self.assertFalse(warm["warmup_tab_left"], warm)
        self.assertIn("WARM", fake.closed)
        self.assertEqual([t["id"] for t in fake.tabs], ["START"])
        self.assertTrue(warmup.verdict(warm).startswith("warm-up failed"))

    def test_it_returns_only_once_the_closing_tab_has_left_the_list(self):
        fake = FakeDevTools(closing_polls=3)
        warm = warm_with(fake)
        self.assertFalse(warm["warmup_tab_left"], warm)
        self.assertEqual([t["id"] for t in fake.tabs], ["START"])
        self.assertGreaterEqual(fake.listed_after_close, 4)

    def test_a_tab_that_will_not_close_makes_the_run_invalid(self):
        fake = FakeDevTools(closable=False)
        warm = warm_with(fake, limit=0.5)
        self.assertTrue(warm["warmup_tab_left"])
        self.assertEqual(warmup.verdict(warm), "warm-up tab left open")

    def test_a_page_never_requested_makes_the_run_invalid_and_is_closed(self):
        fake = FakeDevTools(fetch=False)
        warm = warm_with(fake, limit=0.3)
        self.assertFalse(warm["warmup_reached"])
        self.assertIn("not requested", warm["warmup_error"])
        self.assertEqual(fake.closed, ["WARM"])
        self.assertTrue(warmup.verdict(warm).startswith("warm-up failed"))

    def test_an_unreachable_endpoint_or_no_port_makes_the_run_invalid(self):
        for warm in (warmup.warm_up(core.free_port(), limit=0.5, poll=0.01), warmup.warm_up(None)):
            self.assertFalse(warm["warmup_reached"])
            self.assertFalse(warm["warmup_tab_left"])
            self.assertTrue(warm["warmup_error"])
            self.assertTrue(warmup.verdict(warm).startswith("warm-up failed"))


class TaskSiteUntouched(unittest.TestCase):
    def test_the_warm_up_leaves_no_trace_in_the_task_site(self):
        site = core.Site(base=f"http://127.0.0.1:{core.free_port()}").start()
        try:
            site.reset(7)
            warm = warm_with(FakeDevTools())
            state = site.state()
        finally:
            proc = site.proc
            site.stop()
            proc.wait(timeout=5)
        self.assertTrue(warm["warmup_reached"])
        self.assertNotEqual(warmup.server().base, site.base)
        self.assertEqual((state["events"], state["navs"]), ([], []))


class ReportKeepsWarmAndColdApart(unittest.TestCase):
    def test_rows_are_split_on_the_warm_up(self):
        cold = json.loads((BENCH / "results" / "linux-sonnet-5-5" / "results-web.jsonl").read_text().splitlines()[0])
        warm = dict(cold, warmup=warmup.VERSION)
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "rows.jsonl"
            path.write_text(json.dumps(cold) + "\n" + json.dumps(warm) + "\n")

            def count(which):
                rows, _, _ = report.load_by_taskset([str(path)], None, {cold["arm"]}, None, which)
                return sum(len(v) for v in rows.values())

            self.assertEqual((count(warmup.VERSION), count("cold"), count(None)), (1, 1, 2))
        self.assertEqual(report.WARMUP_DEFAULT, warmup.VERSION)
        self.assertEqual(report.warmup_of(cold), "cold")


if __name__ == "__main__":
    unittest.main()
