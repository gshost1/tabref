"""Compare two release builds' printed views and capture latency using local pages."""

import argparse
from contextlib import ExitStack, contextmanager
import difflib
from functools import partial
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
LARGE = ("rfc9110", "pypi-requests", "wikipedia-wwii")
PAGES = LARGE + ("small-form", "small-article")
SNAPSHOT_ID = re.compile(rb"(?<= \xc2\xb7 refs eN = @)s[0-9a-z]{7}(?=:eN\r?$)", re.M)


class FixtureHandler(SimpleHTTPRequestHandler):
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
