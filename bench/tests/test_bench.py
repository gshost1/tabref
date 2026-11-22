"""Bench plumbing tests: frozen files, arm prompts, wrapper refusals, replay mapping and the scripted sequences' answer
parsers. No Chrome, no network, no model.

Run: python3 -I -m unittest discover -s bench/tests -p 'test_*.py'   (and bench/heldout/tests for the verifiers)
"""
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH / "harness"))
sys.path.insert(0, str(BENCH / "heldout"))
sys.path.insert(0, str(BENCH))
sys.path.insert(0, str(BENCH / "replay"))
import arms  # noqa: E402
import core  # noqa: E402
import tasks_heldout as th  # noqa: E402

spec = importlib.util.spec_from_file_location("replay_run", BENCH / "replay" / "run.py")
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)


def script(tid):
    s = importlib.util.spec_from_file_location(f"s_{tid}", BENCH / "replay" / "tabref" / f"{tid}.py")
    m = importlib.util.module_from_spec(s)
    s.loader.exec_module(m)
    return m


class Frozen(unittest.TestCase):
    def test_hashes(self):
        core.check_frozen()


class Prompts(unittest.TestCase):
    def test_old_arms_are_the_frozen_text(self):
        import common_ab_linux
        import common_linux
        saved = common_linux.SKILLS
        try:
            common_linux.SKILLS = Path("/opt/bench/ho-tools/skills")
            frozen = {a: common_ab_linux.web_system_prompt(a, 4242) for a in ("ad-14", "bh", "pw", "ab")}
        finally:
            common_linux.SKILLS = saved
        for arm, text in frozen.items():
            want = text.replace("/opt/bench/ho-tools/skills", str(arms.skills_dir()))
            self.assertEqual(arms.frozen_system_prompt(arm, 4242), want, arm)
            self.assertEqual(arms.web_system_prompt(arm, 4242), want + arms.start_line(arm, 4242), arm)
            self.assertEqual(arms.web_allowed_tools(arm), common_ab_linux.web_allowed_tools(arm), arm)

    def test_tabref_prompt(self):
        text = arms.web_system_prompt("tabref", 4242)
        self.assertIn("`tabref skill`", text)
        self.assertIn("--cdp 4242", text)
        self.assertTrue(text.endswith(arms.WEB_COMMON + arms.start_line("tabref", 4242)))
        self.assertIn("`tabref skill && tabref open <url>`", text)
        self.assertEqual(arms.web_allowed_tools("tabref")[-1], "Bash(tabref:*)")


class Wrappers(unittest.TestCase):
    def run_wrapper(self, arm, tool, *args, **env):
        e = dict(os.environ, BENCH_TOOLS="/nonexistent", **env)
        return subprocess.run([str(core.BIN / arm / tool), *args], capture_output=True, text=True, env=e, timeout=20)

    def test_ab_refuses_like_the_linux_wrapper(self):
        for args in (["chat", "hi"], ["--cdp", "1", "install"], ["-p", "x", "open"], ["--auto-connect", "open"],
                     ["plugins"]):
            self.assertEqual(self.run_wrapper("ab", "agent-browser", *args).returncode, 64, args)

    def test_tabref_refuses_the_users_own_browser(self):
        env = dict(TABREF_BIN="/bin/true", HO_CDP_PORT="1", HO_WRAP="/bin/true")
        for args in (["connect", "--user-chrome"], ["--cdp", "1", "connect", "--user-chrome"],
                     ["--json", "--timeout", "5", "connect", "--off"], ["--tab=0", "connect"]):
            p = self.run_wrapper("tabref", "tabref", *args, **env)
            self.assertEqual(p.returncode, 64, args)
            self.assertIn("connect", p.stderr)
        self.assertEqual(self.run_wrapper("tabref", "tabref", "open", "connect", **env).returncode, 0)
        self.assertEqual(self.run_wrapper("tabref", "tabref", "--tab", "connect", "snap", **env).returncode, 0)

    def test_tabref_without_binary_fails_cleanly(self):
        p = self.run_wrapper("tabref", "tabref", "skill", TABREF_BIN="/nonexistent/tabref", HO_CDP_PORT="1")
        self.assertEqual(p.returncode, 2)
        self.assertIn("not found", p.stderr)


class Replay(unittest.TestCase):
    def test_kind(self):
        self.assertEqual(replay.kind("ab", "agent-browser --cdp 4 snapshot -i | head"), "snapshot")
        self.assertEqual(replay.kind("ad-14", 'agent-desktop click @s1:e2 >/dev/null'), "click")
        self.assertEqual(replay.kind("ad-14", "agent-desktop skills get web"), "guide")
        self.assertEqual(replay.kind("bh", "browser-harness skill"), "guide")
        self.assertEqual(replay.kind("bh", "browser-harness <<'PY'\nprint(1)\nPY"), "script")
        self.assertEqual(replay.kind("pw", "playwright-cli attach --cdp=http://127.0.0.1:5"), "attach")

    def test_ids(self):
        out = '{"snapshot_id": "s2ga03tmrop6gr"} "@s1nx1gba1dag0w:e6 link"'
        self.assertEqual(replay.ordered(replay.AD_SNAP, out), ["s2ga03tmrop6gr", "s1nx1gba1dag0w"])
        out = "- [Snapshot](.playwright-cli/page-2026-11-11T09-00-51-071Z.yml)"
        self.assertEqual(replay.ordered(replay.PW_FILE, out), ["page-2026-11-11T09-00-51-071Z.yml"])

    def test_ref_helper(self):
        t = replay.Tabref.__new__(replay.Tabref)
        out = "@s8f3k2p9:e1 link \"Shop\"\n@s8f3k2p9:e2 textbox \"Search entire store\"\nAuthor MOD\n@s8f3k2p9:e9 button \"Reply\""
        self.assertEqual(t.ref(out, "search entire store"), "@s8f3k2p9:e2")
        self.assertEqual(t.ref(out, "reply", after=r"\bMOD\b"), "@s8f3k2p9:e9")
        with self.assertRaises(LookupError):
            t.ref(out, "nothing here")


class Answers(unittest.TestCase):
    def test_hw1_parser_on_seeded_reviews(self):
        for rnd in range(5):
            w = th.world(th.web_seed("hw1", rnd))
            text = "\n".join(f"{'★' * r['rating']} Review by {r['author']}\n{r['body']}" for r in w["hw1"]["reviews"])
            got = script("hw1").complainers(text)
            ok, why = th.verify_hw1(w, ", ".join(got), {})
            self.assertTrue(ok, why)

    def test_hw2_parser_on_seeded_orders(self):
        import datetime
        m = script("hw2")
        for rnd in range(5):
            w = th.world(th.web_seed("hw2", rnd))
            text = "\n".join(f"{o['number']} {datetime.date.fromisoformat(o['date']).strftime('%-m/%-d/%y')} "
                             f"{o['ship_to']} {th.fmt_money(o['total'])} {o['status']}" for o in w["hw2"]["orders"])
            found = m.ROW.findall(text)
            self.assertEqual(len(found), len(w["hw2"]["orders"]))


class PwGuideReadable(unittest.TestCase):
    def test_pw_gets_its_skills_folder_and_no_other_arm_does(self):
        from harness import arms
        self.assertEqual(arms.web_extra_args("pw"), ["--add-dir", str(arms.skills_dir())])
        for arm in ("tabref", "ab", "bh", "ad-14"):
            self.assertEqual(arms.web_extra_args(arm), [])


class ArmsNotesMatchTheCode(unittest.TestCase):
    def test_recorded_paragraphs_are_the_prompts_the_runner_sends(self):
        import re
        from harness import arms
        notes = (Path(__file__).resolve().parents[1] / "ARMS.md").read_text()
        section = notes[notes.index("## Tool paragraphs from 2026-11-15"):]
        blocks = dict(re.findall(r"### (\S+)\n\n```\n(.*?)\n```", section, re.S))
        self.assertEqual(sorted(blocks), sorted(arms.START_COMMANDS))
        for arm, text in blocks.items():
            want = arms.web_system_prompt(arm, "PORT").replace(str(arms.skills_dir()), "SKILLS")
            self.assertEqual(text, want, arm)


class ChromeProfile(unittest.TestCase):
    def assert_download_prefs(self, profile, downloads):
        prefs_path = profile / "Default" / "Preferences"
        self.assertTrue(prefs_path.is_file(), "Preferences file must be written")
        self.assertFalse((profile / "Default" / "Preferences.tmp").exists())
        prefs = json.loads(prefs_path.read_text())
        self.assertEqual(prefs["download"], {"default_directory": str(downloads), "prompt_for_download": False})
        self.assertIs(prefs["download_bubble"]["partial_view_enabled"], False)
        self.assertEqual(prefs["profile"]["default_content_setting_values"]["automatic_downloads"], 1)
        self.assertIs(prefs["browser"]["has_seen_welcome_page"], True)

    def test_write_profile_disables_download_bubble_partial_view_and_keeps_download_prefs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            profile = root / "profile"
            downloads = root / "downloads"
            core.write_profile(profile, downloads)
            self.assert_download_prefs(profile, downloads)

    def test_both_chrome_launchers_write_download_bubble_pref_for_every_arm(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            display = core.Display("xvfb", root / "logs")

            cdp_profile, cdp_downloads = root / "cdp-profile", root / "cdp-downloads"
            with mock.patch.object(core.subprocess, "Popen", return_value=mock.MagicMock()) as popen, \
                    mock.patch.object(core, "http_json", return_value={"Browser": "Chrome"}):
                core.launch_chrome(cdp_profile, 9222, cdp_downloads, display, {})
            popen.assert_called_once()
            self.assert_download_prefs(cdp_profile, cdp_downloads)

            ad_profile, ad_downloads = root / "ad-profile", root / "ad-downloads"
            fake_done = mock.MagicMock(stdout=json.dumps({"ok": True, "data": {"pid": 12345}}), stderr="")
            with mock.patch.object(core, "ad_bin", return_value=root / "agent-desktop"), \
                    mock.patch.object(core.subprocess, "run", return_value=fake_done) as run:
                pid = core.launch_chrome_ad(root / "ad-home", ad_profile, ad_downloads, display, {})
            run.assert_called_once()
            self.assertEqual(pid.pid, 12345)
            self.assert_download_prefs(ad_profile, ad_downloads)


if __name__ == "__main__":
    unittest.main()
