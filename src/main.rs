use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn timestamp() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok();
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn date_stamp() -> String {
    let output = Command::new("date").args(["+%Y-%m-%d"]).output().ok();
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ── LSP Client ──────────────────────────────────────────────────────────────

struct LspClient {
    child: std::process::Child,
    rx: mpsc::Receiver<Value>,
    writer: std::process::ChildStdin,
    id: i64,
    logs: Vec<String>,
}

/// Info returned after waiting for diagnostics.
struct DiagnosticsInfo {
    count: usize,
    elapsed_ms: f64,
    message: Value,
}

/// Background reader thread: reads LSP messages from stdout and sends them
/// through a channel. This avoids blocking the main thread on read_line().
fn reader_thread(stdout: std::process::ChildStdout, tx: mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        // Read headers
        let mut content_length: usize = 0;
        let mut in_header = false;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return, // EOF
                Ok(_) => {}
                Err(_) => return,
            }
            let t = line.trim();
            if t.is_empty() {
                if in_header {
                    break;
                }
                continue;
            }
            if let Some(v) = t.strip_prefix("Content-Length:") {
                if let Ok(n) = v.trim().parse::<usize>() {
                    content_length = n;
                    in_header = true;
                    continue;
                }
            }
            if t.starts_with("Content-Type:") {
                in_header = true;
                continue;
            }
            // Skip garbage lines (tracing output, ANSI codes, etc.)
        }
        if content_length == 0 {
            continue;
        }
        // Read body
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
            if tx.send(msg).is_err() {
                return; // receiver dropped
            }
        }
    }
}

impl LspClient {
    fn spawn(cmd: &str, args: &[&str], cwd: &Path) -> Result<Self, String> {
        // Resolve relative command paths to absolute before changing CWD
        let abs_cmd = if cmd.starts_with("..") || cmd.starts_with("./") {
            std::fs::canonicalize(cmd)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| cmd.to_string())
        } else {
            cmd.to_string()
        };
        let mut child = Command::new(&abs_cmd)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("{}: {}", cmd, e))?;
        let writer = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || reader_thread(stdout, tx));

        Ok(Self {
            child,
            rx,
            writer,
            id: 1,
            logs: Vec::new(),
        })
    }

    fn send(&mut self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params});
        self.id += 1;
        let body = serde_json::to_string(&msg).unwrap();
        write!(
            self.writer,
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())
    }

    fn notif(&mut self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","method":method,"params":params});
        let body = serde_json::to_string(&msg).unwrap();
        write!(
            self.writer,
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())
    }

    /// Receive the next message with a real timeout.
    fn recv(&mut self, timeout: Duration) -> Result<Value, String> {
        self.rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => "timeout".to_string(),
            mpsc::RecvTimeoutError::Disconnected => "EOF".to_string(),
        })
    }

    fn read_response(&mut self, timeout: Duration) -> Result<Value, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("timeout".into());
            }
            let msg = self.recv(remaining)?;
            // Capture window/logMessage notifications
            if msg.get("method").and_then(|m| m.as_str()) == Some("window/logMessage") {
                if let Some(text) = msg
                    .get("params")
                    .and_then(|p| p.get("message"))
                    .and_then(|m| m.as_str())
                {
                    self.logs.push(text.to_string());
                }
            }
            if msg.get("id").is_some() {
                return Ok(msg);
            }
        }
    }

    /// Drain messages until we see publishDiagnostics with non-empty diagnostics.
    /// Returns the count and elapsed time. If only empty diagnostics arrive before
    /// timeout, returns those. This is the "time to first valid diagnostics" metric.
    fn wait_for_valid_diagnostics(&mut self, timeout: Duration) -> Result<DiagnosticsInfo, String> {
        let start = Instant::now();
        let deadline = start + timeout;
        let mut last_count = 0usize;
        let mut last_elapsed = 0.0f64;
        let mut last_msg = json!(null);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return if last_count > 0 || last_elapsed > 0.0 {
                    Ok(DiagnosticsInfo {
                        count: last_count,
                        elapsed_ms: last_elapsed,
                        message: last_msg,
                    })
                } else {
                    Err("timeout waiting for diagnostics".into())
                };
            }
            let msg = self.recv(remaining)?;
            // Capture window/logMessage notifications
            if msg.get("method").and_then(|m| m.as_str()) == Some("window/logMessage") {
                if let Some(text) = msg
                    .get("params")
                    .and_then(|p| p.get("message"))
                    .and_then(|m| m.as_str())
                {
                    self.logs.push(text.to_string());
                }
            }
            if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics")
            {
                let count = msg
                    .get("params")
                    .and_then(|p| p.get("diagnostics"))
                    .and_then(|d| d.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                last_count = count;
                last_elapsed = elapsed;
                last_msg = msg;
                if count > 0 {
                    return Ok(DiagnosticsInfo {
                        count,
                        elapsed_ms: elapsed,
                        message: last_msg,
                    });
                }
            }
        }
    }

    fn initialize(&mut self, root: &str) -> Result<(), String> {
        self.send(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root,
                "capabilities": {
                    "textDocument": {
                        "publishDiagnostics": {},
                        "definition": { "dynamicRegistration": false, "linkSupport": true },
                        "declaration": { "dynamicRegistration": false, "linkSupport": true },
                        "hover": { "dynamicRegistration": false, "contentFormat": ["plaintext", "markdown"] },
                        "completion": {
                            "dynamicRegistration": false,
                            "completionItem": { "snippetSupport": false }
                        },
                        "documentSymbol": { "dynamicRegistration": false },
                        "documentLink": { "dynamicRegistration": false },
                        "references": { "dynamicRegistration": false },
                        "rename": { "dynamicRegistration": false },
                        "signatureHelp": { "dynamicRegistration": false },
                        "codeAction": { "dynamicRegistration": false },
                    }
                },
            }),
        )?;
        self.read_response(Duration::from_secs(10))?;
        self.notif("initialized", json!({}))
    }

    fn open_file(&mut self, path: &Path) -> Result<(), String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        self.notif(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri(path),
                    "languageId": "solidity",
                    "version": 1,
                    "text": content,
                }
            }),
        )
    }

    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn uri(p: &Path) -> String {
    format!(
        "file://{}",
        std::fs::canonicalize(p).unwrap_or(p.into()).display()
    )
}

fn available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve symlinks to find the real binary path.
fn resolve_binary(cmd: &str) -> Option<String> {
    let which_out = Command::new("which")
        .arg(cmd)
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    let bin_path = String::from_utf8_lossy(&which_out.stdout)
        .trim()
        .to_string();
    if bin_path.is_empty() {
        return None;
    }
    // Try readlink -f (Linux) or realpath via canonicalize
    std::fs::canonicalize(&bin_path)
        .map(|p| p.to_string_lossy().to_string())
        .ok()
        .or(Some(bin_path))
}

/// Detect server version. Strategy:
/// 1. For solc: parse `solc --version` output for the Version: line
/// 2. For others: try `<cmd> --version` and take the first non-empty line
/// 3. Fallback: resolve binary symlinks, walk up to find package.json
fn detect_version(cmd: &str) -> String {
    // Special handling for solc — its --version prints a banner
    if cmd == "solc" {
        if let Ok(output) = Command::new("solc")
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("Version:") {
                    return line.trim_start_matches("Version:").trim().to_string();
                }
            }
        }
    }

    // Try --version (works for our LSP and some others)
    if let Ok(output) = Command::new(cmd)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout.lines().next().unwrap_or("").trim().to_string();
            if !line.is_empty() {
                return line;
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            let line = stderr.lines().next().unwrap_or("").trim().to_string();
            if !line.is_empty() {
                return line;
            }
        }
    }

    // Fallback: resolve binary path (following symlinks) and find package.json
    if let Some(real_path) = resolve_binary(cmd) {
        let mut dir = Path::new(&real_path).to_path_buf();
        for _ in 0..10 {
            dir = match dir.parent() {
                Some(p) => p.to_path_buf(),
                None => break,
            };
            let pkg = dir.join("package.json");
            if pkg.exists() {
                if let Ok(content) = std::fs::read_to_string(&pkg) {
                    if let Ok(v) = serde_json::from_str::<Value>(&content) {
                        if let Some(ver) = v.get("version").and_then(|v| v.as_str()) {
                            let name = v.get("name").and_then(|n| n.as_str()).unwrap_or(cmd);
                            return format!("{} {}", name, ver);
                        }
                    }
                }
            }
        }
    }

    // Fallback for volta/npm: try `npm info <cmd> version`
    if let Ok(output) = Command::new("npm")
        .args(["info", cmd, "version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        if output.status.success() {
            let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ver.is_empty() {
                return format!("{} {}", cmd, ver);
            }
        }
    }

    "unknown".to_string()
}

type Type<'a> = &'a mut Vec<f64>;

fn stats(samples: Type) -> (f64, f64, f64) {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = samples.len();
    (
        samples[n / 2],
        samples[((n as f64) * 0.95) as usize],
        samples.iter().sum::<f64>() / n as f64,
    )
}

/// Check if an LSP response is valid (has a non-null, non-error result).
fn is_valid_response(resp: &Value) -> bool {
    if resp.get("error").is_some() {
        return false;
    }
    match resp.get("result") {
        None => false,
        Some(r) => {
            if r.is_null() {
                return false;
            }
            if let Some(arr) = r.as_array() {
                return !arr.is_empty();
            }
            true
        }
    }
}

/// Format a response snippet for display.
fn response_summary(resp: &Value) -> String {
    if let Some(err) = resp.get("error") {
        return format!(
            "error: {}",
            err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        );
    }
    // Handle JSON-RPC responses (have "result") or notifications (have "params")
    let payload = resp.get("result").or_else(|| resp.get("params"));
    if let Some(r) = payload {
        if r.is_null() {
            return "null".into();
        }
        let s = serde_json::to_string_pretty(r).unwrap_or_default();
        return if s.len() > 80 {
            format!("{}...", &s[..80])
        } else {
            s
        };
    }
    "no result".into()
}

// ── Servers ─────────────────────────────────────────────────────────────────

struct Server {
    label: &'static str,
    cmd: &'static str,
    args: &'static [&'static str],
}

const SERVERS: &[Server] = &[
    Server {
        label: "Our LSP",
        cmd: "solidity-language-server",
        args: &[],
    },
    Server {
        label: "solc",
        cmd: "solc",
        args: &["--lsp"],
    },
    Server {
        label: "nomicfoundation",
        cmd: "nomicfoundation-solidity-language-server",
        args: &["--stdio"],
    },
    Server {
        label: "juanfranblanco",
        cmd: "vscode-solidity-server",
        args: &["--stdio"],
    },
    Server {
        label: "qiuxiang",
        cmd: "solidity-ls",
        args: &["--stdio"],
    },
];

// ── Bench result per server ─────────────────────────────────────────────────

enum BenchResult {
    /// Valid result with samples and first response
    Ok {
        samples: Vec<f64>,
        first_response: Value,
        diag_info: Option<DiagnosticsInfo>,
    },
    /// Bench ran but response was null/error — invalidated
    Invalid {
        first_response: Value,
        diag_info: Option<DiagnosticsInfo>,
    },
    /// Bench failed to run at all
    Fail(String),
}

struct BenchRow {
    label: String,
    p50: f64,
    p95: f64,
    mean: f64,
    kind: u8, // 0=ok, 1=invalid, 2=fail
    fail_msg: String,
    summary: String,
}

impl BenchRow {
    fn to_json(&self) -> Value {
        match self.kind {
            0 => json!({
                "server": self.label,
                "status": "ok",
                "p50_ms": (self.p50 * 10.0).round() / 10.0,
                "p95_ms": (self.p95 * 10.0).round() / 10.0,
                "mean_ms": (self.mean * 10.0).round() / 10.0,
                "response": self.summary,
            }),
            1 => json!({
                "server": self.label,
                "status": "invalid",
                "response": self.summary,
            }),
            _ => json!({
                "server": self.label,
                "status": "fail",
                "error": self.fail_msg,
            }),
        }
    }
}

fn run_bench<F>(
    name: &str,
    header: &[String],
    servers: &[&Server],
    root: &str,
    cwd: &Path,
    f: F,
) -> Vec<BenchRow>
where
    F: Fn(&Server, &str, &Path) -> BenchResult,
{
    let mut lines = header.to_vec();
    lines.push("| Server | p50 | p95 | mean |".to_string());
    lines.push("|--------|-----|-----|------|".to_string());

    struct Row {
        label: String,
        p50: f64,
        p95: f64,
        mean: f64,
        summary: String,
        diag_suffix: String,
        kind: u8,
        fail_msg: String,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut results = Vec::new();
    for srv in servers {
        eprint!("  {} ... ", srv.label);
        match f(srv, root, cwd) {
            BenchResult::Ok {
                mut samples,
                first_response,
                diag_info,
            } => {
                let (p50, p95, mean) = stats(&mut samples);
                let summary = response_summary(&first_response);
                let diag_suffix = diag_info
                    .map(|di| format!("  [diag: {} in {:.0}ms]", di.count, di.elapsed_ms))
                    .unwrap_or_default();
                eprintln!("done");
                results.push((srv.label.to_string(), p50, p95, mean, summary.clone()));
                rows.push(Row {
                    label: srv.label.to_string(),
                    p50,
                    p95,
                    mean,
                    summary,
                    diag_suffix,
                    kind: 0,
                    fail_msg: String::new(),
                });
            }
            BenchResult::Invalid {
                first_response,
                diag_info,
            } => {
                let summary = response_summary(&first_response);
                let diag_suffix = diag_info
                    .map(|di| format!("  [diag: {} in {:.0}ms]", di.count, di.elapsed_ms))
                    .unwrap_or_default();
                eprintln!("invalid");
                results.push((srv.label.to_string(), 0.0, 0.0, 0.0, summary.clone()));
                rows.push(Row {
                    label: srv.label.to_string(),
                    p50: 0.0,
                    p95: 0.0,
                    mean: 0.0,
                    summary,
                    diag_suffix,
                    kind: 1,
                    fail_msg: String::new(),
                });
            }
            BenchResult::Fail(e) => {
                eprintln!("fail");
                results.push((srv.label.to_string(), 0.0, 0.0, 0.0, "fail".to_string()));
                rows.push(Row {
                    label: srv.label.to_string(),
                    p50: 0.0,
                    p95: 0.0,
                    mean: 0.0,
                    summary: String::new(),
                    diag_suffix: String::new(),
                    kind: 2,
                    fail_msg: e,
                });
            }
        }
    }

    // Rank valid rows (kind=0 with actual results) by mean for medals
    let has_valid_result = |r: &Row| -> bool {
        r.kind == 0 && r.summary != "null" && r.summary != "no result" && !r.summary.is_empty()
    };
    let mut ranked: Vec<(usize, f64)> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| has_valid_result(r))
        .map(|(i, r)| (i, r.mean))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let medals = ["🥇", "🥈", "🥉"];
    let mut row_medal: Vec<&str> = vec![""; rows.len()];
    for (place, (idx, _)) in ranked.iter().enumerate() {
        if place < medals.len() {
            row_medal[*idx] = medals[place];
        }
    }

    for (i, row) in rows.iter().enumerate() {
        match row.kind {
            0 => {
                let medal = if row_medal[i].is_empty() {
                    "".to_string()
                } else {
                    format!(" {}", row_medal[i])
                };
                lines.push(format!(
                    "| {}{} | {:.1} | {:.1} | {:.1} |",
                    row.label, medal, row.p50, row.p95, row.mean
                ));
            }
            1 => {
                lines.push(format!("| {} | - | - | - |", row.label));
            }
            _ => {
                lines.push(format!("| {} | FAIL | FAIL | FAIL |", row.label));
            }
        }
    }

    // Add response details per server in code blocks
    lines.push("".to_string());
    lines.push("### Responses".to_string());
    lines.push("".to_string());
    for row in &rows {
        match row.kind {
            0 => {
                lines.push(format!("**{}**{}", row.label, row.diag_suffix));
                lines.push("```json".to_string());
                lines.push(row.summary.clone());
                lines.push("```".to_string());
                lines.push("".to_string());
            }
            1 => {
                lines.push(format!("**{}**{}", row.label, row.diag_suffix));
                lines.push("```".to_string());
                lines.push(row.summary.clone());
                lines.push("```".to_string());
                lines.push("".to_string());
            }
            _ => {
                lines.push(format!("**{}**", row.label));
                lines.push("```".to_string());
                lines.push(format!("FAIL: {}", row.fail_msg));
                lines.push("```".to_string());
                lines.push("".to_string());
            }
        }
    }

    // Add summary
    lines.push("".to_string());
    let summary = generate_summary(name, &results);
    lines.push(summary);

    let out = lines.join("\n") + "\n";
    let path = format!("results/{}.md", name);
    let _ = std::fs::create_dir_all("results");
    std::fs::write(&path, &out).unwrap();
    println!("{}", out);
    eprintln!("  -> {}", path);

    // Return rows for summary generation
    rows.iter()
        .map(|r| BenchRow {
            label: r.label.clone(),
            p50: r.p50,
            p95: r.p95,
            mean: r.mean,
            kind: r.kind,
            fail_msg: r.fail_msg.clone(),
            summary: r.summary.clone(),
        })
        .collect()
}

fn generate_summary(name: &str, results: &[(String, f64, f64, f64, String)]) -> String {
    // Build a ranked list: servers with valid (>0) mean first, sorted by mean,
    // then failed/timeout servers appended with their summary text.
    let mut valid: Vec<&(String, f64, f64, f64, String)> =
        results.iter().filter(|(_, _, _, m, _)| *m > 0.0).collect();
    valid.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap());

    let failed: Vec<&(String, f64, f64, f64, String)> =
        results.iter().filter(|(_, _, _, m, _)| *m == 0.0).collect();

    match name {
        "spawn" | "diagnostics" => {
            if valid.is_empty() {
                return "No valid results.".to_string();
            }
            let mut parts: Vec<String> = Vec::new();
            let qualifier = if name == "spawn" {
                "startup"
            } else {
                "diagnostics"
            };
            for (i, r) in valid.iter().enumerate() {
                if i == 0 {
                    parts.push(format!("{} fastest {} ({:.0}ms)", r.0, qualifier, r.3));
                } else {
                    parts.push(format!("{} {:.0}ms", r.0, r.3));
                }
            }
            for r in &failed {
                parts.push(format!("{} {}", r.0, r.4));
            }
            format!("{}.", parts.join(", "))
        }
        _ => {
            // Feature benchmarks (definition, declaration, hover, references, documentSymbol)
            let mut parts: Vec<String> = Vec::new();
            for r in &valid {
                if r.3 > 0.0 {
                    parts.push(format!("{} {:.1}ms", r.0, r.3));
                }
            }
            for r in &failed {
                let desc = if r.4 == "fail" || r.4.contains("timeout") {
                    format!("{} timeout", r.0)
                } else if r.4.contains("unsupported") || r.4.contains("Unknown method") {
                    format!("{} unsupported", r.0)
                } else if r.4.is_empty() || r.4 == "[]" {
                    format!("{} no result", r.0)
                } else {
                    format!("{} {}", r.0, r.4)
                };
                parts.push(desc);
            }
            if parts.is_empty() {
                "No valid results.".to_string()
            } else {
                format!("{}.", parts.join(", "))
            }
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────────────

const ALL_BENCHMARKS: &[&str] = &[
    "spawn",
    "diagnostics",
    "definition",
    "declaration",
    "hover",
    "references",
    "documentSymbol",
    "documentLink",
];

fn print_usage() {
    eprintln!("Usage: bench [OPTIONS] <COMMAND>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  all            — run all benchmarks");
    eprintln!("  spawn          — spawn + initialize handshake");
    eprintln!("  diagnostics    — open Pool.sol, time to first diagnostic");
    eprintln!("  definition     — go-to-definition on TickMath in Pool.sol");
    eprintln!("  declaration    — go-to-declaration on TickMath in Pool.sol");
    eprintln!("  hover          — hover on TickMath in Pool.sol");
    eprintln!("  references     — find references on TickMath in Pool.sol");
    eprintln!("  documentSymbol — get document symbols for Pool.sol");
    eprintln!("  documentLink   — get document links for Pool.sol");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -n, --iterations <N>  Number of measured iterations (default: 10)");
    eprintln!("  -w, --warmup <N>      Number of warmup iterations (default: 2)");
    eprintln!("  -t, --timeout <SECS>  Timeout per request in seconds (default: 30)");
    eprintln!("  -h, --help            Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  bench all                  Run all benchmarks (10 iterations, 2 warmup)");
    eprintln!("  bench all -n 1 -w 0        Run all benchmarks once, no warmup");
    eprintln!("  bench diagnostics -n 5     Run diagnostics with 5 iterations");
    eprintln!("  bench all -t 10            Run all benchmarks with 10s timeout");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse flags
    let mut n: usize = 10;
    let mut w: usize = 2;
    let mut timeout_secs: u64 = 30;
    let mut commands: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-n" | "--iterations" => {
                i += 1;
                n = args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: -n requires a number");
                    std::process::exit(1);
                });
            }
            "-w" | "--warmup" => {
                i += 1;
                w = args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: -w requires a number");
                    std::process::exit(1);
                });
            }
            "-t" | "--timeout" => {
                i += 1;
                timeout_secs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: -t requires a number (seconds)");
                    std::process::exit(1);
                });
            }
            other => {
                commands.push(other.to_string());
            }
        }
        i += 1;
    }

    let timeout = Duration::from_secs(timeout_secs);

    if commands.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    // Expand "all" into every benchmark
    let benchmarks: Vec<&str> = if commands.iter().any(|c| c == "all") {
        ALL_BENCHMARKS.to_vec()
    } else {
        commands.iter().map(|s| s.as_str()).collect()
    };

    // Validate commands
    for b in &benchmarks {
        if !ALL_BENCHMARKS.contains(b) {
            eprintln!("Error: unknown benchmark '{}'", b);
            eprintln!();
            print_usage();
            std::process::exit(1);
        }
    }

    let v4 = ["bench/v4-core", "v4-core"]
        .iter()
        .find(|p| Path::new(p).join("src/PoolManager.sol").exists())
        .unwrap_or_else(|| {
            eprintln!("v4-core not found");
            std::process::exit(1);
        });
    let root = uri(Path::new(v4));

    let avail: Vec<&Server> = SERVERS
        .iter()
        .filter(|s| {
            let ok = available(s.cmd);
            if !ok {
                eprintln!("  SKIP {} — not found", s.label);
            }
            ok
        })
        .collect();

    // Detect versions for available servers
    eprintln!("Detecting server versions...");
    let versions: Vec<(&str, String)> = avail
        .iter()
        .map(|s| {
            let ver = detect_version(s.cmd);
            eprintln!("  {} = {}", s.label, ver);
            (s.label, ver)
        })
        .collect();

    let mut all_results: Vec<(&str, Vec<BenchRow>)> = Vec::new();

    // ── spawn ───────────────────────────────────────────────────────────────

    if benchmarks.contains(&"spawn") {
        let rows = run_bench(
            "spawn",
            &[
                format!(
                    "## 1. SPAWN + INITIALIZE (ms) — {} iterations, {} warmup",
                    n, w
                ),
                String::new(),
                "Measures: spawn process -> initialize request -> response -> initialized notification".into(),
                "No files opened.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut samples = Vec::new();
                for i in 0..(w + n) {
                    let start = Instant::now();
                    let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                        Ok(c) => c,
                        Err(e) => return BenchResult::Fail(e),
                    };
                    if let Err(e) = c.initialize(root) {
                        return BenchResult::Fail(e);
                    }
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    if i >= w {
                        samples.push(ms);
                    }
                    c.kill();
                }
                BenchResult::Ok {
                    samples,
                    first_response: json!({"result": "ok"}),
                    diag_info: None,
                }
            },
        );
        all_results.push(("Spawn + Init", rows));
    }

    // ── diagnostics ─────────────────────────────────────────────────────────

    if benchmarks.contains(&"diagnostics") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);

        let rows = run_bench(
            "diagnostics",
            &[
                format!(
                    "## 2. OPEN FILE -> FIRST DIAGNOSTIC (ms) — {} iterations, {} warmup",
                    n, w
                ),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                "Measures: didOpen notification -> first publishDiagnostics response".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut samples = Vec::new();
                let mut first: Option<DiagnosticsInfo> = None;
                for i in 0..(w + n) {
                    let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                        Ok(c) => c,
                        Err(e) => return BenchResult::Fail(e),
                    };
                    if let Err(e) = c.initialize(root) {
                        return BenchResult::Fail(e);
                    }
                    let start = Instant::now();
                    if let Err(e) = c.open_file(&pool_sol) {
                        return BenchResult::Fail(e);
                    }
                    match c.wait_for_valid_diagnostics(timeout) {
                        Ok(diag_info) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                samples.push(ms);
                            }
                            if first.is_none() {
                                first = Some(diag_info);
                            }
                        }
                        Err(e) => return BenchResult::Fail(e),
                    }
                    c.kill();
                }
                let diag_info = first.unwrap_or(DiagnosticsInfo {
                    count: 0,
                    elapsed_ms: 0.0,
                    message: json!(null),
                });
                BenchResult::Ok {
                    samples,
                    first_response: diag_info.message.clone(),
                    diag_info: None,
                }
            },
        );
        all_results.push(("Diagnostics", rows));
    }

    // ── definition ──────────────────────────────────────────────────────────

    if benchmarks.contains(&"definition") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let target_line: u32 = 102;
        let target_col: u32 = 15;

        let rows = run_bench(
            "definition",
            &[
                format!(
                    "## 3. GO TO DEFINITION (ms) — {} iterations, {} warmup",
                    n, w
                ),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                format!(
                    "Target: `TickMath` at line {}:{}",
                    target_line + 1,
                    target_col
                ),
                "Measures: textDocument/definition request -> response".into(),
                "Waits for valid publishDiagnostics before sending requests.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                    Ok(c) => c,
                    Err(e) => return BenchResult::Fail(e),
                };
                if let Err(e) = c.initialize(root) {
                    return BenchResult::Fail(e);
                }
                if let Err(e) = c.open_file(&pool_sol) {
                    return BenchResult::Fail(e);
                }

                // Wait for valid diagnostics (build complete)
                let diag_info = match c.wait_for_valid_diagnostics(timeout) {
                    Ok(info) => info,
                    Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
                };
                eprintln!(
                    "diagnostics: {} items in {:.0}ms ... ",
                    diag_info.count, diag_info.elapsed_ms
                );
                eprint!("    ");

                let file_uri = uri(&pool_sol);
                let mut samples = Vec::new();
                let mut first: Option<Value> = None;
                for i in 0..(w + n) {
                    let start = Instant::now();
                    if let Err(e) = c.send(
                        "textDocument/definition",
                        json!({
                            "textDocument": { "uri": file_uri },
                            "position": { "line": target_line, "character": target_col },
                        }),
                    ) {
                        return BenchResult::Fail(e);
                    }
                    match c.read_response(timeout) {
                        Ok(resp) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                if first.is_none() {
                                    first = Some(resp.clone());
                                }
                                if !is_valid_response(&resp) {
                                    // Dump server logs for debugging
                                    if !c.logs.is_empty() {
                                        eprintln!("\n--- {} server logs ---", srv.label);
                                        for line in &c.logs {
                                            eprintln!("  {}", line);
                                        }
                                        eprintln!("--- end ---");
                                    }
                                    return BenchResult::Invalid {
                                        first_response: resp,
                                        diag_info: Some(diag_info),
                                    };
                                }
                                samples.push(ms);
                            }
                        }
                        Err(e) => return BenchResult::Fail(e),
                    }
                }
                c.kill();
                BenchResult::Ok {
                    samples,
                    first_response: first.unwrap_or(json!(null)),
                    diag_info: Some(diag_info),
                }
            },
        );
        all_results.push(("Go to Definition", rows));
    }

    // ── declaration ─────────────────────────────────────────────────────────

    if benchmarks.contains(&"declaration") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let target_line: u32 = 102;
        let target_col: u32 = 15;

        let rows = run_bench(
            "declaration",
            &[
                format!(
                    "## 4. GO TO DECLARATION (ms) — {} iterations, {} warmup",
                    n, w
                ),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                format!(
                    "Target: `TickMath` at line {}:{}",
                    target_line + 1,
                    target_col
                ),
                "Measures: textDocument/declaration request -> response".into(),
                "Waits for valid publishDiagnostics before sending requests.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                    Ok(c) => c,
                    Err(e) => return BenchResult::Fail(e),
                };
                if let Err(e) = c.initialize(root) {
                    return BenchResult::Fail(e);
                }
                if let Err(e) = c.open_file(&pool_sol) {
                    return BenchResult::Fail(e);
                }

                // Wait for valid diagnostics (build complete)
                let diag_info = match c.wait_for_valid_diagnostics(timeout) {
                    Ok(info) => info,
                    Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
                };
                eprintln!(
                    "diagnostics: {} items in {:.0}ms ... ",
                    diag_info.count, diag_info.elapsed_ms
                );
                eprint!("    ");

                let file_uri = uri(&pool_sol);
                let mut samples = Vec::new();
                let mut first: Option<Value> = None;
                for i in 0..(w + n) {
                    let start = Instant::now();
                    if let Err(e) = c.send(
                        "textDocument/declaration",
                        json!({
                            "textDocument": { "uri": file_uri },
                            "position": { "line": target_line, "character": target_col },
                        }),
                    ) {
                        return BenchResult::Fail(e);
                    }
                    match c.read_response(timeout) {
                        Ok(resp) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                if first.is_none() {
                                    first = Some(resp.clone());
                                }
                                if !is_valid_response(&resp) {
                                    // Dump server logs for debugging
                                    if !c.logs.is_empty() {
                                        eprintln!("\n--- {} server logs ---", srv.label);
                                        for line in &c.logs {
                                            eprintln!("  {}", line);
                                        }
                                        eprintln!("--- end ---");
                                    }
                                    return BenchResult::Invalid {
                                        first_response: resp,
                                        diag_info: Some(diag_info),
                                    };
                                }
                                samples.push(ms);
                            }
                        }
                        Err(e) => return BenchResult::Fail(e),
                    }
                }
                c.kill();
                BenchResult::Ok {
                    samples,
                    first_response: first.unwrap_or(json!(null)),
                    diag_info: Some(diag_info),
                }
            },
        );
        all_results.push(("Go to Declaration", rows));
    }

    // ── hover ─────────────────────────────────────────────────────────

    if benchmarks.contains(&"hover") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let target_line: u32 = 102;
        let target_col: u32 = 15;

        let rows = run_bench(
            "hover",
            &[
                format!("## 5. HOVER (ms) — {} iterations, {} warmup", n, w),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                format!(
                    "Target: `TickMath` at line {}:{}",
                    target_line + 1,
                    target_col
                ),
                "Measures: textDocument/hover request -> response".into(),
                "Waits for valid publishDiagnostics before sending requests.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                    Ok(c) => c,
                    Err(e) => return BenchResult::Fail(e),
                };
                if let Err(e) = c.initialize(root) {
                    return BenchResult::Fail(e);
                }
                if let Err(e) = c.open_file(&pool_sol) {
                    return BenchResult::Fail(e);
                }

                // Wait for valid diagnostics (build complete)
                let diag_info = match c.wait_for_valid_diagnostics(timeout) {
                    Ok(info) => info,
                    Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
                };
                eprintln!(
                    "diagnostics: {} items in {:.0}ms ... ",
                    diag_info.count, diag_info.elapsed_ms
                );
                eprint!("    ");

                let file_uri = uri(&pool_sol);
                let mut samples = Vec::new();
                let mut first: Option<Value> = None;
                for i in 0..(w + n) {
                    let start = Instant::now();
                    if let Err(e) = c.send(
                        "textDocument/hover",
                        json!({
                            "textDocument": { "uri": file_uri },
                            "position": { "line": target_line, "character": target_col },
                        }),
                    ) {
                        return BenchResult::Fail(e);
                    }
                    match c.read_response(timeout) {
                        Ok(resp) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                if first.is_none() {
                                    first = Some(resp.clone());
                                }
                                if !is_valid_response(&resp) {
                                    return BenchResult::Invalid {
                                        first_response: resp,
                                        diag_info: Some(diag_info),
                                    };
                                }
                                samples.push(ms);
                            }
                        }
                        Err(e) => {
                            return BenchResult::Fail(e);
                        }
                    }
                }
                c.kill();
                BenchResult::Ok {
                    samples,
                    first_response: first.unwrap_or(json!(null)),
                    diag_info: Some(diag_info),
                }
            },
        );
        all_results.push(("Hover", rows));
    }

    // ── references ─────────────────────────────────────────────────────────

    if benchmarks.contains(&"references") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let target_line: u32 = 102;
        let target_col: u32 = 15;

        let rows = run_bench(
            "references",
            &[
                format!(
                    "## 6. FIND REFERENCES (ms) — {} iterations, {} warmup",
                    n, w
                ),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                format!(
                    "Target: `TickMath` at line {}:{}",
                    target_line + 1,
                    target_col
                ),
                "Measures: textDocument/references request -> response".into(),
                "Waits for valid publishDiagnostics before sending requests.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                    Ok(c) => c,
                    Err(e) => return BenchResult::Fail(e),
                };
                if let Err(e) = c.initialize(root) {
                    return BenchResult::Fail(e);
                }
                if let Err(e) = c.open_file(&pool_sol) {
                    return BenchResult::Fail(e);
                }

                // Wait for valid diagnostics (build complete)
                let diag_info = match c.wait_for_valid_diagnostics(timeout) {
                    Ok(info) => info,
                    Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
                };
                eprintln!(
                    "diagnostics: {} items in {:.0}ms ... ",
                    diag_info.count, diag_info.elapsed_ms
                );
                eprint!("    ");

                let file_uri = uri(&pool_sol);
                let mut samples = Vec::new();
                let mut first: Option<Value> = None;
                for i in 0..(w + n) {
                    let start = Instant::now();
                    if let Err(e) = c.send(
                        "textDocument/references",
                        json!({
                            "textDocument": { "uri": file_uri },
                            "position": { "line": target_line, "character": target_col },
                            "context": { "includeDeclaration": true }
                        }),
                    ) {
                        return BenchResult::Fail(e);
                    }
                    match c.read_response(timeout) {
                        Ok(resp) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                if first.is_none() {
                                    first = Some(resp.clone());
                                }
                                if !is_valid_response(&resp) {
                                    return BenchResult::Invalid {
                                        first_response: resp,
                                        diag_info: Some(diag_info),
                                    };
                                }
                                samples.push(ms);
                            }
                        }
                        Err(e) => {
                            return BenchResult::Fail(e);
                        }
                    }
                }
                c.kill();
                BenchResult::Ok {
                    samples,
                    first_response: first.unwrap_or(json!(null)),
                    diag_info: Some(diag_info),
                }
            },
        );
        all_results.push(("Find References", rows));
    }

    // ── documentSymbol ─────────────────────────────────────────────────────────

    if benchmarks.contains(&"documentSymbol") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);

        let rows = run_bench(
            "documentSymbol",
            &[
                format!(
                    "## 7. DOCUMENT SYMBOLS (ms) — {} iterations, {} warmup",
                    n, w
                ),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                "Measures: textDocument/documentSymbol request -> response".into(),
                "Waits for valid publishDiagnostics before sending requests.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                    Ok(c) => c,
                    Err(e) => return BenchResult::Fail(e),
                };
                if let Err(e) = c.initialize(root) {
                    return BenchResult::Fail(e);
                }
                if let Err(e) = c.open_file(&pool_sol) {
                    return BenchResult::Fail(e);
                }

                // Wait for valid diagnostics (build complete)
                let diag_info = match c.wait_for_valid_diagnostics(timeout) {
                    Ok(info) => info,
                    Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
                };
                eprintln!(
                    "diagnostics: {} items in {:.0}ms ... ",
                    diag_info.count, diag_info.elapsed_ms
                );
                eprint!("    ");

                let file_uri = uri(&pool_sol);
                let mut samples = Vec::new();
                let mut first: Option<Value> = None;
                for i in 0..(w + n) {
                    let start = Instant::now();
                    if let Err(e) = c.send(
                        "textDocument/documentSymbol",
                        json!({
                            "textDocument": { "uri": file_uri }
                        }),
                    ) {
                        return BenchResult::Fail(e);
                    }
                    match c.read_response(timeout) {
                        Ok(resp) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                if first.is_none() {
                                    first = Some(resp.clone());
                                }
                                if !is_valid_response(&resp) {
                                    return BenchResult::Invalid {
                                        first_response: resp,
                                        diag_info: Some(diag_info),
                                    };
                                }
                                samples.push(ms);
                            }
                        }
                        Err(e) => {
                            return BenchResult::Fail(e);
                        }
                    }
                }
                c.kill();
                BenchResult::Ok {
                    samples,
                    first_response: first.unwrap_or(json!(null)),
                    diag_info: Some(diag_info),
                }
            },
        );
        all_results.push(("Document Symbols", rows));
    }

    // ── documentLink ─────────────────────────────────────────────────────────

    if benchmarks.contains(&"documentLink") {
        let pool_sol = Path::new(v4).join("src/libraries/Pool.sol");
        let line_count = std::fs::read_to_string(&pool_sol)
            .map(|s| s.lines().count())
            .unwrap_or(0);

        let rows = run_bench(
            "documentLink",
            &[
                format!("## 8. DOCUMENT LINKS (ms) — {} iterations, {} warmup", n, w),
                String::new(),
                format!("File: Pool.sol ({} lines)", line_count),
                "Measures: textDocument/documentLink request -> response".into(),
                "Waits for valid publishDiagnostics before sending requests.".into(),
                String::new(),
            ],
            &avail,
            &root,
            Path::new(v4),
            |srv, root, cwd| {
                let mut c = match LspClient::spawn(srv.cmd, srv.args, cwd) {
                    Ok(c) => c,
                    Err(e) => return BenchResult::Fail(e),
                };
                if let Err(e) = c.initialize(root) {
                    return BenchResult::Fail(e);
                }
                if let Err(e) = c.open_file(&pool_sol) {
                    return BenchResult::Fail(e);
                }

                // Wait for valid diagnostics (build complete)
                let diag_info = match c.wait_for_valid_diagnostics(timeout) {
                    Ok(info) => info,
                    Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
                };
                eprintln!(
                    "diagnostics: {} items in {:.0}ms ... ",
                    diag_info.count, diag_info.elapsed_ms
                );
                eprint!("    ");

                let file_uri = uri(&pool_sol);
                let mut samples = Vec::new();
                let mut first: Option<Value> = None;
                for i in 0..(w + n) {
                    let start = Instant::now();
                    if let Err(e) = c.send(
                        "textDocument/documentLink",
                        json!({
                            "textDocument": { "uri": file_uri }
                        }),
                    ) {
                        return BenchResult::Fail(e);
                    }
                    match c.read_response(timeout) {
                        Ok(resp) => {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if i >= w {
                                if first.is_none() {
                                    first = Some(resp.clone());
                                }
                                if !is_valid_response(&resp) {
                                    return BenchResult::Invalid {
                                        first_response: resp,
                                        diag_info: Some(diag_info),
                                    };
                                }
                                samples.push(ms);
                            }
                        }
                        Err(e) => {
                            return BenchResult::Fail(e);
                        }
                    }
                }
                c.kill();
                BenchResult::Ok {
                    samples,
                    first_response: first.unwrap_or(json!(null)),
                    diag_info: Some(diag_info),
                }
            },
        );
        all_results.push(("Document Links", rows));
    }

    // ── Generate outputs ──────────────────────────────────────────────────

    if !all_results.is_empty() {
        let server_labels: Vec<&str> = all_results[0].1.iter().map(|r| r.label.as_str()).collect();
        let ts = timestamp();
        let date = date_stamp();

        // ── JSON output ─────────────────────────────────────────────────

        let json_benchmarks: Vec<Value> = all_results
            .iter()
            .map(|(bench_name, rows)| {
                json!({
                    "name": bench_name,
                    "servers": rows.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
                })
            })
            .collect();

        let json_servers: Vec<Value> = versions
            .iter()
            .map(|(label, ver)| {
                json!({
                    "name": label,
                    "version": ver,
                })
            })
            .collect();

        let json_output = json!({
            "timestamp": ts,
            "date": date,
            "settings": {
                "iterations": n,
                "warmup": w,
                "timeout_secs": timeout.as_secs(),
            },
            "servers": json_servers,
            "benchmarks": json_benchmarks,
        });

        // Write timestamped JSON
        let is_full_run = benchmarks.len() == ALL_BENCHMARKS.len()
            && ALL_BENCHMARKS.iter().all(|b| benchmarks.contains(b));
        let json_dir = if is_full_run {
            "benchmarks".to_string()
        } else {
            let names: Vec<&str> = benchmarks.iter().copied().collect();
            format!("benchmarks/{}", names.join("+"))
        };
        let _ = std::fs::create_dir_all(&json_dir);
        let json_path = format!("{}/{}.json", json_dir, ts.replace(':', "-"));
        let json_pretty = serde_json::to_string_pretty(&json_output).unwrap();
        std::fs::write(&json_path, &json_pretty).unwrap();
        eprintln!("  -> {}", json_path);

        // ── results/README.md (only for full runs) ────────────────────────
        if is_full_run {
            let mut lines: Vec<String> = Vec::new();
            lines.push("# Solidity LSP Benchmark Results".to_string());
            lines.push(String::new());
            lines.push(format!("Date: {}", date));
            lines.push(String::new());

            // Settings table
            lines.push("## Settings".to_string());
            lines.push(String::new());
            lines.push("| Setting | Value |".to_string());
            lines.push("|---------|-------|".to_string());
            lines.push(format!("| Iterations | {} |", n));
            lines.push(format!("| Warmup | {} |", w));
            lines.push(format!("| Timeout | {}s |", timeout.as_secs()));
            lines.push(String::new());

            // Servers table with versions
            lines.push("## Servers".to_string());
            lines.push(String::new());
            lines.push("| Server | Version |".to_string());
            lines.push("|--------|---------|".to_string());
            for (label, ver) in &versions {
                lines.push(format!("| {} | {} |", label, ver));
            }
            lines.push(String::new());

            // Results table — rank servers per benchmark, track wins for trophy
            let has_valid = |r: &BenchRow| -> bool {
                r.kind == 0
                    && r.summary != "null"
                    && r.summary != "no result"
                    && !r.summary.is_empty()
            };

            let medal_icons = ["🥇", "🥈", "🥉"];
            let mut wins: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();

            // Pre-compute medals for each benchmark row
            let mut bench_medals: Vec<Vec<&str>> = Vec::new();
            for (_bench_name, rows) in &all_results {
                let mut ranked: Vec<(usize, f64)> = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| has_valid(r))
                    .map(|(i, r)| (i, r.mean))
                    .collect();
                ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

                let mut row_medals = vec![""; rows.len()];
                for (place, (idx, _)) in ranked.iter().enumerate() {
                    if place < medal_icons.len() {
                        row_medals[*idx] = medal_icons[place];
                    }
                    if place == 0 {
                        *wins.entry(rows[*idx].label.clone()).or_insert(0) += 1;
                    }
                }
                bench_medals.push(row_medals);
            }

            // Find overall winner (most 🥇 wins)
            let trophy_winner = wins
                .iter()
                .max_by_key(|(_, count)| *count)
                .map(|(label, _)| label.clone());

            lines.push("## Results".to_string());
            lines.push(String::new());

            let mut header = "| Benchmark |".to_string();
            let mut separator = "|-----------|".to_string();
            for label in &server_labels {
                let trophy = if trophy_winner.as_deref() == Some(label.as_ref()) {
                    " 🏆"
                } else {
                    ""
                };
                header.push_str(&format!(" {}{} |", label, trophy));
                separator.push_str(&"-".repeat(label.len() + trophy.len() + 2));
                separator.push('|');
            }
            lines.push(header);
            lines.push(separator);

            for (i, (bench_name, rows)) in all_results.iter().enumerate() {
                let mut row = format!("| {} |", bench_name);
                for (j, r) in rows.iter().enumerate() {
                    let cell = match r.kind {
                        0 => {
                            let medal = bench_medals[i][j];
                            let suffix = if medal.is_empty() {
                                "".to_string()
                            } else {
                                format!(" {}", medal)
                            };
                            format!(" {:.1}ms{} |", r.mean, suffix)
                        }
                        1 => {
                            if r.summary.contains("Unknown method")
                                || r.summary.contains("unsupported")
                            {
                                " unsupported |".to_string()
                            } else {
                                " - |".to_string()
                            }
                        }
                        _ => {
                            if r.fail_msg.contains("timeout") {
                                " timeout |".to_string()
                            } else {
                                " FAIL |".to_string()
                            }
                        }
                    };
                    row.push_str(&cell);
                }
                lines.push(row);
            }

            lines.push(String::new());
            lines.push("## Detailed Results".to_string());
            lines.push(String::new());
            for name in ALL_BENCHMARKS {
                if all_results.iter().any(|(_, rows)| {
                    rows.first().map(|_| true).unwrap_or(false)
                        && all_results.iter().any(|(n, _)| {
                            *n == match *name {
                                "spawn" => "Spawn + Init",
                                "diagnostics" => "Diagnostics",
                                "definition" => "Go to Definition",
                                "declaration" => "Go to Declaration",
                                "hover" => "Hover",
                                "references" => "Find References",
                                "documentSymbol" => "Document Symbols",
                                "documentLink" => "Document Links",
                                _ => "",
                            }
                        })
                }) {
                    lines.push(format!("- [{}](./{}.md)", name, name));
                }
            }
            lines.push(String::new());

            let out = lines.join("\n") + "\n";
            let path = "results/README.md";
            let _ = std::fs::create_dir_all("results");
            std::fs::write(path, &out).unwrap();
            println!("{}", out);
            eprintln!("  -> {}", path);
        } // end is_full_run
    }
}
