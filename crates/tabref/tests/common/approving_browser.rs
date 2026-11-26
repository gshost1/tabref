//! A stand-in for the user's own browser with remote debugging turned on at
//! chrome://inspect, which asks its user to allow every new DevTools
//! connection. It is a proxy in front of a real headless Chrome, run as a
//! copy of the Python interpreter named `chrome` that holds the "user's"
//! profile (its lock and its `DevToolsActivePort`), so tabref's owner checks
//! take it for the browser. Each connection it accepts is a prompt: it holds
//! the WebSocket upgrade, writes `pending-N` in its control folder, and goes
//! on only once the test writes `allow-N` (or closes the connection for
//! `deny-N`). A connection its client dropped while held gets `closed-N`.
//! Linux only. Included with `#[path]`.
#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PROXY: &str = r#"import os, select, socket, sys, threading, time
profile, control, real_port, real_path = sys.argv[1:5]
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', 0))
s.listen(16)
lock = os.path.join(profile, 'SingletonLock')
if os.path.lexists(lock):
    os.remove(lock)
os.symlink('host-%d' % os.getpid(), lock)
port_file = os.path.join(profile, 'DevToolsActivePort')
open(port_file + '.tmp', 'w').write('%d\n%s\n' % (s.getsockname()[1], real_path))
os.replace(port_file + '.tmp', port_file)

def mark(name):
    open(os.path.join(control, name), 'w').close()

def has(name):
    return os.path.exists(os.path.join(control, name))

def pipe(a, b):
    try:
        while True:
            data = a.recv(65536)
            if not data:
                break
            b.sendall(data)
    except OSError:
        pass
    for sock in (a, b):
        try:
            sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass

def serve(client, n):
    request = b''
    while b'\r\n\r\n' not in request:
        data = client.recv(65536)
        if not data:
            return
        request += data
    mark('pending-%d' % n)
    while not has('allow-%d' % n):
        if has('deny-%d' % n):
            client.close()
            mark('denied-%d' % n)
            return
        readable, _, _ = select.select([client], [], [], 0.02)
        if readable and not client.recv(1, socket.MSG_PEEK):
            mark('closed-%d' % n)
            client.close()
            return
    upstream = socket.create_connection(('127.0.0.1', int(real_port)))
    upstream.sendall(request)
    mark('allowed-%d' % n)
    threading.Thread(target=pipe, args=(upstream, client), daemon=True).start()
    pipe(client, upstream)

n = 0
while True:
    client, _ = s.accept()
    n += 1
    threading.Thread(target=serve, args=(client, n), daemon=True).start()
"#;

/// The fake browser and the real headless Chrome behind it.
pub struct ApprovingBrowser {
    pub control: PathBuf,
    proxy: Child,
    chrome: Child,
    chrome_profile: PathBuf,
}

impl ApprovingBrowser {
    /// Starts Chrome on a profile of its own under `tools`, and the proxy on
    /// the user's `profile`.
    pub fn start(tools: &Path, profile: &Path) -> Self {
        std::fs::create_dir_all(tools).unwrap();
        std::fs::create_dir_all(profile).unwrap();
        let control = tools.join("prompts");
        let _ = std::fs::remove_dir_all(&control);
        std::fs::create_dir_all(&control).unwrap();
        let chrome_profile = tools.join("real-chrome");
        let chrome = Command::new(tabref_cdp::find_chrome().unwrap())
            .args([
                format!("--user-data-dir={}", chrome_profile.display()),
                "--headless=new".to_owned(),
                "--no-first-run".to_owned(),
                "--remote-debugging-port=0".to_owned(),
                "about:blank".to_owned(),
            ])
            .args(
                std::env::var("TABREF_CHROME_ARGS")
                    .unwrap_or_default()
                    .split_whitespace(),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        let real = wait_for(|| tabref_cdp::read_active_port(&chrome_profile).ok())
            .expect("the real Chrome never wrote its DevToolsActivePort");
        let script = tools.join("approving.py");
        std::fs::write(&script, PROXY).unwrap();
        let _ = std::fs::remove_file(profile.join("DevToolsActivePort"));
        let binary = python_named_chrome(tools);
        let log = tools.join("approving.log");
        let proxy = spawn_retrying(|| {
            Command::new(&binary)
                .arg(&script)
                .arg(profile)
                .arg(&control)
                .arg(real.port.to_string())
                .arg(&real.websocket_path)
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(&log)?)
                .process_group(0)
                .spawn()
        });
        if wait_for(|| tabref_cdp::read_active_port(profile).ok()).is_none() {
            panic!(
                "the approving browser never wrote its DevToolsActivePort: {}",
                std::fs::read_to_string(&log).unwrap_or_default()
            );
        }
        Self {
            control,
            proxy,
            chrome,
            chrome_profile,
        }
    }

    /// How many prompts it has shown: one per connection.
    pub fn prompts(&self) -> usize {
        self.count("pending-")
    }

    /// How many held connections their client dropped before an answer.
    pub fn dropped(&self) -> usize {
        self.count("closed-")
    }

    /// Waits up to ten seconds for prompt `n` to show.
    pub fn wait_for_prompt(&self, n: usize) -> bool {
        wait_for(|| {
            self.control
                .join(format!("pending-{n}"))
                .exists()
                .then_some(())
        })
        .is_some()
    }

    /// Answers prompt `n` with Allow.
    pub fn allow(&self, n: usize) {
        std::fs::write(self.control.join(format!("allow-{n}")), "").unwrap();
    }

    /// Answers prompt `n` with Cancel: the browser closes that connection.
    pub fn deny(&self, n: usize) {
        std::fs::write(self.control.join(format!("deny-{n}")), "").unwrap();
        wait_for(|| {
            self.control
                .join(format!("denied-{n}"))
                .exists()
                .then_some(())
        });
    }

    fn count(&self, prefix: &str) -> usize {
        std::fs::read_dir(&self.control)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
            })
            .count()
    }
}

impl Drop for ApprovingBrowser {
    fn drop(&mut self) {
        for child in [&mut self.proxy, &mut self.chrome] {
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{}", child.id())])
                .status();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.chrome_profile);
    }
}

fn wait_for<T>(mut found: impl FnMut() -> Option<T>) -> Option<T> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        if let Some(value) = found() {
            return Some(value);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Spawns, retrying while the binary is still open for writing in a child
/// another test thread forked while copying it (`ETXTBSY`).
fn spawn_retrying(mut spawn: impl FnMut() -> std::io::Result<Child>) -> Child {
    for _ in 0..50 {
        match spawn() {
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(Duration::from_millis(20));
            }
            other => return other.unwrap(),
        }
    }
    spawn().unwrap()
}

/// A copy of the Python interpreter named `chrome` in `dir`.
fn python_named_chrome(dir: &Path) -> PathBuf {
    let binary = dir.join("chrome");
    if !binary.exists() {
        let output = Command::new("python3")
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .unwrap();
        let python = String::from_utf8(output.stdout).unwrap();
        std::fs::copy(python.trim(), &binary).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    binary
}

use std::os::unix::process::CommandExt;
