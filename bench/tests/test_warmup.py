"""The warm-up before each run's timer: what it asks a browser's DevTools HTTP endpoint for, and what it leaves in
the site's state. No Chrome, no model: a fake DevTools endpoint and the real (frozen) held-out site.

Run: python3 -I -m unittest discover -s bench/tests -p 'test_*.py'
"""
import json
import sys
import threading
import unittest
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH / "harness"))
import core  # noqa: E402


class FakeDevTools:
    """A DevTools HTTP endpoint with one starting tab: /json/new makes a tab whose title stays empty for
    `pending_polls` listings (its document has not committed), then reads `title`."""

    def __init__(self, pending_polls=2, title="Not found", closing_polls=0):
        self.calls, self.closed, self.pending, self.title = [], [], pending_polls, title
        self.closing = closing_polls
        self.listed_after_close = 0
        self.tabs = [{"id": "START", "type": "page", "url": "about:blank", "title": "about:blank"}]
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
                if self.path.startswith("/json/new?"):
                    tab = {"id": "WARM", "type": "page", "url": self.path.split("?", 1)[1], "title": ""}
                    fake.tabs.append(tab)
                    return self.reply(tab)
                self.reply({}, 404)

            def do_GET(self):
                fake.calls.append(("GET", self.path))
                if self.path == "/json/list":
                    if fake.closed and any(t["id"] == "WARM" for t in fake.tabs):
                        fake.listed_after_close += 1
                        if fake.listed_after_close > fake.closing:
                            fake.tabs = [t for t in fake.tabs if t["id"] != "WARM"]
                    for tab in fake.tabs:
                        if tab["id"] == "WARM" and fake.title is not None:
                            if fake.pending > 0:
                                fake.pending -= 1
                            else:
                                tab["title"] = fake.title
                    return self.reply(fake.tabs)
                if self.path.startswith("/json/close/"):
                    fake.closed.append(self.path.rsplit("/", 1)[1])
                    return self.reply("Target is closing")
                self.reply({}, 404)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def stop(self):
        self.server.shutdown()
        self.server.server_close()


class WarmUp(unittest.TestCase):
    def test_one_throwaway_tab_commits_and_is_closed_over_http_only(self):
        fake = FakeDevTools(pending_polls=3)
        try:
            warm = core.warm_up(fake.port, "http://127.0.0.1:8792", poll=0.01)
        finally:
            fake.stop()
        self.assertTrue(warm["warmup_committed"], warm)
        self.assertIsInstance(warm["warmup_s"], float)
        self.assertNotIn("warmup_error", warm)
        self.assertEqual(fake.calls[0], ("PUT", "/json/new?http://127.0.0.1:8792/warm"))
        self.assertEqual(fake.closed, ["WARM"])
        self.assertEqual([t["id"] for t in fake.tabs], ["START"], "the starting about:blank tab is left alone")
        paths = {path.split("?")[0].rsplit("/", 1)[0] if path.startswith("/json/close/") else path.split("?")[0]
                 for _, path in fake.calls}
        self.assertEqual(paths, {"/json/new", "/json/list", "/json/close"})

    def test_it_returns_only_once_the_closing_tab_has_left_the_list(self):
        fake = FakeDevTools(pending_polls=0, closing_polls=3)
        try:
            warm = core.warm_up(fake.port, "http://127.0.0.1:8792", poll=0.01)
            listed = [t["id"] for t in core.http_json(f"http://127.0.0.1:{fake.port}/json/list")]
        finally:
            fake.stop()
        self.assertTrue(warm["warmup_committed"], warm)
        self.assertNotIn("warmup_error", warm)
        self.assertEqual(listed, ["START"])
        self.assertGreaterEqual(fake.listed_after_close, 4)

    def test_a_page_with_no_title_counts_once_chrome_shows_its_address(self):
        fake = FakeDevTools(pending_polls=1, title="127.0.0.1:8792/warm")
        try:
            warm = core.warm_up(fake.port, "http://127.0.0.1:8792", poll=0.01)
        finally:
            fake.stop()
        self.assertTrue(warm["warmup_committed"], warm)

    def test_a_tab_that_never_commits_is_logged_closed_and_does_not_raise(self):
        fake = FakeDevTools(title=None)
        try:
            warm = core.warm_up(fake.port, "http://127.0.0.1:8792", limit=0.2, poll=0.01)
        finally:
            fake.stop()
        self.assertFalse(warm["warmup_committed"])
        self.assertIn("no commit", warm["warmup_error"])
        self.assertEqual(fake.closed, ["WARM"])

    def test_an_unreachable_endpoint_is_logged_and_does_not_raise(self):
        warm = core.warm_up(core.free_port(), "http://127.0.0.1:8792", limit=0.5)
        self.assertFalse(warm["warmup_committed"])
        self.assertTrue(warm["warmup_error"])


class WarmLeavesNoTrace(unittest.TestCase):
    """The real held-out site (frozen, unchanged): /warm is its 404 page, it records no event, and the state the
    verifiers read has no trace of it, while the model's own page requests stay."""

    @classmethod
    def setUpClass(cls):
        cls.site = core.Site(base=f"http://127.0.0.1:{core.free_port()}").start()

    @classmethod
    def tearDownClass(cls):
        proc = cls.site.proc
        cls.site.stop()
        proc.wait(timeout=5)

    def get(self, path):
        try:
            with urllib.request.urlopen(f"{self.site.base}{path}", timeout=5) as r:
                return r.status
        except urllib.error.HTTPError as e:
            return e.code

    def test_warm_records_no_event_and_leaves_no_page_log_entry(self):
        self.site.reset(7)
        self.assertEqual(self.get(core.WARM_PATH), 404)
        self.assertEqual(self.get("/shop/"), 200)
        raw = core.http_json(f"{self.site.base}/__state")
        state = self.site.state()
        self.assertEqual(raw["events"], [])
        self.assertEqual(state["events"], [])
        self.assertIn(core.WARM_PATH, [n["path"] for n in raw["navs"]], "the frozen site logs it; the filter must act")
        self.assertEqual([n["path"] for n in state["navs"]], ["/shop/"])

    def test_the_filter_keeps_everything_else(self):
        state = {"events": [{"kind": "x"}], "navs": [{"path": "/warm"}, {"path": "/warm/x"}, {"path": "/forum/"}],
                 "seed": 3}
        self.assertEqual(core.without_warm(state)["navs"], [{"path": "/warm/x"}, {"path": "/forum/"}])
        self.assertEqual(core.without_warm(state)["events"], [{"kind": "x"}])
        self.assertEqual(core.without_warm({"seed": 1}), {"seed": 1})


if __name__ == "__main__":
    unittest.main()
