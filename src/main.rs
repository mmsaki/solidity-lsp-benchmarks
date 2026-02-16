use clap::{Parser, Subcommand};
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
    /// Output path for the generated report. Omit to skip report generation.
    #[serde(default)]
    report: Option<String>,
    /// Report style: "delta" (default), "readme", or "analysis".
    #[serde(default = "default_report_style")]
    report_style: String,
    #[serde(
        default = "default_response_limit",
        deserialize_with = "deserialize_response_limit",
        rename = "response"
    )]
    response_limit: usize,
    /// Optional trigger character for completion requests (e.g. ".").
    /// When set, the completion request includes a `context` with
    /// `triggerKind: 2` (TriggerCharacter) and the given character.
    #[serde(default)]
    trigger_character: Option<String>,
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
    /// Git ref (branch, tag, or SHA) to checkout and build from.
    #[serde(default)]
    commit: Option<String>,
    /// Path to the git repo to build from. Required when `commit` is set.
    #[serde(default)]
    repo: Option<String>,
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
fn default_report_style() -> String {
    "delta".to_string()
}
fn default_response_limit() -> usize {
    80
}

/// Deserialize `response` field: accepts "full" or a number.
/// - "full" → 0 (no limit)
/// - number → truncate to that many chars
/// - omitted/null → 80 (default)
fn deserialize_response_limit<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val: serde_yaml::Value = serde::Deserialize::deserialize(deserializer)?;
    match &val {
        serde_yaml::Value::String(s) if s == "full" => Ok(0),
        serde_yaml::Value::String(_) => Err(serde::de::Error::custom(
            "response must be \"full\" or a number",
        )),
        serde_yaml::Value::Number(n) => {
            if let Some(v) = n.as_u64() {
                Ok(v as usize)
            } else {
                Err(serde::de::Error::custom(
                    "response must be \"full\" or a positive number",
                ))
            }
        }
        serde_yaml::Value::Null => Ok(80),
        _ => Err(serde::de::Error::custom(
            "response must be \"full\" or a number",
        )),
    }
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
                // if count > 0 {
                return Ok(DiagnosticsInfo { message: last_msg });
                // }
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
                    },
                    "workspace": {
                        "symbol": { "dynamicRegistration": false }
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
    // Absolute path — just check file exists and is executable
    if cmd.starts_with('/') {
        return Path::new(cmd).exists();
    }
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

/// Checkout a git ref in the given repo and `cargo build --release`.
/// Returns the absolute path to the built binary.
fn build_from_commit(repo_path: &str, commit: &str, bin_name: &str) -> Result<String, String> {
    let repo = PathBuf::from(repo_path);
    if !repo.exists() {
        return Err(format!("repo directory not found: {}", repo_path));
    }

    // Save current HEAD so we can restore later
    let head_out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("git rev-parse failed: {}", e))?;
    let original_ref = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
    // If detached, save the SHA instead
    let original_ref = if original_ref == "HEAD" {
        let sha_out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .stdout(Stdio::piped())
            .output()
            .map_err(|e| format!("git rev-parse HEAD failed: {}", e))?;
        String::from_utf8_lossy(&sha_out.stdout).trim().to_string()
    } else {
        original_ref
    };

    eprintln!(
        "  {} checkout {} in {}",
        style("build").cyan(),
        style(commit).bold(),
        repo_path
    );

    // Checkout the requested ref
    let checkout = Command::new("git")
        .args(["checkout", commit])
        .current_dir(&repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("git checkout failed: {}", e))?;
    if !checkout.success() {
        return Err(format!("git checkout {} failed", commit));
    }

    // Build
    eprintln!("  {} cargo build --release", style("build").cyan());
    let build = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("cargo build failed: {}", e))?;
    if !build.success() {
        // Restore original ref before returning error
        let _ = Command::new("git")
            .args(["checkout", &original_ref])
            .current_dir(&repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        return Err(format!("cargo build --release failed for {}", commit));
    }

    // Restore original ref
    let _ = Command::new("git")
        .args(["checkout", &original_ref])
        .current_dir(&repo)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let binary = repo.join("target/release").join(bin_name);
    if !binary.exists() {
        return Err(format!("built binary not found: {}", binary.display()));
    }
    Ok(binary.to_string_lossy().to_string())
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

fn response_summary(resp: &Value, _max_chars: usize) -> Value {
    if let Some(err) = resp.get("error") {
        json!({
            "error": err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        })
    } else if let Some(r) = resp.get("result").or_else(|| resp.get("params")) {
        r.clone()
    } else {
        Value::Null
    }
}

// ── Memory measurement ──────────────────────────────────────────────────────

/// Get the resident set size (RSS) of a process in kilobytes.
/// Uses `ps` on macOS/Linux. Returns None if measurement fails.
fn get_rss(pid: u32) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim().parse::<u64>().ok()
}

// ── Bench result per server ─────────────────────────────────────────────────

enum BenchResult {
    Ok {
        iterations: Vec<(f64, Value)>, // (ms, response json)
        rss_kb: Option<u64>,           // resident set size after indexing
    },
    Invalid {
        first_response: Value,
        rss_kb: Option<u64>,
    },
    Fail {
        error: String,
        rss_kb: Option<u64>,
    },
}

struct BenchRow {
    label: String,
    p50: f64,
    p95: f64,
    mean: f64,
    iterations: Vec<(f64, Value)>, // (ms, response json)
    rss_kb: Option<u64>,           // resident set size after indexing
    kind: u8,
    fail_msg: String,
    summary: Value,
}

impl BenchRow {
    fn to_json(&self) -> Value {
        match self.kind {
            0 => {
                let iter_json: Vec<Value> = self
                    .iterations
                    .iter()
                    .map(|(ms, resp)| {
                        json!({
                            "ms": (ms * 100.0).round() / 100.0,
                            "response": resp,
                        })
                    })
                    .collect();
                let mut obj = json!({
                    "server": self.label,
                    "status": "ok",
                    "p50_ms": (self.p50 * 100.0).round() / 100.0,
                    "p95_ms": (self.p95 * 100.0).round() / 100.0,
                    "mean_ms": (self.mean * 100.0).round() / 100.0,
                    "iterations": iter_json,
                    "response": self.summary,
                });
                if let Some(rss) = self.rss_kb {
                    obj["rss_kb"] = json!(rss);
                }
                obj
            }
            1 => {
                let mut obj = json!({
                    "server": self.label,
                    "status": "invalid",
                    "response": self.summary,
                });
                if let Some(rss) = self.rss_kb {
                    obj["rss_kb"] = json!(rss);
                }
                obj
            }
            _ => {
                let mut obj = json!({
                    "server": self.label,
                    "status": "fail",
                    "error": self.fail_msg,
                });
                if let Some(rss) = self.rss_kb {
                    obj["rss_kb"] = json!(rss);
                }
                obj
            }
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
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    w: usize,
    n: usize,
    on_progress: &dyn Fn(&str),
) -> BenchResult {
    let mut iterations = Vec::new();
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));
        let start = Instant::now();
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd) {
            Ok(c) => c,
            Err(e) => {
                return BenchResult::Fail {
                    error: e,
                    rss_kb: None,
                }
            }
        };
        if let Err(e) = c.initialize(root) {
            return BenchResult::Fail {
                error: e,
                rss_kb: None,
            };
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
        if i >= w {
            iterations.push((ms, json!("ok")));
        }
        c.kill();
    }
    BenchResult::Ok {
        iterations,
        rss_kb: None,
    }
}

/// Benchmark that spawns fresh each iteration, measures didOpen -> diagnostics.
fn bench_diagnostics(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    timeout: Duration,
    w: usize,
    n: usize,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
) -> BenchResult {
    let mut iterations = Vec::new();
    let mut peak_rss: Option<u64> = None;
    for i in 0..(w + n) {
        on_progress(&format!("{}  waiting for diagnostics", iter_msg(i, w, n)));
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd) {
            Ok(c) => c,
            Err(e) => {
                return BenchResult::Fail {
                    error: e,
                    rss_kb: None,
                }
            }
        };
        if let Err(e) = c.initialize(root) {
            return BenchResult::Fail {
                error: e,
                rss_kb: None,
            };
        }
        let start = Instant::now();
        if let Err(e) = c.open_file(target_file) {
            return BenchResult::Fail {
                error: e,
                rss_kb: None,
            };
        }
        match c.wait_for_valid_diagnostics(timeout) {
            Ok(diag_info) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                // Sample RSS after indexing, before kill
                if let Some(rss) = get_rss(c.child.id()) {
                    peak_rss = Some(peak_rss.map_or(rss, |prev: u64| prev.max(rss)));
                }
                on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
                if i >= w {
                    let summary = response_summary(&diag_info.message, response_limit);
                    iterations.push((ms, summary));
                }
            }
            Err(e) => {
                // Sample RSS even on timeout — server is still alive
                let rss = get_rss(c.child.id());
                return BenchResult::Fail {
                    error: e,
                    rss_kb: rss,
                };
            }
        }
        c.kill();
    }
    BenchResult::Ok {
        iterations,
        rss_kb: peak_rss,
    }
}

/// Benchmark an LSP method on a single persistent server session.
/// Spawns once, waits for diagnostics, then iterates the given method.
fn bench_lsp_method(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    method: &str,
    params_fn: &dyn Fn(&str) -> Value, // takes file_uri, returns params
    index_timeout: Duration,
    timeout: Duration,
    w: usize,
    n: usize,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd) {
        Ok(c) => c,
        Err(e) => {
            return BenchResult::Fail {
                error: e,
                rss_kb: None,
            }
        }
    };
    if let Err(e) = c.initialize(root) {
        let rss = get_rss(c.child.id());
        return BenchResult::Fail {
            error: e,
            rss_kb: rss,
        };
    }
    if let Err(e) = c.open_file(target_file) {
        let rss = get_rss(c.child.id());
        return BenchResult::Fail {
            error: e,
            rss_kb: rss,
        };
    }
    on_progress("waiting for diagnostics");
    match c.wait_for_valid_diagnostics(index_timeout) {
        Ok(_) => {}
        Err(e) => {
            // Sample RSS even on timeout — server is still alive
            let rss = get_rss(c.child.id());
            return BenchResult::Fail {
                error: format!("wait_for_diagnostics: {}", e),
                rss_kb: rss,
            };
        }
    }
    // Sample RSS after indexing
    let rss_kb = get_rss(c.child.id());

    let file_uri = uri(target_file);
    let mut iterations = Vec::new();
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));
        let start = Instant::now();
        if let Err(e) = c.send(method, params_fn(&file_uri)) {
            return BenchResult::Fail { error: e, rss_kb };
        }
        match c.read_response(timeout) {
            Ok(resp) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
                if i >= w {
                    if !is_valid_response(&resp) {
                        return BenchResult::Invalid {
                            first_response: resp,
                            rss_kb,
                        };
                    }
                    let summary = response_summary(&resp, response_limit);
                    iterations.push((ms, summary));
                }
            }
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        }
    }
    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

/// Run a benchmark across all servers, showing a spinner per server.
fn run_bench<F>(servers: &[&ServerConfig], response_limit: usize, f: F) -> Vec<BenchRow>
where
    F: Fn(&ServerConfig, &dyn Fn(&str)) -> BenchResult,
{
    let mut rows = Vec::new();
    for srv in servers {
        let pb = spinner(&srv.label);
        let on_progress = |msg: &str| pb.set_message(msg.to_string());
        match f(srv, &on_progress) {
            BenchResult::Ok { iterations, rss_kb } => {
                let mut latencies: Vec<f64> = iterations.iter().map(|(ms, _)| *ms).collect();
                let (p50, p95, mean) = stats(&mut latencies);
                let summary = iterations
                    .first()
                    .map(|(_, s)| s.clone())
                    .unwrap_or(Value::Null);
                finish_pass(&pb, mean, p50, p95);
                rows.push(BenchRow {
                    label: srv.label.to_string(),
                    p50,
                    p95,
                    mean,
                    iterations,
                    rss_kb,
                    summary,
                    kind: 0,
                    fail_msg: String::new(),
                });
            }
            BenchResult::Invalid {
                first_response,
                rss_kb,
            } => {
                let summary = response_summary(&first_response, response_limit);
                finish_fail(&pb, "invalid response");
                rows.push(BenchRow {
                    label: srv.label.to_string(),
                    p50: 0.0,
                    p95: 0.0,
                    mean: 0.0,
                    iterations: vec![],
                    rss_kb,
                    summary,
                    kind: 1,
                    fail_msg: String::new(),
                });
            }
            BenchResult::Fail { error, rss_kb } => {
                finish_fail(&pb, &error);
                rows.push(BenchRow {
                    label: srv.label.to_string(),
                    p50: 0.0,
                    p95: 0.0,
                    mean: 0.0,
                    iterations: vec![],
                    rss_kb,
                    summary: Value::Null,
                    kind: 2,
                    fail_msg: error,
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
    "initialize",
    "textDocument/diagnostic",
    "textDocument/definition",
    "textDocument/declaration",
    "textDocument/typeDefinition",
    "textDocument/implementation",
    "textDocument/hover",
    "textDocument/references",
    "textDocument/completion",
    "textDocument/signatureHelp",
    "textDocument/rename",
    "textDocument/prepareRename",
    "textDocument/documentSymbol",
    "textDocument/documentLink",
    "textDocument/formatting",
    "textDocument/foldingRange",
    "textDocument/selectionRange",
    "textDocument/codeLens",
    "textDocument/inlayHint",
    "textDocument/semanticTokens/full",
    "textDocument/documentColor",
    "workspace/symbol",
];

// ── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "lsp-bench", version = env!("LONG_VERSION"))]
#[command(about = "Benchmark framework for LSP servers")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Config file path
    #[arg(short, long, default_value = "benchmark.yaml")]
    config: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a benchmark.yaml template
    Init {
        /// Output path for the generated config
        #[arg(short, long, default_value = "benchmark.yaml")]
        config: Option<String>,
    },
}

const EXAMPLE_CONFIG: &str = include_str!("../examples/benchmark.template.yaml");

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
    eprintln!("  lsp-bench");
}

fn main() {
    let cli = Cli::parse();

    // Handle init subcommand before loading config
    if let Some(Commands::Init { config }) = cli.command {
        let path = config.as_deref().unwrap_or(&cli.config);
        init_config(path);
        std::process::exit(0);
    }

    // Load config
    let mut cfg = load_config(&cli.config);

    let n = cfg.iterations;
    let w = cfg.warmup;
    let timeout = Duration::from_secs(cfg.timeout);
    let index_timeout = Duration::from_secs(cfg.index_timeout);
    let target_line = cfg.line;
    let target_col = cfg.col;
    let trigger_character = cfg.trigger_character;
    let output_dir = cfg.output;
    let report_path = cfg.report;
    let report_style = cfg.report_style;
    let response_limit = cfg.response_limit;
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
            eprintln!(
                "Error: unknown benchmark '{}'. See DOCS.md for valid names.",
                b
            );
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

    eprintln!("  {} {}", style("config").dim(), cli.config);
    eprintln!(
        "  {} {}  (line {}, col {})",
        style("file").dim(),
        bench_file_rel,
        target_line,
        target_col
    );

    // Build from commit if configured — mutates cmd to the built binary path
    for srv in &mut cfg.servers {
        if let Some(ref commit) = srv.commit {
            let repo_path = srv.repo.as_deref().unwrap_or_else(|| {
                eprintln!("Error: server '{}' has commit but no repo path", srv.label);
                std::process::exit(1);
            });
            match build_from_commit(repo_path, commit, &srv.cmd) {
                Ok(bin_path) => {
                    eprintln!("  {} {} -> {}", style("built").green(), srv.label, bin_path);
                    srv.cmd = bin_path;
                }
                Err(e) => {
                    eprintln!("  {} {} -- {}", style("build failed").red(), srv.label, e);
                    std::process::exit(1);
                }
            }
        }
    }

    let avail: Vec<&ServerConfig> = cfg
        .servers
        .iter()
        .filter(|s| {
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
    let symbol_params = |_file_uri: &str| -> Value { json!({ "query": "" }) };
    let rename_params = |file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": target_line, "character": target_col },
            "newName": "__lsp_bench_rename__",
        })
    };
    let completion_params = |file_uri: &str| -> Value {
        let mut params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": target_line, "character": target_col },
        });
        if let Some(ref tc) = trigger_character {
            params["context"] = json!({
                "triggerKind": 2,
                "triggerCharacter": tc,
            });
        }
        params
    };
    let formatting_params = |file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        })
    };
    let selection_range_params = |file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "positions": [{ "line": target_line, "character": target_col }],
        })
    };
    let inlay_hint_params = |file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 9999, "character": 0 },
            },
        })
    };
    let semantic_tokens_params =
        |file_uri: &str| -> Value { json!({ "textDocument": { "uri": file_uri } }) };

    // (config_key, lsp_method, params_fn)
    // config_key and lsp_method are now the same — the official LSP method name
    let method_benchmarks: Vec<(&str, &str, &dyn Fn(&str) -> Value)> = vec![
        (
            "textDocument/definition",
            "textDocument/definition",
            &position_params,
        ),
        (
            "textDocument/declaration",
            "textDocument/declaration",
            &position_params,
        ),
        (
            "textDocument/typeDefinition",
            "textDocument/typeDefinition",
            &position_params,
        ),
        (
            "textDocument/implementation",
            "textDocument/implementation",
            &position_params,
        ),
        ("textDocument/hover", "textDocument/hover", &position_params),
        (
            "textDocument/references",
            "textDocument/references",
            &ref_params,
        ),
        (
            "textDocument/completion",
            "textDocument/completion",
            &completion_params,
        ),
        (
            "textDocument/signatureHelp",
            "textDocument/signatureHelp",
            &position_params,
        ),
        ("textDocument/rename", "textDocument/rename", &rename_params),
        (
            "textDocument/prepareRename",
            "textDocument/prepareRename",
            &position_params,
        ),
        (
            "textDocument/documentSymbol",
            "textDocument/documentSymbol",
            &doc_params,
        ),
        (
            "textDocument/documentLink",
            "textDocument/documentLink",
            &doc_params,
        ),
        (
            "textDocument/formatting",
            "textDocument/formatting",
            &formatting_params,
        ),
        (
            "textDocument/foldingRange",
            "textDocument/foldingRange",
            &doc_params,
        ),
        (
            "textDocument/selectionRange",
            "textDocument/selectionRange",
            &selection_range_params,
        ),
        (
            "textDocument/codeLens",
            "textDocument/codeLens",
            &doc_params,
        ),
        (
            "textDocument/inlayHint",
            "textDocument/inlayHint",
            &inlay_hint_params,
        ),
        (
            "textDocument/semanticTokens/full",
            "textDocument/semanticTokens/full",
            &semantic_tokens_params,
        ),
        (
            "textDocument/documentColor",
            "textDocument/documentColor",
            &doc_params,
        ),
        ("workspace/symbol", "workspace/symbol", &symbol_params),
    ];

    // ── spawn ────────────────────────────────────────────────────────────

    if benchmarks.contains(&"initialize") {
        num += 1;
        eprintln!(
            "\n{}",
            style(format!("[{}/{}] initialize", num, total)).bold()
        );
        let rows = run_bench(&avail, response_limit, |srv, on_progress| {
            bench_spawn(srv, &root, &cwd, w, n, on_progress)
        });
        all_results.push(("initialize", rows));
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

    if benchmarks.contains(&"textDocument/diagnostic") {
        num += 1;
        eprintln!(
            "\n{}",
            style(format!("[{}/{}] textDocument/diagnostic", num, total)).bold()
        );
        let rows = run_bench(&avail, response_limit, |srv, on_progress| {
            bench_diagnostics(
                srv,
                &root,
                &cwd,
                &bench_sol,
                index_timeout,
                w,
                n,
                response_limit,
                on_progress,
            )
        });
        all_results.push(("textDocument/diagnostic", rows));
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

    // ── all LSP method benchmarks ───────────────────────────────────────

    for (method, lsp_method, params_fn) in &method_benchmarks {
        if benchmarks.contains(method) {
            num += 1;
            eprintln!(
                "\n{}",
                style(format!("[{}/{}] {}", num, total, method)).bold()
            );
            let rows = run_bench(&avail, response_limit, |srv, on_progress| {
                bench_lsp_method(
                    srv,
                    &root,
                    &cwd,
                    &bench_sol,
                    lsp_method,
                    *params_fn,
                    index_timeout,
                    timeout,
                    w,
                    n,
                    response_limit,
                    on_progress,
                )
            });
            all_results.push((method, rows));
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

        // Generate report if configured
        if let Some(ref report_out) = report_path {
            let exe = std::env::current_exe().unwrap();
            let bin_dir = exe.parent().unwrap();
            let (bin_name, args): (&str, Vec<&str>) = match report_style.as_str() {
                "readme" => ("gen-readme", vec!["--quiet", &path, report_out]),
                "analysis" => ("gen-analysis", vec!["--quiet", &path, "-o", report_out]),
                "delta" => ("gen-delta", vec!["--quiet", &path, "-o", report_out]),
                other => {
                    eprintln!(
                        "  {} unknown report_style '{}' (expected: delta, readme, analysis)",
                        style("warn").yellow(),
                        other
                    );
                    return;
                }
            };
            let bin = bin_dir.join(bin_name);
            eprintln!(
                "  {} {} -> {}",
                style("report").dim(),
                report_style,
                report_out
            );
            match std::process::Command::new(&bin).args(&args).status() {
                Ok(s) if s.success() => {}
                Ok(s) => eprintln!(
                    "  {} {} exited with {}",
                    style("warn").yellow(),
                    bin_name,
                    s
                ),
                Err(e) => eprintln!(
                    "  {} {} not found: {} (run cargo build --release)",
                    style("warn").yellow(),
                    bin_name,
                    e
                ),
            }
        }
    }
}
