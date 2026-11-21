"""The sealed fresh task set's plumbing: the seal check, the loader, sealed rows and the per-set report. Uses a
synthetic folder (never the real one); with BENCH_FRESH_DIR set, the real folder's seal is checked too.

Run: python3 -I -B -m unittest discover -s bench/tests -p 'test_*.py'
"""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH / "harness"))
import fresh  # noqa: E402


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


run_web = load("run_web_fresh", BENCH / "harness" / "run_web.py")
bench_report = load("bench_report_fresh", BENCH / "report.py")

FAKE_TASKS = '''WEB_TASKS = [{"id": "t1", "prompt": "open {base}"}, {"id": "t2", "prompt": "open {base}"}]
def web_seed(task_id, run_idx):
    return run_idx
def reset_payload(seed):
    return {}
def web_expectations(task_id):
    return {}
def verify_web(task, ctx, answer, state):
    return bool(answer), "x"
'''


class Sealed(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="fresh-test-"))
        self.addCleanup(shutil.rmtree, self.root, True)
        (self.root / "web").mkdir()
        (self.root / "tasks_fresh.py").write_text(FAKE_TASKS)
        (self.root / "web" / "server.py").write_text("pass\n")
        sums = "".join(f"{hashlib.sha256((self.root / n).read_bytes()).hexdigest()}  {n}\n"
                       for n in ("tasks_fresh.py", "web/server.py"))
        (self.root / "SHA256SUMS").write_text(sums)
        saved = fresh.SEAL
        fresh.SEAL = hashlib.sha256(sums.encode()).hexdigest()
        self.addCleanup(setattr, fresh, "SEAL", saved)

    def test_intact_folder_passes(self):
        self.assertEqual(fresh.verify(self.root), 2)

    def test_wrong_seal_fails(self):
        fresh.SEAL = "0" * 64
        with self.assertRaises(fresh.SealBroken):
            fresh.verify(self.root)

    def test_spent_seal_fails(self):
        saved = fresh.SPENT_SEALS
        self.addCleanup(setattr, fresh, "SPENT_SEALS", saved)
        fresh.SPENT_SEALS = dict(saved, test=fresh.SEAL)
        with self.assertRaises(fresh.SealBroken):
            fresh.verify(self.root)

    def test_current_seal_is_not_spent(self):
        self.assertNotIn(fresh.SEAL, fresh.SPENT_SEALS.values())

    def test_spent_sets_stay_spent(self):
        self.assertEqual(sorted(fresh.SPENT_SEALS), ["v0.1", "v0.2"])
        self.assertEqual(len(set(fresh.SPENT_SEALS.values())), 2)

    def test_edited_file_fails(self):
        (self.root / "web" / "server.py").write_text("pass  \n")
        with self.assertRaises(fresh.SealBroken):
            fresh.verify(self.root)

    def test_extra_file_or_folder_fails(self):
        (self.root / "__pycache__").mkdir()
        with self.assertRaises(fresh.SealBroken):
            fresh.verify(self.root)
        (self.root / "__pycache__").rmdir()
        (self.root / "web" / "note").write_text("x")
        with self.assertRaises(fresh.SealBroken):
            fresh.verify(self.root)

    def test_symlink_fails(self):
        (self.root / "link").symlink_to(self.root / "tasks_fresh.py")
        with self.assertRaises(fresh.SealBroken):
            fresh.verify(self.root)

    def test_loader_writes_no_bytecode(self):
        ts = run_web.load_taskset("fresh", str(self.root))
        self.assertEqual(ts.order, ["t1", "t2"])
        self.assertEqual(ts.script, self.root / "web" / "server.py")
        self.assertFalse((self.root / "__pycache__").exists())
        ts.check()

    def test_missing_folder_is_refused(self):
        saved = os.environ.pop("BENCH_FRESH_DIR", None)
        try:
            with self.assertRaises(SystemExit):
                run_web.load_taskset("fresh", None)
        finally:
            if saved is not None:
                os.environ["BENCH_FRESH_DIR"] = saved

    @unittest.skipUnless(os.environ.get("BENCH_FRESH_DIR"), "BENCH_FRESH_DIR not set")
    def test_real_folder_seal(self):
        fresh.SEAL = "4aa5bccad9a75b0746131c784788402c997831d1d1464962cbcbc748eb07ad39"
        self.assertGreater(fresh.verify(fresh.fresh_dir()), 0)


class Disposable(unittest.TestCase):
    def test_consume_removes_only_a_disposable_copy(self):
        root = Path(tempfile.mkdtemp(prefix="fresh-copy-"))
        self.addCleanup(shutil.rmtree, root, True)
        kept = run_web.TaskSet("fresh", run_web.th, [], None, root, disposable=False)
        kept.consume()
        self.assertTrue(root.exists())
        gone = run_web.TaskSet("fresh", run_web.th, [], None, root, disposable=True)
        gone.consume()
        self.assertFalse(root.exists())
        gone.check()


class VaultRows(unittest.TestCase):
    def setUp(self):
        self.runs = Path(tempfile.mkdtemp(prefix="fresh-runs-"))
        self.addCleanup(shutil.rmtree, self.runs, True)

    def record(self):
        return {"pass_id": "p1", "task": "t1", "arm": "tabref", "round": 0, "seed": 5, "success": False,
                "verify": "secret why", "answer": "secret answer", "strays": ["a/:x"], "valid": False,
                "invalid_reason": "no result event: secret", "wall_s": 3.0}

    def test_free_text_is_held_until_flush(self):
        v = fresh.Vault(self.runs, "p1")
        row = fresh.seal_record(self.record(), v)
        self.assertEqual((row["verify"], row["strays"], row["invalid_reason"]), ("sealed", 1, "no result event"))
        self.assertNotIn("secret", json.dumps(row))
        v.stage_row(row)
        self.assertNotIn("secret", "".join(p.read_text() for p in self.runs.rglob("*") if p.is_file()))
        v.flush()
        detail = json.loads((v.dir / "sealed" / "detail.jsonl").read_text())
        self.assertEqual((detail["answer"], detail["verify"]), ("secret answer", "secret why"))

    def test_a_warm_up_error_is_held_with_the_other_free_text(self):
        v = fresh.Vault(self.runs, "p1")
        row = fresh.seal_record({**self.record(), "warmup_reached": False, "warmup_error": "secret warm text"}, v)
        self.assertNotIn("warmup_error", row)
        self.assertFalse(row["warmup_reached"])
        v.flush()
        detail = json.loads((v.dir / "sealed" / "detail.jsonl").read_text())
        self.assertEqual(detail["warmup_error"], "secret warm text")

    def test_rows_count_only_after_commit(self):
        v = fresh.Vault(self.runs, "p1")
        v.stage_row({"task": "t1"})
        v.stage_row({"task": "t2"})
        self.assertFalse((self.runs / "results-web.jsonl").exists())
        self.assertEqual(fresh.commit(self.runs, "p1"), 2)
        self.assertEqual(len((self.runs / "results-web.jsonl").read_text().splitlines()), 2)
        v.stage_row({"task": "t3"})
        self.assertEqual(fresh.discard(self.runs, "p1"), 1)
        self.assertEqual(fresh.commit(self.runs, "p1"), 0)
        self.assertEqual(len((self.runs / "results-web.jsonl").read_text().splitlines()), 2)

    def test_bad_pass_id_is_refused(self):
        for bad in ("../x", "", "a/b"):
            with self.assertRaises(SystemExit):
                fresh.Vault(self.runs, bad)


class Peeking(unittest.TestCase):
    def stream(self, *cmds, result=""):
        ev = {"type": "assistant", "message": {"id": "m", "content": [
            {"type": "tool_use", "id": str(i), "input": {"command": c}} for i, c in enumerate(cmds)]}}
        res = {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "0", "content": result}]}}
        return run_web.tool_traffic([json.dumps(ev), json.dumps(res), "not json"])

    def test_reads_outside_the_run_are_caught(self):
        sealed, work, tools = "/home/tabbench/fresh", "/home/tabbench/runs/web-fresh/x/work/r", "/home/tabbench/bench-tools"
        for cmd in ("cat ~/fresh/web/server.py", "grep -r verify /home/tabbench", "cat //home/tabbench/fresh/x",
                    "cat ../../detail.jsonl", "tabref open file:///etc/passwd", "tabref open http://127.0.0.1:8793/__state",
                    "cat /home/*/runs/results-web.jsonl", "head $HOME/x", "cat /tmp/bench-rt-1/x"):
            commands, results = self.stream("tabref skill", cmd)
            self.assertTrue(run_web.reads_sealed(commands, results, sealed, allowed=(work, tools)), cmd)

    def test_normal_calls_pass(self):
        work, tools = "/home/tabbench/runs/web-fresh/x/work/r", "/home/tabbench/bench-tools"
        commands, results = self.stream("tabref open http://127.0.0.1:8793/", f"cat {tools}/skills/x/SKILL.md",
                                        f"cat {work}/downloads/a.csv | head", "tabref find 'Total' | grep -i due",
                                        result="ok")
        self.assertIsNone(run_web.reads_sealed(commands, results, "/home/tabbench/fresh", allowed=(work, tools)))

    def test_paths_are_accepted_as_well_as_strings(self):
        from pathlib import Path
        work, tools = Path("/home/tabbench/runs/web-fresh/x/work/r"), Path("/home/tabbench/bench-tools")
        commands, results = self.stream(f"cat {work}/a.csv", result="ok")
        self.assertIsNone(run_web.reads_sealed(commands, results, Path("/home/tabbench/fresh"), allowed=(work, tools)))
        commands, results = self.stream("cat /home/tabbench/fresh/x", result="ok")
        self.assertTrue(run_web.reads_sealed(commands, results, Path("/home/tabbench/fresh"), allowed=(work, tools)))

    def test_a_result_showing_a_sealed_name_is_caught(self):
        commands, results = self.stream("tabref open http://127.0.0.1:8793/", result="link /__reset?seed=1")
        self.assertTrue(run_web.reads_sealed(commands, results, "/home/tabbench/fresh"))


class PerSetReport(unittest.TestCase):
    def raw(self, ts, task, ok, why, arm="tabref", cost=0.1, wall=5):
        r = {"suite": "web", "arm": arm, "task": task, "model": "m", "success": ok, "cli_s": 1, "wall_s": wall,
             "input_total": 100, "output_tokens": 0, "num_turns": 3, "verify": why, "host": "laptop", "valid": True,
             "cost_usd_list": cost}
        if ts:
            r["taskset"] = ts
        return r

    def models(self, rows, taskset=None):
        f = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False)
        f.write("".join(json.dumps(r) + "\n" for r in rows))
        f.close()
        self.addCleanup(os.unlink, f.name)
        args = argparse.Namespace(results=[f.name], baseline=False, model=None, arms=None, taskset=taskset)
        return "\n".join(bench_report.models(args))

    def test_default_is_heldout_and_fresh_is_ranking_only(self):
        rows = [self.raw(None, "hw1", False, "held-out reason")] + [self.raw("fresh", "t1", False, "fresh reason")]
        text = self.models(rows)
        self.assertIn("## Model runs: heldout tasks", text)
        self.assertIn("held-out reason", text)
        self.assertNotIn("fresh tasks", text)
        self.assertNotIn("fresh reason", text)
        text = self.models(rows, "fresh")
        self.assertIn("## Model runs: fresh tasks", text)
        self.assertIn("| 1 | tabref |", text)
        for absent in ("fresh reason", "per task", "loses", "#### t1", "sums of per-task"):
            self.assertNotIn(absent, text)

    def test_ties_on_pass_rate_go_to_the_cheaper_arm(self):
        rows = [self.raw(None, "hw1", True, "x", arm="bh", cost=0.5, wall=10) for _ in range(2)] + \
               [self.raw(None, "hw1", True, "x", arm="tabref", cost=0.1, wall=90) for _ in range(2)]
        lines = [x for x in self.models(rows).splitlines() if x.startswith("| 1 |") or x.startswith("| 2 |")]
        self.assertTrue(lines[0].startswith("| 1 | tabref |"), lines)
        self.assertIn("| 0.100 |", lines[0])


if __name__ == "__main__":
    unittest.main()
