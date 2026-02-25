use clap::Parser;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Parser)]
#[command(name = "gen-report", version = env!("LONG_VERSION"))]
#[command(about = "Generate benchmark report with competition tables and session logs")]
struct Cli {
    /// Path to benchmark JSON (default: latest in benchmarks/)
    input: Option<String>,

    /// Output file path for the competition report
    #[arg(short, long, default_value = "README.md")]
    output: String,

    /// Also generate session logs (session.txt and session.md)
    #[arg(long)]
    session: bool,

    /// Don't print report to stdout
    #[arg(short, long)]
    quiet: bool,
}

fn main() {
    let cli = Cli::parse();
    let output_path = cli.output;
    let quiet = cli.quiet;

    let json_path = match cli.input {
        Some(p) if Path::new(&p).is_dir() => find_latest_json(&p).unwrap_or_else(|| {
            eprintln!("No JSON files found in {}/", p);
            std::process::exit(1);
        }),
        Some(p) => p,
        None => find_latest_json("benchmarks").unwrap_or_else(|| {
            eprintln!("No JSON files found in benchmarks/");
            std::process::exit(1);
        }),
    };

    eprintln!("Reading: {}", json_path);
    let content = std::fs::read_to_string(&json_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", json_path, e);
        std::process::exit(1);
    });
    let data: Value = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Error parsing JSON: {}", e);
        std::process::exit(1);
    });

    // Generate competition report (README.md)
    let md = generate_competition(&data, &json_path);
    std::fs::write(&output_path, &md).unwrap();
    if !quiet {
        println!("{}", md);
    }
    eprintln!("  -> {}", output_path);

    // Generate session logs if requested
    if cli.session {
        let output_dir = Path::new(&output_path).parent().unwrap_or(Path::new("."));

        let txt = generate_session_txt(&data);
        let txt_path = output_dir.join("session.txt");
        std::fs::write(&txt_path, &txt).unwrap();
        eprintln!("  -> {}", txt_path.display());

        let session_md = generate_session_md(&data);
        let md_path = output_dir.join("session.md");
        std::fs::write(&md_path, &session_md).unwrap();
        eprintln!("  -> {}", md_path.display());
    }
}

// ---------------------------------------------------------------------------
// Competition report generation
// ---------------------------------------------------------------------------

fn generate_competition(data: &Value, _json_path: &str) -> String {
    let mut l: Vec<String> = Vec::new();

    // ── Title ──────────────────────────────────────────────────────────
    l.push("# Solidity LSP Competition".into());
    l.push(String::new());

    if let Some(settings) = data.get("settings") {
        let project = settings
            .get("project")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let file = settings
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        l.push(format!("Benchmarked against `{}` — `{}`.", project, file));
        l.push(String::new());
    }

    // ── Settings ───────────────────────────────────────────────────────
    if let Some(settings) = data.get("settings") {
        let iterations = settings
            .get("iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let warmup = settings.get("warmup").and_then(|v| v.as_u64()).unwrap_or(0);
        let timeout = settings
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let file = settings
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let line = settings.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
        let col = settings.get("col").and_then(|v| v.as_u64()).unwrap_or(0);

        l.push("## Settings".into());
        l.push(String::new());
        l.push("| Setting | Value |".into());
        l.push("|---------|-------|".into());
        l.push(format!("| File | `{}` |", file));
        l.push(format!("| Position | line {}, col {} |", line, col));
        l.push(format!(
            "| Iterations | {} ({} warmup) |",
            iterations, warmup
        ));
        l.push(format!("| Timeout | {}s |", timeout));
        l.push(String::new());
    }

    // ── Servers ────────────────────────────────────────────────────────
    if let Some(servers) = data.get("servers").and_then(|s| s.as_array()) {
        l.push("## Servers".into());
        l.push(String::new());
        l.push("| Server | Version |".into());
        l.push("|--------|---------|".into());
        for srv in servers {
            let name = srv.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let version = srv.get("version").and_then(|v| v.as_str()).unwrap_or("?");
            let link = srv.get("link").and_then(|v| v.as_str()).unwrap_or("");
            let name_cell = if link.is_empty() {
                name.to_string()
            } else {
                format!("[{}]({})", name, link)
            };
            l.push(format!("| {} | `{}` |", name_cell, short_version(version)));
        }
        l.push(String::new());
    }

    // ── Per-method sections ────────────────────────────────────────────
    let benchmarks = match data.get("benchmarks").and_then(|b| b.as_array()) {
        Some(b) => b,
        None => return l.join("\n"),
    };

    if benchmarks.is_empty() {
        return l.join("\n");
    }

    l.push("---".into());
    l.push(String::new());

    // ── Summary table ──────────────────────────────────────────────────
    let server_names = collect_server_names(benchmarks);

    l.push("## Summary".into());
    l.push(String::new());

    // Header: | Method | [server1](link) version | [server2](link) version | ...
    let mut header = "| Method |".to_string();
    let mut sep = "|--------|".to_string();
    for name in &server_names {
        header.push_str(&format!(" {} |", name));
        sep.push_str(&"-".repeat(name.len() + 2));
        sep.push('|');
    }
    l.push(header);
    l.push(sep);

    // Each row: method name + p95 per server (bold on fastest)
    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let servers = match bench.get("servers").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => continue,
        };

        // Find the fastest p95 among servers with correct results
        let fastest_p95 = servers
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("ok"))
            .filter(|s| is_correct(bench_name, s))
            .filter_map(|s| s.get("p95_ms").and_then(|v| v.as_f64()))
            .fold(f64::MAX, f64::min);

        let mut row = format!("| [{}](#{}) |", bench_name, slug(bench_name));
        for srv in servers {
            let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let cell = match status {
                "ok" => {
                    let p95 = srv.get("p95_ms").and_then(|v| v.as_f64());
                    let correct = is_correct(bench_name, srv);
                    match p95 {
                        Some(ms) if correct => {
                            let is_fastest = (ms - fastest_p95).abs() < 0.01;
                            if is_fastest {
                                format!(" {} \u{26a1} |", format_latency(ms))
                            } else {
                                format!(" {} |", format_latency(ms))
                            }
                        }
                        Some(ms) => format!(" {} |", format_latency(ms)),
                        None => " - |".to_string(),
                    }
                }
                _ => {
                    let label = classify_error_result(srv);
                    format!(" {} |", label)
                }
            };
            row.push_str(&cell);
        }
        l.push(row);
    }
    l.push(String::new());

    // ── Scorecard ──────────────────────────────────────────────────────
    let mut wins: HashMap<&str, usize> = HashMap::new();
    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let servers = match bench.get("servers").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => continue,
        };
        let fastest_p95 = servers
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("ok"))
            .filter(|s| is_correct(bench_name, s))
            .filter_map(|s| s.get("p95_ms").and_then(|v| v.as_f64()))
            .fold(f64::MAX, f64::min);
        if fastest_p95 < f64::MAX {
            for srv in servers {
                if let Some(p95) = srv.get("p95_ms").and_then(|v| v.as_f64()) {
                    if (p95 - fastest_p95).abs() < 0.01 && is_correct(bench_name, srv) {
                        let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                        *wins.entry(name).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let mut score_rows: Vec<(&&str, &usize)> = wins.iter().collect();
    score_rows.sort_by(|a, b| b.1.cmp(a.1));

    l.push("### Scorecard".into());
    l.push(String::new());
    l.push("| Server | Wins | Out of |".into());
    l.push("|--------|------|--------|".into());
    for (name, count) in &score_rows {
        let total = benchmarks.len();
        let is_leader = score_rows.first().map(|(_, c)| *c) == Some(count);
        if is_leader {
            l.push(format!("| **{}** | **{}** | **{}** |", name, count, total));
        } else {
            l.push(format!("| {} | {} | {} |", name, count, total));
        }
    }
    l.push(String::new());

    // ── Per-method detail sections ─────────────────────────────────────
    l.push("---".into());
    l.push(String::new());
    l.push("## Results".into());
    l.push(String::new());

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let servers = match bench.get("servers").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => continue,
        };

        l.push(format!("### {}", bench_name));
        l.push(String::new());

        // Find best p95 and lowest RSS among servers with correct results
        let best_p95 = servers
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("ok"))
            .filter(|s| is_correct(bench_name, s))
            .filter_map(|s| s.get("p95_ms").and_then(|v| v.as_f64()))
            .fold(f64::MAX, f64::min);
        let best_rss = servers
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("ok"))
            .filter(|s| is_correct(bench_name, s))
            .filter_map(|s| s.get("rss_kb").and_then(|v| v.as_u64()))
            .filter(|&kb| kb > 0)
            .min()
            .unwrap_or(u64::MAX);

        // Table: Server | p95 | RSS | Result
        l.push("| Server | p95 | RSS | Result |".into());
        l.push("|--------|-----|-----|--------|".into());

        for srv in servers {
            let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
            let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");

            match status {
                "ok" => {
                    let p95 = srv.get("p95_ms").and_then(|v| v.as_f64());
                    let rss = srv.get("rss_kb").and_then(|v| v.as_u64());
                    let result = human_result(bench_name, srv);
                    let _correct = check_correctness(bench_name, srv);

                    let p95_str = match p95 {
                        Some(ms) => {
                            let formatted = format_latency(ms);
                            if (ms - best_p95).abs() < 0.01 {
                                format!("{} \u{26a1}", formatted)
                            } else {
                                formatted
                            }
                        }
                        None => "-".into(),
                    };

                    let rss_str = match rss {
                        Some(kb) => {
                            let formatted = format_memory(kb);
                            if kb == best_rss {
                                format!("**{}**", formatted)
                            } else {
                                formatted
                            }
                        }
                        None => "-".into(),
                    };

                    l.push(format!(
                        "| **{}** | {} | {} | {} |",
                        name, p95_str, rss_str, result
                    ));
                }
                "invalid" => {
                    let result = classify_error_result(srv);
                    let rss = srv
                        .get("rss_kb")
                        .and_then(|v| v.as_u64())
                        .filter(|&kb| kb > 0);
                    let rss_str = rss.map(format_memory).unwrap_or_else(|| "-".into());
                    l.push(format!("| **{}** | - | {} | {} |", name, rss_str, result));
                }
                _ => {
                    let result = classify_error_result(srv);
                    let rss = srv
                        .get("rss_kb")
                        .and_then(|v| v.as_u64())
                        .filter(|&kb| kb > 0);
                    let rss_str = rss.map(format_memory).unwrap_or_else(|| "-".into());
                    l.push(format!("| **{}** | - | {} | {} |", name, rss_str, result));
                }
            }
        }
        l.push(String::new());
    }

    // ── Footer ─────────────────────────────────────────────────────────
    l.push("---".into());
    l.push(String::new());
    if let Some(ts) = data.get("timestamp").and_then(|t| t.as_str()) {
        l.push(format!("*Benchmark run: {}*", ts));
        l.push(String::new());
    }

    l.join("\n")
}

// ---------------------------------------------------------------------------
// Response analysis — extract human-readable result per method type
// ---------------------------------------------------------------------------

/// Parse a response value, handling both stringified JSON and native JSON.
/// Some older benchmark outputs double-stringify: the top-level response is a
/// JSON string whose content is *also* a JSON string (e.g. `"\"[...]\""`).
/// We try to parse once, and if the result is still a string that looks like
/// JSON, we parse again.
fn parse_response(srv: &Value) -> Value {
    let raw = match srv.get("response") {
        Some(v) => v,
        None => return Value::Null,
    };

    // If it's already a native JSON value (object/array), return as-is.
    if !raw.is_string() {
        return raw.clone();
    }

    let s = raw.as_str().unwrap_or("");

    // Try to parse the string as JSON.
    match serde_json::from_str::<Value>(s) {
        Ok(Value::String(inner)) => {
            // Double-stringified — try parsing the inner string.
            serde_json::from_str::<Value>(&inner).unwrap_or(Value::String(inner))
        }
        Ok(parsed) => parsed,
        Err(_) => Value::String(s.to_string()),
    }
}

/// Get response as raw text for simple checks.
fn response_text(srv: &Value) -> String {
    match srv.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
        None => String::new(),
    }
}

/// Extract a human-readable result summary based on the benchmark method.
fn human_result(bench_name: &str, srv: &Value) -> String {
    // Try parsing the top-level response first.
    let mut response = parse_response(srv);

    // If parsing failed (still a string), try the first iteration's response
    // which may be stored in a different format.
    if response.is_string() {
        if let Some(iter_resp) = srv
            .get("iterations")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|i| i.get("response"))
        {
            let iter_parsed = match iter_resp {
                Value::String(s) => serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
                other => other.clone(),
            };
            if !iter_parsed.is_string() {
                response = iter_parsed;
            }
        }
    }

    // If still a string, try to extract info from the raw text.
    if response.is_string() {
        return human_result_from_text(bench_name, response.as_str().unwrap_or(""));
    }

    let method = bench_name.to_lowercase();

    // definition / declaration / typeDefinition / implementation
    if method.contains("definition")
        || method.contains("declaration")
        || method.contains("implementation")
    {
        return summarize_location(&response);
    }

    // references
    if method.contains("reference") {
        return summarize_references(&response);
    }

    // hover
    if method.contains("hover") {
        return summarize_hover(&response);
    }

    // completion
    if method.contains("completion") {
        return summarize_completion(&response);
    }

    // diagnostics
    if method.contains("diagnostic") {
        return summarize_diagnostics(&response);
    }

    // document symbols
    if method.contains("symbol") {
        return summarize_symbols(&response);
    }

    // document links
    if method.contains("link") {
        return summarize_links(&response);
    }

    // rename / prepareRename
    if method.contains("rename") {
        return summarize_rename(&response);
    }

    // inlay hints
    if method.contains("inlay") || method.contains("hint") {
        return summarize_inlay_hints(&response);
    }

    // semantic tokens
    if method.contains("semantic") || method.contains("token") {
        return summarize_semantic_tokens(&response);
    }

    // signature help
    if method.contains("signature") {
        return summarize_signature_help(&response);
    }

    // formatting
    if method.contains("format") {
        return summarize_formatting(&response);
    }

    // initialize / spawn
    if method.contains("init") || method.contains("spawn") {
        if response.is_string() && response.as_str() == Some("ok") {
            return "ok".into();
        }
        if response.is_object() {
            return "ok".into();
        }
        return format_response_fallback(&response);
    }

    format_response_fallback(&response)
}

/// definition/declaration → "File.sol:line"
fn summarize_location(response: &Value) -> String {
    // Could be a single location or an array of locations
    let loc = if response.is_array() {
        response.as_array().and_then(|a| a.first())
    } else if response.is_object() {
        Some(response)
    } else {
        None
    };

    let loc = match loc {
        Some(l) => l,
        None => {
            if response == &Value::Null || response_is_empty(response) {
                return "empty".into();
            }
            return format_response_fallback(response);
        }
    };

    // Extract URI (targetUri or uri)
    let uri = loc
        .get("targetUri")
        .or_else(|| loc.get("uri"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract line (targetRange or range)
    let range = loc.get("targetRange").or_else(|| loc.get("range"));
    let line = range
        .and_then(|r| r.get("start"))
        .and_then(|s| s.get("line"))
        .and_then(|l| l.as_u64());

    let file = uri.rsplit('/').next().unwrap_or(uri);
    match line {
        Some(l) => format!("`{}:{}`", file, l),
        None => format!("`{}`", file),
    }
}

/// references → "N references"
fn summarize_references(response: &Value) -> String {
    match response.as_array() {
        Some(arr) => {
            let count = arr.len();
            if count == 0 {
                "0 references".into()
            } else {
                // Show file of first reference for context
                let first_file = arr
                    .first()
                    .and_then(|r| r.get("uri").or_else(|| r.get("targetUri")))
                    .and_then(|v| v.as_str())
                    .and_then(|u| u.rsplit('/').next())
                    .unwrap_or("");
                if first_file.is_empty() {
                    format!("{} references", count)
                } else {
                    format!("{} references", count)
                }
            }
        }
        None => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// hover → first N chars of content
fn summarize_hover(response: &Value) -> String {
    let contents = response.get("contents").or_else(|| response.get("value"));

    let text = match contents {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(obj)) => obj
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => {
            if response_is_empty(response) {
                return "empty".into();
            }
            return format_response_fallback(response);
        }
    };

    if text.is_empty() {
        return "empty".into();
    }

    // Strip markdown code fences and find first meaningful line
    let meaningful = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("```") && !l.starts_with("---"))
        .next()
        .unwrap_or(&text);
    truncate(meaningful, 50)
}

/// completion → "N items (first, second, ...)"
fn summarize_completion(response: &Value) -> String {
    // CompletionList has .items, or it could be a raw array
    let items = response
        .get("items")
        .and_then(|v| v.as_array())
        .or_else(|| response.as_array());

    match items {
        Some(arr) => {
            let count = arr.len();
            let labels: Vec<&str> = arr
                .iter()
                .take(3)
                .filter_map(|item| item.get("label").and_then(|l| l.as_str()))
                .collect();
            if labels.is_empty() {
                format!("{} items", count)
            } else {
                format!("{} items ({})", count, labels.join(", "))
            }
        }
        None => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// diagnostics → "N diagnostics"
fn summarize_diagnostics(response: &Value) -> String {
    let diags = response
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .or_else(|| response.as_array());

    match diags {
        Some(arr) => format!("{} diagnostics", arr.len()),
        None => {
            if response_is_empty(response) {
                "0 diagnostics".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// document symbols → "N symbols"
fn summarize_symbols(response: &Value) -> String {
    match response.as_array() {
        Some(arr) => format!("{} symbols", arr.len()),
        None => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// document links → "N links"
fn summarize_links(response: &Value) -> String {
    match response.as_array() {
        Some(arr) => format!("{} links", arr.len()),
        None => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// rename → "N edits in M files"
fn summarize_rename(response: &Value) -> String {
    // prepareRename returns a range, not a WorkspaceEdit
    if response.get("start").is_some() || response.get("range").is_some() {
        // prepareRename response
        let range = response.get("range").unwrap_or(response);
        let line = range
            .get("start")
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64());
        return match line {
            Some(l) => format!("ready (line {})", l),
            None => "ready".into(),
        };
    }

    // WorkspaceEdit with documentChanges or changes
    let changes = response
        .get("documentChanges")
        .and_then(|v| v.as_array())
        .or_else(|| response.get("changes").and_then(|v| v.as_array()));

    match changes {
        Some(arr) => {
            let file_count = arr.len();
            let edit_count: usize = arr
                .iter()
                .filter_map(|change| {
                    change
                        .get("edits")
                        .and_then(|e| e.as_array())
                        .map(|a| a.len())
                })
                .sum();
            if edit_count > 0 {
                format!("{} edits in {} files", edit_count, file_count)
            } else {
                format!("{} files", file_count)
            }
        }
        None => {
            // changes as object { uri: [edits] }
            if let Some(obj) = response.get("changes").and_then(|v| v.as_object()) {
                let file_count = obj.len();
                let edit_count: usize = obj
                    .values()
                    .filter_map(|v| v.as_array().map(|a| a.len()))
                    .sum();
                format!("{} edits in {} files", edit_count, file_count)
            } else if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// inlay hints → "N hints"
fn summarize_inlay_hints(response: &Value) -> String {
    match response.as_array() {
        Some(arr) => {
            let count = arr.len();
            let labels: Vec<String> = arr
                .iter()
                .take(3)
                .filter_map(|hint| {
                    hint.get("label")
                        .and_then(|l| l.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            if labels.is_empty() {
                format!("{} hints", count)
            } else {
                format!("{} hints ({})", count, labels.join(", "))
            }
        }
        None => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// semantic tokens → "N tokens"
fn summarize_semantic_tokens(response: &Value) -> String {
    // SemanticTokens response has { data: [int, int, int, int, int, ...] }
    // Each token is encoded as 5 consecutive integers.
    if let Some(data) = response.get("data").and_then(|v| v.as_array()) {
        let token_count = data.len() / 5;
        return format!("{} tokens", token_count);
    }
    // May also be a result ID only (delta)
    if response.get("resultId").is_some() {
        return "delta".into();
    }
    if response_is_empty(response) {
        return "empty".into();
    }
    format_response_fallback(response)
}

/// signature help → parameter labels
fn summarize_signature_help(response: &Value) -> String {
    let sigs = response.get("signatures").and_then(|v| v.as_array());
    match sigs {
        Some(arr) if !arr.is_empty() => {
            let label = arr[0].get("label").and_then(|l| l.as_str()).unwrap_or("");
            truncate(label, 50)
        }
        _ => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

/// formatting → "N edits"
fn summarize_formatting(response: &Value) -> String {
    match response.as_array() {
        Some(arr) => format!("{} edits", arr.len()),
        None => {
            if response_is_empty(response) {
                "empty".into()
            } else {
                format_response_fallback(response)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Correctness checking
// ---------------------------------------------------------------------------

/// Boolean helper: is this server's result considered correct?
fn is_correct(bench_name: &str, srv: &Value) -> bool {
    check_correctness(bench_name, srv) == "\u{2713}"
}

fn method_allows_null_result(bench_name: &str) -> bool {
    matches!(
        bench_name,
        "workspace/willCreateFiles" | "workspace/willDeleteFiles" | "workspace/willRenameFiles"
    )
}

/// Check if a server's response indicates a valid, non-empty, meaningful result.
fn check_correctness(bench_name: &str, srv: &Value) -> &'static str {
    let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status != "ok" {
        return "\u{2717}"; // ✗
    }
    let response = parse_response(srv);
    if response.is_null() && method_allows_null_result(bench_name) {
        return "\u{2713}"; // ✓
    }
    if response_is_empty(&response) {
        return "\u{2717}"; // ✗
    }

    // Check if the response contains an error
    if response.get("error").is_some() {
        return "\u{2717}"; // ✗
    }

    let method = bench_name.to_lowercase();

    // For textDocument/rename: 0 edits means it didn't actually rename anything.
    // Do not apply this rule to workspace/willRenameFiles where `null` is valid.
    if method == "textdocument/rename" {
        if let Some(changes) = response.get("documentChanges").and_then(|v| v.as_array()) {
            let edit_count: usize = changes
                .iter()
                .filter_map(|c| c.get("edits").and_then(|e| e.as_array()).map(|a| a.len()))
                .sum();
            if edit_count == 0 {
                return "\u{2717}"; // ✗
            }
        } else if let Some(obj) = response.get("changes").and_then(|v| v.as_object()) {
            let edit_count: usize = obj
                .values()
                .filter_map(|v| v.as_array().map(|a| a.len()))
                .sum();
            if edit_count == 0 {
                return "\u{2717}"; // ✗
            }
        }
    }

    // For definition/declaration/references/hover: empty array means no result
    if (method.contains("definition")
        || method.contains("declaration")
        || method.contains("reference"))
        && response.as_array().map_or(false, |a| a.is_empty())
    {
        return "\u{2717}"; // ✗
    }

    "\u{2713}" // ✓
}

/// Classify a server result into a clean label for error/invalid cases.
fn classify_error_result(srv: &Value) -> String {
    // Check the error field first
    if let Some(error) = srv.get("error").and_then(|v| v.as_str()) {
        if error.contains("timeout") {
            return "timeout".into();
        }
        if error.contains("EOF") || error.contains("broken pipe") {
            return "crash".into();
        }
    }

    // Check the response for error JSON or known patterns
    let response = parse_response(srv);

    // If response is an object with an "error" key, extract the message
    if let Some(err_msg) = response.get("error").and_then(|v| v.as_str()) {
        if err_msg.contains("Unhandled method")
            || err_msg.contains("Method not found")
            || err_msg.contains("Unknown method")
            || err_msg.contains("unsupported")
        {
            return "unsupported".into();
        }
        if err_msg.contains("Unhandled exception") || err_msg.contains("failed with") {
            return "error".into();
        }
        return truncate(err_msg, 40);
    }

    let text = response_text(srv);
    if text.contains("Unknown method")
        || text.contains("unsupported")
        || text.contains("Method not found")
        || text.contains("Unhandled method")
    {
        return "unsupported".into();
    }
    if text == "[]" || text == "null" || text.is_empty() {
        return "empty".into();
    }

    // Check top-level error field
    if let Some(error) = srv.get("error").and_then(|v| v.as_str()) {
        return truncate(error, 40);
    }

    "fail".into()
}

fn response_is_empty(response: &Value) -> bool {
    match response {
        Value::Null => true,
        Value::String(s) => s.is_empty() || s == "null" || s == "[]" || s == "no result",
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Format latency in human-readable form.
fn format_latency(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else {
        format!("{:.1}ms", ms)
    }
}

/// Format memory in human-readable form.
fn format_memory(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 1.0 {
        format!("{:.1} MB", mb)
    } else {
        format!("{} KB", kb)
    }
}

/// Extract human-readable info from a raw response text string (when JSON parsing fails).
fn human_result_from_text(bench_name: &str, text: &str) -> String {
    let method = bench_name.to_lowercase();

    // Count array items by counting top-level `{` after `[`
    if text.trim_start().starts_with('[') {
        let count = text
            .matches("\"uri\"")
            .count()
            .max(text.matches("\"range\"").count());

        if method.contains("reference") {
            return format!("{} references", count);
        }
        if method.contains("symbol") {
            return format!("{} symbols", count);
        }
        if method.contains("link") {
            return format!("{} links", count);
        }
        if method.contains("hint") || method.contains("inlay") {
            return format!("{} hints", count);
        }
        return format!("{} items", count);
    }

    // Diagnostics: count diagnostic entries
    if method.contains("diagnostic") {
        let count = text.matches("\"severity\"").count();
        return format!("{} diagnostics", count);
    }

    // Hover: extract meaningful content from the markdown value
    if method.contains("hover") {
        if let Some(start) = text.find("\"value\": \"") {
            let content = &text[start + 10..];
            // Skip past code fences and extract the declaration or description
            let clean: String = content
                .replace("```solidity\\n", "")
                .replace("```\\n", "")
                .replace("\\n", " ")
                .replace("\\\"", "\"");
            let clean = clean.trim();
            // Take first meaningful chunk
            let first_part = clean.split("---").next().unwrap_or(clean).trim();
            if !first_part.is_empty() {
                return truncate(first_part, 50);
            }
        }
        return "hover content".into();
    }

    truncate(text, 40)
}

fn format_response_fallback(response: &Value) -> String {
    let s = serde_json::to_string(response).unwrap_or_else(|_| "?".into());
    truncate(&s, 40)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

/// Format a JSON value as a compact JS-inspect style string.
///
/// Arrays longer than `max` show first `max` items then `... N more`:
///   `Array(188) [{ label: "Shop", kind: 7 }, { label: "revert", kind: 1 }, ... 186 more]`
///
/// Strings longer than 80 chars are truncated.
/// Nested objects are recursively compacted.
fn compact_json(value: &Value, max: usize) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.len() > 80 {
                format!("\"{}...\"", &s[..77])
            } else {
                format!("\"{}\"", s)
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".into();
            }
            let prefix = format!("Array({}) ", arr.len());
            let items: Vec<String> = arr.iter().take(max).map(|v| compact_json(v, max)).collect();
            let mut out = format!("{}[{}", prefix, items.join(", "));
            if arr.len() > max {
                out.push_str(&format!(", ... {} more", arr.len() - max));
            }
            out.push(']');
            out
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                return "{}".into();
            }
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}: {}", k, compact_json(v, max)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
    }
}

/// Truncate a JSON value for pretty-printing in `<details>` blocks.
/// Arrays longer than `max` keep first `max` items and append a string note.
/// Output is still valid JSON (for syntax highlighting) just shorter.
fn truncate_json(value: &Value, max: usize) -> Value {
    match value {
        Value::Array(arr) => {
            if arr.len() <= max {
                Value::Array(arr.iter().map(|v| truncate_json(v, max)).collect())
            } else {
                let mut items: Vec<Value> = arr
                    .iter()
                    .take(max)
                    .map(|v| truncate_json(v, max))
                    .collect();
                items.push(Value::String(format!(
                    "... {} more ({} total)",
                    arr.len() - max,
                    arr.len()
                )));
                Value::Array(items)
            }
        }
        Value::Object(obj) => {
            let mut result = serde_json::Map::new();
            for (k, v) in obj {
                result.insert(k.clone(), truncate_json(v, max));
            }
            Value::Object(result)
        }
        other => other.clone(),
    }
}

/// Classify a server response for display in the session log.
/// Returns (label, is_real_content) where label is a short tag.
fn classify_response(bench_name: &str, srv: &Value) -> (&'static str, bool) {
    let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status != "ok" {
        let response = parse_response(srv);
        if let Some(err) = response.get("error").and_then(|v| v.as_str()) {
            if err.contains("Unhandled method")
                || err.contains("Method not found")
                || err.contains("Unknown method")
                || err.contains("unsupported")
            {
                return ("unsupported", false);
            }
            return ("error", false);
        }
        return ("error", false);
    }

    let response = parse_response(srv);
    if response.get("error").is_some() {
        return ("error", false);
    }
    if response.is_null() {
        return ("null", false);
    }
    if response_is_empty(&response) {
        return ("empty", false);
    }

    let method = bench_name.to_lowercase();
    if (method.contains("definition")
        || method.contains("declaration")
        || method.contains("reference"))
        && response.as_array().map_or(false, |a| a.is_empty())
    {
        return ("empty", false);
    }

    ("content", true)
}

/// Extract just the semver from a full version string.
/// e.g. "solidity-language-server 0.1.24+commit.xxx" → "0.1.24"
///      "0.8.26+commit.8a97fa7a.Darwin.appleclang" → "0.8.26"
///      "@nomicfoundation/solidity-language-server 0.8.25" → "0.8.25"
fn short_version(version: &str) -> &str {
    // Take last whitespace-delimited token, then strip everything after '+'
    let token = version.split_whitespace().last().unwrap_or(version);
    token.split('+').next().unwrap_or(token)
}

/// Collect server names from the first benchmark entry.
fn collect_server_names(benchmarks: &[Value]) -> Vec<&str> {
    benchmarks[0]
        .get("servers")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("server").and_then(|n| n.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

/// Convert a benchmark name to a markdown anchor slug.
fn slug(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "-")
        .replace('+', "")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

/// Find the most recent .json file in the given directory (non-recursive).
fn find_latest_json(dir: &str) -> Option<String> {
    let path = Path::new(dir);
    if !path.is_dir() {
        return None;
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    entries
        .last()
        .map(|e| e.path().to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Session log generation — human-readable input/output per method
// ---------------------------------------------------------------------------

/// Generate a plain text session log showing input and output for each method.
fn generate_session_txt(data: &Value) -> String {
    let mut l: Vec<String> = Vec::new();

    let settings = data.get("settings");
    let file = settings
        .and_then(|s| s.get("file"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let project = settings
        .and_then(|s| s.get("project"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    l.push(format!("# {} / {}", project, file));
    l.push(String::new());

    let benchmarks = match data.get("benchmarks").and_then(|b| b.as_array()) {
        Some(b) => b,
        None => return l.join("\n"),
    };

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let servers = match bench.get("servers").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => continue,
        };

        // Collect metrics from first server for the heading
        let srv = &servers[0];
        let p95 = srv.get("p95_ms").and_then(|v| v.as_f64());
        let rss = srv
            .get("rss_kb")
            .and_then(|v| v.as_u64())
            .filter(|&kb| kb > 0);
        let mut metrics: Vec<String> = Vec::new();
        if let Some(ms) = p95 {
            metrics.push(format_latency(ms));
        }
        if let Some(kb) = rss {
            metrics.push(format_memory(kb));
        }
        let metrics_str = if metrics.is_empty() {
            String::new()
        } else {
            format!(" ({})", metrics.join(", "))
        };

        l.push(format!("── {}{} ──", bench_name, metrics_str));
        l.push(String::new());

        // Request: full JSON-RPC (copy-pasteable)
        if let Some(input) = bench.get("input").and_then(|v| v.as_str()) {
            if let Ok(parsed) = serde_json::from_str::<Value>(input) {
                l.push(serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| input.to_string()));
            } else {
                l.push(input.to_string());
            }
            l.push(String::new());
        }

        // Response: compact one-liner per server
        for srv in servers {
            let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
            let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");

            let (tag, _) = classify_response(bench_name, srv);
            match status {
                "ok" => {
                    let response = parse_response(srv);
                    let summary = human_result(bench_name, srv);
                    if response.is_null() || response_is_empty(&response) {
                        l.push(format!("← {} [{}] {}", name, tag, summary));
                    } else {
                        let compact = compact_json(&response, 3);
                        let compact_short = if compact.len() > 200 {
                            format!("{}...", &compact[..197])
                        } else {
                            compact
                        };
                        l.push(format!("← {} [{}] {}", name, tag, compact_short));
                    }
                }
                _ => {
                    let label = classify_error_result(srv);
                    l.push(format!("← {} [{}] {}", name, tag, label));
                }
            }
        }

        l.push(String::new());
    }

    l.join("\n")
}

/// Generate a markdown session log for GitHub rendering.
fn generate_session_md(data: &Value) -> String {
    let mut l: Vec<String> = Vec::new();

    let settings = data.get("settings");
    let file = settings
        .and_then(|s| s.get("file"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let project = settings
        .and_then(|s| s.get("project"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    l.push(format!("# Session Log — {} / {}", project, file));
    l.push(String::new());

    let benchmarks = match data.get("benchmarks").and_then(|b| b.as_array()) {
        Some(b) => b,
        None => return l.join("\n"),
    };

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let servers = match bench.get("servers").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => continue,
        };

        l.push(format!("## {}", bench_name));
        l.push(String::new());

        // Show input
        if let Some(input) = bench.get("input").and_then(|v| v.as_str()) {
            l.push("**Request:**".into());
            l.push("```json".into());
            // Pretty-print the input JSON
            if let Ok(parsed) = serde_json::from_str::<Value>(input) {
                l.push(serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| input.to_string()));
            } else {
                l.push(input.to_string());
            }
            l.push("```".into());
            l.push(String::new());
        } else {
            let line = settings
                .and_then(|s| s.get("line"))
                .and_then(|v| v.as_u64());
            let col = settings.and_then(|s| s.get("col")).and_then(|v| v.as_u64());
            if let (Some(line), Some(col)) = (line, col) {
                l.push(format!(
                    "**Request:** `{}` at `{}:{}:{}`",
                    bench_name, file, line, col
                ));
            } else {
                l.push(format!("**Request:** `{}` on `{}`", bench_name, file));
            }
            l.push(String::new());
        }

        // Show responses
        l.push("**Responses:**".into());
        l.push(String::new());

        for srv in servers {
            let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
            let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let p95 = srv.get("p95_ms").and_then(|v| v.as_f64());
            let rss = srv
                .get("rss_kb")
                .and_then(|v| v.as_u64())
                .filter(|&kb| kb > 0);

            let mut metrics: Vec<String> = Vec::new();
            if let Some(ms) = p95 {
                metrics.push(format_latency(ms));
            }
            if let Some(kb) = rss {
                metrics.push(format_memory(kb));
            }
            let metrics_str = if metrics.is_empty() {
                String::new()
            } else {
                format!(" ({})", metrics.join(", "))
            };

            let (tag, has_content) = classify_response(bench_name, srv);
            match status {
                "ok" => {
                    let summary = human_result(bench_name, srv);
                    l.push(format!("**{}**{} — {}", name, metrics_str, summary));

                    let response = parse_response(srv);
                    if !response.is_null() && has_content {
                        let compact = compact_json(&response, 3);
                        let compact_short = if compact.len() > 120 {
                            format!("{}...", &compact[..117])
                        } else {
                            compact
                        };
                        let truncated = truncate_json(&response, 5);
                        let pretty =
                            serde_json::to_string_pretty(&truncated).unwrap_or_else(|_| "?".into());
                        l.push(String::new());
                        l.push("<details>".into());
                        l.push(format!(
                            "<summary>Summary: <code>{}</code></summary>",
                            compact_short
                        ));
                        l.push(String::new());
                        l.push("```json".into());
                        l.push(pretty);
                        l.push("```".into());
                        l.push("</details>".into());
                    } else if !response.is_null() {
                        // Error / empty — show compact inline
                        l.push(format!("\n`[{}]` `{}`", tag, compact_json(&response, 3)));
                    }
                }
                _ => {
                    let label = classify_error_result(srv);
                    l.push(format!("**{}**{} — {}", name, metrics_str, label));
                }
            }
            l.push(String::new());
        }

        l.push("---".into());
        l.push(String::new());
    }

    l.join("\n")
}
