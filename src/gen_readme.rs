use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse flags
    let mut json_path: Option<String> = None;
    let mut output_path = "README.md".to_string();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 < args.len() {
                    output_path = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Error: {} requires a path argument", args[i]);
                    std::process::exit(1);
                }
            }
            "-h" | "--help" => {
                eprintln!("Usage: gen-readme [OPTIONS] [path/to/benchmark.json]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  -o, --output <path>  Output file path (default: README.md)");
                eprintln!("  -h, --help           Show this help");
                eprintln!();
                eprintln!("If no JSON path is given, uses the latest file in benchmarks/");
                std::process::exit(0);
            }
            _ => {
                if args[i].starts_with('-') {
                    eprintln!("Unknown flag: {}", args[i]);
                    std::process::exit(1);
                }
                json_path = Some(args[i].clone());
                i += 1;
            }
        }
    }

    // Find the JSON file to use: explicit path or latest in benchmarks/
    let json_path = json_path.unwrap_or_else(|| {
        find_latest_json("benchmarks").unwrap_or_else(|| {
            eprintln!("No JSON files found in benchmarks/");
            eprintln!("Usage: gen-readme [OPTIONS] [path/to/benchmark.json]");
            std::process::exit(1);
        })
    });

    eprintln!("Reading: {}", json_path);
    let content = std::fs::read_to_string(&json_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", json_path, e);
        std::process::exit(1);
    });
    let data: Value = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Error parsing JSON: {}", e);
        std::process::exit(1);
    });

    let md = generate_readme(&data, &json_path);
    std::fs::write(&output_path, &md).unwrap();
    println!("{}", md);
    eprintln!("  -> {}", output_path);
}

// ---------------------------------------------------------------------------
// README generation
// ---------------------------------------------------------------------------

fn generate_readme(data: &Value, json_path: &str) -> String {
    let mut l: Vec<String> = Vec::new();

    // ── Title ──────────────────────────────────────────────────────────
    l.push("# Solidity LSP Benchmarks".into());
    l.push(String::new());
    l.push(
        "Benchmarks comparing Solidity LSP servers against Uniswap V4-core \
         (`Pool.sol`, 618 lines)."
            .into(),
    );
    l.push(String::new());

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
        let index_timeout = settings
            .get("index_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let file = settings
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or("src/libraries/Pool.sol");
        let line = settings.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
        let col = settings.get("col").and_then(|v| v.as_u64()).unwrap_or(0);

        l.push("## Settings".into());
        l.push(String::new());
        l.push("| Setting | Value |".into());
        l.push("|---------|-------|".into());
        l.push(format!("| File | `{}` |", file));
        l.push(format!("| Target position | line {}, col {} |", line, col));
        l.push(format!("| Iterations | {} |", iterations));
        l.push(format!("| Warmup | {} |", warmup));
        l.push(format!("| Request timeout | {}s |", timeout));
        l.push(format!("| Index timeout | {}s |", index_timeout));
        l.push(String::new());
    }

    // ── Servers ────────────────────────────────────────────────────────
    if let Some(servers) = data.get("servers").and_then(|s| s.as_array()) {
        l.push("## Servers".into());
        l.push(String::new());
        l.push("| Server | Description | Version |".into());
        l.push("|--------|-------------|---------|".into());
        for srv in servers {
            let name = srv.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let link = srv.get("link").and_then(|v| v.as_str()).unwrap_or("");
            let description = srv
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let version = srv.get("version").and_then(|v| v.as_str()).unwrap_or("?");
            let name_cell = if link.is_empty() {
                name.to_string()
            } else {
                format!("[{}]({})", name, link)
            };
            l.push(format!(
                "| {} | {} | `{}` |",
                name_cell, description, version
            ));
        }
        l.push(String::new());
    }

    // ── Summary table (medals + trophy) ────────────────────────────────
    let benchmarks = data.get("benchmarks").and_then(|b| b.as_array());
    if let Some(benchmarks) = benchmarks {
        if !benchmarks.is_empty() {
            let server_names = collect_server_names(benchmarks);
            let medal_icons = ["\u{1F947}", "\u{1F948}", "\u{1F949}"]; // 🥇🥈🥉

            // Pre-compute medals & wins
            let mut wins: HashMap<String, usize> = HashMap::new();
            let mut all_medals: Vec<Vec<&str>> = Vec::new();

            for bench in benchmarks {
                let (row_medals, winner) = rank_servers(bench, &medal_icons);
                if let Some(name) = winner {
                    *wins.entry(name).or_insert(0) += 1;
                }
                all_medals.push(row_medals);
            }

            let trophy_winner = wins
                .iter()
                .max_by_key(|(_, c)| *c)
                .map(|(name, _)| name.clone());

            l.push("## Results".into());
            l.push(String::new());

            // Header row
            let mut header = "| Benchmark |".to_string();
            let mut sep = "|-----------|".to_string();
            for name in &server_names {
                let trophy = if trophy_winner.as_deref() == Some(*name) {
                    " \u{1F3C6}"
                } else {
                    ""
                };
                header.push_str(&format!(" {}{} |", name, trophy));
                sep.push_str(&"-".repeat(name.len() + trophy.len() + 2));
                sep.push('|');
            }
            l.push(header);
            l.push(sep);

            // Data rows
            for (i, bench) in benchmarks.iter().enumerate() {
                let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let mut row = format!("| [{}](#{}) |", bench_name, slug(bench_name));

                if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                    for (j, srv) in servers.iter().enumerate() {
                        let cell = format_summary_cell(srv, i, j, &all_medals);
                        row.push_str(&cell);
                    }
                }
                l.push(row);
            }
            l.push(String::new());

            // ── Winner summary ─────────────────────────────────────────
            if let Some(ref winner) = trophy_winner {
                let total = benchmarks.len();
                let gold = wins.get(winner.as_str()).copied().unwrap_or(0);

                // Count silver/bronze per server
                let mut silvers: HashMap<String, usize> = HashMap::new();
                let mut bronzes: HashMap<String, usize> = HashMap::new();
                for row in &all_medals {
                    for (idx, medal) in row.iter().enumerate() {
                        if let Some(name) = server_names.get(idx) {
                            match *medal {
                                "\u{1F948}" => *silvers.entry(name.to_string()).or_insert(0) += 1,
                                "\u{1F949}" => *bronzes.entry(name.to_string()).or_insert(0) += 1,
                                _ => {}
                            }
                        }
                    }
                }

                l.push(format!(
                    "> **\u{1F3C6} Overall Winner: {}** \u{2014} {} \u{1F947} out of {} benchmarks",
                    winner, gold, total,
                ));
                l.push(String::new());

                // Medal tally table
                l.push("### Medal Tally".into());
                l.push(String::new());
                l.push(
                    "| Server | \u{1F947} Gold | \u{1F948} Silver | \u{1F949} Bronze | Score |"
                        .into(),
                );
                l.push("|--------|------|----------|----------|-------|".into());

                // Build rows sorted by weighted score (gold=3, silver=2, bronze=1)
                let mut tally: Vec<(&str, usize, usize, usize)> = server_names
                    .iter()
                    .map(|name| {
                        let g = wins.get(*name).copied().unwrap_or(0);
                        let s = silvers.get(*name).copied().unwrap_or(0);
                        let b = bronzes.get(*name).copied().unwrap_or(0);
                        (*name, g, s, b)
                    })
                    .collect();
                tally.sort_by(|a, b| {
                    let score_a = a.1 * 3 + a.2 * 2 + a.3;
                    let score_b = b.1 * 3 + b.2 * 2 + b.3;
                    score_b.cmp(&score_a)
                });

                for (name, g, s, b) in &tally {
                    let score = g * 3 + s * 2 + b;
                    let marker = if trophy_winner.as_deref() == Some(*name) {
                        " \u{1F3C6}"
                    } else {
                        ""
                    };
                    l.push(format!(
                        "| **{}**{} | {} | {} | {} | {} |",
                        name, marker, g, s, b, score
                    ));
                }
                l.push(String::new());
            }

            // ── Feature support matrix ─────────────────────────────────
            l.push("## Feature Support".into());
            l.push(String::new());

            let mut header = "| Feature |".to_string();
            let mut sep = "|---------|".to_string();
            for name in &server_names {
                header.push_str(&format!(" {} |", name));
                sep.push_str(&"-".repeat(name.len() + 2));
                sep.push('|');
            }
            l.push(header);
            l.push(sep);

            for bench in benchmarks.iter() {
                let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let mut row = format!("| {} |", bench_name);
                if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                    for srv in servers {
                        let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
                        let response = srv.get("response").and_then(|v| v.as_str()).unwrap_or("");
                        let error = srv.get("error").and_then(|v| v.as_str()).unwrap_or("");
                        let icon = if status == "ok"
                            && response != "null"
                            && response != "[]"
                            && !response.is_empty()
                        {
                            "\u{2705}" // ✅
                        } else if response.contains("Unknown method")
                            || response.contains("unsupported")
                        {
                            "\u{274C}" // ❌
                        } else if error.contains("timeout") {
                            "\u{23F3}" // ⏳
                        } else if status == "ok" {
                            "\u{26A0}\u{FE0F}" // ⚠️ (returned empty/null)
                        } else {
                            "\u{274C}" // ❌
                        };
                        row.push_str(&format!(" {} |", icon));
                    }
                }
                l.push(row);
            }
            l.push(String::new());
            l.push(
                "> \u{2705} = valid response \u{2003} \
                 \u{26A0}\u{FE0F} = empty/null result \u{2003} \
                 \u{23F3} = timeout \u{2003} \
                 \u{274C} = unsupported / failed"
                    .into(),
            );
            l.push(String::new());

            // ── Per-benchmark detail sections ──────────────────────────
            l.push("---".into());
            l.push(String::new());
            l.push("## Detailed Results".into());
            l.push(String::new());

            for bench in benchmarks.iter() {
                let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                l.push(format!("### {}", bench_name));
                l.push(String::new());

                if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                    // Latency table
                    l.push("| Server | Status | Mean | P50 | P95 |".into());
                    l.push("|--------|--------|------|-----|-----|".into());
                    for srv in servers {
                        let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                        let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
                        let status_display = match status {
                            "ok" => "\u{2705} ok".to_string(),
                            "invalid" => "\u{26A0}\u{FE0F} invalid".to_string(),
                            _ => {
                                let err =
                                    srv.get("error").and_then(|v| v.as_str()).unwrap_or("fail");
                                format!("\u{274C} {}", err)
                            }
                        };
                        let mean = format_ms(srv.get("mean_ms"));
                        let p50 = format_ms(srv.get("p50_ms"));
                        let p95 = format_ms(srv.get("p95_ms"));
                        l.push(format!(
                            "| **{}** | {} | {} | {} | {} |",
                            name, status_display, mean, p50, p95
                        ));
                    }
                    l.push(String::new());

                    // Response details per server
                    l.push("<details>".into());
                    l.push("<summary>Response details</summary>".into());
                    l.push(String::new());
                    for srv in servers {
                        let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                        let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");

                        l.push(format!("**{}**", name));
                        l.push(String::new());

                        match status {
                            "ok" | "invalid" => {
                                let response = srv
                                    .get("response")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("(no response)");
                                let truncated = truncate_response(response, 200);
                                l.push("```json".into());
                                l.push(truncated);
                                l.push("```".into());
                            }
                            _ => {
                                let error = srv
                                    .get("error")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown error");
                                l.push(format!("Error: `{}`", error));
                            }
                        }
                        l.push(String::new());
                    }
                    l.push("</details>".into());
                    l.push(String::new());
                }
            }
        }
    }

    // ── Footer ─────────────────────────────────────────────────────────
    l.push("---".into());
    l.push(String::new());
    if let Some(ts) = data.get("timestamp").and_then(|t| t.as_str()) {
        l.push(format!(
            "*Generated from [`{}`]({}) — benchmark run: {}*",
            json_path, json_path, ts
        ));
        l.push(String::new());
    }

    l.push("See [DOCS.md](./DOCS.md) for usage and installation.".into());
    l.push(String::new());

    l.join("\n")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Rank servers by mean latency. Returns (medals_vec, winner_name).
fn rank_servers<'a>(bench: &Value, medal_icons: &[&'a str]) -> (Vec<&'a str>, Option<String>) {
    let servers = match bench.get("servers").and_then(|s| s.as_array()) {
        Some(s) => s,
        None => return (vec![], None),
    };

    let mut ranked: Vec<(usize, f64)> = servers
        .iter()
        .enumerate()
        .filter(|(_, s)| is_valid_result(s))
        .filter_map(|(i, s)| s.get("mean_ms").and_then(|v| v.as_f64()).map(|m| (i, m)))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let mut row_medals = vec![""; servers.len()];
    let mut winner = None;
    for (place, (idx, _)) in ranked.iter().enumerate() {
        if place < medal_icons.len() {
            row_medals[*idx] = medal_icons[place];
        }
        if place == 0 {
            winner = servers[*idx]
                .get("server")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
        }
    }
    (row_medals, winner)
}

/// Check if a server result is valid (ok status + non-empty, non-null response).
fn is_valid_result(srv: &Value) -> bool {
    let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let response = srv.get("response").and_then(|v| v.as_str()).unwrap_or("");
    status == "ok" && response != "null" && response != "no result" && !response.is_empty()
}

/// Format a summary table cell.
fn format_summary_cell(
    srv: &Value,
    bench_idx: usize,
    srv_idx: usize,
    all_medals: &[Vec<&str>],
) -> String {
    let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
    match status {
        "ok" => {
            let mean = srv.get("mean_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let medal = if bench_idx < all_medals.len() && srv_idx < all_medals[bench_idx].len() {
                all_medals[bench_idx][srv_idx]
            } else {
                ""
            };
            let suffix = if medal.is_empty() {
                String::new()
            } else {
                format!(" {}", medal)
            };
            format!(" {:.1}ms{} |", mean, suffix)
        }
        "invalid" => {
            let response = srv.get("response").and_then(|v| v.as_str()).unwrap_or("");
            if response.contains("Unknown method") || response.contains("unsupported") {
                " unsupported |".to_string()
            } else {
                " - |".to_string()
            }
        }
        _ => {
            let error = srv.get("error").and_then(|v| v.as_str()).unwrap_or("");
            if error.contains("timeout") {
                " timeout |".to_string()
            } else {
                " FAIL |".to_string()
            }
        }
    }
}

/// Truncate a response string to max_chars, appending "..." if truncated.
fn truncate_response(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    // Find a clean break point (end of line) near the limit
    let truncated = &s[..max_chars];
    let break_at = truncated.rfind('\n').unwrap_or(max_chars);
    format!("{}...", &s[..break_at])
}

/// Format an optional millisecond value.
fn format_ms(val: Option<&Value>) -> String {
    match val.and_then(|v| v.as_f64()) {
        Some(ms) => format!("{:.1}ms", ms),
        None => "-".to_string(),
    }
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
