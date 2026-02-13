# Documentation

This project produces two binaries:

| Binary | Source | Purpose |
|--------|--------|---------|
| `bench` | `src/main.rs` | Run LSP benchmarks, produce JSON snapshots |
| `gen-readme` | `src/gen_readme.rs` | Read a JSON snapshot, generate `README.md` |

## Quick Start

```sh
git clone --recursive https://github.com/mmsaki/solidity-lsp-benchmarks.git
cd solidity-lsp-benchmarks
cargo build --release
./target/release/bench init       # generates benchmark.yaml
```

Edit `benchmark.yaml` to add your servers, then:

```sh
./target/release/bench all        # run all benchmarks
./target/release/gen-readme       # generate README.md from results
```

The generated config uses `examples/Counter.sol` (included in the repo) as the default benchmark target -- a small contract with NatSpec comments and intentional unused variables to trigger diagnostics.

## Prerequisites

Install any LSP servers you want to benchmark. You only need the ones listed in your config:

- [solidity-language-server](https://github.com/mmsaki/solidity-language-server): `cargo install solidity-language-server`
- [solc](https://docs.soliditylang.org/en/latest/installing-solidity.html)
- [nomicfoundation-solidity-language-server](https://github.com/NomicFoundation/hardhat-vscode): `npm i -g @nomicfoundation/solidity-language-server`
- [vscode-solidity-server](https://github.com/juanfranblanco/vscode-solidity): `npm i -g vscode-solidity-server`
- [solidity-ls](https://github.com/qiuxiang/solidity-ls): `npm i -g solidity-ls`

Servers not found on `$PATH` are automatically skipped during benchmarks.

## Commands

| Command | Description |
|---------|-------------|
| `init` | Generate a `benchmark.yaml` template (won't overwrite existing) |
| `all` | Run all benchmarks |
| `spawn` | Spawn + initialize handshake |
| `diagnostics` | Open file, time to first diagnostic |
| `definition` | Go-to-definition at target position |
| `declaration` | Go-to-declaration at target position |
| `hover` | Hover at target position |
| `references` | Find references at target position |
| `documentSymbol` | Get document symbols |
| `documentLink` | Get document links |

Multiple benchmark commands can be combined:

```sh
bench spawn definition hover      # run these three benchmarks
```

## Configuration

Benchmarks are configured via a YAML file. By default, `bench` looks for `benchmark.yaml` in the current directory. Use `-c` to point to a different config.

### Generating a config

```sh
bench init                        # creates benchmark.yaml
bench init -c my-bench.yaml       # creates at a custom path
```

This writes a commented template targeting `examples/Counter.sol` with placeholder server entries. Edit it to add your servers and (optionally) point to a different project/file.

### Config structure

```yaml
# Project root containing the Solidity files
project: examples

# Target file to benchmark (relative to project root)
file: Counter.sol

# Target position for position-based benchmarks (0-based, see below)
line: 21
col: 8

# Benchmark settings
iterations: 10
warmup: 2
timeout: 10        # seconds per request
index_timeout: 15  # seconds for server to index/warm up

# LSP servers to benchmark
servers:
  - label: mmsaki
    description: Solidity Language Server by mmsaki
    link: https://github.com/mmsaki/solidity-language-server
    cmd: solidity-language-server
    args: []

  - label: solc
    description: Official Solidity compiler LSP
    link: https://docs.soliditylang.org
    cmd: solc
    args: ["--lsp"]
```

### Config fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `project` | yes | -- | Path to the project root (e.g. a git submodule) |
| `file` | yes | -- | Solidity file to benchmark, relative to `project` |
| `line` | no | 102 | Target line for position-based benchmarks (0-based) |
| `col` | no | 15 | Target column for position-based benchmarks (0-based) |
| `iterations` | no | 10 | Number of measured iterations per benchmark |
| `warmup` | no | 2 | Number of warmup iterations (discarded) |
| `timeout` | no | 10 | Timeout per LSP request in seconds |
| `index_timeout` | no | 15 | Time for server to index/warm up in seconds |
| `servers` | yes | -- | List of LSP servers to benchmark |

### Server fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `label` | yes | -- | Short name shown in results (e.g. `solc`) |
| `description` | no | `""` | Longer description for the README |
| `link` | no | `""` | URL to the server's project page |
| `cmd` | yes | -- | Command to spawn the server |
| `args` | no | `[]` | Command-line arguments passed to `cmd` |

### Target position (line and col)

`line` and `col` use **0-based indexing**, matching the [LSP specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position). This means they are offset by 1 from what your editor displays:

| Config value | Editor display |
|--------------|----------------|
| `line: 0` | line 1 |
| `line: 102` | line 103 |
| `col: 0` | column 1 |
| `col: 15` | column 16 |

To find the right values, open the file in your editor, place the cursor on the identifier you want to benchmark, and subtract 1 from both the line and column numbers.

For example, targeting `number` inside `setNumber` in Counter.sol:

```
line 22 (editor):       number = newNumber;
col   9 (editor):       ^
```

In the config, this becomes `line: 21`, `col: 8`.

Another example -- targeting `TickMath` in Pool.sol:

```
line 103 (editor):  tick = TickMath.getTickAtSqrtPrice(sqrtPriceX96);
col  16 (editor):          ^
```

In the config: `line: 102`, `col: 15`.

The position should land on an identifier that LSP methods can act on -- a type name, function call, variable, etc. This is used by `definition`, `declaration`, `hover`, and `references` benchmarks. The `spawn`, `diagnostics`, `documentSymbol`, and `documentLink` benchmarks ignore the position.

### Example configs

**Minimal** -- benchmark a single server against the included Counter.sol:

```yaml
project: examples
file: Counter.sol
line: 21   # "number" in setNumber (editor line 22, col 9)
col: 8
servers:
  - label: solc
    cmd: solc
    args: ["--lsp"]
```

**Quick iteration** -- fast feedback during development:

```yaml
project: examples
file: Counter.sol
line: 21
col: 8
iterations: 1
warmup: 0
timeout: 5
index_timeout: 10
servers:
  - label: mmsaki
    cmd: solidity-language-server
```

**Larger project** -- benchmark against Uniswap V4-core:

```yaml
project: v4-core
file: src/libraries/Pool.sol
line: 102  # "TickMath" (editor line 103, col 16)
col: 15
iterations: 10
warmup: 2
servers:
  - label: mmsaki
    cmd: solidity-language-server
  - label: solc
    cmd: solc
    args: ["--lsp"]
```

**Long timeouts** -- for slow servers that need more indexing time:

```yaml
project: v4-core
file: src/libraries/Pool.sol
line: 102
col: 15
timeout: 30
index_timeout: 60
servers:
  - label: nomicfoundation
    description: Hardhat/Nomic Foundation Solidity Language Server
    link: https://github.com/NomicFoundation/hardhat-vscode
    cmd: nomicfoundation-solidity-language-server
    args: ["--stdio"]
```

### Using a config

```sh
bench all                        # uses benchmark.yaml in current directory
bench all -c pool.yaml           # uses a different config file
bench all -c configs/fast.yaml   # config can be in any path
```

### CLI overrides

Any config value can be overridden from the command line. CLI flags take precedence over the config file.

| Flag | Overrides |
|------|-----------|
| `-c, --config <PATH>` | Config file path (default: `benchmark.yaml`) |
| `-n, --iterations <N>` | `iterations` |
| `-w, --warmup <N>` | `warmup` |
| `-t, --timeout <SECS>` | `timeout` |
| `-T, --index-timeout <SECS>` | `index_timeout` |
| `-s, --server <NAME>` | Filters `servers` list (substring match, repeatable) |
| `-f, --file <PATH>` | `file` |
| `--line <N>` | `line` |
| `--col <N>` | `col` |

```sh
bench all -n 1 -w 0             # override iterations/warmup from config
bench all -s solc -s mmsaki      # only run solc and mmsaki from config
bench all -T 30                  # give servers 30s to index (overrides config)
bench hover -f src/PoolManager.sol --line 50 --col 8  # override file/position
```

## Methodology

### How benchmarks work

Each benchmark sends real LSP requests over JSON-RPC (stdio) and measures wall-clock response time. Every request includes an `id`, and the tool waits for the server to return a response with that same `id` before recording the time and moving on. Requests are **sequential** -- the next iteration only starts after the previous one completes (or times out).

### Two timeouts

There are two separate timeouts that serve different purposes:

- **Index timeout** (`index_timeout`, default 15s): How long the server gets to index the project after opening a file. This is the "warm up" phase where the server analyzes the codebase, builds its AST, resolves imports, etc. This only applies to the diagnostics wait step.
- **Request timeout** (`timeout`, default 10s): How long each individual LSP method call (definition, hover, etc.) gets to respond. Once a server has finished indexing, this is the budget for each request.

### Warmup iterations

Warmup iterations (`warmup`, default 2) run the exact same benchmark but **discard the timing results**. This eliminates one-time costs from the measurements:

- **JIT compilation**: Node.js-based servers (nomicfoundation, juanfranblanco, qiuxiang) use V8, which interprets code on first run and optimizes hot paths later. The first 1-2 calls may be slower.
- **Internal caches**: Some servers cache symbol tables or analysis results after the first request.
- **OS-level caches**: First file reads hit disk; subsequent reads hit the page cache.

For `spawn` and `diagnostics` benchmarks, a fresh server is started for every iteration, so warmup has less effect. For method benchmarks (definition, hover, etc.), the server stays alive across iterations, so warmup helps measure steady-state performance.

Set `warmup: 0` in your config (or `-w 0` on the CLI) to measure real-world "first call" performance.

### Benchmark types

**Spawn + Init**: Starts a fresh server process and performs the LSP initialize/initialized handshake. Measures cold-start time. A fresh server is spawned for every iteration.

**Diagnostics**: Starts a fresh server, opens the target file, and waits for the server to publish diagnostics. Measures how long the server takes to analyze the file. Uses `index_timeout`. A fresh server is spawned for every iteration.

**Method benchmarks** (definition, declaration, hover, references, documentSymbol, documentLink): Starts a single server, opens the target file, waits for diagnostics (using `index_timeout`), then sends repeated LSP method requests at the target position (`line`/`col`). Only the method request time is measured -- the indexing phase is not included in the timings.

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

- `benchmarks/<timestamp>.json` -- full runs
- `benchmarks/<names>/<timestamp>.json` -- partial runs (e.g. `benchmarks/diagnostics/`)

During a run, partial results are saved to `benchmarks/partial/` after each benchmark completes. These are cleaned up automatically when the full run finishes.

`gen-readme` reads a JSON snapshot and writes `README.md` with:
- Summary results table with medals and trophy
- Medal tally and overall winner
- Feature support matrix
- Detailed per-benchmark latency tables (mean/p50/p95)
- Collapsible response details showing actual server responses

## Example files

The repo includes test resources in `examples/`:

- **`examples/Counter.sol`** -- A simple Solidity contract with NatSpec doc comments and intentional unused variables (`unused`, `owner`, `old`, `temp`) that trigger diagnostics warnings from LSP servers. Used as the default benchmark target by `bench init`.

For larger benchmarks, the repo also includes [Uniswap V4-core](https://github.com/Uniswap/v4-core) as a git submodule at `v4-core/` (618-line `Pool.sol`). Clone with `--recursive` to include it.
