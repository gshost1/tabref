#![cfg(target_os = "linux")]

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::MetadataExt;
use std::thread;
use std::time::Duration;

use tabref_core::Deadline;

use crate::discovery::held_port::HeldPort;
use crate::discovery::json_version::endpoint_from_held;

const HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

/// A `/proc/net/tcp` that lists one listener on `port` with `inode`.
fn fake_proc(name: &str, port: u16, inode: u64) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("tabref-httpget-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("net")).unwrap();
    let uid = std::fs::metadata(&root).unwrap().uid();
    std::fs::write(
        root.join("net/tcp"),
        format!(
            "{HEADER}   0: 0100007F:{port:04X} 00000000:0000 0A 00000000:00000000 00:00000000 00000000  {uid}        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
        ),
    )
    .unwrap();
    root
}

/// A listener that counts the bytes its one connection is sent.
fn counting_server() -> (u16, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let mut request = [0_u8; 1024];
        stream.read(&mut request).unwrap_or(0)
    });
    (port, handle)
}

#[test]
fn the_probe_of_a_port_that_changed_hands_sends_nothing() {
    let (port, server) = counting_server();
    let root = fake_proc("changed", port, 2);
    let held = HeldPort {
        port,
        inodes: BTreeSet::from([1]),
        proc_root: root.clone(),
    };

    let error = endpoint_from_held(&held, Deadline::after(1_000).unwrap()).unwrap_err();

    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(error.kind(), Some("cdp_endpoint_changed"));
    assert_eq!(server.join().unwrap(), 0, "the probe was sent");
}

#[test]
fn the_probe_of_a_port_still_held_reads_its_endpoint() {
    let (port, endpoint) = probe_held_port("held", |port| {
        format!("{{\"webSocketDebuggerUrl\":\"ws://127.0.0.1:{port}/devtools/browser/x\"}}")
    });
    assert_eq!(endpoint.unwrap().port(), port);
}

#[test]
fn the_probe_of_a_port_still_held_suggests_a_websocket_for_an_invalid_reply() {
    let (port, endpoint) = probe_held_port("invalid", |_| "{}".to_owned());
    let error = endpoint.unwrap_err();
    assert_eq!(error.kind(), Some("cdp_endpoint_invalid"));
    assert!(
        error
            .suggestion
            .unwrap()
            .contains(&format!("--cdp ws://127.0.0.1:{port}"))
    );
}

fn probe_held_port(
    name: &str,
    body: impl FnOnce(u16) -> String,
) -> (u16, Result<crate::BrowserEndpoint, tabref_core::AppError>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let json = body(port);
    let answer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{json}",
            json.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let root = fake_proc(name, port, 1);
    let held = HeldPort {
        port,
        inodes: BTreeSet::from([1]),
        proc_root: root.clone(),
    };

    let endpoint = endpoint_from_held(&held, Deadline::after(2_000).unwrap());

    let _ = std::fs::remove_dir_all(&root);
    answer.join().unwrap();
    (port, endpoint)
}
