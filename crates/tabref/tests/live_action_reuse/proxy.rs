use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Proxy {
    pub endpoint: String,
    captures: Arc<AtomicUsize>,
}

impl Proxy {
    pub fn new(browser: &tabref_cdp::LaunchedBrowser) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let upstream = browser.port;
        let endpoint = browser
            .endpoint()
            .unwrap()
            .websocket_url
            .as_url()
            .replace(&format!(":{upstream}/"), &format!(":{port}/"));
        let captures = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&captures);
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let client = client.unwrap();
                let server = TcpStream::connect(("127.0.0.1", upstream)).unwrap();
                let _ = client.set_nodelay(true);
                let _ = server.set_nodelay(true);
                let mut incoming = client.try_clone().unwrap();
                let mut outgoing = server.try_clone().unwrap();
                let captures = Arc::clone(&counter);
                std::thread::spawn(move || {
                    let mut parser = Requests::default();
                    let mut bytes = [0; 65536];
                    while let Ok(n) = incoming.read(&mut bytes) {
                        if n == 0 {
                            break;
                        }
                        parser.feed(&bytes[..n], &captures);
                        if outgoing.write_all(&bytes[..n]).is_err() {
                            break;
                        }
                    }
                });
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut &server, &mut &client);
                });
            }
        });
        Self { endpoint, captures }
    }

    pub fn captures(&self) -> usize {
        self.captures.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Requests {
    bytes: Vec<u8>,
    upgraded: bool,
    message: Vec<u8>,
}

impl Requests {
    fn feed(&mut self, bytes: &[u8], captures: &AtomicUsize) {
        self.bytes.extend_from_slice(bytes);
        if !self.upgraded {
            let Some(end) = self.bytes.windows(4).position(|p| p == b"\r\n\r\n") else {
                return;
            };
            self.bytes.drain(..end + 4);
            self.upgraded = true;
        }
        loop {
            if self.bytes.len() < 2 {
                return;
            }
            let tag = self.bytes[1] & 127;
            let extra = match tag {
                126 => 2,
                127 => 8,
                _ => 0,
            };
            let masked = self.bytes[1] & 128 != 0;
            let offset = 2 + extra + if masked { 4 } else { 0 };
            if self.bytes.len() < offset {
                return;
            }
            let length = match extra {
                2 => u16::from_be_bytes(self.bytes[2..4].try_into().unwrap()) as usize,
                8 => u64::from_be_bytes(self.bytes[2..10].try_into().unwrap()) as usize,
                _ => tag as usize,
            };
            if self.bytes.len() < offset + length {
                return;
            }
            let opcode = self.bytes[0] & 15;
            if opcode == 1 || opcode == 0 {
                for i in 0..length {
                    let mask = if masked {
                        self.bytes[2 + extra + i % 4]
                    } else {
                        0
                    };
                    self.message.push(self.bytes[offset + i] ^ mask);
                }
                if self.bytes[0] & 128 != 0 {
                    let value: serde_json::Value = serde_json::from_slice(&self.message).unwrap();
                    if value["method"] == "Accessibility.getFullAXTree" {
                        captures.fetch_add(1, Ordering::SeqCst);
                    }
                    self.message.clear();
                }
            }
            self.bytes.drain(..offset + length);
        }
    }
}
