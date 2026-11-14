#!/usr/bin/env python3
"""Pages for the per-command latency probe, on 127.0.0.1 only (the frozen bench site has no iframes).

  /simple   one heading, a short paragraph, a Name field, a Count button (JS only, no navigation), three links
  /heavy    the same controls on top, then a 300-row table, a 400-item list, and four iframes: two same-origin
            (/frame?n=1,2) and two cross-site (http://localhost:<port>/frame?n=3,4, out-of-process under site
            isolation), each with its own heading, field, button and text
  /frame    one iframe document

Usage: python3 -I bench/latency/fixture.py [port]   (default 8793)
"""
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

CONTROLS = """<h1>Latency fixture</h1>
<p id="lead">The quick brown fox reads the marker sentence: amber falcon seventeen.</p>
<label for="name">Name</label> <input id="name" name="name" type="text">
<button id="count" type="button" onclick="this.dataset.n=(+this.dataset.n||0)+1;document.getElementById('out').textContent='clicked '+this.dataset.n">Count</button>
<span id="out">clicked 0</span>
<nav><a href="/simple">Simple</a> · <a href="/heavy">Heavy</a> · <a href="/simple#end">End</a></nav>"""


def page(title, body):
    return f"<!doctype html><html lang=en><head><meta charset=utf-8><title>{title}</title></head><body>{body}</body></html>"


def simple():
    return page("Simple page", CONTROLS)


def heavy(port):
    rows = "".join(f"<tr><td>{i}</td><td>Item {i}</td><td><a href='#r{i}'>open {i}</a></td><td>${i * 1.25:.2f}</td>"
                   f"<td><input aria-label='qty {i}' value='{i % 7}' size=2></td></tr>" for i in range(1, 301))
    items = "".join(f"<li>List entry number {i} with some filler text to read</li>" for i in range(1, 401))
    frames = "".join(f"<iframe title='frame {n}' src='{src}/frame?n={n}' width=360 height=160></iframe>"
                     for n, src in ((1, ""), (2, ""), (3, f"http://localhost:{port}"), (4, f"http://localhost:{port}")))
    return page("Heavy page", CONTROLS + frames + f"<table><thead><tr><th>#</th><th>Name</th><th>Link</th><th>Price</th>"
                f"<th>Qty</th></tr></thead><tbody>{rows}</tbody></table><ul>{items}</ul><p id=end>End of page</p>")


def frame(n):
    return page(f"Frame {n}", f"<h2>Frame {n}</h2><p>Inside frame {n}: cobalt heron {n}.</p>"
                f"<input aria-label='frame field {n}'><button type=button>Frame button {n}</button>")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        u = urlparse(self.path)
        port = self.server.server_address[1]
        body = {"/simple": simple, "/heavy": lambda: heavy(port),
                "/frame": lambda: frame(int(parse_qs(u.query).get("n", ["0"])[0]))}.get(u.path)
        if body is None:
            self.send_response(204 if u.path == "/favicon.ico" else 404)
            self.end_headers()
            return
        data = body().encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8793
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    main()
