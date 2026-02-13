# Documentation

This project produces two binaries:

| Binary | Source | Purpose |
|--------|--------|---------|
| `bench` | `src/main.rs` | Run LSP benchmarks, produce JSON snapshots |
| `gen-readme` | `src/gen_readme.rs` | Read a JSON snapshot, generate `README.md` |

## Prerequisites

- [solidity-language-server](https://github.com/mmsaki/solidity-language-server): `cargo install solidity-language-server`
- [solc](https://docs.soliditylang.org/en/latest/installing-solidity.html)
- [nomicfoundation-solidity-language-server](https://github.com/NomicFoundation/hardhat-vscode) `npm i -g @nomicfoundation/solidity-language-server`
- [vscode-solidity-server](https://github.com/juanfranblanco/vscode-solidity): `npm i -g vscode-solidity-server`
- [solidity-ls](https://github.com/qiuxiang/solidity-ls): `npm i -g solidity-ls`

## Run Benchmarks

```sh
git clone --recursive https://github.com/mmsaki/solidity-lsp-benchmarks.git
cd solidity-lsp-benchmarks
cargo build --release
./target/release/bench [OPTIONS] <COMMAND>
```

### Commands

| Command | Description |
|---------|-------------|
| `all` | Run all benchmarks |
| `spawn` | Spawn + initialize handshake |
| `diagnostics` | Open Pool.sol, time to first diagnostic |
| `definition` | Go-to-definition on TickMath in Pool.sol |
| `declaration` | Go-to-declaration on TickMath in Pool.sol |
| `hover` | Hover on TickMath in Pool.sol |
| `references` | Find references on TickMath in Pool.sol |
| `documentSymbol` | Get document symbols for Pool.sol |
| `documentLink` | Get document links for Pool.sol |

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `-n, --iterations` | 10 | Number of measured iterations |
| `-w, --warmup` | 2 | Number of warmup iterations (discarded) |
| `-t, --timeout` | 10 | Timeout per request in seconds |
| `-T, --index-timeout` | 15 | Time for server to index/warm up in seconds |
| `-s, --server` | all | Only run against this server (can repeat, substring match) |
| `-h, --help` | | Show help message |

### Examples

```sh
bench all                              # Run all benchmarks, all servers
bench all -n 1 -w 0                   # Run all benchmarks once, no warmup
bench diagnostics -n 5                 # Run diagnostics with 5 iterations
bench spawn definition                 # Run specific benchmarks
bench all -T 30                        # Give servers 30s to index
bench all -t 5 -T 20                  # 20s to index, 5s per request
bench all -s solc                      # Run all benchmarks, only solc
bench diagnostics -s nomic -s solc     # Diagnostics for two servers
bench hover -s mmsaki -n 1 -w 0       # Single hover, only mmsaki
```

## Methodology

### How benchmarks work

Each benchmark sends real LSP requests over JSON-RPC (stdio) and measures wall-clock response time. Every request includes an `id`, and the tool waits for the server to return a response with that same `id` before recording the time and moving on. Requests are **sequential** — the next iteration only starts after the previous one completes (or times out).

### Two timeouts

There are two separate timeouts that serve different purposes:

- **Index timeout** (`-T`, default 15s): How long the server gets to index the project after opening a file. This is the "warm up" phase where the server analyzes the codebase, builds its AST, resolves imports, etc. This only applies to the diagnostics wait step.
- **Request timeout** (`-t`, default 10s): How long each individual LSP method call (definition, hover, etc.) gets to respond. Once a server has finished indexing, this is the budget for each request.

### Warmup iterations

Warmup iterations (`-w`, default 2) run the exact same benchmark but **discard the timing results**. This eliminates one-time costs from the measurements:

- **JIT compilation**: Node.js-based servers (nomicfoundation, juanfranblanco, qiuxiang) use V8, which interprets code on first run and optimizes hot paths later. The first 1-2 calls may be slower.
- **Internal caches**: Some servers cache symbol tables or analysis results after the first request.
- **OS-level caches**: First file reads hit disk; subsequent reads hit the page cache.

For `spawn` and `diagnostics` benchmarks, a fresh server is started for every iteration, so warmup has less effect. For method benchmarks (definition, hover, etc.), the server stays alive across iterations, so warmup helps measure steady-state performance.

Use `-w 0` if you want to measure real-world "first call" performance.

### Benchmark types

**Spawn + Init**: Starts a fresh server process and performs the LSP initialize/initialized handshake. Measures cold-start time. A fresh server is spawned for every iteration.

**Diagnostics**: Starts a fresh server, opens `Pool.sol` (618 lines from Uniswap V4), and waits for the server to publish diagnostics. Measures how long the server takes to analyze the file. Uses the index timeout (`-T`). A fresh server is spawned for every iteration.

**Method benchmarks** (definition, declaration, hover, references, documentSymbol, documentLink): Starts a single server, opens `Pool.sol`, waits for diagnostics (using the index timeout), then sends repeated LSP method requests. Only the method request time is measured — the indexing phase is not included in the timings.

### Result statuses

Each server gets one of three statuses per benchmark:

| Status | Meaning |
|--------|---------|
| **ok** | Server responded with valid, non-empty results. Latency stats (p50, p95, mean) are recorded. |
| **invalid** | Server responded, but the result was empty, null, or an error (e.g. `"Unknown method"`). The server doesn't support this feature. |
| **fail** | Server didn't respond in time (timeout), crashed (EOF), or couldn't be spawned. The error reason is recorded. |

### Statistics

For successful benchmarks, three latency metrics are reported:

- **p50** (median): The typical response time. Half of iterations were faster, half were slower.
- **p95**: The worst-case response time (excluding outliers). 95% of iterations were faster.
- **mean**: The arithmetic average across all measured iterations.

## Generate README

After running benchmarks, generate the README from JSON data:

```sh
./target/release/gen-readme                                        # uses latest JSON in benchmarks/
./target/release/gen-readme benchmarks/2026-02-13T01-45-26Z.json   # use a specific snapshot
./target/release/gen-readme -o results.md                          # custom output path
./target/release/gen-readme --help                                 # show help
```

## Output

`bench` produces JSON snapshots:

- `benchmarks/<timestamp>.json` — full runs
- `benchmarks/<names>/<timestamp>.json` — partial runs (e.g. `benchmarks/diagnostics/`)

`gen-readme` reads a JSON snapshot and writes `README.md` with:
- Summary results table with medals (🥇🥈🥉) and trophy (🏆)
- Medal tally and overall winner
- Feature support matrix
- Detailed per-benchmark latency tables (mean/p50/p95)
- Collapsible response details showing actual server responses
