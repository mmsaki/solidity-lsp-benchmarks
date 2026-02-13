# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against Uniswap V4-core (`Pool.sol`, 618 lines).

## Servers

- **(Our LSP)** — [solidity-language-server](https://github.com/mmsaki/solidity-language-server) (Rust)
- **solc** — Solidity compiler built-in LSP (C++)
- **nomicfoundation** — nomicfoundation-solidity-language-server (Node.js)
- **juanfranblanco** — vscode-solidity-server (Node.js)
- **qiuxiang** — solidity-ls (TypeScript)

## Results

10 iterations, 2 warmup, 10s timeout

| Benchmark | Our LSP | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|---------|------|-----------------|----------------|----------|
| Spawn + Init | 4.1ms ⚡ | 122.5ms | 860.6ms | 510.2ms | 67.4ms |
| Diagnostics | 650.0ms | 132.5ms ⚡ | 914.3ms | FAIL | 256.6ms |
| Go to Definition | 27.0ms ⚡ | - | timeout | FAIL | timeout |
| Go to Declaration | 31.0ms ⚡ | - | timeout | FAIL | timeout |
| Hover | - | - | timeout | FAIL | timeout |
| Find References | 20.7ms ⚡ | - | timeout | FAIL | timeout |
| Document Symbols | 22.2ms ⚡ | - | timeout | FAIL | timeout |

Detailed results per benchmark in [results/](./results).

## Prerequisites

- [solidity-language-server](https://github.com/mmsaki/solidity-language-server): `cargo install solidity-language-server`
- [solc](https://docs.soliditylang.org/en/latest/installing-solidity.html)
- [nomicfoundation-solidity-language-server](https://github.com/NomicFoundation/hardhat-vscode) `npm i -g @nomicfoundation/solidity-language-server`
- [vscode-solidity-server](https://github.com/juanfranblanco/vscode-solidity): `npm i -g vscode-solidity-server`
- [solidity-ls](https://github.com/qiuxiang/solidity-ls): `npm i -g solidity-ls`

## Run

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
| `-w, --warmup` | 2 | Number of warmup iterations |
| `-t, --timeout` | 30 | Timeout per request in seconds |
| `-h, --help` | | Show help message |

### Examples

```sh
bench all                   # Run all benchmarks (10 iterations, 2 warmup)
bench all -n 1 -w 0         # Run all benchmarks once, no warmup
bench diagnostics -n 5      # Run diagnostics with 5 iterations
bench spawn definition      # Run specific benchmarks
bench all -t 10             # Run all benchmarks with 10s timeout
```

## Output

Each run generates:

- `results/<benchmark>.md` — detailed per-benchmark results
- `results/README.md` — summary table with server versions and settings
- `benchmarks/<date>.json` — machine-readable results with timestamps
- `benchmarks/history.json` — append-only history of all runs

The JSON output includes server versions, benchmark settings, and full p50/p95/mean data — useful for dashboards and tracking improvements over time.

## Methodology

- Configurable iterations and warmup per server per benchmark
- Reports p50, p95, mean latency
- Detects server versions automatically (`--version`, `package.json`)
- Spawns each server as a child process, communicates via JSON-RPC over stdio
- Waits for `publishDiagnostics` before sending feature requests
- ⚡ marks the fastest server per benchmark
