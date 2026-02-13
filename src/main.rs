use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    project: String,
    file: String,
    #[serde(default = "default_line")]
    line: u32,
    #[serde(default = "default_col")]
    col: u32,
    #[serde(default = "default_iterations")]
    iterations: usize,
    #[serde(default = "default_warmup")]
    warmup: usize,
    #[serde(default = "default_timeout")]
    timeout: u64,
    #[serde(default = "default_index_timeout")]
    index_timeout: u64,
    #[serde(default = "default_output")]
    output: String,
    #[serde(default)]
    benchmarks: Vec<String>,
    servers: Vec<ServerConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    label: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    link: String,
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
}

fn default_line() -> u32 {
    102
}
fn default_col() -> u32 {
    15
}
fn default_iterations() -> usize {
    10
}
fn default_warmup() -> usize {
    2
}
fn default_timeout() -> u64 {
    10
}
fn default_index_timeout() -> u64 {
    15
}
fn default_output() -> String {
    "benchmarks".to_string()
}

fn load_config(path: &str) -> Config {
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error reading config {}: {}", path, e);
        std::process::exit(1);
    });
    serde_yaml::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Error parsing config {}: {}", path, e);
        std::process::exit(1);
    })
}

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
    #[allow(dead_code)]
    logs: Vec<String>,
}

struct DiagnosticsInfo {
    message: Value,
}

fn reader_thread(stdout: std::process::ChildStdout, tx: mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length: usize = 0;
        let mut in_header = false;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return,
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
        }
        if content_length == 0 {
            continue;
        }
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
            if tx.send(msg).is_err() {
                return;
            }
        }
    }
}

impl LspClient {
    fn spawn(cmd: &str, args: &[String], cwd: &Path) -> Result<Self, String> {
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

    fn wait_for_valid_diagnostics(&mut self, timeout: Duration) -> Result<DiagnosticsInfo, String> {
        let start = Instant::now();
        let deadline = start + timeout;
        let mut last_count = 0usize;
        let mut last_msg = json!(null);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return if last_count > 0 {
                    Ok(DiagnosticsInfo { message: last_msg })
                } else {
                    Err("timeout".into())
                };
            }
            let msg = self.recv(remaining)?;
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
                last_count = count;
                last_msg = msg;
                if count > 0 {
                    return Ok(DiagnosticsInfo { message: last_msg });
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
    std::fs::canonicalize(&bin_path)
        .map(|p| p.to_string_lossy().to_string())
        .ok()
        .or(Some(bin_path))
}

fn detect_version(cmd: &str) -> String {
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

fn stats(samples: &mut Vec<f64>) -> (f64, f64, f64) {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = samples.len();
    (
        samples[n / 2],
        samples[((n as f64) * 0.95) as usize],
        samples.iter().sum::<f64>() / n as f64,
    )
}

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

fn response_summary(resp: &Value, max_chars: usize) -> String {
    let full = if let Some(err) = resp.get("error") {
        format!(
            "error: {}",
            err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        )
    } else if let Some(r) = resp.get("result").or_else(|| resp.get("params")) {
        if r.is_null() {
            "null".into()
        } else {
            serde_json::to_string_pretty(r).unwrap_or_default()
        }
    } else {
        "no result".into()
    };
    if full.len() <= max_chars {
        full
    } else {
        let break_at = full[..max_chars].rfind('\n').unwrap_or(max_chars);
        format!("{}...", &full[..break_at])
    }
}

// ── Servers (alias for config) ──────────────────────────────────────────────

type Server = ServerConfig;

// ── Bench result per server ─────────────────────────────────────────────────

enum BenchResult {
    Ok {
        samples: Vec<f64>,
        first_response: Value,
    },
    Invalid {
        first_response: Value,
    },
    Fail(String),
}

struct BenchRow {
    label: String,
    p50: f64,
    p95: f64,
    mean: f64,
    kind: u8,
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

// ── Progress ────────────────────────────────────────────────────────────────

fn spinner(label: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {prefix:<20} {msg}")
            .unwrap()
            .tick_chars("🌑🌒🌓🌔🌕🌖🌗🌘 "),
    );
    pb.set_prefix(label.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn finish_pass(pb: &ProgressBar, mean: f64, p50: f64, p95: f64) {
    pb.finish_with_message(format!(
        "{}  {:.1}ms mean  ({:.1}ms p50, {:.1}ms p95)",
        style("pass").green().bold(),
        mean,
        p50,
        p95
    ));
}

fn finish_fail(pb: &ProgressBar, msg: &str) {
    pb.finish_with_message(format!("{}  {}", style("fail").red().bold(), msg));
}

fn iter_msg(i: usize, w: usize, n: usize) -> String {
    if i < w {
        format!("warmup {}/{}", i + 1, w)
    } else {
        format!("iter {}/{}", i - w + 1, n)
    }
}

// ── Reusable benchmark runners ──────────────────────────────────────────────

/// Benchmark that spawns a fresh server each iteration (e.g. spawn+init).
fn bench_spawn(
    srv: &Server,
    root: &str,
    cwd: &Path,
    w: usize,
    n: usize,
    on_progress: &dyn Fn(&str),
) -> BenchResult {
    let mut samples = Vec::new();
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));
        let start = Instant::now();
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd) {
            Ok(c) => c,
            Err(e) => return BenchResult::Fail(e),
        };
        if let Err(e) = c.initialize(root) {
            return BenchResult::Fail(e);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
        if i >= w {
            samples.push(ms);
        }
        c.kill();
    }
    BenchResult::Ok {
        samples,
        first_response: json!({"result": "ok"}),
    }
}

/// Benchmark that spawns fresh each iteration, measures didOpen -> diagnostics.
fn bench_diagnostics(
    srv: &Server,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    timeout: Duration,
    w: usize,
    n: usize,
    on_progress: &dyn Fn(&str),
) -> BenchResult {
    let mut samples = Vec::new();
    let mut first: Option<DiagnosticsInfo> = None;
    for i in 0..(w + n) {
        on_progress(&format!("{}  waiting for diagnostics", iter_msg(i, w, n)));
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd) {
            Ok(c) => c,
            Err(e) => return BenchResult::Fail(e),
        };
        if let Err(e) = c.initialize(root) {
            return BenchResult::Fail(e);
        }
        let start = Instant::now();
        if let Err(e) = c.open_file(target_file) {
            return BenchResult::Fail(e);
        }
        match c.wait_for_valid_diagnostics(timeout) {
            Ok(diag_info) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
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
    let diag = first.unwrap_or(DiagnosticsInfo {
        message: json!(null),
    });
    BenchResult::Ok {
        samples,
        first_response: diag.message,
    }
}

/// Benchmark an LSP method on a single persistent server session.
/// Spawns once, waits for diagnostics, then iterates the given method.
fn bench_lsp_method(
    srv: &Server,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    method: &str,
    params_fn: &dyn Fn(&str) -> Value, // takes file_uri, returns params
    index_timeout: Duration,
    timeout: Duration,
    w: usize,
    n: usize,
    on_progress: &dyn Fn(&str),
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd) {
        Ok(c) => c,
        Err(e) => return BenchResult::Fail(e),
    };
    if let Err(e) = c.initialize(root) {
        return BenchResult::Fail(e);
    }
    if let Err(e) = c.open_file(target_file) {
        return BenchResult::Fail(e);
    }
    on_progress("waiting for diagnostics");
    match c.wait_for_valid_diagnostics(index_timeout) {
        Ok(_) => {}
        Err(e) => return BenchResult::Fail(format!("wait_for_diagnostics: {}", e)),
    }

    let file_uri = uri(target_file);
    let mut samples = Vec::new();
    let mut first: Option<Value> = None;
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));
        let start = Instant::now();
        if let Err(e) = c.send(method, params_fn(&file_uri)) {
            return BenchResult::Fail(e);
        }
        match c.read_response(timeout) {
            Ok(resp) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
                if i >= w {
                    if first.is_none() {
                        first = Some(resp.clone());
                    }
                    if !is_valid_response(&resp) {
                        return BenchResult::Invalid {
                            first_response: resp,
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
    }
}

/// Run a benchmark across all servers, showing a spinner per server.
fn run_bench<F>(servers: &[&Server], f: F) -> Vec<BenchRow>
where
    F: Fn(&Server, &dyn Fn(&str)) -> BenchResult,
{
    let mut rows = Vec::new();
    for srv in servers {
        let pb = spinner(&srv.label);
        let on_progress = |msg: &str| pb.set_message(msg.to_string());
        match f(srv, &on_progress) {
            BenchResult::Ok {
                mut samples,
                first_response,
            } => {
                let (p50, p95, mean) = stats(&mut samples);
                let summary = response_summary(&first_response, 500);
                finish_pass(&pb, mean, p50, p95);
                rows.push(BenchRow {
                    label: srv.label.to_string(),
                    p50,
                    p95,
                    mean,
                    summary,
                    kind: 0,
                    fail_msg: String::new(),
                });
            }
            BenchResult::Invalid { first_response } => {
                let summary = response_summary(&first_response, 500);
                finish_fail(&pb, "invalid response");
                rows.push(BenchRow {
                    label: srv.label.to_string(),
                    p50: 0.0,
                    p95: 0.0,
                    mean: 0.0,
                    summary,
                    kind: 1,
                    fail_msg: String::new(),
                });
            }
            BenchResult::Fail(e) => {
                finish_fail(&pb, &e);
                rows.push(BenchRow {
                    label: srv.label.to_string(),
                    p50: 0.0,
                    p95: 0.0,
                    mean: 0.0,
                    summary: String::new(),
                    kind: 2,
                    fail_msg: e,
                });
            }
        }
    }
    rows
}

// ── JSON output ─────────────────────────────────────────────────────────────

fn save_json(
    results: &[(&str, Vec<BenchRow>)],
    versions: &[(&str, String)],
    servers: &[&ServerConfig],
    n: usize,
    w: usize,
    timeout: &Duration,
    index_timeout: &Duration,
    project: &str,
    bench_file: &str,
    target_line: u32,
    target_col: u32,
    dir: &str,
) -> String {
    let ts = timestamp();
    let date = date_stamp();
    let json_benchmarks: Vec<Value> = results
        .iter()
        .map(|(name, rows)| {
            json!({
                "name": name,
                "servers": rows.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let json_servers: Vec<Value> = versions
        .iter()
        .map(|(label, ver)| {
            let mut obj = json!({"name": label, "version": ver});
            if let Some(srv) = servers.iter().find(|s| s.label == *label) {
                if !srv.description.is_empty() {
                    obj["description"] = json!(srv.description);
                }
                if !srv.link.is_empty() {
                    obj["link"] = json!(srv.link);
                }
            }
            obj
        })
        .collect();
    let output = json!({
        "timestamp": ts,
        "date": date,
        "settings": {
            "iterations": n,
            "warmup": w,
            "timeout_secs": timeout.as_secs(),
            "index_timeout_secs": index_timeout.as_secs(),
            "project": project,
            "file": bench_file,
            "line": target_line,
            "col": target_col,
        },
        "servers": json_servers,
        "benchmarks": json_benchmarks,
    });
    let _ = std::fs::create_dir_all(dir);
    let path = format!("{}/{}.json", dir, ts.replace(':', "-"));
    let pretty = serde_json::to_string_pretty(&output).unwrap();
    std::fs::write(&path, &pretty).unwrap();
    path
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
    eprintln!("Usage: bench [OPTIONS]");
    eprintln!("       bench init");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -c, --config <PATH>   Config file (default: benchmark.yaml)");
    eprintln!("  -s, --server <NAME>   Filter servers (repeatable)");
    eprintln!("  -h, --help            Show this help");
    eprintln!();
    eprintln!("All settings are configured in benchmark.yaml. See DOCS.md for details.");
}

const EXAMPLE_CONFIG: &str = r#"# Solidity LSP Benchmark Configuration
# See DOCS.md for details on all fields

# Project root containing the Solidity files
project: examples

# Target file to benchmark (relative to project root)
file: Counter.sol

# Target position for position-based benchmarks (0-based)
# These use LSP protocol indexing, so subtract 1 from your editor's
# line and column numbers. For example, editor line 22 col 9 -> line: 21, col: 8
#
#   line 22 (editor):       number = newNumber;
#   col   9 (editor):       ^
#
# The position should land on an identifier (variable, function, type)
# that LSP methods can act on (definition, hover, references, etc.)
line: 21
col: 8

# Benchmark settings
iterations: 10    # number of measured iterations
warmup: 2         # warmup iterations (discarded)
timeout: 10       # seconds per LSP request
index_timeout: 15 # seconds for server to index/warm up
output: benchmarks # directory for JSON results

# Which benchmarks to run (omit or use "all" to run everything)
benchmarks:
  - all

# LSP servers to benchmark
servers:
  - label: my-server
    description: My Solidity Language Server
    link: https://github.com/example/my-server
    cmd: my-solidity-lsp
    args: []

  # - label: solc
  #   description: Official Solidity compiler LSP
  #   link: https://docs.soliditylang.org
  #   cmd: solc
  #   args: ["--lsp"]
"#;

fn init_config(path: &str) {
    if Path::new(path).exists() {
        eprintln!("{} already exists", path);
        std::process::exit(1);
    }
    std::fs::write(path, EXAMPLE_CONFIG).unwrap_or_else(|e| {
        eprintln!("Error writing {}: {}", path, e);
        std::process::exit(1);
    });
    eprintln!("Created {}", path);
    eprintln!();
    eprintln!("Edit the file to configure your servers, then run:");
    eprintln!("  bench all");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut config_path = "benchmark.yaml".to_string();
    let mut commands: Vec<String> = Vec::new();
    let mut server_filter: Vec<String> = Vec::new();

    // CLI overrides (None = use config value)
    let mut cli_n: Option<usize> = None;
    let mut cli_w: Option<usize> = None;
    let mut cli_timeout: Option<u64> = None;
    let mut cli_index_timeout: Option<u64> = None;
    let mut cli_file: Option<String> = None;
    let mut cli_line: Option<u32> = None;
    let mut cli_col: Option<u32> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-c" | "--config" => {
                i += 1;
                config_path = args
                    .get(i)
                    .unwrap_or_else(|| {
                        eprintln!("Error: -c requires a path");
                        std::process::exit(1);
                    })
                    .clone();
            }
            "-n" | "--iterations" => {
                i += 1;
                cli_n = Some(args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: -n requires a number");
                    std::process::exit(1);
                }));
            }
            "-w" | "--warmup" => {
                i += 1;
                cli_w = Some(args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: -w requires a number");
                    std::process::exit(1);
                }));
            }
            "-t" | "--timeout" => {
                i += 1;
                cli_timeout = Some(args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: -t requires a number (seconds)");
                    std::process::exit(1);
                }));
            }
            "-T" | "--index-timeout" => {
                i += 1;
                cli_index_timeout =
                    Some(args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                        eprintln!("Error: -T requires a number (seconds)");
                        std::process::exit(1);
                    }));
            }
            "-s" | "--server" => {
                i += 1;
                let name = args.get(i).unwrap_or_else(|| {
                    eprintln!("Error: -s requires a server name");
                    std::process::exit(1);
                });
                server_filter.push(name.to_lowercase());
            }
            "-f" | "--file" => {
                i += 1;
                cli_file = Some(
                    args.get(i)
                        .unwrap_or_else(|| {
                            eprintln!("Error: -f requires a file path");
                            std::process::exit(1);
                        })
                        .clone(),
                );
            }
            "--line" => {
                i += 1;
                cli_line = Some(args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: --line requires a number");
                    std::process::exit(1);
                }));
            }
            "--col" => {
                i += 1;
                cli_col = Some(args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Error: --col requires a number");
                    std::process::exit(1);
                }));
            }
            "init" => commands.push("init".to_string()),
            other => {
                eprintln!("Error: unknown argument '{}'", other);
                print_usage();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Handle init before loading config
    if commands.iter().any(|c| c == "init") {
        init_config(&config_path);
        std::process::exit(0);
    }

    // Load config, apply CLI overrides
    let mut cfg = load_config(&config_path);
    if let Some(v) = cli_n {
        cfg.iterations = v;
    }
    if let Some(v) = cli_w {
        cfg.warmup = v;
    }
    if let Some(v) = cli_timeout {
        cfg.timeout = v;
    }
    if let Some(v) = cli_index_timeout {
        cfg.index_timeout = v;
    }
    if let Some(v) = cli_file {
        cfg.file = v;
    }
    if let Some(v) = cli_line {
        cfg.line = v;
    }
    if let Some(v) = cli_col {
        cfg.col = v;
    }

    let n = cfg.iterations;
    let w = cfg.warmup;
    let timeout = Duration::from_secs(cfg.timeout);
    let index_timeout = Duration::from_secs(cfg.index_timeout);
    let target_line = cfg.line;
    let target_col = cfg.col;
    let output_dir = cfg.output;
    let partial_dir = format!("{}/partial", output_dir);

    // Resolve which benchmarks to run from config
    let benchmarks: Vec<&str> =
        if cfg.benchmarks.is_empty() || cfg.benchmarks.iter().any(|c| c == "all") {
            ALL_BENCHMARKS.to_vec()
        } else {
            cfg.benchmarks.iter().map(|s| s.as_str()).collect()
        };

    for b in &benchmarks {
        if !ALL_BENCHMARKS.contains(b) {
            eprintln!("Error: unknown benchmark '{}'", b);
            print_usage();
            std::process::exit(1);
        }
    }

    let project = cfg.project.clone();
    let cwd = PathBuf::from(&project);
    if !cwd.exists() {
        eprintln!("Error: project directory not found: {}", project);
        std::process::exit(1);
    }
    let root = uri(&cwd);
    let bench_file_rel = &cfg.file;
    let bench_sol = cwd.join(bench_file_rel);
    if !bench_sol.exists() {
        eprintln!("Error: benchmark file not found: {}", bench_sol.display());
        std::process::exit(1);
    }

    eprintln!("  {} {}", style("config").dim(), config_path);
    eprintln!(
        "  {} {}  (line {}, col {})",
        style("file").dim(),
        bench_file_rel,
        target_line,
        target_col
    );

    let avail: Vec<&Server> = cfg
        .servers
        .iter()
        .filter(|s| {
            if !server_filter.is_empty()
                && !server_filter
                    .iter()
                    .any(|f| s.label.to_lowercase().contains(f))
            {
                return false;
            }
            let ok = available(&s.cmd);
            if !ok {
                eprintln!("  {} {} -- not found", style("skip").yellow(), s.label);
            }
            ok
        })
        .collect();

    eprintln!("\n{}", style("Detecting versions...").dim());
    let versions: Vec<(&str, String)> = avail
        .iter()
        .map(|s| {
            let ver = detect_version(&s.cmd);
            eprintln!("  {} = {}", style(&s.label).bold(), ver);
            (s.label.as_str(), ver)
        })
        .collect();

    let total = benchmarks.len();
    let mut num = 0usize;
    let mut all_results: Vec<(&str, Vec<BenchRow>)> = Vec::new();

    // Position + method params for definition/declaration/hover/references
    let position_params = |file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": target_line, "character": target_col },
        })
    };
    let doc_params = |file_uri: &str| -> Value { json!({ "textDocument": { "uri": file_uri } }) };
    let ref_params = |file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": target_line, "character": target_col },
            "context": { "includeDeclaration": true },
        })
    };

    // (command, display_name, lsp_method, params_fn)
    let method_benchmarks: Vec<(&str, &str, &str, &dyn Fn(&str) -> Value)> = vec![
        (
            "definition",
            "Go to Definition",
            "textDocument/definition",
            &position_params,
        ),
        (
            "declaration",
            "Go to Declaration",
            "textDocument/declaration",
            &position_params,
        ),
        ("hover", "Hover", "textDocument/hover", &position_params),
        (
            "references",
            "Find References",
            "textDocument/references",
            &ref_params,
        ),
        (
            "documentSymbol",
            "Document Symbols",
            "textDocument/documentSymbol",
            &doc_params,
        ),
        (
            "documentLink",
            "Document Links",
            "textDocument/documentLink",
            &doc_params,
        ),
    ];

    // ── spawn ────────────────────────────────────────────────────────────

    if benchmarks.contains(&"spawn") {
        num += 1;
        eprintln!(
            "\n{}",
            style(format!("[{}/{}] Spawn + Init", num, total)).bold()
        );
        let rows = run_bench(&avail, |srv, on_progress| {
            bench_spawn(srv, &root, &cwd, w, n, on_progress)
        });
        all_results.push(("Spawn + Init", rows));
        let p = save_json(
            &all_results,
            &versions,
            &avail,
            n,
            w,
            &timeout,
            &index_timeout,
            &project,
            bench_file_rel,
            target_line,
            target_col,
            &partial_dir,
        );
        eprintln!("  {} {}", style("saved").dim(), style(&p).dim());
    }

    // ── diagnostics ──────────────────────────────────────────────────────

    if benchmarks.contains(&"diagnostics") {
        num += 1;
        eprintln!(
            "\n{}",
            style(format!("[{}/{}] Diagnostics", num, total)).bold()
        );
        let rows = run_bench(&avail, |srv, on_progress| {
            bench_diagnostics(
                srv,
                &root,
                &cwd,
                &bench_sol,
                index_timeout,
                w,
                n,
                on_progress,
            )
        });
        all_results.push(("Diagnostics", rows));
        let p = save_json(
            &all_results,
            &versions,
            &avail,
            n,
            w,
            &timeout,
            &index_timeout,
            &project,
            bench_file_rel,
            target_line,
            target_col,
            &partial_dir,
        );
        eprintln!("  {} {}", style("saved").dim(), style(&p).dim());
    }

    // ── all LSP method benchmarks (definition, declaration, hover, etc.) ─

    for (cmd, display_name, method, params_fn) in &method_benchmarks {
        if benchmarks.contains(cmd) {
            num += 1;
            eprintln!(
                "\n{}",
                style(format!("[{}/{}] {}", num, total, display_name)).bold()
            );
            let rows = run_bench(&avail, |srv, on_progress| {
                bench_lsp_method(
                    srv,
                    &root,
                    &cwd,
                    &bench_sol,
                    method,
                    *params_fn,
                    index_timeout,
                    timeout,
                    w,
                    n,
                    on_progress,
                )
            });
            all_results.push((display_name, rows));
            let p = save_json(
                &all_results,
                &versions,
                &avail,
                n,
                w,
                &timeout,
                &index_timeout,
                &project,
                bench_file_rel,
                target_line,
                target_col,
                &partial_dir,
            );
            eprintln!("  {} {}", style("saved").dim(), style(&p).dim());
        }
    }

    // ── Final output ─────────────────────────────────────────────────────

    if !all_results.is_empty() {
        let path = save_json(
            &all_results,
            &versions,
            &avail,
            n,
            w,
            &timeout,
            &index_timeout,
            &project,
            bench_file_rel,
            target_line,
            target_col,
            &output_dir,
        );
        eprintln!("\n  {} {}", style("->").green().bold(), path);

        // Clean up partial saves — the final snapshot has everything
        let _ = std::fs::remove_dir_all(&partial_dir);
    }
}
