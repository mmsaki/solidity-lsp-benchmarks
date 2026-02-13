use serde_json::Value;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut json_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut lead_server: Option<String> = None;
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
                    lead_server = Some(args[i + 1].clone());
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
                eprintln!("Usage: gen-analysis [OPTIONS] [INPUT] [OUTPUT]");
                eprintln!();
                eprintln!("Generate analysis report from benchmark JSON.");
                eprintln!();
                eprintln!("Arguments:");
                eprintln!("  INPUT   Path to benchmark JSON (default: latest in benchmarks/)");
                eprintln!("  OUTPUT  Output file path (default: ANALYSIS.md)");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  -o, --output <path>    Same as OUTPUT positional argument");
                eprintln!("  --base <server>        Server for head-to-head comparison (default: first server)");
                eprintln!("  -q, --quiet            Don't print analysis to stdout");
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
    let output_path = output_path.unwrap_or_else(|| "ANALYSIS.md".to_string());

    let json_path = json_path.unwrap_or_else(|| {
        find_latest_json("benchmarks").unwrap_or_else(|| {
            eprintln!("No JSON files found in benchmarks/");
            eprintln!("Usage: gen-analysis [OPTIONS] [path/to/benchmark.json]");
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

    let benchmarks = match data.get("benchmarks").and_then(|b| b.as_array()) {
        Some(b) => b,
        None => {
            l.push("No benchmark data found.".into());
            return l.join("\n");
        }
    };

    let server_names = collect_server_names(benchmarks);

    // ── 1. Consistency: p50/p95 spread ──────────────────────────────────
    l.push("## Consistency (p50 vs p95 Spread)".into());
    l.push(String::new());
    l.push("How much latency varies between typical and worst-case iterations. Lower spread = more predictable.".into());
    l.push(String::new());

    l.push("| Benchmark | Server | p50 | p95 | Spread | Spike |".into());
    l.push("|-----------|--------|-----|-----|--------|-------|".into());

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
            for srv in servers {
                let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status != "ok" {
                    continue;
                }
                let p50 = srv.get("p50_ms").and_then(|v| v.as_f64());
                let p95 = srv.get("p95_ms").and_then(|v| v.as_f64());
                if let (Some(p50), Some(p95)) = (p50, p95) {
                    let spread = p95 - p50;
                    let spike = if p50 > 0.0 { p95 / p50 } else { 1.0 };
                    let spread_flag = if spread > 10.0 {
                        format!("**{:.1}ms**", spread)
                    } else {
                        format!("{:.1}ms", spread)
                    };
                    let spike_flag = if spike > 1.5 {
                        format!("**{:.2}x**", spike)
                    } else {
                        format!("{:.2}x", spike)
                    };
                    l.push(format!(
                        "| {} | {} | {:.1}ms | {:.1}ms | {} | {} |",
                        bench_name, name, p50, p95, spread_flag, spike_flag
                    ));
                }
            }
        }
    }
    l.push(String::new());

    // ── 2. Per-iteration range ──────────────────────────────────────────
    l.push("## Per-Iteration Range".into());
    l.push(String::new());
    l.push("Min and max latency across all measured iterations. Shows the full range of observed performance.".into());
    l.push(String::new());

    l.push("| Benchmark | Server | Min | Max | Range |".into());
    l.push("|-----------|--------|-----|-----|-------|".into());

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
            for srv in servers {
                let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                if let Some(iterations) = srv.get("iterations").and_then(|v| v.as_array()) {
                    let latencies: Vec<f64> = iterations
                        .iter()
                        .filter_map(|it| it.get("ms").and_then(|v| v.as_f64()))
                        .collect();
                    if latencies.is_empty() {
                        continue;
                    }
                    let min = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let range = max - min;
                    let range_flag = if range > 10.0 {
                        format!("**{:.2}ms**", range)
                    } else {
                        format!("{:.2}ms", range)
                    };
                    l.push(format!(
                        "| {} | {} | {:.2}ms | {:.2}ms | {} |",
                        bench_name, name, min, max, range_flag
                    ));
                }
            }
        }
    }
    l.push(String::new());

    // ── 3. Capability matrix ────────────────────────────────────────────
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
                let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
                let response = srv.get("response").and_then(|v| v.as_str()).unwrap_or("");
                let error = srv.get("error").and_then(|v| v.as_str()).unwrap_or("");
                let cell = match status {
                    "ok" if response != "null" && response != "[]" && !response.is_empty() => "ok",
                    "ok" | "invalid"
                        if response.contains("Unknown method")
                            || response.contains("unsupported") =>
                    {
                        "no"
                    }
                    "ok" | "invalid" => "empty",
                    _ if error.contains("timeout")
                        || error.contains("wait_for_diagnostics: timeout") =>
                    {
                        "timeout"
                    }
                    _ => "crash",
                };
                row.push_str(&format!(" {} |", cell));
            }
        }
        l.push(row);
    }
    l.push(String::new());

    // Capability summary
    let total_benchmarks = benchmarks.len();
    l.push("**Summary:**".into());
    l.push(String::new());
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
                    let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    let response = srv.get("response").and_then(|v| v.as_str()).unwrap_or("");
                    if status == "ok"
                        && response != "null"
                        && response != "[]"
                        && !response.is_empty()
                    {
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

    // ── 4. Overhead comparison ──────────────────────────────────────────
    l.push("## Overhead Comparison".into());
    l.push(String::new());
    l.push("How each server's mean latency compares to the fastest server per benchmark.".into());
    l.push(String::new());

    l.push("| Benchmark | Server | Mean | vs Fastest | Overhead |".into());
    l.push("|-----------|--------|------|------------|----------|".into());

    for bench in benchmarks {
        let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
            // Find the fastest ok server
            let fastest: Option<f64> = servers
                .iter()
                .filter(|s| {
                    s.get("status").and_then(|v| v.as_str()).unwrap_or("") == "ok"
                        && s.get("response").and_then(|v| v.as_str()).unwrap_or("") != "null"
                })
                .filter_map(|s| s.get("mean_ms").and_then(|v| v.as_f64()))
                .fold(None, |min, val| Some(min.map_or(val, |m: f64| m.min(val))));

            let fastest_ms = match fastest {
                Some(f) => f,
                None => continue,
            };

            for srv in servers {
                let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                let status = srv.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status != "ok" {
                    let error = srv.get("error").and_then(|v| v.as_str()).unwrap_or("");
                    let reason = if error.contains("timeout") {
                        "timeout"
                    } else if status == "invalid" {
                        "empty"
                    } else {
                        "crash"
                    };
                    l.push(format!(
                        "| {} | {} | {} | - | - |",
                        bench_name, name, reason
                    ));
                    continue;
                }
                let mean = srv.get("mean_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let overhead = if fastest_ms > 0.0 {
                    mean / fastest_ms
                } else {
                    1.0
                };
                let overhead_str = if (overhead - 1.0).abs() < 0.01 {
                    "**1.0x (fastest)**".to_string()
                } else if overhead > 10.0 {
                    format!("**{:.1}x**", overhead)
                } else {
                    format!("{:.1}x", overhead)
                };
                l.push(format!(
                    "| {} | {} | {:.2}ms | {:.2}ms | {} |",
                    bench_name, name, mean, fastest_ms, overhead_str
                ));
            }
        }
    }
    l.push(String::new());

    // ── 5. Memory usage (RSS) ──────────────────────────────────────────
    // Check if any server has rss_kb data
    let has_rss = benchmarks.iter().any(|bench| {
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

    if has_rss {
        l.push("## Memory Usage (RSS)".into());
        l.push(String::new());
        l.push(
            "Resident Set Size measured after indexing (post-diagnostics). Shows how much memory each server holds in RAM."
                .into(),
        );
        l.push(String::new());

        l.push("| Benchmark | Server | RSS |".into());
        l.push("|-----------|--------|-----|".into());

        for bench in benchmarks {
            let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                for srv in servers {
                    let name = srv.get("server").and_then(|v| v.as_str()).unwrap_or("?");
                    if let Some(rss) = srv.get("rss_kb").and_then(|v| v.as_u64()) {
                        let mb = rss as f64 / 1024.0;
                        l.push(format!("| {} | {} | {:.1} MB |", bench_name, name, mb));
                    }
                }
            }
        }
        l.push(String::new());

        // Summary: peak RSS per server across all benchmarks
        l.push("**Peak RSS per server:**".into());
        l.push(String::new());
        l.push("| Server | Peak RSS |".into());
        l.push("|--------|----------|".into());
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
            if let Some(p) = peak {
                let mb = p as f64 / 1024.0;
                l.push(format!("| {} | {:.1} MB |", sname, mb));
            }
        }
        l.push(String::new());
    }

    // ── 6. Head-to-head: lead server vs each competitor ────────────────
    let lead_name: Option<&&str> = if let Some(override_name) = lead_override {
        server_names
            .iter()
            .find(|n| **n == override_name)
            .or_else(|| {
                eprintln!(
                    "Warning: --base '{}' not found in servers, using first server",
                    override_name
                );
                server_names.first()
            })
    } else {
        server_names.first()
    };
    if let Some(lead) = lead_name {
        l.push(format!("## Head-to-Head: {} vs Competition", lead));
        l.push(String::new());
        l.push(format!(
            "How {} compares to each server on every benchmark. Positive = {} is faster, negative = {} is slower.",
            lead, lead, lead
        ));
        l.push(String::new());

        // Build a header with each competitor
        let competitors: Vec<&&str> = server_names.iter().filter(|n| **n != *lead).collect();
        if !competitors.is_empty() {
            let mut header = "| Benchmark |".to_string();
            let mut sep = "|-----------|".to_string();
            for comp in &competitors {
                header.push_str(&format!(" vs {} |", comp));
                sep.push_str(&"-".repeat(comp.len() + 5));
                sep.push('|');
            }
            l.push(header);
            l.push(sep);

            for bench in benchmarks {
                let bench_name = bench.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                if let Some(servers) = bench.get("servers").and_then(|s| s.as_array()) {
                    // Find lead server's mean
                    let lead_mean = servers
                        .iter()
                        .find(|s| s.get("server").and_then(|v| v.as_str()).unwrap_or("") == *lead)
                        .and_then(|s| {
                            if s.get("status").and_then(|v| v.as_str()).unwrap_or("") == "ok" {
                                s.get("mean_ms").and_then(|v| v.as_f64())
                            } else {
                                None
                            }
                        });

                    let mut row = format!("| {} |", bench_name);
                    for comp in &competitors {
                        let comp_name: &str = comp;
                        let comp_srv = servers.iter().find(|s| {
                            s.get("server").and_then(|v| v.as_str()).unwrap_or("") == comp_name
                        });
                        let comp_status = comp_srv
                            .and_then(|s| s.get("status").and_then(|v| v.as_str()))
                            .unwrap_or("");
                        let comp_mean = comp_srv
                            .filter(|_| comp_status == "ok")
                            .and_then(|s| s.get("mean_ms").and_then(|v| v.as_f64()));
                        let comp_error = comp_srv
                            .and_then(|s| s.get("error").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        match (lead_mean, comp_mean) {
                            (Some(lm), Some(cm)) => {
                                if (lm - cm).abs() < 0.01 {
                                    row.push_str(" tied |");
                                } else if lm < cm {
                                    // Lead is faster
                                    let factor = cm / lm;
                                    if factor > 10.0 {
                                        row.push_str(&format!(" **{:.1}x faster** |", factor));
                                    } else {
                                        row.push_str(&format!(" {:.1}x faster |", factor));
                                    }
                                } else {
                                    // Lead is slower
                                    let factor = lm / cm;
                                    if factor > 10.0 {
                                        row.push_str(&format!(" {:.1}x slower |", factor));
                                    } else {
                                        row.push_str(&format!(" {:.1}x slower |", factor));
                                    }
                                }
                            }
                            (Some(_), None) => {
                                // Competitor failed
                                if comp_error.contains("timeout") {
                                    row.push_str(" competitor timeout |");
                                } else if comp_status == "invalid" {
                                    row.push_str(" competitor empty |");
                                } else {
                                    row.push_str(" competitor crash |");
                                }
                            }
                            (None, Some(_)) => {
                                row.push_str(" - |");
                            }
                            (None, None) => {
                                row.push_str(" both failed |");
                            }
                        }
                    }
                    l.push(row);
                }
            }
            l.push(String::new());
        }
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
