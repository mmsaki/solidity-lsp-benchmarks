# lsp-bench

A benchmarking framework for [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) (LSP) servers. Measures latency, correctness, and memory usage for any LSP server that communicates over JSON-RPC stdio.

## Install

```sh
cargo install lsp-bench
```

## Quick Start

```sh
lsp-bench init          # generates benchmark.yaml
# edit benchmark.yaml with your project and servers
lsp-bench               # run benchmarks
```

## What It Measures

| Benchmark | What it tests |
|-----------|---------------|
| `initialize` | Cold-start time (fresh process per iteration) |
| `textDocument/diagnostic` | Time to analyze a file and return diagnostics |
| `textDocument/definition` | Go to Definition latency |
| `textDocument/declaration` | Go to Declaration latency |
| `textDocument/typeDefinition` | Go to Type Definition latency |
| `textDocument/implementation` | Go to Implementation latency |
| `textDocument/hover` | Hover information latency |
| `textDocument/references` | Find References latency |
| `textDocument/completion` | Completion suggestions latency |
| `textDocument/signatureHelp` | Signature Help latency |
| `textDocument/rename` | Rename symbol latency |
| `textDocument/prepareRename` | Prepare Rename latency |
| `textDocument/documentSymbol` | Document Symbols latency |
| `textDocument/documentLink` | Document Links latency |
| `textDocument/formatting` | Document Formatting latency |
| `textDocument/foldingRange` | Folding Ranges latency |
| `textDocument/selectionRange` | Selection Ranges latency |
| `textDocument/codeLens` | Code Lens latency |
| `textDocument/inlayHint` | Inlay Hints latency |
| `textDocument/semanticTokens/full` | Semantic Tokens latency |
| `textDocument/documentColor` | Document Color latency |
| `workspace/symbol` | Workspace Symbol search latency |

Each benchmark records per-iteration latency (p50, p95, mean), the full LSP response, and resident memory (RSS).

## Configuration

Create a `benchmark.yaml`:

```yaml
project: my-project
file: src/main.rs
line: 45
col: 12

iterations: 10
warmup: 2
timeout: 10
index_timeout: 15

benchmarks:
  - all

# Response output: "full" or a number (default: 80)
# response: full

servers:
  - label: my-server
    cmd: my-language-server
    args: ["--stdio"]

  - label: other-server
    cmd: other-lsp
    args: ["--stdio"]
```

### Config Fields

| Field | Default | Description |
|-------|---------|-------------|
| `project` | -- | Path to project root |
| `file` | -- | Target file to benchmark (relative to project) |
| `line` | 102 | Target line for position-based benchmarks (0-based) |
| `col` | 15 | Target column (0-based) |
| `iterations` | 10 | Measured iterations per benchmark |
| `warmup` | 2 | Warmup iterations (discarded) |
| `timeout` | 10 | Seconds per LSP request |
| `index_timeout` | 15 | Seconds for server to index |
| `output` | `benchmarks` | Directory for JSON results |
| `benchmarks` | all | List of benchmarks to run |
| `response` | 80 | `full` (no truncation) or a number (truncate to N chars) |
| `report` | -- | Output path for generated report |
| `report_style` | `delta` | Report format: `delta`, `readme`, or `analysis` |

## CLI

```sh
lsp-bench                            # uses benchmark.yaml
lsp-bench -c my-config.yaml          # custom config
lsp-bench -s my-server               # filter to one server
lsp-bench -n 1 -w 0 -s my-server    # quick single iteration
lsp-bench -T 30                      # 30s index timeout
```

| Flag | Description |
|------|-------------|
| `-c, --config <PATH>` | Config file (default: `benchmark.yaml`) |
| `-n, --iterations <N>` | Override iterations |
| `-w, --warmup <N>` | Override warmup |
| `-t, --timeout <SECS>` | Override request timeout |
| `-T, --index-timeout <SECS>` | Override index timeout |
| `-s, --server <NAME>` | Filter servers (repeatable) |
| `-f, --file <PATH>` | Override target file |
| `--line <N>` | Override target line |
| `--col <N>` | Override target column |

## Binaries

| Binary | Purpose |
|--------|---------|
| `lsp-bench` | Run benchmarks, produce JSON snapshots |
| `gen-readme` | Generate README with medals and feature matrix |
| `gen-analysis` | Generate per-feature analysis report |
| `gen-delta` | Generate compact comparison table |

## Output

JSON snapshots with per-iteration latency, response data, and memory:

```json
{
  "server": "my-server",
  "status": "ok",
  "mean_ms": 8.8,
  "p50_ms": 8.8,
  "p95_ms": 10.1,
  "rss_kb": 40944,
  "iterations": [
    { "ms": 8.80, "response": "{ ... }" },
    { "ms": 8.45, "response": "{ ... }" }
  ]
}
```

## Methodology

- Real LSP requests over JSON-RPC stdio
- Sequential iterations (next starts after previous completes)
- Fresh server process for `initialize` and `textDocument/diagnostic`
- Persistent server for method benchmarks (definition, hover, etc.)
- RSS memory sampled after indexing
- Warmup iterations discarded from measurements

## License

MIT
