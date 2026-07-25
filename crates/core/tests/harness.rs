//! Test harness that manages a `wrangler dev` subprocess.
//!
//! Each test gets a fresh server with ephemeral state.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Once;
use std::time::Duration;

/// A running `wrangler dev` instance with ephemeral state.
pub struct DevServer {
    child: Option<Child>,
    pub port: u16,
    /// Temp directory for DO storage — cleaned up on drop.
    _persist_dir: tempfile::TempDir,
}

impl DevServer {
    /// The WebSocket base URL, e.g. `ws://127.0.0.1:PORT`
    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    /// The HTTP base URL, e.g. `http://127.0.0.1:PORT`
    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Spawn `pnpm wrangler dev` on a random port and wait until it's ready.
    ///
    /// The server directory is resolved relative to the workspace root.
    pub async fn start() -> Self {
        static TRACING: Once = Once::new();
        TRACING.call_once(|| {
            let _ = tracing_subscriber::fmt::try_init();
        });

        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let server_dir = workspace_root.join("apps/server");

        // Pick a random port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Ephemeral storage directory — each test gets a clean slate
        let persist_dir = tempfile::tempdir().expect("failed to create temp dir");

        let mut child = Command::new("pnpm")
            .args([
                "wrangler",
                "dev",
                "--port",
                &port.to_string(),
                "--ip",
                "127.0.0.1",
                "--inspector-port",
                "0",
                "--persist-to",
                persist_dir.path().to_str().unwrap(),
            ])
            .current_dir(&server_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .env("NO_COLOR", "1")
            .spawn()
            .expect("failed to spawn wrangler dev — is pnpm available?");

        // Watch both stdout and stderr for the "Ready on" line.
        let (tx, rx) = std::sync::mpsc::channel();

        let stderr = child.stderr.take().unwrap();
        let tx_err = tx.clone();
        let port_err = port;
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        eprintln!("[wrangler:{port_err}:err] {line}");
                        if line.contains("Ready on") {
                            let _ = tx_err.send(());
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let stdout = child.stdout.take().unwrap();
        let port_out = port;
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        eprintln!("[wrangler:{port_out}:out] {line}");
                        if line.contains("Ready on") {
                            let _ = tx.send(());
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for the "Ready" signal or timeout
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(()) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if std::time::Instant::now() > deadline {
                        child.kill().ok();
                        panic!("wrangler dev did not become ready within 30s on port {port}");
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    child.kill().ok();
                    panic!("wrangler dev exited before becoming ready");
                }
            }
        }

        // Additional warmup: poll with HTTP until /festivals responds
        let http_url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        for _ in 0..20 {
            match client.get(format!("{http_url}/festivals")).send().await {
                Ok(resp) if resp.status().is_success() => break,
                _ => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }

        DevServer {
            child: Some(child),
            port,
            _persist_dir: persist_dir,
        }
    }
}

impl Drop for DevServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Send SIGTERM on unix, kill on windows
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
                }
                // Give it a moment to shut down gracefully
                std::thread::sleep(Duration::from_millis(500));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
