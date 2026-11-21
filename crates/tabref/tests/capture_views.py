"""Compare two release builds' printed views and capture latency using local pages."""

import argparse
from contextlib import ExitStack, contextmanager
import difflib
from functools import partial
from hashlib import sha256
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import re
import shlex
import statistics
import subprocess
import tempfile
import threading
import time
from urllib.error import URLError
from urllib.request import urlopen


FIXTURES = Path(__file__).parent / "fixtures" / "capture"
LARGE = ("rfc9110", "release-history", "long-article")
PAGES = LARGE + ("small-form", "small-article")
SNAPSHOT_ID = re.compile(rb"(?<= \xc2\xb7 refs eN = @)s[0-9a-z]{7}(?=:eN\r?$)", re.M)


def page(title, body):
    return f"""<!doctype html>
<html lang="en"><meta charset="utf-8"><title>{title}</title>
<style>
body {{ font: 16px/1.55 system-ui; margin: 1rem auto; max-width: 65rem; }}
aside {{ float: right; width: 20rem; margin: 0 0 1rem 1rem; }}
.entry {{ display: grid; grid-template-columns: 2fr 1fr 2fr; padding: .6rem;
          border-bottom: 1px solid #ddd; }}
th, td {{ border: 1px solid #ddd; padding: .3rem; }}
table {{ border-collapse: collapse; }}
nav ul {{ padding-left: 2rem; }}
figure {{ margin: 1rem; }}
</style>
<body>{body}</body></html>""".encode()


def release_history():
    rows = []
    notes = []
    for number in range(170, 0, -1):
        version = f"2.{number // 10}.{number % 10}"
        date = f"{2020 + number // 35}-{number % 12 + 1:02d}-{number % 28 + 1:02d}"
        wheel = f"sample_package-{version}-py3-none-any.whl"
        archive = f"sample_package-{version}.tar.gz"
        wheel_digest = sha256(wheel.encode()).hexdigest()
        source_digest = sha256(archive.encode()).hexdigest()
        rows.append(f"""
        <article class="entry" aria-labelledby="release-{number}"
                 data-release-version="{version}" data-release-channel="stable">
          <div class="version"><h3 id="release-{number}"><a href="#notes-{number}">
            Sample package {version}</a></h3><span class="channel">Stable release</span></div>
          <div class="date"><time datetime="{date}">{date}</time></div>
          <div class="artifacts"><ul><li><a href="#wheel-{number}"><code>{wheel}</code></a></li>
            <li><a href="#source-{number}">Source archive</a></li>
            <li><a href="#notes-{number}">Changes for {version}</a></li>
          </ul></div>
        </article>""")
        notes.append(f"""
        <details class="release-notes" id="notes-{number}">
          <summary><span>Release {version} notes and distribution metadata</span></summary>
          <div class="notes"><p>Release {number} improves the example client's handling
          of delayed responses and includes new examples for reading paginated results.
          The examples cover an empty record set, a server-directed retry and a request
          whose page cursor has expired. The client retains the public response schema.</p>
          <dl class="distribution-files">
            <dt>Wheel for all supported platforms</dt>
            <dd id="wheel-{number}" data-digest-algorithm="sha256" data-digest="{wheel_digest}">
              <code>{wheel}</code><span>Universal Python 3 wheel, {32 + number % 10} KiB</span>
            </dd>
            <dt>Source distribution</dt>
            <dd id="source-{number}" data-digest-algorithm="sha256" data-digest="{source_digest}">
              <code>{archive}</code><span>Compressed source bundle, {76 + number % 20} KiB</span>
            </dd>
          </dl>
          <p>Both files were published on {date}. The source bundle includes the examples,
          integration tests and the build configuration. The wheel contains the importable
          client and its package metadata.</p></div>
        </details>""")
    return page("Sample package release history", f"""
    <header><form role="search"><label for="package">Search projects</label>
      <input type="search" id="package"><button>Search</button></form></header>
    <aside aria-label="Project links"><ul><li><a href="#history">Release history</a></li>
      <li><a href="#notes">Distribution files</a></li></ul></aside>
    <main><h1>Sample package</h1>
      <p>A sample client library for a catalog of public records.</p>
      <h2 id="history">Release history</h2>{''.join(rows)}
      <h2 id="notes">Release notes</h2>{''.join(notes)}
    </main>""")


def long_article():
    sections = []
    contents = []
    citations = []
    for chapter in range(1, 26):
        section = f"district-{chapter}"
        entries = []
        for record in range(1, 51):
            collection = (chapter * 53 + record) % 360 + 1
            entries.append(f"""
            <li class="contents-entry" data-catalog-key="river-districts/planning-register/{chapter}/{record}">
              <span class="contents-number">{chapter}.{record}</span>
              <a href="#{section}-{(record - 1) % 5 + 1}"
                 title="District {chapter} planning register, records from volume {record} of the municipal collection">
                <span class="contents-title">
                District {chapter} planning register, volume {record}</span></a>
              <a href="#source-{collection}"
                 title="Source {collection} in the archive index of neighborhood records and planning registers">
                <span>Archive index {collection}</span></a></li>""")
        contents.append(f'<li><a href="#{section}">District {chapter}</a><ul>{"".join(entries)}</ul></li>')
        paragraphs = []
        for topic in range(1, 6):
            passage = []
            for revision in range(1, 5):
                source = ((chapter - 1) * 20 + (topic - 1) * 4 + revision) % 360 + 1
                passage.append(f"""
                In {1820 + chapter * 3 + topic * 2 + revision}, the records for district {chapter}
                described the {revision} phase of the neighborhood plan, including the market
                route, the floodplain and the transfer of public land near the station.
                The survey listed the streets beside the station in the order they reached
                the river. At the public meeting, residents compared the survey with the
                earlier district map and raised questions about access to the ferry landing.
                <sup class="reference"><a href="#source-{source}"><span>[{source}]</span></a></sup>""")
            paragraphs.append(f'<p id="{section}-{topic}">{"".join(passage)}</p>')
        sections.append(f'<section><h2 id="{section}">District {chapter}</h2>{"".join(paragraphs)}</section>')
    for index in range(1, 361):
        year = 1840 + index % 130
        links = "".join(
            f'<a href="#district-{(index + part) % 25 + 1}" '
            f'data-archive-path="river-districts/civic-surveys/edition/{year}/collection/{index}/item/{part}" '
            f'title="{label} record {index}.{part} in the {year} civic survey of the river districts">'
            f'<span>{label} {index}.{part}</span></a> '
            for part, label in enumerate(("Catalog", "Archive", "Map", "Register", "Index"), 1)
        )
        citations.append(f"""
        <li id="source-{index}"><cite><span class="collection-title">Civic survey of the river
          districts, collection {index}, edition of {year}</span></cite>.
          <span class="archive-label">Regional records office, reading room {index % 8 + 1}.
          Includes the boundary proposals, ordinance summaries and land use plans.</span>
          <span class="archive-links">{links}</span></li>""")
    tables = []
    for group, label in enumerate(("Survey records", "Planning maps", "Municipal registers")):
        rows = "".join(
            f'<tr><th scope="row">District {index}</th><td>{1900 + index + group}</td>'
            f'<td>{index * (103 + group)}</td></tr>'
            for index in range(1, 26)
        )
        tables.append(f"""<table><caption>{label}</caption>
        <thead><tr><th scope="col">District</th><th scope="col">Year</th>
        <th scope="col">Records</th></tr></thead><tbody>{rows}</tbody></table>""")
    return page("History of the River Districts", f"""
    <header><h1>History of the River Districts</h1></header>
    <main><aside>{"".join(tables)}</aside>
    <nav aria-label="Contents"><h2>Contents</h2><ul>{''.join(contents)}</ul></nav>
    <article>{''.join(sections)}<h2>References</h2><ol>{''.join(citations)}</ol></article>
    </main>""")


class FixtureHandler(SimpleHTTPRequestHandler):
    generated = {
        "/release-history.html": release_history(),
        "/long-article.html": long_article(),
    }

    def do_GET(self):
        body = self.generated.get(self.path)
        if body is None:
            super().do_GET()
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


@contextmanager
def fixture_server():
    handler = partial(FixtureHandler, directory=FIXTURES)
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            yield f"http://127.0.0.1:{server.server_port}"
        finally:
            server.shutdown()
            thread.join()


@contextmanager
def browser(chrome):
    with tempfile.TemporaryDirectory(prefix="tabref-capture-", ignore_cleanup_errors=True) as directory:
        profile = Path(directory) / "chrome"
        args = shlex.split(os.environ.get("TABREF_CHROME_ARGS", ""))
        with subprocess.Popen(
            [chrome, *args, "--headless", "--no-first-run", "--no-default-browser-check",
             "--remote-debugging-port=0", f"--user-data-dir={profile}", "about:blank"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        ) as process:
            try:
                port = wait_for_port(process, profile / "DevToolsActivePort")
                yield directory, port
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def wait_for_port(process, port_file):
    until = time.monotonic() + 15
    while process.poll() is None and time.monotonic() < until:
        try:
            port = int(port_file.read_text().splitlines()[0])
            with urlopen(f"http://127.0.0.1:{port}/json/version", timeout=1):
                return port
        except (OSError, URLError, ValueError, IndexError):
            time.sleep(0.05)
    raise RuntimeError("headless Chrome did not start")


@contextmanager
def cli(binary, chrome, trace_file):
    with browser(chrome) as (home, port):
        env = dict(os.environ, TABREF_HOME=home, TABREF_REFS="short")
        env.pop("TABREF_NO_DAEMON", None)
        if trace_file:
            env["TABREF_TRACE"] = str(trace_file)

        def run(*args):
            command = [binary, "--cdp", str(port), "--timeout", "30000", *args]
            start = time.perf_counter()
            result = subprocess.run(command, env=env, capture_output=True, timeout=35)
            ms = (time.perf_counter() - start) * 1000
            if result.returncode:
                raise RuntimeError(f"{args}: {result.stderr.decode(errors='replace')}")
            return ms, result.stdout

        run("daemon", "start")
        try:
            run("snap")
            if b"daemon running:" not in run("daemon", "status")[1]:
                raise RuntimeError("capture comparison requires the daemon")
            yield run
        finally:
            run("daemon", "stop")


def captures(run, url):
    opened = run("open", url)
    run("js", "document.body.setAttribute('data-capture-test', 'fresh'); 0")
    snapped = run("snap", "--all", "--full")
    return opened, snapped


def check_views(before, after, label):
    before = SNAPSHOT_ID.sub(b"SNAPSHOT", before)
    after = SNAPSHOT_ID.sub(b"SNAPSHOT", after)
    if before != after:
        diff = difflib.diff_bytes(
            difflib.unified_diff, before.splitlines(keepends=True),
            after.splitlines(keepends=True), b"baseline", b"candidate",
        )
        raise AssertionError(f"{label}\n" + b"".join(diff).decode(errors="replace"))


def compare(binaries, chrome, rounds, trace_dir):
    timings = {}
    with fixture_server() as origin, ExitStack() as stack:
        clis = [
            stack.enter_context(cli(binary, chrome, trace_dir / f"{key}.log" if trace_dir else None))
            for key, binary in zip(("baseline", "candidate"), binaries)
        ]
        for page in PAGES:
            url = f"{origin}/{page}.html"
            for turn in range(rounds):
                order = range(2) if turn % 2 == 0 else reversed(range(2))
                views = {index: captures(clis[index], url) for index in order}
                for step, command in enumerate(("open", "snap")):
                    old_ms, old_view = views[0][step]
                    new_ms, new_view = views[1][step]
                    check_views(old_view, new_view, f"{page} {command}, run {turn + 1}")
                    baseline, candidate = timings.setdefault((page, command), ([], []))
                    baseline.append(old_ms)
                    candidate.append(new_ms)
    return timings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=lambda path: Path(path).resolve())
    parser.add_argument("candidate", type=lambda path: Path(path).resolve())
    parser.add_argument("--chrome", default="google-chrome")
    parser.add_argument("--rounds", type=int, default=7)
    parser.add_argument(
        "--trace-dir", type=lambda path: Path(path).resolve(),
        help="pass per-binary TABREF_TRACE paths to locally instrumented builds",
    )
    parser.add_argument("--require-large-speedup", type=float, metavar="PERCENT")
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("--rounds must be positive")
    if args.trace_dir:
        args.trace_dir.mkdir(parents=True, exist_ok=True)
    timings = compare((args.baseline, args.candidate), args.chrome, args.rounds, args.trace_dir)
    print("Page | Command | Baseline median (min–max), ms | Candidate median (min–max), ms | Faster")
    print("--- | --- | ---: | ---: | ---:")
    misses = []
    for (page, command), (old, new) in timings.items():
        medians = tuple(map(statistics.median, (old, new)))
        speedup = (1 - medians[1] / medians[0]) * 100
        columns = [f"{median:.1f} ({min(runs):.1f}–{max(runs):.1f})"
                   for median, runs in zip(medians, (old, new))]
        print(f"{page} | {command} | {' | '.join(columns)} | {speedup:+.1f}%")
        if page in LARGE and args.require_large_speedup is not None:
            if speedup < args.require_large_speedup:
                misses.append(f"{page} {command}: {speedup:+.1f}%")
    print(f"Printed views match for all {len(timings) * args.rounds} pairs.")
    if misses:
        raise SystemExit(f"Below {args.require_large_speedup:g}% improvement: {', '.join(misses)}")


if __name__ == "__main__":
    main()
