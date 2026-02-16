use clap::Parser;
use serde_json::Value;
use std::path::Path;

#[derive(Parser)]
#[command(name = "gen-analysis", version = env!("LONG_VERSION"))]
#[command(about = "Generate per-feature analysis report from benchmark JSON")]
struct Cli {
    /// Path to benchmark JSON (default: latest in benchmarks/)
    input: Option<String>,

    /// Output file path
    #[arg(short, long, default_value = "ANALYSIS.md")]
    output: String,

    /// Server for head-to-head comparison (default: first server)
    #[arg(long)]
    base: Option<String>,

    /// Don't print analysis to stdout
    #[arg(short, long)]
    quiet: bool,
}

fn main() {
    let cli = Cli::parse();
    let output_path = cli.output;
    let lead_server = cli.base;
    let quiet = cli.quiet;

    let json_path = match cli.input {
        Some(p) if Path::new(&p).is_dir() => find_latest_json(&p).unwrap_or_else(|| {
            eprintln!("No JSON files found in {}/", p);
            std::process::exit(1);
        }),
        Some(p) => p,
        None => find_latest_json("benchmarks").unwrap_or_else(|| {
            eprintln!("No JSON files found in benchmarks/");
            eprintln!("Usage: gen-analysis [OPTIONS] [path/to/benchmark.json]");
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

    let md = generate_analysis(&data, &json_path, lead_server.as_deref());
    std::fs::write(&output_path, &md).unwrap();
    if !quiet {
        println!("{}", md);
    }
    eprintln!("  -> {}", output_path);
}

// ---------------------------------------------------------------------------
// Analysis generation
// ---------------------------------------------------------------------------

fn generate_analysis(data: &Value, json_path: &str, lead_override: Option<&str>) -> String {
    let mut l: Vec<String> = Vec::new();

    // ── Title ────────────────────────────────────────────────────────────
    l.push("# Benchmark Analysis".into());
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
        let iterations = settings
            .get("iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        l.push(format!(
            "Analysis of `{}` (`{}`) — {} iterations per benchmark.",
            project, file, iterations
        ));
        l.push(String::new());
    }

    // ── Servers table ──────────────────────────────────────────────────
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

    let benchmarks = match data.get("benchmarks").and_then(|b| b.as_array()) {
        Some(b) => b,
        None => {
            l.push("No benchmark data found.".into());
            return l.join("\n");
        }
    };

    let server_names = collect_server_names(benchmarks);

    // ── Resolve base server for head-to-head ─────────────────────────────
    let lead_name: Option<&str> = if let Some(override_name) = lead_override {
        if server_names.iter().any(|n| *n == override_name) {
            Some(override_name)
        } else {
            eprintln!(
                "Warning: --base '{}' not found in servers, using first server",
                override_name
            );
            server_names.first().copied()
        }
    } else {
        server_names.first().copied()
    };

    // ── 1. Capability Matrix (global) ────────────────────────────────────
    l.push("## Capability Matrix".into());
    l.push(String::new());

    let mut header = "| Benchmark |".to_string();
    let mut sep = "|-----------|".to_string();
    for name in &server_names {
        header.push_str(&format!(" {} |", name));
        sep.push_str(&"-".repeat(name.len() + 2));
        sep.push('|');
    }
    l.push(header);
    l.push(sep);

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let mut row = format!("| {} |", bench_name);
        if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
            for srv in servers {
                let cell = server_status_label(srv);
                row.push_str(&format!(" {} |", cell));
            }
        }
        l.push(row);
    }
    l.push(String::new());

    // Capability summary
    let total_benchmarks = benchmarks.len();
    l.push("| Server | Working | Failed | Success Rate |".into());
    l.push("|--------|---------|--------|--------------|".into());

    for name in &server_names {
        let mut ok_count = 0;
        let mut fail_count = 0;
        for bench in benchmarks {
            if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                for srv in servers {
                    let srv_name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("");
                    if srv_name != *name {
                        continue;
                    }
                    if server_status_label(srv) == "ok" {
                        ok_count += 1;
                    } else {
                        fail_count += 1;
                    }
                }
            }
        }
        let rate = if total_benchmarks > 0 {
            (ok_count as f64 / total_benchmarks as f64) * 100.0
        } else {
            0.0
        };
        l.push(format!(
            "| {} | {}/{} | {}/{} | {:.0}% |",
            name, ok_count, total_benchmarks, fail_count, total_benchmarks, rate
        ));
    }
    l.push(String::new());

    // ── 2. Per-feature sections ──────────────────────────────────────────
    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let servers = match bench.get("servers").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => continue,
        };

        l.push(format!("## {}", bench_name));
        l.push(String::new());

        // Find fastest mean among ok servers (for overhead calc)
        let fastest_mean: Option<f64> = servers
            .iter()
            .filter(|s| server_status_label(s) == "ok")
            .filter_map(|s| s.get("mean_ms").and_then(|v| v.as_f64()))
            .fold(None, |min, val| Some(min.map_or(val, |m: f64| m.min(val))));

        // Check what data is available for this benchmark
        let has_p50 = servers
            .iter()
            .any(|s| s.get("p50_ms").and_then(|v| v.as_f64()).is_some());
        let has_iterations = servers
            .iter()
            .any(|s| s.get("iterations").and_then(|v| v.as_array()).is_some());
        let has_rss = servers
            .iter()
            .any(|s| s.get("rss_kb").and_then(|v| v.as_u64()).is_some());

        // Build dynamic columns
        // Always: Server, Status, Mean
        // Conditional: p50, p95, Spread, Spike (if p50 exists)
        // Conditional: Min, Max, Range (if iterations exist)
        // Conditional: Overhead (if fastest_mean exists and >1 ok server)
        // Conditional: RSS (if rss data exists)
        // Conditional: vs Base (if lead_name set and >1 server)
        let ok_count = servers
            .iter()
            .filter(|s| server_status_label(s) == "ok")
            .count();
        let show_overhead = fastest_mean.is_some() && ok_count > 1;
        let show_h2h = lead_name.is_some() && server_names.len() > 1;

        let mut hdr = "| Server | Status |".to_string();
        let mut div = "|--------|--------|".to_string();
        if has_rss {
            hdr.push_str(" Mem |");
            div.push_str("-----|");
        }
        hdr.push_str(" Mean |");
        div.push_str("------|");
        if has_p50 {
            hdr.push_str(" p50 | p95 | Spread | Spike |");
            div.push_str("-----|-----|--------|-------|");
        }
        if has_iterations {
            hdr.push_str(" Min | Max | Range |");
            div.push_str("-----|-----|-------|");
        }
        if show_overhead {
            hdr.push_str(" Overhead |");
            div.push_str("----------|");
        }
        if show_h2h {
            hdr.push_str(&format!(" vs {} |", lead_name.unwrap()));
            div.push_str(&"-".repeat(lead_name.unwrap().len() + 5));
            div.push('|');
        }
        l.push(hdr);
        l.push(div);

        // Find lead server's mean for head-to-head
        let lead_mean: Option<f64> = lead_name.and_then(|lead| {
            servers
                .iter()
                .find(|s| s.get("server").and_then(|v| v.as_str()).unwrap_or("") == lead)
                .filter(|s| server_status_label(s) == "ok")
                .and_then(|s| s.get("mean_ms").and_then(|v| v.as_f64()))
        });

        for srv in servers {
            let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
            let status_label = server_status_label(srv);
            let mean = srv.get("mean_ms").and_then(|v| v.as_f64());

            let mean_str = match mean {
                Some(m) if status_label == "ok" => format!("{:.2}ms", m),
                _ => "-".to_string(),
            };

            let mut row = format!("| {} | {} |", name, status_label);

            // Mem (RSS) — right after status
            if has_rss {
                if let Some(rss) = srv.get("rss_kb").and_then(|v| v.as_u64()) {
                    let mb = rss as f64 / 1024.0;
                    row.push_str(&format!(" {:.1} MB |", mb));
                } else {
                    row.push_str(" - |");
                }
            }

            row.push_str(&format!(" {} |", mean_str));

            // p50/p95/spread/spike
            if has_p50 {
                let p50 = srv.get("p50_ms").and_then(|v| v.as_f64());
                let p95 = srv.get("p95_ms").and_then(|v| v.as_f64());
                match (p50, p95) {
                    (Some(p50v), Some(p95v)) => {
                        let spread = p95v - p50v;
                        let spike = if p50v > 0.0 { p95v / p50v } else { 1.0 };
                        let spread_str = if spread > 10.0 {
                            format!("**{:.1}ms**", spread)
                        } else {
                            format!("{:.1}ms", spread)
                        };
                        let spike_str = if spike > 1.5 {
                            format!("**{:.2}x**", spike)
                        } else {
                            format!("{:.2}x", spike)
                        };
                        row.push_str(&format!(
                            " {:.1}ms | {:.1}ms | {} | {} |",
                            p50v, p95v, spread_str, spike_str
                        ));
                    }
                    _ => {
                        row.push_str(" - | - | - | - |");
                    }
                }
            }

            // min/max/range from iterations
            if has_iterations {
                if let Some(iterations) = srv.get("iterations").and_then(|v| v.as_array()) {
                    let latencies: Vec<f64> = iterations
                        .iter()
                        .filter_map(|it| it.get("ms").and_then(|v| v.as_f64()))
                        .collect();
                    if latencies.is_empty() {
                        row.push_str(" - | - | - |");
                    } else {
                        let min = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        let range = max - min;
                        let range_str = if range > 10.0 {
                            format!("**{:.2}ms**", range)
                        } else {
                            format!("{:.2}ms", range)
                        };
                        row.push_str(&format!(" {:.2}ms | {:.2}ms | {} |", min, max, range_str));
                    }
                } else {
                    row.push_str(" - | - | - |");
                }
            }

            // overhead vs fastest
            if show_overhead {
                if let (Some(m), Some(f)) = (mean, fastest_mean) {
                    if status_label == "ok" {
                        let overhead = if f > 0.0 { m / f } else { 1.0 };
                        let overhead_str = if (overhead - 1.0).abs() < 0.01 {
                            "**1.0x (fastest)**".to_string()
                        } else if overhead > 10.0 {
                            format!("**{:.1}x**", overhead)
                        } else {
                            format!("{:.1}x", overhead)
                        };
                        row.push_str(&format!(" {} |", overhead_str));
                    } else {
                        row.push_str(" - |");
                    }
                } else {
                    row.push_str(" - |");
                }
            }

            // head-to-head vs base
            if show_h2h {
                let is_lead = lead_name.map_or(false, |lead| name == lead);
                if is_lead {
                    row.push_str(" - |");
                } else {
                    let srv_mean = if status_label == "ok" { mean } else { None };
                    let error = srv.get("error").and_then(|v| v.as_str()).unwrap_or("");
                    match (lead_mean, srv_mean) {
                        (Some(lm), Some(cm)) => {
                            if lm <= 0.0 || cm <= 0.0 || (lm - cm).abs() < 0.01 {
                                row.push_str(" tied |");
                            } else if cm < lm {
                                // This server is faster than base
                                let factor = lm / cm;
                                if factor > 10.0 {
                                    row.push_str(&format!(" **{:.1}x faster** |", factor));
                                } else {
                                    row.push_str(&format!(" {:.1}x faster |", factor));
                                }
                            } else {
                                // This server is slower than base
                                let factor = cm / lm;
                                if factor > 10.0 {
                                    row.push_str(&format!(" **{:.1}x slower** |", factor));
                                } else {
                                    row.push_str(&format!(" {:.1}x slower |", factor));
                                }
                            }
                        }
                        (Some(_), None) => {
                            if error.contains("timeout") {
                                row.push_str(" timeout |");
                            } else if status_label == "empty" || status_label == "no" {
                                row.push_str(" empty |");
                            } else {
                                row.push_str(" crash |");
                            }
                        }
                        (None, Some(_)) => {
                            row.push_str(" base failed |");
                        }
                        (None, None) => {
                            row.push_str(" both failed |");
                        }
                    }
                }
            }

            l.push(row);
        }
        l.push(String::new());
    }

    // ── 3. Peak Memory Summary (global, only if RSS data exists) ─────────
    let has_any_rss = benchmarks.iter().any(|bench| {
        bench
            .get("servers")
            .and_then(|s| s.as_array())
            .map(|servers| {
                servers
                    .iter()
                    .any(|s| s.get("rss_kb").and_then(|v| v.as_u64()).is_some())
            })
            .unwrap_or(false)
    });

    if has_any_rss {
        l.push("## Peak Memory (RSS)".into());
        l.push(String::new());

        let mut peak_header = "|".to_string();
        let mut peak_sep = "|".to_string();
        let mut peak_row = "|".to_string();
        for sname in &server_names {
            let mut peak: Option<u64> = None;
            for bench in benchmarks {
                if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                    for srv in servers {
                        let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("");
                        if name == *sname {
                            if let Some(rss) = srv.get("rss_kb").and_then(|v| v.as_u64()) {
                                peak = Some(peak.map_or(rss, |p: u64| p.max(rss)));
                            }
                        }
                    }
                }
            }
            peak_header.push_str(&format!(" {} |", sname));
            peak_sep.push_str(&"-".repeat(sname.len() + 2));
            peak_sep.push('|');
            if let Some(p) = peak {
                let mb = p as f64 / 1024.0;
                peak_row.push_str(&format!(" {:.1} MB |", mb));
            } else {
                peak_row.push_str(" - |");
            }
        }
        l.push(peak_header);
        l.push(peak_sep);
        l.push(peak_row);
        l.push(String::new());
    }

    // ── Footer ──────────────────────────────────────────────────────────
    l.push("---".into());
    l.push(String::new());
    if let Some(ts) = data.get("timestamp").and_then(|t| t.as_str()) {
        l.push(format!(
            "*Generated from [`{}`]({}) — benchmark run: {}*",
            json_path, json_path, ts
        ));
        l.push(String::new());
    }

    l.join("\n")
}

/// Determine display label for a server result: ok, empty, no, timeout, crash
fn server_status_label(srv: &Value) -> &'static str {
    let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let response_str = srv
        .get("response")
        .map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| serde_json::to_string(v).unwrap_or_default())
        })
        .unwrap_or_default();
    let error = srv.get("error").and_then(|v| v.as_str()).unwrap_or("");
    match status {
        "ok" if response_str != "null" && response_str != "[]" && !response_str.is_empty() => "ok",
        "ok" | "invalid"
            if response_str.contains("Unknown method") || response_str.contains("unsupported") =>
        {
            "no"
        }
        "ok" | "invalid" => "empty",
        _ if error.contains("timeout") || error.contains("wait_for_diagnostics: timeout") => {
            "timeout"
        }
        _ => "crash",
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
