"""The warm-up before each run's timer, the same for every arm.

A just-started Chrome holds its first page load until it has read its profile (the cookie store); under load that
takes seconds, and without a warm-up the wait lands in whichever arm's first call comes first. So before the timer
and the model's (or the replay's) first call, the fresh Chrome loads one throwaway page from a server the harness
owns on its own 127.0.0.1 port, never the task site, whose state stays untouched. One cookie store serves the whole
profile and cookies ignore ports, so this warms the task site too. The request arriving at that server is the proof:
Chrome sends a request only after the cookie store is read.

Only the DevTools HTTP endpoint is used (no tool's daemon is started; the starting about:blank tab is left alone):
PUT /json/new?<warm url>, wait for the request, /json/close/<id>, and wait until the tab has left /json/list. When
the tab's id never came back (a PUT reply later than its timeout), every page whose URL is the warm URL is closed
instead. A run whose warm-up failed is marked invalid (`verdict`), never kept as a cold row; rows carry
`warmup: VERSION` so the report never pools them with earlier cold rows.
"""
import json
import threading
import time
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

VERSION = "page-2026-11-15"
PAGE = b"<!doctype html><title>warm</title><p>warm"


class WarmServer:
    """A throwaway page on 127.0.0.1:<own port>; `wait(path)` returns once a request for that path arrived."""

    def __init__(self):
        seen, cond = set(), threading.Condition()
        self.seen, self.cond = seen, cond

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                with cond:
                    seen.add(self.path)
                    cond.notify_all()
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
                self.send_header("Content-Length", str(len(PAGE)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(PAGE)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def wait(self, path, timeout):
        with self.cond:
            return self.cond.wait_for(lambda: path in self.seen, timeout)

    def stop(self):
        self.server.shutdown()
        self.server.server_close()


_SERVER = {}


def server():
    """The process's warm server, started on first use."""
    if "s" not in _SERVER:
        _SERVER["s"] = WarmServer()
    return _SERVER["s"]


def devtools_json(url, timeout, method=None):
    with urllib.request.urlopen(urllib.request.Request(url, method=method), timeout=timeout) as r:
        return json.loads(r.read())


def warm_up(port, warm=None, limit=30.0, poll=0.1):
    """Warms the browser on DevTools `port` (see the module doc). Never raises. Returns the row fields: `warmup`
    (VERSION), `warmup_s`, `warmup_reached` (the request arrived), `warmup_tab_left` (a warm tab is still listed at
    the end) and, when something went wrong, `warmup_error`."""
    if not port:
        return {"warmup": VERSION, "warmup_s": None, "warmup_reached": False, "warmup_tab_left": False,
                "warmup_error": "no DevTools port"}
    warm = warm or server()
    t0 = time.time()
    devtools = f"http://127.0.0.1:{port}"
    path = f"/warm/{uuid.uuid4().hex}"
    url = f"{warm.base}{path}"
    out = {"warmup": VERSION, "warmup_s": None, "warmup_reached": False, "warmup_tab_left": False}
    tab = None
    try:
        tab = devtools_json(f"{devtools}/json/new?{url}", timeout=limit, method="PUT")["id"]
        out["warmup_reached"] = warm.wait(path, max(0.0, limit - (time.time() - t0)))
        if not out["warmup_reached"]:
            out["warmup_error"] = f"the warm page was not requested within {limit:g} s"
    except (OSError, ValueError, KeyError, TypeError) as e:
        out["warmup_error"] = f"{type(e).__name__}: {e}"[:200]
    finally:
        gone = close_warm_tabs(devtools, tab, url, poll, limit=min(5.0, limit))
        out["warmup_tab_left"] = gone is False
    out["warmup_s"] = round(time.time() - t0, 2)
    return out


def close_warm_tabs(devtools, tab, url, poll, limit=5.0):
    """Closes the warm tab, by id, or, when the id is unknown, every page whose URL is the warm URL; /json/close only
    starts closing, so it waits until none is listed. True when none is left, False when one still is, None when
    the endpoint never listed its targets."""
    start, sent, listed_once = time.time(), set(), False
    while time.time() - start < limit:
        try:
            listed = devtools_json(f"{devtools}/json/list", timeout=5)
            listed_once = True
        except (OSError, ValueError):
            time.sleep(poll)
            continue
        left = {t.get("id") for t in listed if t.get("id") == tab or str(t.get("url", "")).startswith(url)}
        if not left:
            return True
        for target in left - sent:
            sent.add(target)
            try:
                urllib.request.urlopen(f"{devtools}/json/close/{target}", timeout=5).read()
            except OSError:
                pass
        time.sleep(poll)
    return False if listed_once else None


def verdict(warm):
    """Why a run with this warm-up is not a valid warm row, or None."""
    if warm.get("warmup_tab_left"):
        return "warm-up tab left open"
    if not warm.get("warmup_reached"):
        return f"warm-up failed: {warm.get('warmup_error') or 'not reached'}"
    return None
