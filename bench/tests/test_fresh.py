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
        ts.recheck()

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
        fresh.SEAL = "1067387e3e7631ff2a1d28675cb2a2391c99324a57f2fc7f5e45dbdd2fd5468a"
        self.assertGreater(fresh.verify(fresh.fresh_dir()), 0)


class SealedRows(unittest.TestCase):
    def test_free_text_moves_to_the_detail_file(self):
        d = Path(tempfile.mkdtemp(prefix="fresh-rows-"))
        self.addCleanup(shutil.rmtree, d, True)
        rec = {"task": "t1", "arm": "tabref", "round": 0, "seed": 5, "success": False, "verify": "secret why",
               "answer": "secret answer", "strays": ["a/:x"], "valid": False, "invalid_reason": "no result event: secret",
               "wall_s": 3.0}
        row = run_web.seal_record(rec, d / "detail.jsonl")
        self.assertEqual((row["verify"], row["strays"], row["invalid_reason"]), ("sealed", 1, "no result event"))
        self.assertNotIn("answer", row)
        self.assertNotIn("secret", json.dumps(row))
        detail = json.loads((d / "detail.jsonl").read_text())
        self.assertEqual((detail["answer"], detail["verify"]), ("secret answer", "secret why"))


class PerSetReport(unittest.TestCase):
    def raw(self, ts, task, ok, why):
        r = {"suite": "web", "arm": "tabref", "task": task, "model": "m", "success": ok, "cli_s": 1, "wall_s": 5,
             "input_total": 100, "output_tokens": 0, "num_turns": 3, "verify": why, "host": "laptop", "valid": True}
        if ts:
            r["taskset"] = ts
        return r

    def test_sections_per_set_and_no_fresh_text(self):
        rows = [self.raw(None, "hw1", False, "held-out reason")] + [self.raw("fresh", "t1", False, "fresh reason")]
        f = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False)
        f.write("".join(json.dumps(r) + "\n" for r in rows))
        f.close()
        self.addCleanup(os.unlink, f.name)
        args = argparse.Namespace(results=[f.name], baseline=False, model=None, arms=None, taskset=None)
        text = "\n".join(bench_report.models(args))
        self.assertIn("## Model runs: heldout tasks", text)
        self.assertIn("## Model runs: fresh tasks", text)
        self.assertIn("held-out reason", text)
        self.assertNotIn("fresh reason", text)
        args.taskset = "heldout"
        self.assertNotIn("fresh tasks", "\n".join(bench_report.models(args)))


if __name__ == "__main__":
    unittest.main()
