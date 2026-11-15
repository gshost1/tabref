"""Shared plumbing for the model runner, the latency probe and the replay: paths, the display session, Chrome,
per-arm environments, the bench site, and the wrappers' tool log.

Ported from the Linux run's run_web_linux.py / run_web_ab_linux.py (launch flags, fresh profile, stop, daemon
cleanup, tool-log totals); what changed is only where things live, so the same code runs on this laptop and on a
boxd VM:

  BENCH_TOOLS   installed arms (bench/tools/install.sh); default <main checkout>/.bench-tools
  BENCH_RUNS    run output; default bench/runs (untracked)
  TABREF_BIN    tabref binary; default <checkout>/target/release/tabref, checkout = --tabref-checkout or the
                main checkout
  display       headless   Chrome --headless=new, private D-Bus (no display; ad-14 cannot run)
                xvfb       private Xvfb + D-Bus + AT-SPI + openbox, headed Chrome (every arm, like the VM)
                host       the session's own DISPLAY and D-Bus (a boxd VM's GNOME session, as in the Linux run)
"""
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
HELDOUT = BENCH / "heldout"
BIN = HERE / "bin"
WRAP = HELDOUT / "linux" / "wrap.sh"
CHROME_FLAGS = ["--no-first-run", "--no-default-browser-check", "--disable-sync", "--password-store=basic",
                "--force-renderer-accessibility"]
FROZEN = {"heldout/tasks_heldout.py": "55a13cb15cec84e348630caf2d89dc254a6b6beaafc5ea0405b6ae56ecdac7c8",
          "heldout/web/server.py": "b6b60b534ed17aea189fc004973436514128874c3b831551b0f28507f2be8f0f",
          "tasks.py": "3cc9144bac48ab09fed83b1b14fb40beabc87fbf75ce087f9d7ec9718b8ddc0b",
          "heldout/linux/common_linux.py": "0594ba1ee4ee1f9fbe0b88d61460bd5a55513050c5aea84149c7a6ed0c43c1cd",
          "heldout/linux/common_ab_linux.py": "6c9f13724762675581fed01e697e6f2e8000a2e046cdcaea73a904c75accfed4",
          "heldout/linux/wrap.sh": "5ee33b1bbfdafc0228a28a02d1878c3b9b40f40c3f5a2d528c5175bddf3ee8a9",
          "heldout/linux/TASKS_LINUX.md": "474c971d608466944b53e6e8a24601e91dc9f076a3fe6fcd88a47c2c76a42cc2",
          "heldout/linux/TASKS_LINUX_AB.md": "76986707364e517b635a2c344d8096253265b8af68b29fcb73ee1d994f0370c6"}


def check_frozen():
    import hashlib
    bad = {p: hashlib.sha256((BENCH / p).read_bytes()).hexdigest() for p in FROZEN}
    bad = {p: h for p, h in bad.items() if h != FROZEN[p]}
    if bad:
        raise SystemExit(f"frozen files changed: {bad}")


def main_checkout():
    try:
        common = subprocess.run(["git", "-C", str(BENCH), "rev-parse", "--path-format=absolute", "--git-common-dir"],
                                capture_output=True, text=True, timeout=10).stdout.strip()
        return Path(common).parent if common else BENCH.parent
    except (OSError, subprocess.SubprocessError):
        return BENCH.parent


def tools_dir():
    return Path(os.environ.get("BENCH_TOOLS") or main_checkout() / ".bench-tools")


def runs_dir():
    return Path(os.environ.get("BENCH_RUNS") or BENCH / "runs")


def tabref_bin(checkout=None):
    if os.environ.get("TABREF_BIN"):
        return Path(os.environ["TABREF_BIN"])
    return Path(checkout or main_checkout()) / "target" / "release" / "tabref"


def ad_bin():
    return tools_dir() / "agent-desktop-1.4.0" / "agent-desktop"


def claude_bin():
    if os.environ.get("BENCH_CLAUDE"):
        return Path(os.environ["BENCH_CLAUDE"])
    arch = {"x86_64": "x64", "aarch64": "arm64"}.get(os.uname().machine, os.uname().machine)
    return tools_dir() / "claude-code-2.1.322" / "node_modules" / "@anthropic-ai" / f"claude-code-linux-{arch}" / "claude"


def arm_available(arm, tabref=None):
    """None when the arm can run here, else the reason it is skipped."""
    t = tools_dir()
    need = {"ad-14": ad_bin(), "bh": t / "bh-venv" / "bin" / "browser-harness",
            "pw": t / "playwright-cli-0.1.24" / "node_modules" / ".bin" / "playwright-cli",
            "ab": t / "agent-browser-0.42.0" / "node_modules" / "agent-browser",
            "tabref": Path(tabref or tabref_bin())}
    p = need.get(arm)
    if p is None:
        return f"unknown arm {arm}"
    return None if p.exists() else f"{arm}: {p} not found"


def http_json(url, timeout=5, method=None):
    with urllib.request.urlopen(urllib.request.Request(url, method=method), timeout=timeout) as r:
        return json.loads(r.read())


WARM_PATH = "/warm"


def warm_up(port, base, limit=30.0, poll=0.1):
    """Loads one throwaway page before the timer starts, the same for every arm. A just-started Chrome holds its
    first page load until it has read its profile (the cookie store), which under load takes seconds; without this
    the wait lands in whichever arm's first call comes first. It goes through the DevTools HTTP endpoint only (no
    tool's daemon is started and the starting about:blank tab is left alone): PUT /json/new?<base>/warm, poll
    /json/list until that tab has committed (Chrome leaves a tab's title empty until its document commits, then
    shows the page's title or host/path), then /json/close/<id>. The site answers /warm with its 404 page, and
    Site.state() drops it from the page log. Never raises: returns the seconds spent, whether the page committed,
    and why not."""
    t0 = time.time()
    devtools = f"http://127.0.0.1:{port}"
    out = {"warmup_s": None, "warmup_committed": False}
    tab = None
    try:
        tab = http_json(f"{devtools}/json/new?{base}{WARM_PATH}", method="PUT")["id"]
        while time.time() - t0 < limit:
            mine = [t for t in http_json(f"{devtools}/json/list") if t.get("id") == tab]
            if mine and mine[0].get("title"):
                out["warmup_committed"] = True
                break
            time.sleep(poll)
        else:
            out["warmup_error"] = f"no commit within {limit:.0f} s"
    except (OSError, ValueError, KeyError, TypeError) as e:
        out["warmup_error"] = f"{type(e).__name__}: {e}"[:200]
    finally:
        if tab and not close_tab(devtools, tab, poll):
            out["warmup_error"] = (out.get("warmup_error", "") + "; warm-up tab still listed after close").lstrip("; ")
    out["warmup_s"] = round(time.time() - t0, 2)
    return out


def close_tab(devtools, tab, poll, limit=5.0):
    """/json/close only starts closing a tab; it stays in /json/list for a moment, where a tool's first call could
    pick it. Waits until it is gone. Whether it went."""
    start = time.time()
    try:
        urllib.request.urlopen(f"{devtools}/json/close/{tab}", timeout=5).read()
        while time.time() - start < limit:
            if all(t.get("id") != tab for t in http_json(f"{devtools}/json/list")):
                return True
            time.sleep(poll)
    except (OSError, ValueError):
        pass
    return False


def without_warm(state):
    """The site state with the warm-up's own page request taken out of its page log, so a verifier that reads the
    pages opened (hw7 reads `navs`) sees only what the model did."""
    if isinstance(state, dict) and isinstance(state.get("navs"), list):
        state = dict(state, navs=[n for n in state["navs"] if not (isinstance(n, dict) and n.get("path") == WARM_PATH)])
    return state


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def base_env(home):
    """A clean environment: none of the operator's session, keys or tool settings reach the arms or the model."""
    env = {k: os.environ[k] for k in ("USER", "LOGNAME", "LANG", "LC_ALL", "TZ") if k in os.environ}
    env.update({"HOME": str(home), "SHELL": "/bin/bash", "TERM": "dumb", "PATH": "/usr/bin:/bin",
                "XDG_RUNTIME_DIR": short_dir(f"home:{home}"), "BENCH_TOOLS": str(tools_dir())})
    env.setdefault("LANG", "C.UTF-8")
    Path(home).mkdir(parents=True, exist_ok=True)
    return env


_SHORT = {}


def short_dir(key):
    """A private runtime folder under /tmp: Chrome, D-Bus and agent-browser put sockets there, and a socket path
    must stay under 108 bytes."""
    if key not in _SHORT:
        import atexit
        import tempfile
        _SHORT[key] = tempfile.mkdtemp(prefix="bench-rt-")
        atexit.register(shutil.rmtree, _SHORT[key], True)
    return _SHORT[key]


BUS_CONF = """<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:dir={run}</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy>
</busconfig>
"""


def exit_on_sigterm():
    """Turn SIGTERM into SystemExit, so `finally` blocks stop Chrome, daemons and the display session."""
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))


class Display:
    """The session Chrome and the arms run in (see the module doc). Its D-Bus has no service folders, so nothing is
    activated on it (no desktop portals); AT-SPI's bus launcher is started explicitly."""

    def __init__(self, mode, log_dir):
        self.mode, self.log_dir, self.procs, self.env = mode, Path(log_dir), [], {}

    def start(self):
        self.log_dir.mkdir(parents=True, exist_ok=True)
        if self.mode == "host":
            self.env = {k: os.environ[k] for k in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS",
                                                    "XDG_RUNTIME_DIR", "XAUTHORITY") if k in os.environ}
            return self
        run = short_dir(f"display:{self.log_dir}")
        self.env = {"XDG_RUNTIME_DIR": str(run)}
        conf = Path(run) / "session.conf"
        conf.write_text(BUS_CONF.format(run=run))
        bus = subprocess.Popen(["dbus-daemon", f"--config-file={conf}", "--nofork", "--print-address=1"], stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL, text=True, start_new_session=True,
                               env=dict(os.environ, XDG_RUNTIME_DIR=str(run)))
        self.procs.append(bus)
        self.env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
        if self.mode == "headless":
            return self
        num = next(n for n in range(90, 200) if not Path(f"/tmp/.X{n}-lock").exists()
                   and not Path(f"/tmp/.X11-unix/X{n}").exists())
        self.env.update({"DISPLAY": f":{num}", "GTK_MODULES": "gail:atk-bridge", "NO_AT_BRIDGE": "0",
                         "ACCESSIBILITY_ENABLED": "1", "QT_ACCESSIBILITY": "1"})
        self._spawn(["Xvfb", f":{num}", "-screen", "0", "1920x1080x24", "-nolisten", "tcp"], "xvfb.log")
        for _ in range(50):
            if Path(f"/tmp/.X11-unix/X{num}").exists():
                break
            time.sleep(0.1)
        self._spawn(["/usr/libexec/at-spi-bus-launcher", "--launch-immediately"], "atspi.log")
        self._spawn(["openbox"], "openbox.log")
        time.sleep(1.0)
        return self

    def _spawn(self, cmd, log):
        env = dict(os.environ, **self.env)
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("XDG_SESSION_TYPE", None)
        fh = open(self.log_dir / log, "a")
        self.procs.append(subprocess.Popen(cmd, env=env, stdout=fh, stderr=fh, start_new_session=True))

    def stop(self):
        for p in reversed(self.procs):
            try:
                os.killpg(p.pid, signal.SIGTERM)
            except (ProcessLookupError, PermissionError):
                pass
        for p in self.procs:
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
        self.procs = []

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()


def profile_processes(profile):
    """Chrome processes of this profile: an argv entry equal to --user-data-dir=<profile> (not a mention of it)."""
    flag = f"--user-data-dir={profile}".encode()
    out = subprocess.run(["pgrep", "-u", str(os.getuid()), "-f", "--", f"--user-data-dir={profile}"], capture_output=True,
                         text=True).stdout
    pids = []
    for p in out.split():
        try:
            if flag in Path(f"/proc/{p}/cmdline").read_bytes().split(b"\0"):
                pids.append(int(p))
        except OSError:
            pass
    return pids


def write_profile(profile, downloads):
    profile.mkdir(parents=True)
    (profile / "Default").mkdir()
    prefs = {"download": {"default_directory": str(downloads), "prompt_for_download": False},
             "profile": {"default_content_setting_values": {"automatic_downloads": 1}},
             "browser": {"has_seen_welcome_page": True}}
    staged = profile / "Default" / "Preferences.tmp"
    staged.write_text(json.dumps(prefs))
    os.replace(staged, profile / "Default" / "Preferences")


class Pid:
    def __init__(self, pid):
        self.pid = pid

    def poll(self):
        try:
            os.kill(self.pid, 0)
            return None
        except ProcessLookupError:
            return 0


def chrome_flags(display):
    return CHROME_FLAGS + (["--headless=new"] if display.mode == "headless" else [])


def launch_chrome(profile, port, downloads, display, env, chrome="/usr/bin/google-chrome"):
    """run_web_linux.launch_chrome: a fresh profile, the bench flags, a DevTools port, about:blank."""
    write_profile(profile, downloads)
    args = [chrome, f"--user-data-dir={profile}"] + chrome_flags(display) + [f"--remote-debugging-port={port}",
                                                                              "about:blank"]
    proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
                            env=dict(env, **display.env))
    deadline = time.time() + 45
    while time.time() < deadline:
        try:
            http_json(f"http://127.0.0.1:{port}/json/version")
            return proc
        except Exception:
            time.sleep(0.3)
    stop_chrome(proc, profile)
    raise RuntimeError("chrome devtools endpoint never came up")


def launch_chrome_ad(home, profile, downloads, display, env, app="google-chrome"):
    """run_web_linux.launch_chrome_ad: agent-desktop 1.4.0 launches Chrome with its own DevTools endpoint."""
    if display.mode == "headless":
        raise RuntimeError("ad-14 needs a display session (agent-desktop launch refuses without one); use --display xvfb")
    write_profile(profile, downloads)
    run_env = dict(env, **display.env, AGENT_DESKTOP_HOME=str(home))
    cmd = [str(ad_bin()), "launch", app, "--cdp", "--download-dir", str(downloads)] + \
        [f"--arg={f}" for f in [f"--user-data-dir={profile}"] + chrome_flags(display)]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=60, env=run_env)
    try:
        data = json.loads(p.stdout)
    except ValueError:
        data = {}
    pid = (data.get("data") or {}).get("pid")
    if not data.get("ok") or not pid:
        for left in profile_processes(profile):
            os.kill(left, signal.SIGTERM)
        raise RuntimeError(f"agent-desktop launch --cdp failed: {p.stdout.strip()[:300]} {p.stderr.strip()[:200]}")
    return Pid(pid)


def ad_endpoint_port(home):
    try:
        return json.loads((Path(home) / "web" / "endpoints.json").read_text())["entries"][0]["port"]
    except (OSError, ValueError, KeyError, IndexError):
        return None


def stop_chrome(proc, profile):
    for sig, grace in ((signal.SIGTERM, 10), (signal.SIGKILL, 5)):
        if proc is not None:
            try:
                os.killpg(proc.pid, sig)
            except (ProcessLookupError, PermissionError):
                pass
        for pid in profile_processes(profile):
            try:
                os.kill(pid, sig)
            except ProcessLookupError:
                pass
        deadline = time.time() + grace
        while time.time() < deadline:
            if not profile_processes(profile):
                return True
            time.sleep(0.2)
    return not profile_processes(profile)


def core_dir():
    """run_web_linux's /opt/bench/core: node, npm, npx, jq and claude, linked into one PATH folder."""
    core = runs_dir() / "core"
    core.mkdir(parents=True, exist_ok=True)
    node = tools_dir() / "node-v26.13.0" / "bin"
    links = {n: str(node / n) if (node / n).exists() else shutil.which(n) for n in ("node", "npm", "npx")}
    links["jq"] = shutil.which("jq")
    links["claude"] = str(claude_bin()) if claude_bin().exists() else shutil.which("claude")
    for name, target in links.items():
        if target:
            link = core / name
            if link.is_symlink() or link.exists():
                link.unlink()
            link.symlink_to(Path(target).resolve())
    return core


def arm_env(arm, work, cdp_port, display, home, tabref=None):
    """The environment a run's tool calls see: the arm's wrapper folder first on PATH, the wrapper's log, a
    per-run arm home, and the run's DevTools port (bh, pw, ab and tabref read it from here). The two homes are short
    folders under /tmp (unix sockets live there), linked from the work folder as ad-home and arm-home."""
    work = Path(work)
    work.mkdir(parents=True, exist_ok=True)
    env = base_env(home)
    env.update(display.env)
    homes = {}
    for name in ("ad-home", "arm-home"):
        homes[name] = short_dir(f"{name}:{work}")
        os.chmod(homes[name], 0o700)
        link = work / name
        if link.is_symlink():
            link.unlink()
        elif link.exists():
            shutil.rmtree(link)
        link.symlink_to(homes[name])
    env.update({"PATH": f"{BIN / arm}:{core_dir()}:/usr/bin:/bin", "AGENT_DESKTOP_HOME": homes["ad-home"],
                "AD_LOG": str(work / "tool.log"), "HO_ARM_HOME": homes["arm-home"],
                "HO_CDP_PORT": str(cdp_port or ""), "HO_RUNS": str(runs_dir()), "HO_WRAP": str(WRAP),
                "TABREF_BIN": str(tabref or tabref_bin())})
    return env


def stop_arm_daemons(arm, env=None):
    """Daemons a CDP tool started for a run must not carry state into the next one. Only processes that carry this
    run's own HO_ARM_HOME or TABREF_HOME in their environment are stopped (every wrapper exports both), so other
    runs, other sessions and the operator's own copies of these tools are never touched. tabref gets its own
    `tabref daemon stop` first."""
    if not env:
        return
    tabref_home = str(Path(env["HO_ARM_HOME"]) / "tabref-home")
    if arm == "tabref" and Path(env["TABREF_BIN"]).exists():
        stop_env = dict(env, TABREF_HOME=tabref_home, TABREF_CDP=env["HO_CDP_PORT"])
        stop_env.pop("ANTHROPIC_API_KEY", None)
        try:
            subprocess.run([env["TABREF_BIN"], "daemon", "stop"], env=stop_env, capture_output=True, timeout=15)
        except subprocess.SubprocessError:
            pass
    kill_marked([f"HO_ARM_HOME={env['HO_ARM_HOME']}".encode(), f"TABREF_HOME={tabref_home}".encode()])


def ancestors():
    pid, out = os.getpid(), set()
    while pid > 1:
        out.add(pid)
        try:
            pid = int(Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[1])
        except (OSError, ValueError, IndexError):
            break
    return out


def marked_pids(markers):
    """This user's processes whose environment holds one of `markers` (exact KEY=VALUE entries)."""
    mine, uid, out = ancestors(), os.getuid(), []
    for d in Path("/proc").iterdir():
        if not d.name.isdigit() or int(d.name) in mine:
            continue
        try:
            if d.stat().st_uid != uid:
                continue
            entries = set((d / "environ").read_bytes().split(b"\0"))
        except OSError:
            continue
        if any(m in entries for m in markers):
            out.append(int(d.name))
    return out


def kill_marked(markers, grace=3.0):
    pids = marked_pids(markers)
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for pid in pids:
            try:
                os.kill(pid, sig)
            except ProcessLookupError:
                pass
        deadline = time.time() + grace
        while time.time() < deadline and any(Path(f"/proc/{p}").exists() for p in pids):
            time.sleep(0.1)
        pids = [p for p in pids if Path(f"/proc/{p}").exists()]
        if not pids:
            return


def real_dirs(work):
    """The run's work folder plus the folders its ad-home and arm-home links point to (rglob does not follow them)."""
    out = [Path(work)]
    for name in ("ad-home", "arm-home"):
        link = Path(work) / name
        if link.is_symlink():
            out.append(link.resolve())
    return out


def on_vm():
    try:
        return subprocess.run(["systemd-detect-virt", "--vm"], capture_output=True, text=True,
                              timeout=10).stdout.strip() not in ("", "none")
    except (OSError, subprocess.SubprocessError):
        return False


def scrub(text):
    """Result text without the operator's paths: the checkout becomes `.`, the home `~`."""
    text = str(text).replace(str(main_checkout()) + "/", "./").replace(str(BENCH.parent) + "/", "./")
    return text.replace(str(Path.home()), "~")


def host_label():
    """A generic host name for result rows: BENCH_HOST, else `vm` or `laptop` (no machine names in results)."""
    return os.environ.get("BENCH_HOST") or ("vm" if on_vm() else "laptop")


BENCH_USER = os.environ.get("BENCH_USER", "tabbench")


def isolation_problem(display_mode):
    """Why a model pass must not run here, or None. On a VM anything goes (the VM is the sandbox). Elsewhere the
    model must run as the dedicated unprivileged user (tools/tabbench.sh sets it up): not root, no admin groups, no
    readable home of another user, and never on the operator's display or D-Bus."""
    if on_vm():
        return None
    import grp
    import pwd
    me = pwd.getpwuid(os.getuid())
    if me.pw_name != BENCH_USER:
        return (f"model passes run only as the user {BENCH_USER!r} or on a VM; this is {me.pw_name!r}. "
                "Set it up once with bench/tools/tabbench.sh setup")
    groups = {g.gr_name for g in grp.getgrall() if me.pw_name in g.gr_mem} | {grp.getgrgid(me.pw_gid).gr_name}
    admin = groups & {"sudo", "wheel", "adm", "admin", "lxd", "docker", "libvirt"}
    if os.getuid() == 0 or admin:
        return f"{me.pw_name} must be unprivileged; it is in {sorted(admin) or ['root']}"
    readable = [str(h) for h in Path("/home").iterdir()
                if h.is_dir() and h != Path(me.pw_dir) and os.access(h, os.R_OK | os.X_OK)]
    if readable:
        return (f"{me.pw_name} can read {readable}; make them private (e.g. `chmod 750 {readable[0]}`) "
                "so the model cannot read the operator's files")
    if display_mode == "host":
        return "--display host would hand the model the operator's display and D-Bus; use xvfb (or run on a VM)"
    return None


def run_in_session(argv, cwd, env, timeout, stdin=None):
    """Run one tool call in its own session; on timeout the whole process group is killed (a daemon the tool forked
    cannot hold the call open) and the call reports rc 124 at its wall time. Returns rc, stdout, stderr, seconds,
    timed_out."""
    t0 = time.time()
    p = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        out, err = p.communicate(stdin, timeout=timeout)
        return p.returncode, out or "", err or "", time.time() - t0, False
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        out, err = p.communicate()
        return 124, out or "", (err or "") + f"\nCommand timed out after {timeout} s", time.time() - t0, True


def tool_log_lines(path):
    """wrap.sh lines: t0, t1, rc, stdout bytes, args."""
    out = []
    if Path(path).exists():
        for line in Path(path).read_text(errors="replace").splitlines():
            parts = line.split("\t", 4)
            if len(parts) >= 5:
                out.append({"t0": float(parts[0]), "t1": float(parts[1]), "rc": int(parts[2]), "bytes": int(parts[3]),
                            "args": parts[4]})
    return out


def tool_log_totals(path):
    lines = tool_log_lines(path)
    return len(lines), round(sum(x["t1"] - x["t0"] for x in lines), 3)


class Site:
    """A task set's site, started by this run on 127.0.0.1:<port>; a port in use is an error. The default script is
    heldout/web/server.py (frozen); the fresh set passes its sealed server. -B: no bytecode next to either. `log` is a
    file path, or a callable that receives the server's output line by line (the fresh set keeps it in memory)."""

    def __init__(self, base="http://127.0.0.1:8792", log=None, script=None):
        self.base, self.log, self.proc = base, log, None
        self.script = Path(script) if script else HELDOUT / "web" / "server.py"

    def start(self):
        port = int(self.base.rsplit(":", 1)[1])
        with socket.socket() as probe:
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                probe.bind(("127.0.0.1", port))
            except OSError:
                raise SystemExit(f"port {port} is taken: another site (or another bench) is there; pass --base with "
                                 "a free port rather than sharing its state")
        sink = self.log if callable(self.log) else None
        if self.log and not sink:
            Path(self.log).parent.mkdir(parents=True, exist_ok=True)
        fh = subprocess.PIPE if sink else open(self.log, "a") if self.log else subprocess.DEVNULL
        self.proc = subprocess.Popen([sys.executable, "-I", "-B", str(self.script), self.base.rsplit(":", 1)[1]],
                                     stdout=fh, stderr=subprocess.STDOUT if sink else fh, start_new_session=True)
        if sink:
            import threading
            threading.Thread(target=lambda: [sink(x.decode(errors="replace")) for x in self.proc.stdout],
                             daemon=True).start()
        for _ in range(50):
            if self.proc.poll() is not None:
                raise SystemExit(f"{self.script.name} exited at start")
            try:
                http_json(f"{self.base}/__state")
                return self
            except Exception:
                time.sleep(0.2)
        raise SystemExit(f"{self.script.name} did not come up")

    def reset(self, seed):
        return http_json(f"{self.base}/__reset?seed={seed}")

    def state(self):
        return without_warm(http_json(f"{self.base}/__state"))

    def stop(self):
        if self.proc is not None:
            self.proc.terminate()
            self.proc = None

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()
