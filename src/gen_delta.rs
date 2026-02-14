use serde_json::Value;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut json_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut base_server: Option<String> = None;
    let mut head_server: Option<String> = None;
    let mut quiet = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 < args.len() {
                    output_path = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: {} requires a path argument", args[i]);
                    std::process::exit(1);
                }
            }
            "--base" => {
                if i + 1 < args.len() {
                    base_server = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: {} requires a server name", args[i]);
                    std::process::exit(1);
                }
            }
            "--head" => {
                if i + 1 < args.len() {
                    head_server = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: {} requires a server name", args[i]);
                    std::process::exit(1);
                }
            }
            "-q" | "--quiet" => {
                quiet = true;
                i += 1;
            }
            "-h" | "--help" => {
                eprintln!("Usage: gen-delta [OPTIONS] [INPUT]");
                eprintln!();
                eprintln!("Generate compact delta comparison table from benchmark JSON.");
                eprintln!();
                eprintln!("Arguments:");
                eprintln!("  INPUT   Path to benchmark JSON (default: latest in benchmarks/)");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  -o, --output <path>    Write table to file (default: stdout only)");
                eprintln!("  --base <server>        Baseline server (default: first server)");
                eprintln!(
                    "  --head <server>        Head server to compare (default: second server)"
                );
                eprintln!("  -q, --quiet            Don't print table to stdout");
                eprintln!("  -h, --help             Show this help");
                std::process::exit(0);
            }
            _ => {
                if args[i].starts_with('-') {
                    eprintln!("Unknown flag: {}", args[i]);
                    std::process::exit(1);
                }
                if json_path.is_none() {
                    json_path = Some(args[i].clone());
                } else if output_path.is_none() {
                    output_path = Some(args[i].clone());
                } else {
                    eprintln!("Unexpected argument: {}", args[i]);
                    std::process::exit(1);
                }
                i += 1;
            }
        }
    }

    let json_path = match json_path {
        Some(p) if Path::new(&p).is_dir() => find_latest_json(&p).unwrap_or_else(|| {
            eprintln!("No JSON files found in {}/", p);
            std::process::exit(1);
        }),
        Some(p) => p,
        None => find_latest_json("benchmarks").unwrap_or_else(|| {
            eprintln!("No JSON files found in benchmarks/");
            eprintln!("Usage: gen-delta [OPTIONS] [path/to/benchmark.json]");
            std::process::exit(1);
        }),
    };

    let content = std::fs::read_to_string(&json_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", json_path, e);
        std::process::exit(1);
    });
    let data: Value = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Error parsing JSON: {}", e);
        std::process::exit(1);
    });

    // Discover servers from the JSON
    let server_entries = data["servers"].as_array().cloned().unwrap_or_default();
    let server_names: Vec<String> = server_entries
        .iter()
        .filter_map(|s| s["name"].as_str().map(String::from))
        .collect();

    let find_server_meta = |name: &str| -> Option<&Value> {
        server_entries
            .iter()
            .find(|s| s["name"].as_str() == Some(name))
    };

    if server_names.len() < 2 {
        eprintln!(
            "Error: need at least 2 servers for delta comparison, found {}",
            server_names.len()
        );
        std::process::exit(1);
    }

    let base = base_server.unwrap_or_else(|| server_names[0].clone());
    let head = head_server.unwrap_or_else(|| server_names[1].clone());

    if !server_names.contains(&base) {
        eprintln!(
            "Error: base server '{}' not found. Available: {}",
            base,
            server_names.join(", ")
        );
        std::process::exit(1);
    }
    if !server_names.contains(&head) {
        eprintln!(
            "Error: head server '{}' not found. Available: {}",
            head,
            server_names.join(", ")
        );
        std::process::exit(1);
    }

    let benchmarks = data["benchmarks"].as_array().unwrap_or_else(|| {
        eprintln!("Error: no benchmarks array in JSON");
        std::process::exit(1);
    });

    // Collect rows: (name, base_ms, head_ms, delta, base_rss, head_rss)
    struct Row {
        name: String,
        base_ms: String,
        head_ms: String,
        delta: String,
        base_rss: String,
        head_rss: String,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut has_rss = false;

    for bench in benchmarks {
        let name = bench["name"].as_str().unwrap_or("?");
        let servers = match bench["servers"].as_array() {
            Some(s) => s,
            None => continue,
        };

        let find_server = |label: &str| -> Option<&Value> {
            servers.iter().find(|s| s["server"].as_str() == Some(label))
        };

        let base_entry = find_server(&base);
        let head_entry = find_server(&head);

        let base_ms = base_entry.and_then(|e| {
            if e["status"].as_str() == Some("ok") {
                e["mean_ms"].as_f64()
            } else {
                None
            }
        });
        let head_ms = head_entry.and_then(|e| {
            if e["status"].as_str() == Some("ok") {
                e["mean_ms"].as_f64()
            } else {
                None
            }
        });

        let (base_str, head_str, delta_str) = match (base_ms, head_ms) {
            (Some(b), Some(h)) => {
                let delta = format_delta(b, h);
                (format_ms(b), format_ms(h), delta)
            }
            (Some(b), None) => {
                let status = head_entry
                    .and_then(|e| e["status"].as_str())
                    .unwrap_or("--");
                (format_ms(b), status.to_string(), "--".to_string())
            }
            (None, Some(h)) => {
                let status = base_entry
                    .and_then(|e| e["status"].as_str())
                    .unwrap_or("--");
                (status.to_string(), format_ms(h), "--".to_string())
            }
            (None, None) => ("--".to_string(), "--".to_string(), "--".to_string()),
        };

        let base_rss = base_entry
            .and_then(|e| e["rss_kb"].as_u64())
            .map(format_rss)
            .unwrap_or_default();
        let head_rss = head_entry
            .and_then(|e| e["rss_kb"].as_u64())
            .map(format_rss)
            .unwrap_or_default();

        if !base_rss.is_empty() || !head_rss.is_empty() {
            has_rss = true;
        }

        rows.push(Row {
            name: name.to_string(),
            base_ms: base_str,
            head_ms: head_str,
            delta: delta_str,
            base_rss,
            head_rss,
        });
    }

    // Build the report
    let mut table = String::new();

    // Server info header
    for name in [&base, &head] {
        if let Some(meta) = find_server_meta(name) {
            let version = meta["version"].as_str().unwrap_or("");
            let link = meta["link"].as_str().unwrap_or("");
            let description = meta["description"].as_str().unwrap_or("");
            let short_commit = extract_short_commit(version);

            table.push_str(&format!("**{}**", name));
            if !short_commit.is_empty() {
                table.push_str(&format!(" · `{}`", short_commit));
            }
            if !link.is_empty() {
                if !description.is_empty() {
                    table.push_str(&format!(" · [{}]({})", description, link));
                } else {
                    table.push_str(&format!(" · [link]({})", link));
                }
            } else if !description.is_empty() {
                table.push_str(&format!(" · {}", description));
            }
            table.push('\n');
        }
    }
    table.push('\n');

    // Use short commit hash as column header when available
    let base_col_label = find_server_meta(&base)
        .and_then(|m| m["version"].as_str())
        .map(|v| {
            let short = extract_short_commit(v);
            if short != v {
                short
            } else {
                base.clone()
            }
        })
        .unwrap_or_else(|| base.clone());
    let head_col_label = find_server_meta(&head)
        .and_then(|m| m["version"].as_str())
        .map(|v| {
            let short = extract_short_commit(v);
            if short != v {
                short
            } else {
                head.clone()
            }
        })
        .unwrap_or_else(|| head.clone());

    // Compute column widths
    let col0 = "Benchmark"
        .len()
        .max(rows.iter().map(|r| r.name.len()).max().unwrap_or(0));
    let col1 = base_col_label
        .len()
        .max(rows.iter().map(|r| r.base_ms.len()).max().unwrap_or(0));
    let col2 = head_col_label
        .len()
        .max(rows.iter().map(|r| r.head_ms.len()).max().unwrap_or(0));
    let col3 = "Delta"
        .len()
        .max(rows.iter().map(|r| r.delta.len()).max().unwrap_or(0));

    if has_rss {
        // RSS column headers use "RSS base" / "RSS head"
        let rss_base_hdr = format!("RSS {}", base_col_label);
        let rss_head_hdr = format!("RSS {}", head_col_label);
        let col4 = rss_base_hdr
            .len()
            .max(rows.iter().map(|r| r.base_rss.len()).max().unwrap_or(0));
        let col5 = rss_head_hdr
            .len()
            .max(rows.iter().map(|r| r.head_rss.len()).max().unwrap_or(0));

        // Header
        table.push_str(&format!(
            "| {:<col0$} | {:>col1$} | {:>col2$} | {:>col3$} | {:>col4$} | {:>col5$} |\n",
            "Benchmark", base_col_label, head_col_label, "Delta", rss_base_hdr, rss_head_hdr
        ));
        table.push_str(&format!(
            "|{:-<w0$}|{:->w1$}|{:->w2$}|{:->w3$}|{:->w4$}|{:->w5$}|\n",
            "",
            "",
            "",
            "",
            "",
            "",
            w0 = col0 + 2,
            w1 = col1 + 2,
            w2 = col2 + 2,
            w3 = col3 + 2,
            w4 = col4 + 2,
            w5 = col5 + 2,
        ));

        // Data rows
        for r in &rows {
            let base_rss = if r.base_rss.is_empty() {
                "--"
            } else {
                &r.base_rss
            };
            let head_rss = if r.head_rss.is_empty() {
                "--"
            } else {
                &r.head_rss
            };
            table.push_str(&format!(
                "| {:<col0$} | {:>col1$} | {:>col2$} | {:>col3$} | {:>col4$} | {:>col5$} |\n",
                r.name, r.base_ms, r.head_ms, r.delta, base_rss, head_rss
            ));
        }
    } else {
        // No RSS data — latency-only table
        table.push_str(&format!(
            "| {:<col0$} | {:>col1$} | {:>col2$} | {:>col3$} |\n",
            "Benchmark", base_col_label, head_col_label, "Delta"
        ));
        table.push_str(&format!(
            "|{:-<w0$}|{:->w1$}|{:->w2$}|{:->w3$}|\n",
            "",
            "",
            "",
            "",
            w0 = col0 + 2,
            w1 = col1 + 2,
            w2 = col2 + 2,
            w3 = col3 + 2,
        ));

        for r in &rows {
            table.push_str(&format!(
                "| {:<col0$} | {:>col1$} | {:>col2$} | {:>col3$} |\n",
                r.name, r.base_ms, r.head_ms, r.delta,
            ));
        }
    }

    if !quiet {
        print!("{}", table);
    }

    if let Some(path) = output_path {
        std::fs::write(&path, &table).unwrap_or_else(|e| {
            eprintln!("Error writing {}: {}", path, e);
            std::process::exit(1);
        });
        eprintln!("Wrote {}", path);
    }
}

fn format_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else if ms >= 10.0 {
        format!("{:.1}ms", ms)
    } else {
        format!("{:.2}ms", ms)
    }
}

fn format_rss(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    format!("{:.1}MB", mb)
}

fn format_delta(base_ms: f64, head_ms: f64) -> String {
    if base_ms <= 0.0 || head_ms <= 0.0 {
        return "--".to_string();
    }
    let ratio = base_ms / head_ms;
    // Within 5% → tied
    if (ratio - 1.0).abs() < 0.05 {
        "1.0x (tied)".to_string()
    } else if ratio > 1.0 {
        // head is faster (lower ms)
        format!("{:.1}x faster", ratio)
    } else {
        // head is slower
        format!("{:.1}x slower", 1.0 / ratio)
    }
}

/// Extract a short commit hash from a version string.
/// Handles formats like:
///   "solidity-language-server 0.1.14+commit.3d6a3d1.macos.aarch64" -> "3d6a3d1"
///   "0.8.33+commit.64118f21.Darwin.appleclang" -> "64118f21"
/// Falls back to the full version string if no commit pattern found.
fn extract_short_commit(version: &str) -> String {
    // Look for "+commit.<hash>" pattern
    if let Some(pos) = version.find("+commit.") {
        let after = &version[pos + 8..]; // skip "+commit."
        let end = after.find('.').unwrap_or(after.len());
        return after[..end].to_string();
    }
    // Fall back: if version looks like a bare SHA (7-40 hex chars), use it
    let trimmed = version.trim();
    if trimmed.len() >= 7 && trimmed.len() <= 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return trimmed.to_string();
    }
    // Otherwise return the full version
    version.to_string()
}

fn find_latest_json(dir: &str) -> Option<String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
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
