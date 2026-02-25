use clap::{Parser, Subcommand};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

// ── Server Registry ─────────────────────────────────────────────────────────

/// A version entry in the server registry. Overrides the parent server's fields.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct ServerVersion {
    #[serde(default)]
    cmd: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    repo: Option<String>,
}

/// A server definition in the registry, with optional named versions.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ServerRegistryEntry {
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    link: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    versions: HashMap<String, ServerVersion>,
}

/// Server registry: name → definition. Loaded from servers.yaml.
type ServerRegistry = HashMap<String, ServerRegistryEntry>;

/// Load a server registry file. Returns empty map if file doesn't exist.
fn load_server_registry(path: &Path) -> ServerRegistry {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "  {} parsing {}: {}",
                style("warn").yellow(),
                path.display(),
                e
            );
            HashMap::new()
        }),
        Err(_) => HashMap::new(),
    }
}

/// Resolve a server reference like "mmsaki" or "mmsaki@v0.1.20" into a ServerConfig.
/// Falls back to treating the name as a cmd if not found in the registry.
fn resolve_server(name: &str, registry: &ServerRegistry) -> ServerConfig {
    let (base_name, version) = match name.split_once('@') {
        Some((b, v)) => (b, Some(v)),
        None => (name, None),
    };

    if let Some(entry) = registry.get(base_name) {
        let label = match version {
            Some(v) => format!("{} {}", base_name, v),
            None => base_name.to_string(),
        };

        // Start with base entry values
        let mut cmd = entry.cmd.clone();
        let mut args = entry.args.clone();
        let mut link = entry.link.clone();
        let mut description = entry.description.clone();
        let mut commit = entry.commit.clone();
        let mut repo = entry.repo.clone();

        // If a version is specified, override with version-specific values
        if let Some(v) = version {
            if let Some(ver) = entry.versions.get(v) {
                if let Some(ref c) = ver.cmd {
                    cmd = c.clone();
                }
                if let Some(ref a) = ver.args {
                    args = a.clone();
                }
                if let Some(ref l) = ver.link {
                    link = l.clone();
                }
                if let Some(ref d) = ver.description {
                    description = d.clone();
                }
                if let Some(ref c) = ver.commit {
                    commit = Some(c.clone());
                }
                if let Some(ref r) = ver.repo {
                    repo = Some(r.clone());
                }
            } else {
                eprintln!(
                    "  {} version '{}' not found for server '{}', using base",
                    style("warn").yellow(),
                    v,
                    base_name
                );
            }
        }

        ServerConfig {
            label,
            cmd,
            args,
            link,
            description,
            commit,
            repo,
        }
    } else {
        // Not in registry — treat the name as both label and cmd
        ServerConfig {
            label: name.to_string(),
            cmd: name.to_string(),
            args: Vec::new(),
            link: String::new(),
            description: String::new(),
            commit: None,
            repo: None,
        }
    }
}

/// Find a servers.yaml file by checking: explicit path, next to config, parent dirs.
fn discover_servers_file(config_path: &str, explicit: Option<&str>) -> Option<PathBuf> {
    // 1. Explicit path from config or CLI
    if let Some(p) = explicit {
        let path = Path::new(config_path)
            .parent()
            .unwrap_or(Path::new("."))
            .join(p);
        if path.exists() {
            return Some(path);
        }
        // Also try as absolute/relative to cwd
        let abs = Path::new(p);
        if abs.exists() {
            return Some(abs.to_path_buf());
        }
    }

    // 2. Look next to the config file
    if let Some(dir) = Path::new(config_path).parent() {
        let candidate = dir.join("servers.yaml");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 3. Walk parent directories
    let mut dir = Path::new(config_path)
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());
    while let Some(d) = dir {
        let candidate = d.join("servers.yaml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }

    None
}

// ── Config ──────────────────────────────────────────────────────────────────

/// Expected result for a goto-definition (or similar) response.
///
/// ```yaml
/// expect:
///   file: SafeCast.sol
///   line: 39
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct CompletionItemExpect {
    /// Expected label for a completion item.
    #[serde(default)]
    label: Option<String>,
    /// Substring that must appear in completion item detail.
    #[serde(default, rename = "detailContains")]
    detail_contains: Option<String>,
    /// Prefix that completion item sortText must start with.
    #[serde(default, rename = "sortTextPrefix")]
    sort_text_prefix: Option<String>,
    /// Whether completion item should include additionalTextEdits.
    #[serde(default, rename = "hasAdditionalTextEdits")]
    has_additional_text_edits: Option<bool>,
    /// Substring that must appear in at least one additionalTextEdit.newText.
    #[serde(default, rename = "additionalTextEditsContain")]
    additional_text_edits_contain: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct ExpectConfig {
    /// Expected filename suffix (e.g. "SafeCast.sol"). Matches if the response
    /// URI ends with this string.
    #[serde(default)]
    file: Option<String>,
    /// Expected 0-based line number in the response.
    #[serde(default)]
    line: Option<u32>,
    /// Expected number of items in an array response (e.g. references count).
    #[serde(default)]
    count: Option<usize>,
    /// Minimum number of items in an array response. Passes if actual >= min_count.
    #[serde(default, rename = "minCount")]
    min_count: Option<usize>,
    /// Completion-item predicates that must match at least one completion item.
    #[serde(default, rename = "containsItems")]
    contains_items: Vec<CompletionItemExpect>,
    /// Completion-item predicates that must not match any completion item.
    #[serde(default, rename = "absentItems")]
    absent_items: Vec<CompletionItemExpect>,
}

/// A file snapshot sent via didChange, with its own cursor position.
///
/// ```yaml
/// didChange:
///   - file: src/libraries/Pool.v2.sol
///     line: 107
///     col: 15
///     expect:
///       file: SafeCast.sol
///       line: 39
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
struct FileSnapshot {
    /// Path to the snapshot file (relative to project).
    file: String,
    /// 0-based line for the benchmark request after this snapshot.
    line: u32,
    /// 0-based column for the benchmark request after this snapshot.
    col: u32,
    /// Expected response (for --verify mode).
    #[serde(default)]
    expect: Option<ExpectConfig>,
}

/// A file to open via didOpen, then re-request on the original file.
///
/// ```yaml
/// didOpen:
///   - file: src/PoolManager.sol
///     expect:
///       minCount: 50
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
struct DidOpenStep {
    /// Path to the file to open (relative to project).
    file: String,
    /// Optional line override for the re-request on the original file.
    /// If omitted, uses the method's line.
    #[serde(default)]
    line: Option<u32>,
    /// Optional col override for the re-request on the original file.
    /// If omitted, uses the method's col.
    #[serde(default)]
    col: Option<u32>,
    /// Expected response after opening this file (for --verify mode).
    #[serde(default)]
    expect: Option<ExpectConfig>,
}

/// A rename step in a multi-rename sequence for workspace/willRenameFiles.
///
/// Each step renames a file and validates the result. The bench harness
/// executes the full LSP lifecycle for each step:
///   willRenameFiles → apply edits → rename on disk → didRenameFiles → wait for re-index
///
/// ```yaml
/// renameSteps:
///   - file: A.sol            # file to rename (relative to project root)
///     newName: AA.sol         # new filename
///     expect:
///       count: 1              # expect 1 file with edits
///   - file: AA.sol            # rename back
///     newName: A.sol
///     expect:
///       count: 1
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
struct RenameStep {
    /// File to rename (relative to project root).
    file: String,
    /// New filename (just the filename, not a path).
    #[serde(rename = "newName")]
    new_name: String,
    /// Expected response (for --verify mode).
    #[serde(default)]
    expect: Option<ExpectConfig>,
}

/// A create step in a lifecycle sequence for workspace/willCreateFiles.
///
/// ```yaml
/// createSteps:
///   - file: test/NewFile.sol
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
struct CreateStep {
    /// File to create (relative to project root).
    file: String,
    /// Expected response (for --verify mode).
    #[serde(default)]
    expect: Option<ExpectConfig>,
}

/// A delete step in a lifecycle sequence for workspace/willDeleteFiles.
///
/// ```yaml
/// deleteSteps:
///   - file: src/Foo.sol
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
struct DeleteStep {
    /// File to delete (relative to project root).
    file: String,
    /// Expected response (for --verify mode).
    #[serde(default)]
    expect: Option<ExpectConfig>,
}

/// Per-method configuration overrides.
///
/// ```yaml
/// methods:
///   textDocument/completion:
///     line: 105
///     col: 28
///     trigger: "."
///   textDocument/definition:
///     line: 50
///     col: 10
///     didChange:
///       - file: src/libraries/Pool.v2.sol
///         line: 107
///         col: 15
///       - file: src/libraries/Pool.v3.sol
///         line: 112
///         col: 15
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct MethodConfig {
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    col: Option<u32>,
    /// Start line for range-based requests (e.g. semanticTokens/range).
    #[serde(default, rename = "startLine")]
    start_line: Option<u32>,
    /// Start column for range-based requests (e.g. semanticTokens/range).
    #[serde(default, rename = "startCol")]
    start_col: Option<u32>,
    /// Trigger character (e.g. ".") — only used for textDocument/completion.
    #[serde(default)]
    trigger: Option<String>,
    /// New name for textDocument/rename (defaults to "__lsp_bench_rename__").
    #[serde(default, rename = "newName")]
    new_name: Option<String>,
    /// Override the target file for this method. For workspace/willRenameFiles,
    /// this sets the oldUri instead of using the top-level `file`. Path is
    /// relative to the project root (e.g. "src/libraries/Pool.sol").
    #[serde(default)]
    file: Option<String>,
    /// Expected response for the base request (no didChange). Used by --verify.
    #[serde(default)]
    expect: Option<ExpectConfig>,
    /// File snapshots sent sequentially via didChange. Each snapshot is one
    /// iteration: send content, run one request at that snapshot's line/col.
    #[serde(default, rename = "didChange")]
    did_change: Vec<FileSnapshot>,
    /// Files to open sequentially via didOpen. After each open (and waiting for
    /// diagnostics), the benchmark request is re-sent on the original file.
    /// Used to test cross-file features like forward references: open file A,
    /// get references, open file B (which imports A), get references again —
    /// the count should grow as the server discovers more cross-file references.
    #[serde(default, rename = "didOpen")]
    did_open: Vec<DidOpenStep>,
    /// Cold-start mode: spawn a fresh server per iteration and measure the full
    /// end-to-end time from didOpen through diagnostics through the method response.
    /// This captures what the user actually feels — compilation + request latency.
    #[serde(default)]
    cold: bool,
    /// Sequential rename steps for workspace/willRenameFiles. Each step is a
    /// full rename lifecycle: willRenameFiles → apply edits on disk → didRenameFiles
    /// → wait for re-index. This tests the real-world multi-rename scenario where
    /// each rename mutates state and the next rename must work on the new state.
    #[serde(default, rename = "renameSteps")]
    rename_steps: Vec<RenameStep>,
    /// Sequential create steps for workspace/willCreateFiles. Each step is a
    /// full create lifecycle: willCreateFiles → apply edits on disk (if any)
    /// → create file on disk → didCreateFiles.
    #[serde(default, rename = "createSteps")]
    create_steps: Vec<CreateStep>,
    /// Sequential delete steps for workspace/willDeleteFiles. Each step is a
    /// full delete lifecycle: willDeleteFiles → apply edits on disk (if any)
    /// → delete file on disk → didDeleteFiles.
    #[serde(default, rename = "deleteSteps")]
    delete_steps: Vec<DeleteStep>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default = "default_project")]
    project: String,
    #[serde(default)]
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
    #[serde(default)]
    exclude: Vec<String>,
    /// Output path for the generated report. Omit to skip report generation.
    #[serde(default)]
    report: Option<String>,
    /// Report style (deprecated — only "competition" format is supported).
    #[serde(default = "default_report_style")]
    report_style: String,
    #[serde(
        default = "default_response_limit",
        deserialize_with = "deserialize_response_limit",
        rename = "response"
    )]
    response_limit: usize,
    /// Deprecated: use methods.textDocument/completion.trigger instead.
    #[serde(default)]
    trigger_character: Option<String>,
    /// Per-method position and trigger overrides.
    #[serde(default)]
    methods: HashMap<String, MethodConfig>,
    /// Servers to benchmark. Accepts inline definitions (objects with label/cmd)
    /// or string references resolved against the server registry ("mmsaki",
    /// "solc", "mmsaki@v0.1.20"). Defaults to ["mmsaki"] if omitted.
    #[serde(
        default = "default_servers",
        deserialize_with = "deserialize_servers_opt"
    )]
    servers: Vec<ServerConfig>,
    /// Path to a servers.yaml registry file. Auto-discovered next to the config
    /// file if not specified.
    #[serde(default, rename = "servers_file")]
    servers_file: Option<String>,
    /// Sub-configs to run sequentially. The parent's settings are merged as
    /// defaults into each sub-config (sub-config values win).
    #[serde(default)]
    include: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

fn default_project() -> String {
    ".".to_string()
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
fn default_servers() -> Vec<ServerConfig> {
    vec![ServerConfig {
        label: "mmsaki".to_string(),
        cmd: "solidity-language-server".to_string(),
        args: Vec::new(),
        link: "https://github.com/mmsaki/solidity-language-server".to_string(),
        description: String::new(),
        commit: None,
        repo: None,
    }]
}

/// Deserialize `servers` field: accepts an array of objects (inline ServerConfig)
/// or strings (registry references like "mmsaki", "solc", "mmsaki@v0.1.20").
/// String entries are stored with label=string, cmd="" as placeholders —
/// they get resolved against the server registry after loading.
/// Returns default servers if the field is missing or null.
fn deserialize_servers_opt<'de, D>(deserializer: D) -> Result<Vec<ServerConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val: serde_yaml::Value = serde::Deserialize::deserialize(deserializer)?;
    if val.is_null() {
        return Ok(default_servers());
    }
    let items = match val.as_sequence() {
        Some(s) => s,
        None => {
            return Err(serde::de::Error::custom(
                "servers must be an array of strings or objects",
            ))
        }
    };
    let mut servers = Vec::new();
    for item in items {
        match item {
            serde_yaml::Value::String(name) => {
                // String reference — placeholder, resolved later against registry
                servers.push(ServerConfig {
                    label: name.clone(),
                    cmd: String::new(), // empty = needs registry resolution
                    args: Vec::new(),
                    link: String::new(),
                    description: String::new(),
                    commit: None,
                    repo: None,
                });
            }
            serde_yaml::Value::Mapping(_) => {
                // Inline object — deserialize as ServerConfig directly
                let cfg: ServerConfig =
                    serde_yaml::from_value(item.clone()).map_err(serde::de::Error::custom)?;
                servers.push(cfg);
            }
            _ => {
                return Err(serde::de::Error::custom(
                    "servers entries must be strings or objects",
                ));
            }
        }
    }
    Ok(servers)
}

/// Resolve string-reference servers against the registry.
/// Servers with an empty `cmd` are looked up in the registry.
/// Servers with a non-empty `cmd` (inline definitions) are left as-is.
fn resolve_servers(servers: &mut Vec<ServerConfig>, registry: &ServerRegistry) {
    for i in 0..servers.len() {
        if servers[i].cmd.is_empty() {
            servers[i] = resolve_server(&servers[i].label, registry);
        }
    }
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

/// Check if a config has `include` entries (either via raw YAML or parsed Config).
/// Returns Some((resolved paths, parent defaults YAML)) if found, None otherwise.
/// Parent defaults are all keys in the parent config except `include`.
fn check_include(path: &str) -> Option<(Vec<String>, serde_yaml::Value)> {
    let content = std::fs::read_to_string(path).ok()?;
    let raw: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    let items = raw.get("include")?.as_sequence()?;
    if items.is_empty() {
        return None;
    }
    let parent_dir = Path::new(path).parent().unwrap_or(Path::new("."));
    let paths: Vec<String> = items
        .iter()
        .filter_map(|v| {
            v.as_str()
                .map(|s| parent_dir.join(s).to_string_lossy().to_string())
        })
        .collect();
    // Build defaults: everything in the parent except `include`
    let mut defaults = raw.clone();
    if let serde_yaml::Value::Mapping(ref mut m) = defaults {
        m.remove(&serde_yaml::Value::String("include".to_string()));
    }
    Some((paths, defaults))
}

/// Merge parent defaults with a sub-config. Sub-config keys win.
/// Only top-level keys are merged (no deep merge).
fn merge_configs(defaults: &serde_yaml::Value, child_path: &str) -> Option<serde_yaml::Value> {
    let content = std::fs::read_to_string(child_path).ok()?;
    let child: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    let mut merged = defaults.clone();
    if let (serde_yaml::Value::Mapping(ref mut base), serde_yaml::Value::Mapping(ref overrides)) =
        (&mut merged, &child)
    {
        for (k, v) in overrides {
            base.insert(k.clone(), v.clone());
        }
    }
    Some(merged)
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
    writer: Option<std::process::ChildStdin>,
    id: i64,
    logs: Arc<Mutex<Vec<String>>>,
}

struct DiagnosticsInfo {
    message: Value,
}

fn reader_thread(
    stdout: std::process::ChildStdout,
    tx: mpsc::Sender<Value>,
    logs: Arc<Mutex<Vec<String>>>,
) {
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
            // Capture window/logMessage notifications
            if msg.get("method").and_then(|m| m.as_str()) == Some("window/logMessage") {
                if let Some(text) = msg
                    .get("params")
                    .and_then(|p| p.get("message"))
                    .and_then(|m| m.as_str())
                {
                    if let Ok(mut l) = logs.lock() {
                        l.push(text.to_string());
                    }
                }
            }
            if tx.send(msg).is_err() {
                return;
            }
        }
    }
}

impl LspClient {
    fn spawn(cmd: &str, args: &[String], cwd: &Path, verbose: bool) -> Result<Self, String> {
        let abs_cmd = if cmd.starts_with("..") || cmd.starts_with("./") {
            std::fs::canonicalize(cmd)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| cmd.to_string())
        } else {
            cmd.to_string()
        };
        let stderr_cfg = if verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        };
        let mut child = Command::new(&abs_cmd)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr_cfg)
            .spawn()
            .map_err(|e| format!("{}: {}", cmd, e))?;
        let writer = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let logs_clone = logs.clone();
        std::thread::spawn(move || reader_thread(stdout, tx, logs_clone));
        Ok(Self {
            child,
            rx,
            writer: Some(writer),
            id: 1,
            logs,
        })
    }

    fn send(&mut self, method: &str, params: Value) -> Result<i64, String> {
        let id = self.id;
        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.id += 1;
        let body = serde_json::to_string(&msg).unwrap();
        let w = self.writer.as_mut().ok_or("stdin closed")?;
        write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn notif(&mut self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","method":method,"params":params});
        let body = serde_json::to_string(&msg).unwrap();
        let w = self.writer.as_mut().ok_or("stdin closed")?;
        write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }

    /// Send a JSON-RPC response to a server-initiated request.
    fn respond(&mut self, id: Value, result: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","id":id,"result":result});
        let body = serde_json::to_string(&msg).unwrap();
        let w = self.writer.as_mut().ok_or("stdin closed")?;
        write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }

    fn recv(&mut self, timeout: Duration) -> Result<Value, String> {
        self.rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => "timeout".to_string(),
            mpsc::RecvTimeoutError::Disconnected => "EOF".to_string(),
        })
    }

    fn read_response(&mut self, expected_id: i64, timeout: Duration) -> Result<Value, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("timeout".into());
            }
            let msg = self.recv(remaining)?;
            // Match by id — skip server-to-client requests and notifications
            if msg.get("id").and_then(|v| v.as_i64()) == Some(expected_id) {
                return Ok(msg);
            }
        }
    }

    /// Wait for a `$/progress` end notification (e.g. background project
    /// indexing). Drains all notifications until it sees one with
    /// `value.kind == "end"`, or until timeout. Auto-responds to
    /// `window/workDoneProgress/create` requests from the server.
    fn wait_for_progress_end(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            let msg = match self.recv(remaining) {
                Ok(m) => m,
                Err(_) => return,
            };
            // Auto-respond to server requests (e.g. window/workDoneProgress/create)
            if let Some(id) = msg.get("id").cloned() {
                if msg.get("method").is_some() {
                    let _ = self.respond(id, json!(null));
                    continue;
                }
            }
            if msg.get("method").and_then(|m| m.as_str()) == Some("$/progress") {
                let kind = msg
                    .get("params")
                    .and_then(|p| p.get("value"))
                    .and_then(|v| v.get("kind"))
                    .and_then(|k| k.as_str());
                if kind == Some("end") {
                    return;
                }
            }
        }
    }

    fn wait_for_valid_diagnostics(&mut self, timeout: Duration) -> Result<DiagnosticsInfo, String> {
        let start = Instant::now();
        let deadline = start + timeout;
        let mut _last_count = 0usize;
        let mut last_msg = json!(null);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return if _last_count > 0 {
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
                _last_count = count;
                last_msg = msg;
                // if count > 0 {
                return Ok(DiagnosticsInfo { message: last_msg });
                // }
            }
        }
    }

    fn initialize(&mut self, root: &str) -> Result<(), String> {
        let id = self.send(
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
                        "documentHighlight": { "dynamicRegistration": false },
                        "selectionRange": { "dynamicRegistration": false },
                    },
                    "window": {
                        "workDoneProgress": true
                    },
                    "workspace": {
                        "symbol": { "dynamicRegistration": false },
                        "fileOperations": {
                            "willRename": true
                        }
                    }
                },
            }),
        )?;
        self.read_response(id, Duration::from_secs(10))?;
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

    /// Send a full-document textDocument/didChange notification.
    fn did_change(&mut self, file_uri: &str, version: i32, text: &str) -> Result<(), String> {
        self.notif(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": file_uri, "version": version },
                "contentChanges": [{ "text": text }],
            }),
        )
    }

    /// Graceful LSP shutdown: send `shutdown` request, wait for response,
    /// send `exit` notification, then wait for the process to exit.
    /// Falls back to SIGKILL if the server doesn't exit within 5 seconds.
    /// This allows profilers (DHAT, etc.) to write output on clean exit.
    fn kill(mut self) {
        self.shutdown_gracefully();
    }

    fn shutdown_gracefully(&mut self) {
        // 1. Send shutdown request
        if let Ok(id) = self.send("shutdown", json!(null)) {
            // Wait up to 5s for shutdown response
            let _ = self.read_response(id, Duration::from_secs(5));
        }
        // 2. Send exit notification
        let _ = self.notif("exit", json!(null));
        // 3. Close stdin — tower-lsp's serve loop detects EOF and returns,
        //    allowing destructors (e.g. DHAT profiler) to run.
        drop(self.writer.take());
        // 4. Wait for process to exit (up to 5s), then force kill
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return, // exited cleanly
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return;
                }
            }
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Try graceful shutdown first (allows profilers to write output),
        // fall back to kill if it fails.
        self.shutdown_gracefully();
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn uri(p: &Path) -> String {
    // canonicalize resolves symlinks and produces an absolute path.
    // If it fails (file doesn't exist yet, e.g. rename target), fall back
    // to making the path absolute via current_dir + join.
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        }
    });
    format!("file://{}", abs.display())
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

fn method_allows_null_result(method: &str) -> bool {
    matches!(
        method,
        "workspace/willCreateFiles" | "workspace/willDeleteFiles" | "workspace/willRenameFiles"
    )
}

fn is_valid_response_for_method(method: &str, resp: &Value) -> bool {
    if resp.get("error").is_some() {
        return false;
    }
    match resp.get("result") {
        None => false,
        Some(r) => {
            if r.is_null() {
                return method_allows_null_result(method);
            }
            // Direct array result (e.g. definition, references) — must be non-empty
            if let Some(arr) = r.as_array() {
                return !arr.is_empty();
            }
            // Completion response: { isIncomplete: bool, items: [...] }
            // An empty items array means no completions were returned
            if let Some(items) = r.get("items").and_then(|v| v.as_array()) {
                return !items.is_empty();
            }
            true
        }
    }
}

fn is_valid_response(resp: &Value) -> bool {
    is_valid_response_for_method("", resp)
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

// ── Expectation checking ─────────────────────────────────────────────────────

/// Check whether an LSP response matches the expected result.
/// Returns Ok(()) on match, Err(message) on mismatch.
fn check_expectation(resp: &Value, expect: &ExpectConfig) -> Result<(), String> {
    // Extract result from response envelope
    let result = resp
        .get("result")
        .or_else(|| resp.get("params"))
        .unwrap_or(resp);

    // Handle array responses (e.g. textDocument/definition can return Location[])
    let location = if let Some(arr) = result.as_array() {
        if arr.is_empty() {
            return Err("response is empty array".to_string());
        }
        &arr[0]
    } else if result.is_object() {
        result
    } else if result.is_null() {
        return Err("response is null".to_string());
    } else {
        result
    };

    // Check file (URI ends with expected suffix)
    if let Some(ref expected_file) = expect.file {
        let uri = location
            .get("targetUri")
            .or_else(|| location.get("uri"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !uri.ends_with(expected_file) {
            return Err(format!(
                "file: expected \"{}\" but got \"{}\"",
                expected_file,
                uri.rsplit('/').next().unwrap_or(uri)
            ));
        }
    }

    // Check line
    if let Some(expected_line) = expect.line {
        // Try targetRange first (definitionLink), then range (Location)
        let range = location
            .get("targetRange")
            .or_else(|| location.get("range"));
        let actual_line = range
            .and_then(|r| r.get("start"))
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64())
            .map(|l| l as u32);
        match actual_line {
            Some(line) if line == expected_line => {}
            Some(line) => {
                return Err(format!("line: expected {} but got {}", expected_line, line));
            }
            None => {
                return Err(format!(
                    "line: expected {} but response has no range",
                    expected_line
                ));
            }
        }
    }

    // Check exact count (array responses)
    if let Some(expected_count) = expect.count {
        let actual_count = result.as_array().map(|a| a.len()).unwrap_or(0);
        if actual_count != expected_count {
            return Err(format!(
                "count: expected {} but got {}",
                expected_count, actual_count
            ));
        }
    }

    // Check minimum count (array responses)
    if let Some(min) = expect.min_count {
        let actual_count = result.as_array().map(|a| a.len()).unwrap_or(0);
        if actual_count < min {
            return Err(format!(
                "minCount: expected >= {} but got {}",
                min, actual_count
            ));
        }
    }

    // Check completion item predicates
    if !expect.contains_items.is_empty() || !expect.absent_items.is_empty() {
        let completion_items: Vec<&Value> = result
            .get("items")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .or_else(|| result.as_array().map(|a| a.iter().collect()))
            .unwrap_or_default();

        if completion_items.is_empty() {
            return Err("containsItems/absentItems: response has no completion items".to_string());
        }

        for item_expect in &expect.contains_items {
            if !completion_items
                .iter()
                .any(|item| completion_item_matches(item, item_expect))
            {
                return Err(format!(
                    "containsItems: no completion item matched {}",
                    completion_item_expect_to_string(item_expect)
                ));
            }
        }

        for item_expect in &expect.absent_items {
            if completion_items
                .iter()
                .any(|item| completion_item_matches(item, item_expect))
            {
                return Err(format!(
                    "absentItems: found unexpected completion item matching {}",
                    completion_item_expect_to_string(item_expect)
                ));
            }
        }
    }

    Ok(())
}

fn completion_item_matches(item: &Value, expect: &CompletionItemExpect) -> bool {
    if let Some(ref label) = expect.label {
        if item.get("label").and_then(|v| v.as_str()) != Some(label.as_str()) {
            return false;
        }
    }

    if let Some(ref detail_contains) = expect.detail_contains {
        let detail = item.get("detail").and_then(|v| v.as_str()).unwrap_or("");
        if !detail.contains(detail_contains) {
            return false;
        }
    }

    if let Some(ref prefix) = expect.sort_text_prefix {
        let sort_text = item.get("sortText").and_then(|v| v.as_str()).unwrap_or("");
        if !sort_text.starts_with(prefix) {
            return false;
        }
    }

    if let Some(expected_has_edits) = expect.has_additional_text_edits {
        let has_edits = item
            .get("additionalTextEdits")
            .or_else(|| item.get("additional_text_edits"))
            .map(|v| !v.is_null())
            .unwrap_or(false);
        if has_edits != expected_has_edits {
            return false;
        }
    }

    if let Some(ref needle) = expect.additional_text_edits_contain {
        let edits = item
            .get("additionalTextEdits")
            .or_else(|| item.get("additional_text_edits"))
            .and_then(|v| v.as_array());
        let found = edits.is_some_and(|arr| {
            arr.iter().any(|e| {
                e.get("newText")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.contains(needle))
            })
        });
        if !found {
            return false;
        }
    }

    true
}

fn completion_item_expect_to_string(expect: &CompletionItemExpect) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = &expect.label {
        parts.push(format!("label={v}"));
    }
    if let Some(v) = &expect.detail_contains {
        parts.push(format!("detailContains={v}"));
    }
    if let Some(v) = &expect.sort_text_prefix {
        parts.push(format!("sortTextPrefix={v}"));
    }
    if let Some(v) = expect.has_additional_text_edits {
        parts.push(format!("hasAdditionalTextEdits={v}"));
    }
    if let Some(v) = &expect.additional_text_edits_contain {
        parts.push(format!("additionalTextEditsContain={v}"));
    }
    if parts.is_empty() {
        "{}".to_string()
    } else {
        format!("{{{}}}", parts.join(", "))
    }
}

/// Result of verifying expectations across a benchmark run.
struct VerifyTally {
    passed: usize,
    failed: usize,
    skipped: usize, // no expect field
}

impl VerifyTally {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }
}

// ── Memory measurement ──────────────────────────────────────────────────────

/// Get the resident set size (RSS) of a process in kilobytes.
/// Uses sysinfo first, then falls back to `ps` on macOS/Linux.
/// Returns None if measurement fails.
fn get_rss(pid: u32) -> Option<u64> {
    // Prefer native process inspection via sysinfo (works better in restricted shells).
    let target = Pid::from_u32(pid);
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory()),
    );
    sys.refresh_pids_specifics(&[target], ProcessRefreshKind::new().with_memory());
    if let Some(proc_) = sys.process(target) {
        // sysinfo reports bytes; convert to kilobytes to match report schema.
        let mem_bytes = proc_.memory();
        if mem_bytes > 0 {
            return Some(mem_bytes / 1024);
        }
    }

    // Fallback: shell out to ps where available.
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
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
    verbose: bool,
) -> BenchResult {
    let mut iterations = Vec::new();
    let mut peak_rss: Option<u64> = None;
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));
        let start = Instant::now();
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
        if let Some(rss) = get_rss(c.child.id()) {
            peak_rss = Some(peak_rss.map_or(rss, |prev: u64| prev.max(rss)));
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
        rss_kb: peak_rss,
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
    verbose: bool,
) -> BenchResult {
    let mut iterations = Vec::new();
    let mut peak_rss: Option<u64> = None;
    for i in 0..(w + n) {
        on_progress(&format!("{}  waiting for diagnostics", iter_msg(i, w, n)));
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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

/// Cold-start benchmark: spawns a fresh server per iteration, measures the full
/// end-to-end time from didOpen through diagnostics through the method response.
/// This captures the real user experience — compilation + request latency.
fn bench_lsp_method_cold(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    method: &str,
    params_fn: &dyn Fn(&str, &str) -> Value,
    timeout: Duration,
    w: usize,
    n: usize,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    let mut iterations = Vec::new();
    let mut peak_rss: Option<u64> = None;
    for i in 0..(w + n) {
        on_progress(&format!("{}  cold start", iter_msg(i, w, n)));
        let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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

        let file_uri = uri(target_file);

        // Start timing from didOpen — this is what the user feels
        let start = Instant::now();
        if let Err(e) = c.open_file(target_file) {
            return BenchResult::Fail {
                error: e,
                rss_kb: None,
            };
        }

        // Wait for diagnostics (compilation)
        on_progress(&format!("{}  waiting for diagnostics", iter_msg(i, w, n)));
        match c.wait_for_valid_diagnostics(timeout) {
            Ok(_) => {}
            Err(e) => {
                let rss = get_rss(c.child.id());
                return BenchResult::Fail {
                    error: format!("wait_for_diagnostics: {}", e),
                    rss_kb: rss,
                };
            }
        }

        // Send the actual method request
        on_progress(&format!("{}  sending {}", iter_msg(i, w, n), method));
        let req_id = match c.send(method, params_fn(method, &file_uri)) {
            Ok(id) => id,
            Err(e) => {
                let rss = get_rss(c.child.id());
                return BenchResult::Fail {
                    error: e,
                    rss_kb: rss,
                };
            }
        };
        match c.read_response(req_id, timeout) {
            Ok(resp) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                if let Some(rss) = get_rss(c.child.id()) {
                    peak_rss = Some(peak_rss.map_or(rss, |prev: u64| prev.max(rss)));
                }
                on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
                if i >= w {
                    let summary = response_summary(&resp, response_limit);
                    iterations.push((ms, summary));
                }
            }
            Err(e) => {
                let rss = get_rss(c.child.id());
                return BenchResult::Fail {
                    error: e,
                    rss_kb: rss,
                };
            }
        }
        // Print server logs for this iteration
        if verbose {
            if let Ok(logs) = c.logs.lock() {
                for log in logs.iter() {
                    eprintln!("    {}", style(log).dim());
                }
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
    params_fn: &dyn Fn(&str, &str) -> Value, // takes (method, file_uri), returns params
    index_timeout: Duration,
    timeout: Duration,
    w: usize,
    n: usize,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
    // For methods that depend on the background project index (e.g.
    // willRenameFiles), wait for the $/progress end notification so the
    // full project index is in the cache before we send requests.
    if method.contains("willRename") || method.contains("willDelete") {
        on_progress("waiting for project index");
        c.wait_for_progress_end(index_timeout);
    }

    // Sample RSS after indexing
    let rss_kb = get_rss(c.child.id());

    let file_uri = uri(target_file);
    let mut iterations = Vec::new();
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));

        let deadline = Instant::now() + timeout;
        loop {
            let start = Instant::now();
            let req_id = match c.send(method, params_fn(method, &file_uri)) {
                Ok(id) => id,
                Err(e) => return BenchResult::Fail { error: e, rss_kb },
            };
            match c.read_response(req_id, timeout) {
                Ok(resp) => {
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    if is_valid_response_for_method(method, &resp) {
                        on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
                        if i >= w {
                            let summary = response_summary(&resp, response_limit);
                            iterations.push((ms, summary));
                        }
                        break;
                    }
                    if Instant::now() >= deadline {
                        return BenchResult::Invalid {
                            first_response: resp,
                            rss_kb,
                        };
                    }
                }
                Err(e) => return BenchResult::Fail { error: e, rss_kb },
            }
        }
    }
    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

/// A resolved snapshot: absolute path + position to benchmark at.
struct ResolvedSnapshot {
    path: PathBuf,
    line: u32,
    col: u32,
    expect: Option<ExpectConfig>,
}

/// Benchmark an LSP method across sequential file snapshots on a single server.
/// Spawns once, opens the original file, waits for diagnostics, then for each
/// snapshot: sends didChange → sends one request at that snapshot's line/col.
/// Each snapshot is one iteration. Returns a single BenchResult with one
/// iteration per snapshot.
fn bench_lsp_snapshots(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    method: &str,
    params_fn: &dyn Fn(&str, &str) -> Value,
    snapshots: &[ResolvedSnapshot],
    index_timeout: Duration,
    timeout: Duration,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
            let rss = get_rss(c.child.id());
            return BenchResult::Fail {
                error: format!("wait_for_diagnostics: {}", e),
                rss_kb: rss,
            };
        }
    }
    let rss_kb = get_rss(c.child.id());
    let file_uri = uri(target_file);

    let total = snapshots.len();
    let mut iterations = Vec::new();
    for (si, snap) in snapshots.iter().enumerate() {
        let version = (si + 2) as i32; // didOpen was version 1
        let snap_name = snap
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        on_progress(&format!("[{}/{}] didChange {}", si + 1, total, snap_name));

        // Send the snapshot content
        match std::fs::read_to_string(&snap.path) {
            Ok(content) => {
                if let Err(e) = c.did_change(&file_uri, version, &content) {
                    return BenchResult::Fail { error: e, rss_kb };
                }
            }
            Err(e) => {
                return BenchResult::Fail {
                    error: format!("{}: {}", snap.path.display(), e),
                    rss_kb,
                }
            }
        }

        // Build params from the method's params_fn, then override position
        let mut params = params_fn(method, &file_uri);
        if let Some(obj) = params.as_object_mut() {
            obj.insert(
                "position".to_string(),
                json!({ "line": snap.line, "character": snap.col }),
            );
        }
        let start = Instant::now();
        let req_id = match c.send(method, params) {
            Ok(id) => id,
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        };
        match c.read_response(req_id, timeout) {
            Ok(resp) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                let summary = response_summary(&resp, response_limit);
                on_progress(&format!(
                    "[{}/{}] {}  {:.1}ms{}",
                    si + 1,
                    total,
                    snap_name,
                    ms,
                    if is_valid_response_for_method(method, &resp) {
                        ""
                    } else {
                        "  (null)"
                    }
                ));
                iterations.push((ms, summary));
            }
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        }
    }
    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

/// A resolved didOpen step: absolute path + optional position override.
struct ResolvedDidOpen {
    path: PathBuf,
    line: Option<u32>,
    col: Option<u32>,
    expect: Option<ExpectConfig>,
}

/// Benchmark an LSP method with sequential didOpen steps.
///
/// Flow:
///   1. Spawn server, open primary file, wait for diagnostics
///   2. Send the benchmark request (iteration 0 = baseline)
///   3. For each didOpen step:
///      a. Open the additional file via textDocument/didOpen
///      b. Wait for diagnostics on the new file
///      c. Re-send the benchmark request on the **original** file
///   4. Each step produces one iteration in the result
///
/// This tests cross-file features like forward references: opening more files
/// populates the AST cache, so the reference count should grow.
fn bench_lsp_didopen(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    method: &str,
    params_fn: &dyn Fn(&str, &str) -> Value,
    steps: &[ResolvedDidOpen],
    base_line: u32,
    base_col: u32,
    index_timeout: Duration,
    timeout: Duration,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
            let rss = get_rss(c.child.id());
            return BenchResult::Fail {
                error: format!("wait_for_diagnostics: {}", e),
                rss_kb: rss,
            };
        }
    }
    let rss_kb = get_rss(c.child.id());
    let file_uri = uri(target_file);
    let total = steps.len() + 1; // +1 for baseline
    let mut iterations = Vec::new();

    // Iteration 0: baseline request before any didOpen
    {
        on_progress(&format!("[1/{}] baseline", total));
        let start = Instant::now();
        let req_id = match c.send(method, params_fn(method, &file_uri)) {
            Ok(id) => id,
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        };
        match c.read_response(req_id, timeout) {
            Ok(resp) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                let summary = response_summary(&resp, response_limit);
                on_progress(&format!("[1/{}] baseline  {:.1}ms", total, ms));
                iterations.push((ms, summary));
            }
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        }
    }

    // Subsequent iterations: open each file, wait for diagnostics, re-request
    for (si, step) in steps.iter().enumerate() {
        let step_name = step
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        on_progress(&format!("[{}/{}] didOpen {}", si + 2, total, step_name));

        // Open the additional file
        if let Err(e) = c.open_file(&step.path) {
            return BenchResult::Fail { error: e, rss_kb };
        }

        // Wait for diagnostics on the newly opened file
        match c.wait_for_valid_diagnostics(index_timeout) {
            Ok(_) => {}
            Err(e) => {
                return BenchResult::Fail {
                    error: format!("wait_for_diagnostics after didOpen {}: {}", step_name, e),
                    rss_kb,
                };
            }
        }

        // Re-send the benchmark request on the original file
        let req_line = step.line.unwrap_or(base_line);
        let req_col = step.col.unwrap_or(base_col);
        let params = json!({
            "textDocument": { "uri": &file_uri },
            "position": { "line": req_line, "character": req_col },
            "context": { "includeDeclaration": true },
        });
        let start = Instant::now();
        let req_id = match c.send(method, params) {
            Ok(id) => id,
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        };
        match c.read_response(req_id, timeout) {
            Ok(resp) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                let summary = response_summary(&resp, response_limit);
                on_progress(&format!(
                    "[{}/{}] {}  {:.1}ms",
                    si + 2,
                    total,
                    step_name,
                    ms
                ));
                iterations.push((ms, summary));
            }
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        }
    }
    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

/// Benchmark `workspace/willRenameFiles` with a full multi-rename lifecycle.
///
/// Flow:
///   1. Spawn server, open a file, wait for diagnostics + project index
///   2. For each rename step:
///      a. Send `workspace/willRenameFiles` — record the WorkspaceEdit response
///      b. Apply the returned text edits to files on disk
///      c. Rename the file on disk (oldUri → newUri)
///      d. Send `workspace/didRenameFiles` notification
///      e. Send `didChange` for each file that was edited (so server text_cache is updated)
///      f. Wait for the server to re-index ($/progress end)
///   3. After all steps, restore all files to their original state
///
/// Each step produces one iteration in the result, recording timing and the
/// response (WorkspaceEdit). This tests the real-world scenario where a user
/// renames files multiple times and each rename must work on the mutated state.
fn bench_lsp_rename_sequence(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    steps: &[RenameStep],
    index_timeout: Duration,
    timeout: Duration,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
    if let Err(e) = c.wait_for_valid_diagnostics(index_timeout) {
        let rss = get_rss(c.child.id());
        return BenchResult::Fail {
            error: format!("wait_for_diagnostics: {}", e),
            rss_kb: rss,
        };
    }
    on_progress("waiting for project index");
    c.wait_for_progress_end(index_timeout);

    let rss_kb = get_rss(c.child.id());
    // Always run rename cycles and return to the original filename by renaming
    // back. If the provided steps do not return to the starting file path,
    // synthesize a final reverse step.
    let mut run_steps: Vec<RenameStep> = steps.to_vec();
    if !run_steps.is_empty() {
        let original = PathBuf::from(&run_steps[0].file);
        let original_name = original
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| run_steps[0].file.clone());

        let mut current = original.clone();
        for step in &run_steps {
            let old_path = PathBuf::from(&step.file);
            // Prefer the declared step source if it diverges, so we still
            // compute a reasonable final path for mixed step sequences.
            if old_path != current {
                current = old_path;
            }
            current = current
                .parent()
                .map(|p| p.join(&step.new_name))
                .unwrap_or_else(|| PathBuf::from(&step.new_name));
        }

        if current != original {
            run_steps.push(RenameStep {
                file: current.to_string_lossy().to_string(),
                new_name: original_name,
                expect: None,
            });
        }
    }
    let total = run_steps.len();
    let mut iterations = Vec::new();

    // Track file renames so we can restore at the end.
    // Each entry: (current_path, original_path, original_content).
    let mut restore_list: Vec<(PathBuf, PathBuf, Vec<u8>)> = Vec::new();
    // Snapshot which rename paths existed before the sequence started.
    // Only these paths are restored from rename_list; paths created during
    // the sequence (e.g. intermediate rename targets) are not treated as
    // originals.
    let mut initially_existing: HashSet<PathBuf> = HashSet::new();
    for step in &run_steps {
        let old_path = cwd.join(&step.file);
        let new_path = old_path.parent().unwrap().join(&step.new_name);
        if old_path.exists() {
            initially_existing.insert(old_path);
        }
        if new_path.exists() {
            initially_existing.insert(new_path);
        }
    }
    // Track content changes to non-renamed files so we can restore them too.
    // Key: absolute path, Value: original content.
    let mut content_restore: HashMap<PathBuf, Vec<u8>> = HashMap::new();

    // didChange version counter (per-file)
    let mut versions: HashMap<String, i32> = HashMap::new();

    for (si, step) in run_steps.iter().enumerate() {
        let step_label = format!("{} → {}", step.file, step.new_name);
        on_progress(&format!("[{}/{}] {}", si + 1, total, step_label));

        let old_path = cwd.join(&step.file);
        if !old_path.exists() {
            // File might have been renamed in a previous step — check restore_list
            let found = restore_list.iter().find(|(cur, _, _)| {
                cur.file_name().map(|f| f.to_string_lossy().to_string())
                    == old_path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
            });
            if found.is_none() {
                return BenchResult::Fail {
                    error: format!(
                        "rename step {}: file not found: {}",
                        si + 1,
                        old_path.display()
                    ),
                    rss_kb,
                };
            }
        }

        let old_uri_str = uri(&old_path);
        let new_path = old_path.parent().unwrap().join(&step.new_name);
        let new_uri_str = uri(&new_path);

        // Save original content for restore (only on first touch)
        if initially_existing.contains(&old_path)
            && !restore_list.iter().any(|(_, orig, _)| orig == &old_path)
        {
            if let Ok(content) = std::fs::read(&old_path) {
                restore_list.push((old_path.clone(), old_path.clone(), content));
            }
        }

        // 1. Send workspace/willRenameFiles
        let params = json!({
            "files": [{
                "oldUri": old_uri_str,
                "newUri": new_uri_str,
            }]
        });
        let start = Instant::now();
        let req_id = match c.send("workspace/willRenameFiles", params) {
            Ok(id) => id,
            Err(e) => {
                restore_files(&restore_list, &content_restore);
                return BenchResult::Fail { error: e, rss_kb };
            }
        };
        let resp = match c.read_response(req_id, timeout) {
            Ok(r) => r,
            Err(e) => {
                restore_files(&restore_list, &content_restore);
                return BenchResult::Fail { error: e, rss_kb };
            }
        };
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let summary = response_summary(&resp, response_limit);
        on_progress(&format!(
            "[{}/{}] {}  {:.1}ms",
            si + 1,
            total,
            step_label,
            ms
        ));
        iterations.push((ms, summary.clone()));

        // Print server logs accumulated so far
        if verbose {
            if let Ok(logs) = c.logs.lock() {
                for log in logs.iter() {
                    eprintln!("  {} {}", style("log").dim(), log);
                }
            }
        }

        // Print the response for debugging
        let edit_count = resp
            .get("result")
            .and_then(|r| r.get("changes"))
            .and_then(|c| c.as_object())
            .map(|m| m.len())
            .unwrap_or(0);
        if edit_count > 0 {
            eprintln!("  {} {} file(s) with edits", style("→").green(), edit_count);
        } else {
            eprintln!("  {} no edits returned", style("→").yellow());
        }

        // 2. Apply the returned text edits to files on disk
        let edits = resp
            .get("result")
            .and_then(|r| r.get("changes"))
            .and_then(|c| c.as_object())
            .cloned()
            .unwrap_or_default();

        for (file_uri, file_edits) in &edits {
            let file_path = file_uri.strip_prefix("file://").unwrap_or(file_uri);
            let file_path = PathBuf::from(file_path);

            // Save original content for restore (only on first touch)
            if !content_restore.contains_key(&file_path) {
                if let Ok(orig) = std::fs::read(&file_path) {
                    content_restore.insert(file_path.clone(), orig);
                }
            }

            // Read current content
            let mut content = match std::fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "  {} failed to read {} for edit: {}",
                        style("warn").yellow(),
                        file_path.display(),
                        e
                    );
                    continue;
                }
            };

            // Apply edits in reverse order (so byte offsets stay valid)
            if let Some(edit_arr) = file_edits.as_array() {
                let mut sorted_edits: Vec<&Value> = edit_arr.iter().collect();
                sorted_edits.sort_by(|a, b| {
                    let a_line = a
                        .get("range")
                        .and_then(|r| r.get("start"))
                        .and_then(|s| s.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0);
                    let a_col = a
                        .get("range")
                        .and_then(|r| r.get("start"))
                        .and_then(|s| s.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0);
                    let b_line = b
                        .get("range")
                        .and_then(|r| r.get("start"))
                        .and_then(|s| s.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0);
                    let b_col = b
                        .get("range")
                        .and_then(|r| r.get("start"))
                        .and_then(|s| s.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0);
                    (b_line, b_col).cmp(&(a_line, a_col))
                });

                for edit in sorted_edits {
                    let new_text = edit.get("newText").and_then(|t| t.as_str()).unwrap_or("");
                    let start_line = edit
                        .get("range")
                        .and_then(|r| r.get("start"))
                        .and_then(|s| s.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as usize;
                    let start_col = edit
                        .get("range")
                        .and_then(|r| r.get("start"))
                        .and_then(|s| s.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as usize;
                    let end_line = edit
                        .get("range")
                        .and_then(|r| r.get("end"))
                        .and_then(|s| s.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as usize;
                    let end_col = edit
                        .get("range")
                        .and_then(|r| r.get("end"))
                        .and_then(|s| s.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as usize;

                    // Convert line:col to byte offset
                    let lines: Vec<&str> = content.lines().collect();
                    let start_byte = lines[..start_line]
                        .iter()
                        .map(|l| l.len() + 1) // +1 for newline
                        .sum::<usize>()
                        + start_col;
                    let end_byte =
                        lines[..end_line].iter().map(|l| l.len() + 1).sum::<usize>() + end_col;

                    content = format!(
                        "{}{}{}",
                        &content[..start_byte],
                        new_text,
                        &content[end_byte..]
                    );
                }
            }

            // Write the edited content back to disk
            if let Err(e) = std::fs::write(&file_path, &content) {
                eprintln!(
                    "  {} failed to write {}: {}",
                    style("warn").yellow(),
                    file_path.display(),
                    e
                );
            }

            // Send didChange to the server so its text_cache is updated
            let ver = versions.entry(file_uri.clone()).or_insert(1);
            *ver += 1;
            if let Err(e) = c.did_change(file_uri, *ver, &content) {
                eprintln!(
                    "  {} failed to send didChange for {}: {}",
                    style("warn").yellow(),
                    file_uri,
                    e
                );
            }
        }

        // 3. Rename the file on disk
        if old_path.exists() {
            if let Err(e) = std::fs::rename(&old_path, &new_path) {
                restore_files(&restore_list, &content_restore);
                return BenchResult::Fail {
                    error: format!("rename on disk failed: {}", e),
                    rss_kb,
                };
            }
            // Update restore_list to track the new current location
            for entry in &mut restore_list {
                if entry.0 == old_path {
                    entry.0 = new_path.clone();
                }
            }
        }

        // 4. Send workspace/didRenameFiles notification
        let did_rename_params = json!({
            "files": [{
                "oldUri": old_uri_str,
                "newUri": new_uri_str,
            }]
        });
        if let Err(e) = c.notif("workspace/didRenameFiles", did_rename_params) {
            restore_files(&restore_list, &content_restore);
            return BenchResult::Fail {
                error: format!("didRenameFiles notification failed: {}", e),
                rss_kb,
            };
        }

        // 5. Wait for the server to re-index
        on_progress(&format!(
            "[{}/{}] {} — waiting for re-index",
            si + 1,
            total,
            step_label
        ));
        c.wait_for_progress_end(index_timeout);
    }

    // Restore all files to original state
    restore_files(&restore_list, &content_restore);

    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

/// Restore files to their original state after a rename sequence.
fn restore_files(
    rename_list: &[(PathBuf, PathBuf, Vec<u8>)],
    content_map: &HashMap<PathBuf, Vec<u8>>,
) {
    // Restore renamed files: move back to original path and restore content
    for (current_path, original_path, original_content) in rename_list {
        if current_path != original_path && current_path.exists() {
            let _ = std::fs::rename(current_path, original_path);
        }
        let _ = std::fs::write(original_path, original_content);
    }
    // Restore content changes to non-renamed files
    for (path, content) in content_map {
        let _ = std::fs::write(path, content);
    }
}

/// Convert LSP line/character to byte offset for UTF-8 text.
fn lsp_pos_to_byte_offset(text: &str, line: usize, character: usize) -> usize {
    let mut current_line = 0usize;
    let mut byte_idx = 0usize;

    for l in text.split_inclusive('\n') {
        if current_line == line {
            let line_text = l.strip_suffix('\n').unwrap_or(l);
            let mut col = 0usize;
            let mut line_byte = 0usize;
            for ch in line_text.chars() {
                if col >= character {
                    break;
                }
                line_byte += ch.len_utf8();
                col += 1;
            }
            return byte_idx + line_byte;
        }
        byte_idx += l.len();
        current_line += 1;
    }

    text.len()
}

/// Apply a list of LSP TextEdits (JSON form) to UTF-8 text.
fn apply_text_edits_from_json(mut content: String, edits_json: &[Value]) -> String {
    let mut edits: Vec<(usize, usize, String)> = edits_json
        .iter()
        .filter_map(|e| {
            let new_text = e.get("newText")?.as_str()?.to_string();
            let start_line = e.get("range")?.get("start")?.get("line")?.as_u64()? as usize;
            let start_col = e.get("range")?.get("start")?.get("character")?.as_u64()? as usize;
            let end_line = e.get("range")?.get("end")?.get("line")?.as_u64()? as usize;
            let end_col = e.get("range")?.get("end")?.get("character")?.as_u64()? as usize;
            let start = lsp_pos_to_byte_offset(&content, start_line, start_col);
            let end = lsp_pos_to_byte_offset(&content, end_line, end_col);
            Some((start, end, new_text))
        })
        .collect();

    edits.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
    for (start, end, new_text) in edits {
        if start <= end && end <= content.len() {
            content.replace_range(start..end, &new_text);
        }
    }
    content
}

/// Apply WorkspaceEdit.changes (JSON form) to disk and notify didChange.
fn apply_workspace_changes_to_disk(
    c: &mut LspClient,
    edits_obj: &serde_json::Map<String, Value>,
    content_restore: &mut HashMap<PathBuf, Vec<u8>>,
    versions: &mut HashMap<String, i32>,
) {
    for (file_uri, file_edits) in edits_obj {
        let file_path = PathBuf::from(file_uri.strip_prefix("file://").unwrap_or(file_uri));

        if !content_restore.contains_key(&file_path) {
            if let Ok(orig) = std::fs::read(&file_path) {
                content_restore.insert(file_path.clone(), orig);
            }
        }

        let current = std::fs::read_to_string(&file_path).unwrap_or_default();
        let edits_arr = match file_edits.as_array() {
            Some(a) => a,
            None => continue,
        };
        let next = apply_text_edits_from_json(current, edits_arr);

        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&file_path, &next).is_ok() {
            let ver = versions.entry(file_uri.clone()).or_insert(1);
            *ver += 1;
            let _ = c.did_change(file_uri, *ver, &next);
        }
    }
}

fn bench_lsp_create_sequence(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    steps: &[CreateStep],
    index_timeout: Duration,
    timeout: Duration,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
    if let Err(e) = c.wait_for_valid_diagnostics(index_timeout) {
        let rss = get_rss(c.child.id());
        return BenchResult::Fail {
            error: format!("wait_for_diagnostics: {}", e),
            rss_kb: rss,
        };
    }
    on_progress("waiting for project index");
    c.wait_for_progress_end(index_timeout);

    let rss_kb = get_rss(c.child.id());
    let mut iterations = Vec::new();
    let mut content_restore: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    let mut created_paths: Vec<PathBuf> = Vec::new();
    let mut versions: HashMap<String, i32> = HashMap::new();
    let total = steps.len();

    for (si, step) in steps.iter().enumerate() {
        let new_path = cwd.join(&step.file);
        let new_uri = uri(&new_path);
        on_progress(&format!("[{}/{}] create {}", si + 1, total, step.file));

        if new_path.exists() {
            if !content_restore.contains_key(&new_path) {
                if let Ok(orig) = std::fs::read(&new_path) {
                    content_restore.insert(new_path.clone(), orig);
                }
            }
            let _ = std::fs::remove_file(&new_path);
        }

        let params = json!({ "files": [{ "uri": new_uri }] });
        let start = Instant::now();
        let req_id = match c.send("workspace/willCreateFiles", params) {
            Ok(id) => id,
            Err(e) => {
                restore_files(&[], &content_restore);
                return BenchResult::Fail { error: e, rss_kb };
            }
        };
        let resp = match c.read_response(req_id, timeout) {
            Ok(r) => r,
            Err(e) => {
                restore_files(&[], &content_restore);
                return BenchResult::Fail { error: e, rss_kb };
            }
        };
        if !is_valid_response_for_method("workspace/willCreateFiles", &resp) {
            restore_files(&[], &content_restore);
            return BenchResult::Invalid {
                first_response: resp,
                rss_kb,
            };
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let summary = response_summary(&resp, response_limit);
        iterations.push((ms, summary));

        if let Some(changes) = resp
            .get("result")
            .and_then(|r| r.get("changes"))
            .and_then(|c| c.as_object())
        {
            apply_workspace_changes_to_disk(&mut c, changes, &mut content_restore, &mut versions);
        }

        if !new_path.exists() {
            if let Some(parent) = new_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&new_path, "");
            created_paths.push(new_path.clone());
        }

        let did_create = json!({ "files": [{ "uri": uri(&new_path) }] });
        let _ = c.notif("workspace/didCreateFiles", did_create);
        c.wait_for_progress_end(index_timeout);
    }

    for p in created_paths {
        let _ = std::fs::remove_file(p);
    }
    restore_files(&[], &content_restore);

    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

fn bench_lsp_delete_sequence(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    steps: &[DeleteStep],
    index_timeout: Duration,
    timeout: Duration,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
    if let Err(e) = c.wait_for_valid_diagnostics(index_timeout) {
        let rss = get_rss(c.child.id());
        return BenchResult::Fail {
            error: format!("wait_for_diagnostics: {}", e),
            rss_kb: rss,
        };
    }
    on_progress("waiting for project index");
    c.wait_for_progress_end(index_timeout);

    let rss_kb = get_rss(c.child.id());
    let mut iterations = Vec::new();
    let mut content_restore: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    let mut versions: HashMap<String, i32> = HashMap::new();
    let total = steps.len();

    for (si, step) in steps.iter().enumerate() {
        let del_path = cwd.join(&step.file);
        on_progress(&format!("[{}/{}] delete {}", si + 1, total, step.file));

        if !del_path.exists() {
            restore_files(&[], &content_restore);
            return BenchResult::Fail {
                error: format!("delete step target missing: {}", del_path.display()),
                rss_kb,
            };
        }
        if !content_restore.contains_key(&del_path) {
            if let Ok(orig) = std::fs::read(&del_path) {
                content_restore.insert(del_path.clone(), orig);
            }
        }

        let params = json!({ "files": [{ "uri": uri(&del_path) }] });
        let start = Instant::now();
        let req_id = match c.send("workspace/willDeleteFiles", params) {
            Ok(id) => id,
            Err(e) => {
                restore_files(&[], &content_restore);
                return BenchResult::Fail { error: e, rss_kb };
            }
        };
        let resp = match c.read_response(req_id, timeout) {
            Ok(r) => r,
            Err(e) => {
                restore_files(&[], &content_restore);
                return BenchResult::Fail { error: e, rss_kb };
            }
        };
        if !is_valid_response_for_method("workspace/willDeleteFiles", &resp) {
            restore_files(&[], &content_restore);
            return BenchResult::Invalid {
                first_response: resp,
                rss_kb,
            };
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let summary = response_summary(&resp, response_limit);
        iterations.push((ms, summary));

        if let Some(changes) = resp
            .get("result")
            .and_then(|r| r.get("changes"))
            .and_then(|c| c.as_object())
        {
            apply_workspace_changes_to_disk(&mut c, changes, &mut content_restore, &mut versions);
        }

        let _ = std::fs::remove_file(&del_path);
        let did_delete = json!({ "files": [{ "uri": uri(&del_path) }] });
        let _ = c.notif("workspace/didDeleteFiles", did_delete);
        c.wait_for_progress_end(index_timeout);
    }

    restore_files(&[], &content_restore);
    c.kill();
    BenchResult::Ok { iterations, rss_kb }
}

/// Benchmark `textDocument/semanticTokens/full/delta`.
///
/// Flow:
///   1. Spawn server, open file, wait for diagnostics
///   2. Send `semanticTokens/full` to prime the cache and get a `resultId`
///   3. Optionally send `didChange` snapshots to mutate the file
///   4. For each iteration: send `semanticTokens/full/delta` with `previousResultId`,
///      extract the new `resultId` from the response for the next iteration
fn bench_lsp_delta(
    srv: &ServerConfig,
    root: &str,
    cwd: &Path,
    target_file: &Path,
    snapshots: &[ResolvedSnapshot],
    index_timeout: Duration,
    timeout: Duration,
    w: usize,
    n: usize,
    response_limit: usize,
    on_progress: &dyn Fn(&str),
    verbose: bool,
) -> BenchResult {
    on_progress("spawning");
    let mut c = match LspClient::spawn(&srv.cmd, &srv.args, cwd, verbose) {
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
            let rss = get_rss(c.child.id());
            return BenchResult::Fail {
                error: format!("wait_for_diagnostics: {}", e),
                rss_kb: rss,
            };
        }
    }
    let rss_kb = get_rss(c.child.id());
    let file_uri = uri(target_file);

    // Step 2: Send semanticTokens/full to prime the cache
    on_progress("priming semanticTokens/full");
    let prime_params = json!({ "textDocument": { "uri": &file_uri } });
    let prime_id = match c.send("textDocument/semanticTokens/full", prime_params) {
        Ok(id) => id,
        Err(e) => return BenchResult::Fail { error: e, rss_kb },
    };
    let prime_resp = match c.read_response(prime_id, timeout) {
        Ok(r) => r,
        Err(e) => {
            return BenchResult::Fail {
                error: format!("prime semanticTokens/full: {}", e),
                rss_kb,
            }
        }
    };
    if !is_valid_response(&prime_resp) {
        c.kill();
        return BenchResult::Invalid {
            first_response: prime_resp,
            rss_kb,
        };
    }
    let mut result_id = prime_resp
        .pointer("/result/resultId")
        .and_then(|v| v.as_str())
        .unwrap_or("0")
        .to_string();

    // Step 3: Send didChange snapshots if any
    if !snapshots.is_empty() {
        for (si, snap) in snapshots.iter().enumerate() {
            let version = (si + 2) as i32;
            match std::fs::read_to_string(&snap.path) {
                Ok(content) => {
                    if let Err(e) = c.did_change(&file_uri, version, &content) {
                        return BenchResult::Fail { error: e, rss_kb };
                    }
                }
                Err(e) => {
                    return BenchResult::Fail {
                        error: format!("{}: {}", snap.path.display(), e),
                        rss_kb,
                    }
                }
            }
        }
    }

    // Step 4: Iterate delta requests
    let mut iterations = Vec::new();
    for i in 0..(w + n) {
        on_progress(&iter_msg(i, w, n));

        let params = json!({
            "textDocument": { "uri": &file_uri },
            "previousResultId": &result_id,
        });
        let start = Instant::now();
        let req_id = match c.send("textDocument/semanticTokens/full/delta", params) {
            Ok(id) => id,
            Err(e) => return BenchResult::Fail { error: e, rss_kb },
        };
        match c.read_response(req_id, timeout) {
            Ok(resp) => {
                if !is_valid_response(&resp) {
                    c.kill();
                    return BenchResult::Invalid {
                        first_response: resp,
                        rss_kb,
                    };
                }
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                // Extract new resultId for next iteration (works for both full and delta responses)
                if let Some(rid) = resp.pointer("/result/resultId").and_then(|v| v.as_str()) {
                    result_id = rid.to_string();
                }
                on_progress(&format!("{}  {:.1}ms", iter_msg(i, w, n), ms));
                if i >= w {
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
    results: &[(&str, Option<Value>, Vec<BenchRow>)],
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
    methods: &HashMap<String, MethodConfig>,
    dir: &str,
) -> String {
    let ts = timestamp();
    let date = date_stamp();
    let json_benchmarks: Vec<Value> = results
        .iter()
        .map(|(name, input, rows)| {
            let mut obj = json!({
                "name": name,
                "servers": rows.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
            });
            if let Some(params) = input {
                obj["input"] = params.clone();
            }
            obj
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
    // Build methods overrides JSON (only include non-empty)
    let methods_json: Value = if methods.is_empty() {
        Value::Null
    } else {
        let map: serde_json::Map<String, Value> = methods
            .iter()
            .map(|(k, v)| {
                let mut obj = serde_json::Map::new();
                if let Some(l) = v.line {
                    obj.insert("line".into(), json!(l));
                }
                if let Some(c) = v.col {
                    obj.insert("col".into(), json!(c));
                }
                if let Some(ref t) = v.trigger {
                    obj.insert("trigger".into(), json!(t));
                }
                if let Some(ref n) = v.new_name {
                    obj.insert("newName".into(), json!(n));
                }
                (k.clone(), Value::Object(obj))
            })
            .collect();
        Value::Object(map)
    };
    let mut settings = json!({
        "iterations": n,
        "warmup": w,
        "timeout_secs": timeout.as_secs(),
        "index_timeout_secs": index_timeout.as_secs(),
        "project": project,
        "file": bench_file,
        "line": target_line,
        "col": target_col,
    });
    if !methods.is_empty() {
        settings["methods"] = methods_json;
    }
    let output = json!({
        "timestamp": ts,
        "date": date,
        "settings": settings,
        "servers": json_servers,
        "benchmarks": json_benchmarks,
    });
    let _ = std::fs::create_dir_all(dir);
    let path = format!("{}/results.json", dir);
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
    "textDocument/documentHighlight",
    "textDocument/documentLink",
    "textDocument/formatting",
    "textDocument/foldingRange",
    "textDocument/selectionRange",
    "textDocument/codeLens",
    "textDocument/inlayHint",
    "textDocument/semanticTokens/full",
    "textDocument/semanticTokens/range",
    "textDocument/semanticTokens/full/delta",
    "textDocument/documentColor",
    "workspace/symbol",
    "workspace/willRenameFiles",
    "workspace/willCreateFiles",
    "workspace/willDeleteFiles",
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

    /// Path to servers.yaml registry file (overrides auto-discovery)
    #[arg(short, long)]
    servers: Option<String>,

    /// Verify responses match `expect` fields in config. Exits non-zero on mismatch.
    #[arg(long)]
    verify: bool,

    /// Show server logs (window/logMessage and stderr). Off by default.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a benchmark.yaml template
    Init {
        /// Output path for the generated config
        #[arg(short, long, default_value = "benchmark.yaml")]
        config: Option<String>,
    },
    /// Replay a JSON-RPC request from benchmark output against an LSP server
    Replay {
        /// Server command (e.g. "solc --lsp", "solidity-ls --stdio")
        #[arg(short, long)]
        server: String,

        /// JSON-RPC input string (from benchmark output's "input" field)
        #[arg(short, long)]
        input: String,

        /// Project root directory (defaults to current directory)
        #[arg(short, long)]
        project: Option<String>,

        /// File to open before sending the request (extracted from input URI if omitted)
        #[arg(short, long)]
        file: Option<String>,

        /// Timeout in seconds for the response (default: 30)
        #[arg(short, long, default_value = "30")]
        timeout: u64,
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

fn replay(server: &str, input: &str, project: Option<&str>, file: Option<&str>, timeout_secs: u64) {
    // Parse the JSON-RPC input string
    let rpc: Value = serde_json::from_str(input).unwrap_or_else(|e| {
        eprintln!("Error: invalid JSON-RPC input: {}", e);
        std::process::exit(1);
    });
    let method = rpc
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_else(|| {
            eprintln!("Error: input missing \"method\" field");
            std::process::exit(1);
        });
    let params = rpc.get("params").cloned().unwrap_or(json!({}));

    // Extract file URI from params if not provided explicitly
    let file_uri = params
        .get("textDocument")
        .and_then(|td| td.get("uri"))
        .and_then(|u| u.as_str());

    let file_path: Option<PathBuf> = file.map(PathBuf::from).or_else(|| {
        file_uri
            .and_then(|u| u.strip_prefix("file://"))
            .map(PathBuf::from)
    });

    // Resolve project root
    let cwd = project
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    if !cwd.exists() {
        eprintln!("Error: project directory not found: {}", cwd.display());
        std::process::exit(1);
    }

    // Parse server command
    let parts: Vec<&str> = server.split_whitespace().collect();
    if parts.is_empty() {
        eprintln!("Error: empty server command");
        std::process::exit(1);
    }
    let cmd = parts[0];
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    let root = uri(&cwd);
    let timeout = Duration::from_secs(timeout_secs);

    eprintln!("  {} {}", style("server").dim(), server);
    eprintln!("  {} {}", style("method").dim(), method);
    if let Some(ref fp) = file_path {
        eprintln!("  {} {}", style("file").dim(), fp.display());
    }
    eprintln!();

    // Spawn server
    eprintln!("{}", style("Spawning server...").dim());
    let mut client = LspClient::spawn(cmd, &args, &cwd, true).unwrap_or_else(|e| {
        eprintln!("Error: failed to spawn server: {}", e);
        std::process::exit(1);
    });

    // Initialize
    eprintln!("{}", style("Initializing...").dim());
    if let Err(e) = client.initialize(&root) {
        eprintln!("Error: initialize failed: {}", e);
        std::process::exit(1);
    }

    // Open file if we have one
    if let Some(ref fp) = file_path {
        if fp.exists() {
            eprintln!("{}", style("Opening file...").dim());
            if let Err(e) = client.open_file(fp) {
                eprintln!("Error: open file failed: {}", e);
                std::process::exit(1);
            }
            // Give server a moment to index
            std::thread::sleep(Duration::from_millis(500));
        } else {
            eprintln!(
                "  {} file not found: {}",
                style("warn").yellow(),
                fp.display()
            );
        }
    }

    // Send the request
    eprintln!("{}", style("Sending request...").dim());
    let req_id = match client.send(method, params) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error: send failed: {}", e);
            std::process::exit(1);
        }
    };

    // Read response
    match client.read_response(req_id, timeout) {
        Ok(resp) => {
            let pretty = serde_json::to_string_pretty(&resp).unwrap();
            eprintln!();
            println!("{}", pretty);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    // Handle subcommands before loading config
    match cli.command {
        Some(Commands::Init { config }) => {
            let path = config.as_deref().unwrap_or(&cli.config);
            init_config(path);
            std::process::exit(0);
        }
        Some(Commands::Replay {
            server,
            input,
            project,
            file,
            timeout,
        }) => {
            replay(
                &server,
                &input,
                project.as_deref(),
                file.as_deref(),
                timeout,
            );
            std::process::exit(0);
        }
        None => {}
    }

    // Check if this config includes sub-configs to run.
    // Parent defaults (everything except `include`) are merged into each
    // sub-config: the sub-config's keys win over parent defaults.
    if let Some((configs, defaults)) = check_include(&cli.config) {
        eprintln!(
            "{} running {} configs",
            style(">>").cyan().bold(),
            configs.len()
        );
        let has_defaults = matches!(&defaults, serde_yaml::Value::Mapping(m) if !m.is_empty());
        let exe = std::env::current_exe().unwrap();
        let mut all_ok = true;
        for (i, cfg_path) in configs.iter().enumerate() {
            eprintln!(
                "\n{} [{}/{}] {}",
                style(">>").cyan().bold(),
                i + 1,
                configs.len(),
                cfg_path
            );
            // Merge parent defaults into the sub-config and write a temp file.
            let tmp_path = format!("{}.merged.yaml", cfg_path);
            let run_path: String = if has_defaults {
                match merge_configs(&defaults, cfg_path) {
                    Some(merged) => {
                        let yaml = serde_yaml::to_string(&merged).unwrap();
                        std::fs::write(&tmp_path, &yaml).unwrap();
                        tmp_path.clone()
                    }
                    None => {
                        eprintln!(
                            "  {} could not merge defaults into {}",
                            style("warn").yellow(),
                            cfg_path
                        );
                        cfg_path.clone()
                    }
                }
            } else {
                cfg_path.clone()
            };
            let mut args = vec!["-c".to_string(), run_path.clone()];
            // Forward --servers flag to sub-processes so they find the registry
            if let Some(ref servers_path) = cli.servers {
                args.push("--servers".to_string());
                args.push(servers_path.clone());
            }
            if cli.verify {
                args.push("--verify".to_string());
            }
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let result = std::process::Command::new(&exe).args(&arg_refs).status();
            // Clean up temp file
            let _ = std::fs::remove_file(&tmp_path);
            match result {
                Ok(s) if s.success() => {}
                Ok(s) => {
                    eprintln!("  {} {} exited with {}", style("fail").red(), cfg_path, s);
                    all_ok = false;
                }
                Err(e) => {
                    eprintln!("  {} {}: {}", style("error").red(), cfg_path, e);
                    all_ok = false;
                }
            }
        }
        eprintln!(
            "\n{} complete",
            if all_ok {
                style("done").green().bold()
            } else {
                style("done").yellow().bold()
            }
        );
        std::process::exit(if all_ok { 0 } else { 1 });
    }

    // Load config
    let mut cfg = load_config(&cli.config);

    // Load server registry and resolve string references
    let servers_file_hint = cfg.servers_file.clone().or(cli.servers.clone());
    let registry_path = discover_servers_file(&cli.config, servers_file_hint.as_deref());
    let registry = match &registry_path {
        Some(p) => {
            eprintln!("  {} {}", style("servers").dim(), p.display());
            load_server_registry(p)
        }
        None => HashMap::new(),
    };
    resolve_servers(&mut cfg.servers, &registry);
    let verify = cli.verify;
    let verbose = cli.verbose;

    let n = cfg.iterations;
    let w = cfg.warmup;
    let timeout = Duration::from_secs(cfg.timeout);
    let index_timeout = Duration::from_secs(cfg.index_timeout);
    let target_line = cfg.line;
    let target_col = cfg.col;
    let methods = cfg.methods.clone();
    // Support legacy trigger_character — migrate to methods map
    let trigger_character = cfg.trigger_character.clone().or_else(|| {
        methods
            .get("textDocument/completion")
            .and_then(|m| m.trigger.clone())
    });
    let output_dir = cfg.output;
    let report_path = cfg.report;
    let _report_style = cfg.report_style;
    let response_limit = cfg.response_limit;
    let partial_dir = format!("{}/partial", output_dir);

    // Resolve which benchmarks to run from config
    let benchmarks: Vec<&str> = {
        let base: Vec<&str> =
            if cfg.benchmarks.is_empty() || cfg.benchmarks.iter().any(|c| c == "all") {
                ALL_BENCHMARKS.to_vec()
            } else {
                cfg.benchmarks.iter().map(|s| s.as_str()).collect()
            };
        if cfg.exclude.is_empty() {
            base
        } else {
            base.into_iter()
                .filter(|b| !cfg.exclude.iter().any(|e| e == b))
                .collect()
        }
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

    // Filter to available servers
    cfg.servers.retain(|s| {
        let ok = available(&s.cmd);
        if !ok {
            eprintln!("  {} {} -- not found", style("skip").yellow(), s.label);
        }
        ok
    });

    // Detect versions and resolve "latest" labels
    eprintln!("\n{}", style("Detecting versions...").dim());
    let mut detected_versions: Vec<String> = Vec::new();
    for srv in &mut cfg.servers {
        let ver = detect_version(&srv.cmd);
        eprintln!("  {} = {}", style(&srv.label).bold(), ver);

        // Replace "latest" with the actual version in the label and link
        if srv.label.ends_with(" latest") {
            let base = srv.label.trim_end_matches(" latest");
            srv.label = base.to_string();
            if !srv.link.is_empty() && !ver.is_empty() {
                // Extract semver from version string (e.g. "solidity-language-server 0.1.24+commit..." → "0.1.24")
                let semver = ver
                    .split_whitespace()
                    .last()
                    .unwrap_or(&ver)
                    .split('+')
                    .next()
                    .unwrap_or(&ver);
                srv.link = format!(
                    "{}/releases/tag/v{}",
                    srv.link.trim_end_matches('/'),
                    semver
                );
            }
        }
        detected_versions.push(ver);
    }

    let versions: Vec<(&str, String)> = cfg
        .servers
        .iter()
        .zip(detected_versions.into_iter())
        .map(|(s, ver)| (s.label.as_str(), ver))
        .collect();
    let avail: Vec<&ServerConfig> = cfg.servers.iter().collect();

    let total = benchmarks.len();
    let mut num = 0usize;
    let mut all_results: Vec<(&str, Option<Value>, Vec<BenchRow>)> = Vec::new();
    let mut tally = VerifyTally::new();

    // Resolve line/col for a given method, falling back to global defaults.
    let pos_for = |method: &str| -> (u32, u32) {
        methods
            .get(method)
            .map(|m| (m.line.unwrap_or(target_line), m.col.unwrap_or(target_col)))
            .unwrap_or((target_line, target_col))
    };

    // Resolve start line/col for range-based requests, falling back to (0, 0).
    let start_pos_for = |method: &str| -> (u32, u32) {
        methods
            .get(method)
            .map(|m| (m.start_line.unwrap_or(0), m.start_col.unwrap_or(0)))
            .unwrap_or((0, 0))
    };

    // Position + method params for definition/declaration/hover/references
    let position_params = |method: &str, file_uri: &str| -> Value {
        let (l, c) = pos_for(method);
        json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": l, "character": c },
        })
    };
    let doc_params =
        |_method: &str, file_uri: &str| -> Value { json!({ "textDocument": { "uri": file_uri } }) };
    let ref_params = |method: &str, file_uri: &str| -> Value {
        let (l, c) = pos_for(method);
        json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": l, "character": c },
            "context": { "includeDeclaration": true },
        })
    };
    let symbol_params = |_method: &str, _file_uri: &str| -> Value { json!({ "query": "" }) };
    let rename_params = |method: &str, file_uri: &str| -> Value {
        let (l, c) = pos_for(method);
        let new_name = methods
            .get(method)
            .and_then(|m| m.new_name.as_deref())
            .unwrap_or("__lsp_bench_rename__");
        json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": l, "character": c },
            "newName": new_name,
        })
    };
    let completion_params = |method: &str, file_uri: &str| -> Value {
        let (l, c) = pos_for(method);
        let mut params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": l, "character": c },
        });
        // Trigger from methods map, then legacy top-level trigger_character
        let tc = methods
            .get(method)
            .and_then(|m| m.trigger.as_deref())
            .or(trigger_character.as_deref());
        if let Some(tc) = tc {
            params["context"] = json!({
                "triggerKind": 2,
                "triggerCharacter": tc,
            });
        }
        params
    };
    let formatting_params = |_method: &str, file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        })
    };
    let selection_range_params = |method: &str, file_uri: &str| -> Value {
        let (l, c) = pos_for(method);
        json!({
            "textDocument": { "uri": file_uri },
            "positions": [{ "line": l, "character": c }],
        })
    };
    let inlay_hint_params = |_method: &str, file_uri: &str| -> Value {
        json!({
            "textDocument": { "uri": file_uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 9999, "character": 0 },
            },
        })
    };
    let semantic_tokens_params =
        |_method: &str, file_uri: &str| -> Value { json!({ "textDocument": { "uri": file_uri } }) };
    let will_rename_params = |method: &str, file_uri: &str| -> Value {
        let method_cfg = methods.get(method);
        let new_name = method_cfg
            .and_then(|m| m.new_name.as_deref())
            .unwrap_or("__lsp_bench_renamed__.sol");
        // Use per-method file override if set, otherwise use the top-level file.
        let old_uri = match method_cfg.and_then(|m| m.file.as_deref()) {
            Some(rel_path) => uri(&cwd.join(rel_path)),
            None => file_uri.to_string(),
        };
        // Derive newUri by replacing the filename in oldUri
        let new_uri = if let Some(pos) = old_uri.rfind('/') {
            format!("{}/{}", &old_uri[..pos], new_name)
        } else {
            new_name.to_string()
        };
        json!({
            "files": [{
                "oldUri": old_uri,
                "newUri": new_uri,
            }]
        })
    };

    let will_create_params = |method: &str, file_uri: &str| -> Value {
        let method_cfg = methods.get(method);
        let new_name = method_cfg
            .and_then(|m| m.new_name.as_deref())
            .unwrap_or("__lsp_bench_created__.sol");
        // Build a URI for the new file in the same directory as the target file.
        let new_uri = if let Some(pos) = file_uri.rfind('/') {
            format!("{}/{}", &file_uri[..pos], new_name)
        } else {
            new_name.to_string()
        };
        json!({
            "files": [{
                "uri": new_uri,
            }]
        })
    };
    let will_delete_params = |method: &str, file_uri: &str| -> Value {
        let method_cfg = methods.get(method);
        // Use per-method file override if set, otherwise use the top-level file.
        let target_uri = match method_cfg.and_then(|m| m.file.as_deref()) {
            Some(rel_path) => uri(&cwd.join(rel_path)),
            None => file_uri.to_string(),
        };
        json!({
            "files": [{
                "uri": target_uri,
            }]
        })
    };
    let semantic_tokens_range_params = |method: &str, file_uri: &str| -> Value {
        let (sl, sc) = start_pos_for(method);
        let (el, ec) = pos_for(method);
        json!({
            "textDocument": { "uri": file_uri },
            "range": {
                "start": { "line": sl, "character": sc },
                "end": { "line": el, "character": ec },
            },
        })
    };
    // (config_key, lsp_method, params_fn)
    // config_key and lsp_method are now the same — the official LSP method name
    // params_fn takes (method_name, file_uri) so it can resolve per-method overrides.
    let method_benchmarks: Vec<(&str, &str, &dyn Fn(&str, &str) -> Value)> = vec![
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
            "textDocument/documentHighlight",
            "textDocument/documentHighlight",
            &position_params,
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
            "textDocument/semanticTokens/range",
            "textDocument/semanticTokens/range",
            &semantic_tokens_range_params,
        ),
        (
            "textDocument/documentColor",
            "textDocument/documentColor",
            &doc_params,
        ),
        ("workspace/symbol", "workspace/symbol", &symbol_params),
        (
            "workspace/willRenameFiles",
            "workspace/willRenameFiles",
            &will_rename_params,
        ),
        (
            "workspace/willCreateFiles",
            "workspace/willCreateFiles",
            &will_create_params,
        ),
        (
            "workspace/willDeleteFiles",
            "workspace/willDeleteFiles",
            &will_delete_params,
        ),
    ];

    // ── spawn ────────────────────────────────────────────────────────────

    if benchmarks.contains(&"initialize") {
        num += 1;
        eprintln!(
            "\n{}",
            style(format!("[{}/{}] initialize", num, total)).bold()
        );
        let rows = run_bench(&avail, response_limit, |srv, on_progress| {
            bench_spawn(srv, &root, &cwd, w, n, on_progress, verbose)
        });
        all_results.push(("initialize", None, rows));
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
            &methods,
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
                verbose,
            )
        });
        all_results.push(("textDocument/diagnostic", None, rows));
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
            &methods,
            &partial_dir,
        );
        eprintln!("  {} {}", style("saved").dim(), style(&p).dim());
    }

    // ── semanticTokens/full/delta (special-cased: needs result_id chaining) ──

    if benchmarks.contains(&"textDocument/semanticTokens/full/delta") {
        num += 1;
        eprintln!(
            "\n{}",
            style(format!(
                "[{}/{}] textDocument/semanticTokens/full/delta",
                num, total
            ))
            .bold()
        );
        let snapshots: Vec<ResolvedSnapshot> = methods
            .get("textDocument/semanticTokens/full/delta")
            .map(|m| {
                m.did_change
                    .iter()
                    .map(|s| ResolvedSnapshot {
                        path: cwd.join(&s.file),
                        line: s.line,
                        col: s.col,
                        expect: s.expect.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !snapshots.is_empty() {
            eprintln!(
                "  {} {} snapshot(s) via didChange",
                style("edit").cyan(),
                snapshots.len()
            );
        }
        let rows = run_bench(&avail, response_limit, |srv, on_progress| {
            bench_lsp_delta(
                srv,
                &root,
                &cwd,
                &bench_sol,
                &snapshots,
                index_timeout,
                timeout,
                w,
                n,
                response_limit,
                on_progress,
                verbose,
            )
        });
        all_results.push(("textDocument/semanticTokens/full/delta", None, rows));
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
            &methods,
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
            let snapshots: Vec<ResolvedSnapshot> = methods
                .get(*method)
                .map(|m| {
                    m.did_change
                        .iter()
                        .map(|s| ResolvedSnapshot {
                            path: cwd.join(&s.file),
                            line: s.line,
                            col: s.col,
                            expect: s.expect.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let did_open_steps: Vec<ResolvedDidOpen> = methods
                .get(*method)
                .map(|m| {
                    m.did_open
                        .iter()
                        .map(|s| ResolvedDidOpen {
                            path: cwd.join(&s.file),
                            line: s.line,
                            col: s.col,
                            expect: s.expect.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !snapshots.is_empty() {
                eprintln!(
                    "  {} {} snapshot(s) via didChange",
                    style("edit").cyan(),
                    snapshots.len()
                );
            }
            if !did_open_steps.is_empty() {
                eprintln!(
                    "  {} {} file(s) via didOpen",
                    style("open").cyan(),
                    did_open_steps.len()
                );
            }
            let rename_steps: Vec<RenameStep> = methods
                .get(*method)
                .map(|m| m.rename_steps.clone())
                .unwrap_or_default();
            let create_steps: Vec<CreateStep> = methods
                .get(*method)
                .map(|m| m.create_steps.clone())
                .unwrap_or_default();
            let delete_steps: Vec<DeleteStep> = methods
                .get(*method)
                .map(|m| m.delete_steps.clone())
                .unwrap_or_default();
            if !rename_steps.is_empty() {
                eprintln!(
                    "  {} {} rename step(s) (full lifecycle)",
                    style("rename").magenta(),
                    rename_steps.len()
                );
            }
            if !create_steps.is_empty() {
                eprintln!(
                    "  {} {} create step(s) (full lifecycle)",
                    style("create").cyan(),
                    create_steps.len()
                );
            }
            if !delete_steps.is_empty() {
                eprintln!(
                    "  {} {} delete step(s) (full lifecycle)",
                    style("delete").yellow(),
                    delete_steps.len()
                );
            }
            let is_cold = methods.get(*method).map_or(false, |m| m.cold);
            if is_cold {
                eprintln!(
                    "  {} fresh server per iteration (cold start)",
                    style("cold").red()
                );
            }
            let rows = if !rename_steps.is_empty() {
                run_bench(&avail, response_limit, |srv, on_progress| {
                    bench_lsp_rename_sequence(
                        srv,
                        &root,
                        &cwd,
                        &bench_sol,
                        &rename_steps,
                        index_timeout,
                        timeout,
                        response_limit,
                        on_progress,
                        verbose,
                    )
                })
            } else if !create_steps.is_empty() {
                run_bench(&avail, response_limit, |srv, on_progress| {
                    bench_lsp_create_sequence(
                        srv,
                        &root,
                        &cwd,
                        &bench_sol,
                        &create_steps,
                        index_timeout,
                        timeout,
                        response_limit,
                        on_progress,
                        verbose,
                    )
                })
            } else if !delete_steps.is_empty() {
                run_bench(&avail, response_limit, |srv, on_progress| {
                    bench_lsp_delete_sequence(
                        srv,
                        &root,
                        &cwd,
                        &bench_sol,
                        &delete_steps,
                        index_timeout,
                        timeout,
                        response_limit,
                        on_progress,
                        verbose,
                    )
                })
            } else if is_cold {
                run_bench(&avail, response_limit, |srv, on_progress| {
                    bench_lsp_method_cold(
                        srv,
                        &root,
                        &cwd,
                        &bench_sol,
                        lsp_method,
                        *params_fn,
                        timeout,
                        w,
                        n,
                        response_limit,
                        on_progress,
                        verbose,
                    )
                })
            } else if !did_open_steps.is_empty() {
                let bl = methods
                    .get(*method)
                    .and_then(|m| m.line)
                    .unwrap_or(target_line);
                let bc = methods
                    .get(*method)
                    .and_then(|m| m.col)
                    .unwrap_or(target_col);
                run_bench(&avail, response_limit, |srv, on_progress| {
                    bench_lsp_didopen(
                        srv,
                        &root,
                        &cwd,
                        &bench_sol,
                        lsp_method,
                        *params_fn,
                        &did_open_steps,
                        bl,
                        bc,
                        index_timeout,
                        timeout,
                        response_limit,
                        on_progress,
                        verbose,
                    )
                })
            } else if snapshots.is_empty() {
                run_bench(&avail, response_limit, |srv, on_progress| {
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
                        verbose,
                    )
                })
            } else {
                run_bench(&avail, response_limit, |srv, on_progress| {
                    bench_lsp_snapshots(
                        srv,
                        &root,
                        &cwd,
                        &bench_sol,
                        lsp_method,
                        *params_fn,
                        &snapshots,
                        index_timeout,
                        timeout,
                        response_limit,
                        on_progress,
                        verbose,
                    )
                })
            };

            // ── Verify expectations ──────────────────────────────────────
            if verify {
                let method_expect = methods.get(*method).and_then(|m| m.expect.as_ref());
                for row in &rows {
                    if row.kind != 0 {
                        continue; // skip failed/invalid servers
                    }
                    if !did_open_steps.is_empty() {
                        // didOpen mode: iteration 0 = baseline, then 1 per didOpen step
                        for (i, (_ms, resp)) in row.iterations.iter().enumerate() {
                            if i == 0 {
                                // Baseline — check method-level expect
                                match method_expect {
                                    Some(exp) => match check_expectation(resp, exp) {
                                        Ok(()) => {
                                            tally.passed += 1;
                                            eprintln!(
                                                "  {} [baseline] {}",
                                                style("✓").green().bold(),
                                                row.label,
                                            );
                                        }
                                        Err(msg) => {
                                            tally.failed += 1;
                                            eprintln!(
                                                "  {} [baseline] {} — {}",
                                                style("✗").red().bold(),
                                                row.label,
                                                msg,
                                            );
                                        }
                                    },
                                    None => {
                                        tally.skipped += 1;
                                    }
                                }
                            } else if let Some(step) = did_open_steps.get(i - 1) {
                                let step_name =
                                    step.path.file_name().unwrap_or_default().to_string_lossy();
                                let expect = step.expect.as_ref().or(method_expect);
                                match expect {
                                    Some(exp) => match check_expectation(resp, exp) {
                                        Ok(()) => {
                                            tally.passed += 1;
                                            eprintln!(
                                                "  {} [{}] {}",
                                                style("✓").green().bold(),
                                                i,
                                                step_name,
                                            );
                                        }
                                        Err(msg) => {
                                            tally.failed += 1;
                                            eprintln!(
                                                "  {} [{}] {} — {}",
                                                style("✗").red().bold(),
                                                i,
                                                step_name,
                                                msg,
                                            );
                                        }
                                    },
                                    None => {
                                        tally.skipped += 1;
                                    }
                                }
                            }
                        }
                    } else if !snapshots.is_empty() {
                        // Snapshot mode: 1:1 mapping between iterations and snapshots
                        for (i, ((_ms, resp), snap)) in
                            row.iterations.iter().zip(snapshots.iter()).enumerate()
                        {
                            let snap_name =
                                snap.path.file_name().unwrap_or_default().to_string_lossy();
                            // Per-snapshot expect takes precedence, then method-level
                            let expect = snap.expect.as_ref().or(method_expect);
                            match expect {
                                Some(exp) => match check_expectation(resp, exp) {
                                    Ok(()) => {
                                        tally.passed += 1;
                                        eprintln!(
                                            "  {} [{}] {}",
                                            style("✓").green().bold(),
                                            i + 1,
                                            snap_name,
                                        );
                                    }
                                    Err(msg) => {
                                        tally.failed += 1;
                                        eprintln!(
                                            "  {} [{}] {} — {}",
                                            style("✗").red().bold(),
                                            i + 1,
                                            snap_name,
                                            msg,
                                        );
                                    }
                                },
                                None => {
                                    tally.skipped += 1;
                                }
                            }
                        }
                    } else {
                        // Non-snapshot mode: check method-level expect against each iteration
                        match method_expect {
                            Some(exp) => {
                                // Just check the first iteration (all should be the same)
                                if let Some((_ms, resp)) = row.iterations.first() {
                                    match check_expectation(resp, exp) {
                                        Ok(()) => {
                                            tally.passed += 1;
                                            eprintln!(
                                                "  {} {}",
                                                style("✓").green().bold(),
                                                row.label,
                                            );
                                        }
                                        Err(msg) => {
                                            tally.failed += 1;
                                            eprintln!(
                                                "  {} {} — {}",
                                                style("✗").red().bold(),
                                                row.label,
                                                msg,
                                            );
                                        }
                                    }
                                }
                            }
                            None => {
                                tally.skipped += 1;
                            }
                        }
                    }
                }
            }

            let params = params_fn(method, &uri(&bench_sol));
            let rpc = json!({"jsonrpc": "2.0", "id": 1, "method": lsp_method, "params": params});
            let input = Some(Value::String(serde_json::to_string(&rpc).unwrap()));
            all_results.push((method, input, rows));
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
                &methods,
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
            &methods,
            &output_dir,
        );
        eprintln!("\n  {} {}", style("->").green().bold(), path);

        // Clean up partial saves — the final snapshot has everything
        let _ = std::fs::remove_dir_all(&partial_dir);

        // Generate report if configured
        if let Some(ref report_out) = report_path {
            // Resolve report path relative to output_dir so session files
            // land alongside results.json (not in the CWD)
            let resolved_report = if std::path::Path::new(report_out.as_str()).is_relative() {
                format!("{}/{}", output_dir, report_out)
            } else {
                report_out.clone()
            };
            let exe = std::env::current_exe().unwrap();
            let bin_dir = exe.parent().unwrap();
            let bin_name = "gen-report";
            let args: Vec<&str> = vec!["--quiet", "--session", &path, "-o", &resolved_report];
            let bin = bin_dir.join(bin_name);
            eprintln!("  {} -> {}", style("report").dim(), resolved_report);
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

    // ── Verify summary ────────────────────────────────────────────────
    if verify {
        eprintln!();
        let total_checks = tally.passed + tally.failed;
        if total_checks == 0 && tally.skipped > 0 {
            eprintln!(
                "  {} no expect fields found in config (skipped {})",
                style("warn").yellow(),
                tally.skipped
            );
        } else if tally.failed == 0 {
            eprintln!(
                "  {} {}/{} expectations passed",
                style("verify").green().bold(),
                tally.passed,
                total_checks
            );
        } else {
            eprintln!(
                "  {} {}/{} expectations failed",
                style("verify").red().bold(),
                tally.failed,
                total_checks
            );
            std::process::exit(1);
        }
    }
}
