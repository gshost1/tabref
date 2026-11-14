"""The scoring rule (heldout/report.py, fork 7faca3235: cannot reward failure) and its use in bench/report.py, plus
the run-safety helpers the review asked for. Report tests are ported from the fork's tests/test_harness.py.

Run: python3 -I -m unittest discover -s bench/tests -p 'test_*.py'
"""
import argparse
import importlib.util
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH / "harness"))
sys.path.insert(0, str(BENCH / "heldout"))
import core  # noqa: E402
import report  # noqa: E402  (heldout/report.py)


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


bench_report = load("bench_report", BENCH / "report.py")
run_web = load("run_web", BENCH / "harness" / "run_web.py")
latency = load("latency_run", BENCH / "latency" / "run.py")


def row(arm, task, ok, cli, wall, tok=1000, turns=3, suite="native"):
    return report.normalize({"suite": suite, "arm": arm, "task": task, "model": "m", "success": ok, "cli_s": cli,
                             "wall_s": wall, "input_total": tok, "output_tokens": 0, "num_turns": turns, "verify": "x"})


class ReportTests(unittest.TestCase):
    def hn6_like(self):
        """ad-up fails fast (1/5 at ~24 s), ad-14 passes 4/5 at ~50 s, as on hn6."""
        fast = [row("ad-up", "hn6", i == 0, 4.5, 24) for i in range(5)]
        slow = [row("ad-14", "hn6", i < 4, 6.2, 50) for i in range(5)]
        return fast + slow

    def test_failing_fast_does_not_rank_first(self):
        ranked, _, _ = report.ranking(self.hn6_like(), "native")
        self.assertEqual([x["arm"] for x in ranked], ["ad-14", "ad-up"])
        per = {x["arm"]: x["per"]["wall"] for x in ranked}
        self.assertEqual(per["ad-up"], 120.0)
        self.assertEqual(per["ad-14"], 62.5)

    def test_one_more_failure_never_improves_per_success(self):
        base = [row("a", "t", True, 1, 10) for _ in range(4)]
        worse = base + [row("a", "t", False, 0.1, 1)]
        self.assertLess(report.cell_stats(base, "wall")["per_success"], report.cell_stats(worse, "wall")["per_success"])

    def test_statistics_are_separate(self):
        rs = [row("a", "t", ok, 1, w) for ok, w in [(True, 10), (True, 20), (False, 1), (False, 2), (True, 30)]]
        s = report.cell_stats(rs, "wall")
        self.assertEqual(s["pass_med"], 20)
        self.assertEqual(s["all_med"], 10)
        self.assertEqual(s["max"], 30)
        self.assertAlmostEqual(s["p90"], 26.0)
        self.assertAlmostEqual(s["per_success"], 21.0)

    def test_no_pass_has_no_pass_median(self):
        s = report.cell_stats([row("a", "t", False, 1, 5)], "wall")
        self.assertIsNone(s["pass_med"])
        self.assertIsNone(s["per_success"])

    def test_losses_skip_faster_but_less_reliable_arms(self):
        self.assertEqual(report.loss_rows(self.hn6_like(), "ad-14"), [])
        self.assertIn(("native", "hn6", "pass", "1/5", "ad-14", "4/5"), report.loss_rows(self.hn6_like(), "ad-up"))

    def test_losses_list_pass_and_metric(self):
        rows = [row("ad-14", "hw1", True, 1, 10, suite="web"), row("ad-14", "hw1", False, 3, 30, suite="web"),
                row("bh", "hw1", True, 0.5, 12, suite="web"), row("bh", "hw1", True, 0.7, 14, suite="web")]
        text = "\n".join(report.losses(rows))
        self.assertIn("| web | hw1 | pass | 1/2 | bh | 2/2 |", text)
        self.assertIn("| web | hw1 | CLI s | 1.0 | bh |", text)
        self.assertNotIn("wall s", text)

    def test_render_separates_and_labels(self):
        text = report.render(self.hn6_like(), 0)
        self.assertIn("ranking (cannot reward failing)", text)
        self.assertIn("pass median / all median / p90 / max", text)
        self.assertIn("rewards fast failures", text)
        header = next(line for line in text.splitlines() if line.startswith("| rank |"))
        self.assertLess(header.index("CLI"), header.index("wall"))


class BenchReportTests(unittest.TestCase):
    def write(self, rows):
        f = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False)
        f.write("".join(json.dumps(r) + "\n" for r in rows))
        f.close()
        return f.name

    def raw(self, arm, task, ok, wall, host):
        return {"suite": "web", "arm": arm, "task": task, "model": "m", "success": ok, "cli_s": 1, "wall_s": wall,
                "input_total": 100, "output_tokens": 0, "num_turns": 3, "verify": "x", "host": host, "valid": True}

    def test_models_ranks_by_passes_and_shows_hosts(self):
        rows = [self.raw("tabref", "hw1", i == 0, 5, "laptop") for i in range(5)] + \
               [self.raw("bh", "hw1", True, 50, "linux") for _ in range(5)]
        text = "\n".join(bench_report.models(argparse.Namespace(results=[self.write(rows)], baseline=False, model=None,
                                                                 arms=None)))
        ranking = [line for line in text.splitlines() if line.startswith("| 1 |") or line.startswith("| 2 |")]
        self.assertTrue(ranking[0].startswith("| 1 | bh | linux | 5/5"), ranking)
        self.assertTrue(ranking[1].startswith("| 2 | tabref | laptop | 1/5"), ranking)
        self.assertIn("rewards fast failures", text)
        self.assertIn("Where tabref loses", text)


class RunSafetyTests(unittest.TestCase):
    def test_usage_from_assistant_events_counts_each_message_once(self):
        ev = {"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": 10, "output_tokens": 2}}}
        ev2 = {"type": "assistant", "message": {"id": "m2", "usage": {"input_tokens": 5, "output_tokens": 1}}}
        lines = [json.dumps(ev), json.dumps(ev), json.dumps(ev2), "not json"]
        self.assertEqual(run_web.assistant_usage(lines), {"input_tokens": 15, "output_tokens": 3})

    def test_timeout_kills_the_group_and_reports_124(self):
        rc, out, err, secs, timed_out = core.run_in_session(["bash", "-c", "sleep 30 & sleep 30"], "/", os.environ, 1)
        self.assertEqual((rc, timed_out), (124, True))
        self.assertLess(secs, 10)

    def test_cleanup_kills_only_this_runs_processes(self):
        home = tempfile.mkdtemp()
        mine = subprocess.Popen(["sleep", "60"], env=dict(os.environ, HO_ARM_HOME=home))
        other = subprocess.Popen(["sleep", "60"], env=dict(os.environ, HO_ARM_HOME=home + "-other"))
        try:
            core.stop_arm_daemons("bh", {"HO_ARM_HOME": home, "TABREF_BIN": "/nonexistent", "HO_CDP_PORT": "1"})
            time.sleep(0.3)
            self.assertIsNotNone(mine.poll())
            self.assertIsNone(other.poll())
        finally:
            for p in (mine, other):
                p.kill()
                p.wait()

    def test_site_refuses_a_taken_port(self):
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            s.listen()
            with self.assertRaises(SystemExit):
                core.Site(f"http://127.0.0.1:{s.getsockname()[1]}").start()

    def test_model_pass_refused_for_the_operator(self):
        if core.on_vm() or os.environ.get("USER") == core.BENCH_USER:
            self.skipTest("only meaningful as a normal user on a non-VM host")
        self.assertIn("only as the user", core.isolation_problem("xvfb"))

    def test_strays_scan_behind_the_home_links(self):
        work = Path(tempfile.mkdtemp())
        real = Path(tempfile.mkdtemp())
        (work / "arm-home").symlink_to(real)
        (real / ":stray").write_text("x")
        moved = run_web.collect_strays(time.time() - 10, work, work / "strays")
        self.assertEqual(len(moved), 1)


class LatencyCheckTests(unittest.TestCase):
    def test_success_markers(self):
        ab = latency.AB(1)
        self.assertTrue(ab.ok("click", "✓ Done"))
        self.assertFalse(ab.ok("click", "✗ Element not found"))
        self.assertTrue(ab.ok("text", "The marker sentence: amber falcon seventeen."))
        self.assertFalse(ab.ok("text", "no match"))
        bh = latency.BH(1)
        self.assertTrue(bh.ok("click", "clicked 3"))
        self.assertFalse(bh.ok("click", "clicked 0"))
        self.assertTrue(latency.AD(1).ok("fill", '{"version":"2.7","ok":true,"command":"set-value"}'))


if __name__ == "__main__":
    unittest.main()
