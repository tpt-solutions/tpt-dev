//! Runs the generated REPL page in a real browser, headless, in real time.
//!
//! Opt-in. Set `TPT_DEV_DOC_BROWSER` to a browser binary, or to `1` to use
//! Chrome or Edge from their default install paths. Unset, set to a missing
//! file, or with no browser found, the test prints why it skipped and passes.
//!
//! The test writes the REPL page for three examples (a pass, a failing assert,
//! and an infinite loop), serves the directory over HTTP on 127.0.0.1, starts the
//! browser with a remote-debugging port, and drives the page over the DevTools
//! protocol. It clicks each Run button and reads back the status it shows, and
//! any Content-Security-Policy violation the page reports. Only std is used: the
//! WebSocket client and the JSON string handling below are minimal on purpose.
#![cfg(feature = "std")]

use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tpt_dev_doc::extract;
use tpt_dev_doc::site::{REPL_FILE_NAME, write_repl_page};

/// Browsers tried, in order, when `TPT_DEV_DOC_BROWSER=1`.
const DEFAULT_BROWSERS: &[&str] = &[
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
];

/// Wall-clock limit for the whole browser run. Each example has a 5 s time
/// limit in the page, so a run that reaches this limit has hung.
const WALL_LIMIT: Duration = Duration::from_secs(90);

/// The three examples: one passes, one fails an assert, one never returns.
const EXAMPLES: &str = "Passing.\n\n```rust\nassert_eq!(2 + 2, 4);\n```\n\n\
    Failing.\n\n```rust\nassert_eq!(2 + 2, 5);\n```\n\n\
    Looping.\n\n```rust\nlet mut n: u64 = 0;\nloop {\n    n = core::hint::black_box(n.wrapping_add(1));\n}\n```\n";

/// Runs inside the REPL page. It first checks that the page's CSP blocks a
/// cross-origin fetch. Then it clicks each Run button in turn, waits for the
/// page to show a result, and returns plain text: the control count, the CSP
/// violations from the examples, then one line per button.
const DRIVER: &str = r#"(async () => {
  const violations = [];
  document.addEventListener("securitypolicyviolation", (event) => {
    violations.push(event.violatedDirective + " " + event.blockedURI);
  });
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  // Negative control: a cross-origin fetch must be blocked by the page's CSP.
  try {
    await fetch("http://example.invalid/csp-control");
  } catch (error) {}
  await sleep(200);
  const control = violations.length;
  violations.length = 0;
  const lines = [];
  for (const button of document.querySelectorAll("button[data-wasm]")) {
    const output = button.nextElementSibling;
    const start = performance.now();
    button.click();
    while (output.textContent === "running" && performance.now() - start < 30000) {
      await sleep(20);
    }
    lines.push(button.dataset.wasm + " | " + output.textContent + " | " + Math.round(performance.now() - start) + " ms");
  }
  return "control blocked: " + control + "\n" + "violations: " + (violations.length === 0 ? "0" : violations.join("; ")) + "\n" + lines.join("\n");
})()"#;

/// The browser to use, or the reason it was skipped.
fn browser_from_env() -> Result<PathBuf, String> {
    let Some(setting) = std::env::var_os("TPT_DEV_DOC_BROWSER") else {
        return Err("set TPT_DEV_DOC_BROWSER to a browser path, or to 1 for Chrome or Edge".into());
    };
    if setting == "1" {
        return DEFAULT_BROWSERS
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
            .ok_or_else(|| "no browser binary found at the default Chrome or Edge paths".into());
    }
    let explicit = PathBuf::from(&setting);
    if explicit.is_file() {
        return Ok(explicit);
    }
    Err(format!(
        "TPT_DEV_DOC_BROWSER is not a file: {}",
        explicit.display()
    ))
}

/// Serves `dir` over HTTP on 127.0.0.1, on a free port. The listener runs on
/// background threads until the test process exits.
fn serve(dir: PathBuf) -> io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let dir = dir.clone();
            thread::spawn(move || {
                let _ = handle_http(stream, &dir);
            });
        }
    });
    Ok(port)
}

fn handle_http(mut stream: TcpStream, dir: &Path) -> io::Result<()> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let text = String::from_utf8_lossy(&request);
    let target = text.split_whitespace().nth(1).unwrap_or("/");
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let name = match path.trim_start_matches('/') {
        "" => "index.html",
        other => other,
    };
    let safe = !name.contains("..") && !name.contains(['\\', ':']);
    let body = if safe {
        fs::read(dir.join(name)).ok()
    } else {
        None
    };
    match body {
        Some(bytes) => {
            let content_type = match Path::new(name).extension().and_then(|ext| ext.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("wasm") => "application/wasm",
                _ => "application/octet-stream",
            };
            write_response(&mut stream, "200 OK", content_type, &bytes)
        }
        None => write_response(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}

/// A browser process that is killed when dropped, so a failed test does not
/// leave one running.
struct Browser(Child);

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A WebSocket connection to the DevTools endpoint. Client frames are masked,
/// as the protocol requires. Only text messages are used.
struct Cdp {
    stream: TcpStream,
    buf: Vec<u8>,
    next_id: u64,
    deadline: Instant,
}

impl Cdp {
    fn connect(port: u16, path: &str, deadline: Instant) -> io::Result<Self> {
        let mut stream = TcpStream::connect(("127.0.0.1", port))?;
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(request.as_bytes())?;
        let mut cdp = Cdp {
            stream,
            buf: Vec::new(),
            next_id: 1,
            deadline,
        };
        let header_end = cdp.fill_until(|buf| buf.windows(4).position(|w| w == b"\r\n\r\n"))?;
        let head = String::from_utf8_lossy(&cdp.buf[..header_end]).into_owned();
        cdp.buf.drain(..header_end + 4);
        if !head.starts_with("HTTP/1.1 101") {
            return Err(io::Error::other(format!("websocket refused: {head}")));
        }
        Ok(cdp)
    }

    /// Reads from the socket until `found` returns a position in the buffer.
    fn fill_until(&mut self, found: impl Fn(&[u8]) -> Option<usize>) -> io::Result<usize> {
        loop {
            if let Some(at) = found(&self.buf) {
                return Ok(at);
            }
            self.read_more()?;
        }
    }

    fn read_more(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 65536];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(io::Error::other("connection closed")),
                Ok(read) => {
                    self.buf.extend_from_slice(&chunk[..read]);
                    return Ok(());
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    if Instant::now() > self.deadline {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "deadline passed"));
                    }
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn take(&mut self, n: usize) -> io::Result<Vec<u8>> {
        while self.buf.len() < n {
            self.read_more()?;
        }
        Ok(self.buf.drain(..n).collect())
    }

    /// Reads one complete text message, skipping pings and other opcodes.
    fn read_text(&mut self) -> io::Result<String> {
        let mut message = Vec::new();
        loop {
            let head = self.take(2)?;
            let fin = head[0] & 0x80 != 0;
            let opcode = head[0] & 0x0f;
            let len = match head[1] & 0x7f {
                126 => u16::from_be_bytes(self.take(2)?.try_into().unwrap_or([0; 2])) as usize,
                127 => u64::from_be_bytes(self.take(8)?.try_into().unwrap_or([0; 8])) as usize,
                short => short as usize,
            };
            let payload = self.take(len)?;
            match opcode {
                0x0 | 0x1 => message.extend_from_slice(&payload),
                0x8 => return Err(io::Error::other("websocket closed by browser")),
                _ => {}
            }
            if fin && (opcode == 0x0 || opcode == 0x1) {
                return Ok(String::from_utf8_lossy(&message).into_owned());
            }
        }
    }

    fn send_text(&mut self, text: &str) -> io::Result<()> {
        let payload = text.as_bytes();
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        let mut frame = vec![0x81];
        match payload.len() {
            len if len < 126 => frame.push(0x80 | len as u8),
            len if len <= u16::MAX as usize => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(len as u16).to_be_bytes());
            }
            len => {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(len as u64).to_be_bytes());
            }
        }
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&frame)
    }

    /// Sends a DevTools command and returns the reply for its id.
    fn call(&mut self, method: &str, params: &str, session: Option<&str>) -> io::Result<String> {
        let id = self.next_id;
        self.next_id += 1;
        let session = session
            .map(|s| format!(",\"sessionId\":{}", json_string(s)))
            .unwrap_or_default();
        let message = format!(
            "{{\"id\":{id},\"method\":{},\"params\":{params}{session}}}",
            json_string(method)
        );
        self.send_text(&message)?;
        loop {
            let reply = self.read_text()?;
            if reply.contains(&format!("\"id\":{id},")) {
                if reply.contains("\"error\":") {
                    return Err(io::Error::other(format!("{method} failed: {reply}")));
                }
                return Ok(reply);
            }
        }
    }
}

/// Encodes `text` as a JSON string literal.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Decodes the JSON string literal that starts at `text[start..]` (just after
/// its opening quote). Stops at the closing quote.
fn decode_json_string(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).unwrap_or(0xfffd);
                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                Some(other) => out.push(other),
                None => break,
            },
            other => out.push(other),
        }
    }
    out
}

/// The string value of `"key":"..."` in a DevTools reply.
fn string_field(reply: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = reply.find(&marker)? + marker.len();
    Some(decode_json_string(&reply[start..]))
}

/// Waits until `evaluate(expression)` returns the string `want`, or the deadline passes.
fn wait_for_value(cdp: &mut Cdp, session: &str, expression: &str, want: &str) -> io::Result<()> {
    loop {
        let reply = evaluate(cdp, session, expression)?;
        if reply == want {
            return Ok(());
        }
        if Instant::now() > cdp.deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "page did not load"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Evaluates `expression` in the page and returns its string result.
fn evaluate(cdp: &mut Cdp, session: &str, expression: &str) -> io::Result<String> {
    let params = format!(
        "{{\"expression\":{},\"awaitPromise\":true,\"returnByValue\":true}}",
        json_string(expression)
    );
    let reply = cdp.call("Runtime.evaluate", &params, Some(session))?;
    if reply.contains("\"exceptionDetails\"") {
        return Err(io::Error::other(format!("page script threw: {reply}")));
    }
    string_field(&reply, "value").ok_or_else(|| io::Error::other(format!("no value: {reply}")))
}

/// Starts the browser with a remote-debugging port, and returns it with the
/// DevTools WebSocket path. The port is read from `DevToolsActivePort` in the
/// profile directory.
fn start_browser(
    browser: &Path,
    profile: &Path,
    deadline: Instant,
) -> Result<(Browser, u16, String), Box<dyn Error>> {
    let child = Command::new(browser)
        .arg("--headless=new")
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let browser = Browser(child);
    let active = profile.join("DevToolsActivePort");
    loop {
        if let Ok(text) = fs::read_to_string(&active) {
            let mut lines = text.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                if let Ok(port) = port.trim().parse::<u16>() {
                    return Ok((browser, port, path.trim().to_string()));
                }
            }
        }
        if Instant::now() > deadline {
            return Err("browser did not open a DevTools port".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn repl_page_runs_in_a_headless_browser() -> Result<(), Box<dyn Error>> {
    let browser_path = match browser_from_env() {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("SKIPPED: browser test did not run: {reason}");
            return Ok(());
        }
    };
    let version = Command::new(&browser_path).arg("--version").output()?;
    eprintln!(
        "browser: {} ({})",
        browser_path.display(),
        String::from_utf8_lossy(&version.stdout).trim()
    );

    let dir = std::env::temp_dir().join(format!("tpt-dev-doc-browser-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let examples = extract(EXAMPLES);
    write_repl_page(&dir, REPL_FILE_NAME, "Browser check", &examples)?;
    let port = serve(dir.clone())?;
    let deadline = Instant::now() + WALL_LIMIT;

    let (_browser, devtools_port, ws_path) =
        start_browser(&browser_path, &dir.join("profile"), deadline)?;
    let mut cdp = Cdp::connect(devtools_port, &ws_path, deadline)?;
    let target = cdp.call(
        "Target.createTarget",
        &format!(
            "{{\"url\":{}}}",
            json_string(&format!("http://127.0.0.1:{port}/{REPL_FILE_NAME}"))
        ),
        None,
    )?;
    let target_id = string_field(&target, "targetId").ok_or("no targetId")?;
    let attached = cdp.call(
        "Target.attachToTarget",
        &format!(
            "{{\"targetId\":{},\"flatten\":true}}",
            json_string(&target_id)
        ),
        None,
    )?;
    let session = string_field(&attached, "sessionId").ok_or("no sessionId")?;
    let loaded = format!(
        "location.pathname.endsWith(\"/{REPL_FILE_NAME}\") && document.readyState === \"complete\" ? \"complete\" : \"waiting\""
    );
    wait_for_value(&mut cdp, &session, &loaded, "complete")?;

    let report = evaluate(&mut cdp, &session, DRIVER)?;
    let _ = fs::remove_dir_all(&dir);

    eprintln!("{report}");
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines.len(), 5, "unexpected report: {report}");
    assert_eq!(
        lines[0], "control blocked: 1",
        "the CSP did not block the control fetch"
    );
    assert_eq!(
        lines[1], "violations: 0",
        "the page's CSP blocked an example"
    );
    assert!(
        lines[2].contains("| passed |"),
        "passing example: {}",
        lines[2]
    );
    assert!(
        lines[3].contains("| failed: "),
        "failing example: {}",
        lines[3]
    );
    assert!(
        lines[4].contains("| stopped: time limit reached |"),
        "infinite loop: {}",
        lines[4]
    );
    Ok(())
}
