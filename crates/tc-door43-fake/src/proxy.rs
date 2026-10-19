//! A proxy for tests that refuses every request and remembers what it was asked. A client
//! that was told to use it as its proxy sends it a `CONNECT host:port` for an https server;
//! the test reads that line to see that the request went through the proxy and to which
//! server, and no server is ever reached. It listens on `127.0.0.1` on a free port. It also
//! answers a SOCKS4 handshake, which it refuses the same way.
//!
//! [`FakePacScript`] serves a PAC script the same way, for the Windows proxy lookup's tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// The longest request head the proxy reads.
const MAX_HEAD: usize = 16 << 10;

/// The first byte of a SOCKS4 request.
const SOCKS4: u8 = 4;

/// Whether `head` holds a whole request: an HTTP head, or a SOCKS4 request, which ends with
/// the NUL after its user name.
fn is_whole(head: &[u8]) -> bool {
    match head {
        [SOCKS4, rest @ ..] => rest.len() > 7 && rest[7..].contains(&0),
        _ => head.windows(4).any(|w| w == b"\r\n\r\n"),
    }
}

pub struct FakeProxy {
    address: std::net::SocketAddr,
    heads: Arc<Mutex<Vec<String>>>,
    _stop: oneshot::Sender<()>,
}

impl FakeProxy {
    pub async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let heads: Arc<Mutex<Vec<String>>> = Arc::default();
        let (stop, stopped) = oneshot::channel();
        let seen = heads.clone();
        tokio::spawn(async move {
            let serving = async {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let seen = seen.clone();
                    tokio::spawn(async move {
                        let mut head = Vec::new();
                        let mut buf = [0u8; 1024];
                        while !is_whole(&head) && head.len() < MAX_HEAD {
                            match stream.read(&mut buf).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) => head.extend_from_slice(&buf[..n]),
                            }
                        }
                        let (request, refusal): (String, &[u8]) = match head[..] {
                            [SOCKS4, 1, p1, p2, a, b, c, d, ..] => (
                                format!(
                                    "SOCKS4 CONNECT {a}.{b}.{c}.{d}:{}",
                                    u16::from_be_bytes([p1, p2])
                                ),
                                // Version 0, "request rejected or failed".
                                &[0, 0x5b, 0, 0, 0, 0, 0, 0],
                            ),
                            _ => (
                                String::from_utf8_lossy(&head)
                                    .lines()
                                    .next()
                                    .unwrap_or_default()
                                    .to_owned(),
                                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            ),
                        };
                        seen.lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(request);
                        let _ = stream.write_all(refusal).await;
                    });
                }
            };
            tokio::select! {
                () = serving => {}
                _ = stopped => {}
            }
        });
        Ok(Self {
            address,
            heads,
            _stop: stop,
        })
    }

    /// The proxy's address as a proxy URL.
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// The first line of each request it received, in order, such as
    /// `CONNECT door43.test:443 HTTP/1.1`, or `SOCKS4 CONNECT 192.0.2.1:443` for a SOCKS4
    /// handshake.
    pub fn requests(&self) -> Vec<String> {
        self.heads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A server for tests that hands out one PAC script, as a school's or an office's network
/// does, and counts how often it was fetched. It answers every request with the script, on
/// `127.0.0.1` on a free port.
pub struct FakePacScript {
    address: std::net::SocketAddr,
    path: String,
    fetched: Arc<AtomicUsize>,
    _stop: oneshot::Sender<()>,
}

/// Makes each script's path one no earlier script of this process had.
static SCRIPTS: AtomicUsize = AtomicUsize::new(0);

impl FakePacScript {
    pub async fn start(script: &str) -> std::io::Result<Self> {
        Self::serve(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{script}",
            script.len()
        ))
        .await
    }

    /// A server that has no script: it answers 404 Not Found.
    pub async fn missing() -> std::io::Result<Self> {
        Self::serve(
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        )
        .await
    }

    async fn serve(reply: String) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let fetched = Arc::new(AtomicUsize::new(0));
        let (stop, stopped) = oneshot::channel();
        let count = fetched.clone();
        tokio::spawn(async move {
            let serving = async {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let count = count.clone();
                    let reply = reply.clone();
                    tokio::spawn(async move {
                        let mut head = Vec::new();
                        let mut buf = [0u8; 1024];
                        while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < MAX_HEAD {
                            match stream.read(&mut buf).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => head.extend_from_slice(&buf[..n]),
                            }
                        }
                        count.fetch_add(1, Ordering::SeqCst);
                        let _ = stream.write_all(reply.as_bytes()).await;
                    });
                }
            };
            tokio::select! {
                () = serving => {}
                _ = stopped => {}
            }
        });
        // WinHTTP keeps a script's answers for its URL, for every program on the computer, so
        // each server gets a path no earlier one had, in this run or an earlier one.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = format!(
            "/proxy-{}-{stamp}-{}.pac",
            std::process::id(),
            SCRIPTS.fetch_add(1, Ordering::SeqCst)
        );
        Ok(Self {
            address,
            path,
            fetched,
            _stop: stop,
        })
    }

    /// The script's address.
    pub fn url(&self) -> String {
        format!("http://{}{}", self.address, self.path)
    }

    /// How many times the script was fetched.
    pub fn fetched(&self) -> usize {
        self.fetched.load(Ordering::SeqCst)
    }
}
