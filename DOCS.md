# Documentation

This project produces two binaries:

| Binary | Source | Purpose |
|--------|--------|---------|
| `lsp-bench` | `src/main.rs` | Run LSP benchmarks, produce JSON results |
| `gen-report` | `src/gen_report.rs` | Generate competition tables, session logs from results |

## Quick Start

```sh
git clone --recursive https://github.com/mmsaki/solidity-lsp-benchmarks.git
cd solidity-lsp-benchmarks
cargo build --release
./target/release/lsp-bench init       # generates benchmark.yaml
```

Edit `benchmark.yaml` to add your servers and choose which benchmarks to run, then:

```sh
./target/release/lsp-bench            # run benchmarks (generates README if configured)
```

To generate a report manually from a JSON result:

```sh
./target/release/gen-report benchmarks/v4-core/results.json --session
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
| `lsp-bench` | Run benchmarks from config |
| `lsp-bench init` | Generate a `benchmark.yaml` template (won't overwrite existing) |
| `lsp-bench replay` | Replay a JSON-RPC request from benchmark output against an LSP server |

## Configuration

Benchmarks are configured via a YAML file. By default, `lsp-bench` looks for `benchmark.yaml` in the current directory. Use `-c` to point to a different config.

### Generating a config

```sh
lsp-bench init                        # creates benchmark.yaml
lsp-bench init -c my-bench.yaml       # creates at a custom path
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
output: benchmarks # directory for JSON results

# Which benchmarks to run
benchmarks:
  - all

# Generate a report after benchmarks (omit to skip)
# report: README.md

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
| `output` | no | `benchmarks` | Directory for JSON result files |
| `benchmarks` | no | all | List of benchmarks to run (see below) |
| `report` | no | -- | Output path for the generated report (omit to skip report generation) |
| `response` | no | `80` | Response output: `full` (no truncation) or a number (truncate to N chars) |
| `methods` | no | -- | Per-method position and trigger overrides (see below) |
| `servers` | yes | -- | List of LSP servers to benchmark |

### Selecting benchmarks

The `benchmarks` field controls which benchmarks to run. Use `all` to run everything, or list specific ones:

```yaml
# Run all benchmarks
benchmarks:
  - all

# Or pick specific ones
benchmarks:
  - initialize
  - textDocument/diagnostic
  - textDocument/definition
  - textDocument/hover
```

If omitted, all benchmarks are run.

### Per-method overrides

The `methods` map lets you set a different position or trigger character for specific LSP methods. Methods not listed fall back to the global `line`/`col`. Only include methods you want to override.

```yaml
line: 105
col: 27

methods:
  textDocument/completion:
    trigger: "."               # just add a trigger, use global line/col
  textDocument/hover:
    line: 44
    col: 30                    # override position for hover
  textDocument/definition:
    line: 200
    col: 15
  textDocument/rename:
    newName: "MyNewName"       # custom rename identifier (default: __lsp_bench_rename__)
```

| Field | Description |
|-------|-------------|
| `line` | Override line for this method (falls back to global `line`) |
| `col` | Override column for this method (falls back to global `col`) |
| `trigger` | Trigger character for completion (e.g. `"."`) — only used by `textDocument/completion` |
| `newName` | New name for `textDocument/rename` (defaults to `"__lsp_bench_rename__"`) |
| `start_line` | Start line for range-based methods like `textDocument/semanticTokens/range` |
| `start_col` | Start column for range-based methods |
| `expect` | Expected response for `--verify` mode (see [Verification](#verification) below) |
| `didChange` | List of file snapshots to send via `textDocument/didChange` before benchmarking (see below) |
| `didOpen` | List of additional files to open sequentially, measuring cross-file effects (see below) |
| `cold` | Cold-start mode: spawn a fresh server per iteration, measure end-to-end from didOpen through the method response (see below) |

You can override just one field — for example, `trigger: "."` alone uses the global position but adds the trigger character. An empty entry like `textDocument/hover: {}` is the same as not listing it at all.

### didChange snapshots

The `didChange` field lets you benchmark LSP responses against edited (unsaved) buffer states. Each entry is a file snapshot sent to the server via `textDocument/didChange`, with its own cursor position for the benchmark request.

```yaml
methods:
  textDocument/definition:
    line: 102
    col: 15
    didChange:
      - file: src/libraries/Pool.sol.snapshot0
        line: 102
        col: 15
      - file: src/libraries/Pool.sol.snapshot1
        line: 107
        col: 15
```

| Field | Description |
|-------|-------------|
| `file` | Path to the snapshot file (relative to project) |
| `line` | 0-based line for the benchmark request after this snapshot |
| `col` | 0-based column for the benchmark request after this snapshot |
| `expect` | Expected response for this snapshot (overrides method-level `expect` for `--verify`) |

**How it works:**

1. The original file (`file` in the top-level config) is opened via `textDocument/didOpen`
2. The server indexes it and publishes diagnostics (normal startup)
3. For each snapshot in order: the snapshot file's content is sent via `textDocument/didChange` (full document sync, incrementing version), then one benchmark request is sent at that snapshot's `line`/`col`
4. Each snapshot is one iteration in the results — no separate warmup or iteration count

**When to use this:**

- Testing goto definition / hover / completions on buffers that have been edited but not saved
- Verifying server behavior when AST byte offsets are stale (buffer diverges from last build)
- Reproducing bugs that only appear after specific edit sequences

**Snapshot file naming:**

Use `.snapshot1`, `.snapshot2`, etc. as the file extension to prevent your editor from auto-formatting them:

```
Pool.sol              ← original (opened via didOpen)
Pool.sol.snapshot0    ← minor edit (e.g. trailing newline)
Pool.sol.snapshot1    ← larger edit (e.g. lines inserted at top)
```

When `didChange` is not set, the benchmark runs normally with warmup + iterations of the same request.

### didOpen chain

The `didOpen` field benchmarks how opening additional files affects LSP responses on your primary file. This is useful for measuring cross-file features like forward references — as more files are opened, the server discovers more symbols and the response grows.

```yaml
methods:
  textDocument/references:
    line: 6
    col: 5
    expect:
      minCount: 10
    didOpen:
      - file: src/PoolManager.sol
        expect:
          minCount: 30
      - file: src/PoolSwapRouter.sol
        expect:
          minCount: 50
```

| Field | Description |
|-------|-------------|
| `file` | Path to the file to open (relative to project) |
| `line` | Optional line override for the benchmark request after this file is opened |
| `col` | Optional column override for the benchmark request after this file is opened |
| `expect` | Expected response for this step (overrides method-level `expect` for `--verify`) |

**How it works:**

1. The primary file (`file` in the top-level config) is opened via `textDocument/didOpen`
2. The server indexes it and publishes diagnostics
3. A baseline benchmark request is sent at the method's `line`/`col` (iteration 0)
4. For each didOpen step in order: the file is opened via `textDocument/didOpen` → the tool waits for the server to publish diagnostics on the new file → then re-sends the benchmark request on the **original** file
5. Each step produces one iteration in the results

**When to use this:**

- Measuring how reference counts grow as the server discovers more files
- Testing cross-file goto definition or hover after imports are resolved
- Benchmarking incremental indexing — how long does re-analysis take after opening a new file

**Difference from didChange:** `didChange` sends edited content for the *same* file (dirty buffers). `didOpen` opens *additional* files to expand the server's knowledge of the project.

### Cold-start benchmarks

The `cold` field measures the full end-to-end latency a user feels when opening a file and using an LSP feature for the first time. Unlike normal benchmarks (which wait for diagnostics before timing), cold-start benchmarks include compilation time in the measurement.

```yaml
methods:
  textDocument/definition:
    line: 91
    col: 35
    cold: true
  textDocument/references:
    line: 96
    col: 24
    cold: true
```

**How it works:**

1. A fresh server is spawned for each iteration (no shared state between runs)
2. The timer starts immediately before `textDocument/didOpen`
3. The server compiles the file and publishes diagnostics
4. The benchmark request is sent
5. The timer stops when the response arrives

The result captures: server startup + compilation + AST parsing + method response — the real user experience.

**When to use this:**

- Measuring cold-start performance across LSP server versions
- Identifying compilation bottlenecks (solc vs forge, large import graphs)
- Tracking regressions in time-to-first-response

**Example output:**

```
[1/1] textDocument/definition
  cold fresh server per iteration (cold start)
```

Cold-start times are typically seconds (not milliseconds) since they include compiler invocation. Normal `warmup` and `iterations` settings still apply — warmup iterations are discarded as usual.

### Verification

The `--verify` flag turns benchmarks into regression tests. Add `expect` fields to your config to declare the expected response, then run with `--verify` to check actual results against expectations.

```yaml
methods:
  textDocument/definition:
    line: 217
    col: 26
    expect:
      file: SafeCast.sol    # response URI must end with this
      line: 39              # response range.start.line must match (0-based)
    didChange:
      - file: src/libraries/Pool.sol.chain0
        line: 217
        col: 26
      - file: src/libraries/Pool.sol.dirty
        line: 216
        col: 22
        expect:             # per-snapshot override
          file: SafeCast.sol
          line: 39
```

```sh
lsp-bench --config goto-toInt128.yaml --verify
```

Output:

```
[1/1] textDocument/definition
  edit 2 snapshot(s) via didChange
  ✓ [1] Pool.sol.chain0
  ✓ [2] Pool.sol.dirty

  verify 2/2 expectations passed
```

On failure, the mismatch is reported and the process exits with code 1:

```
  ✗ [1] Pool.sol.dirty — line: expected 39 but got 55

  verify 1/2 expectations failed
```

**Expect fields:**

| Field | Description |
|-------|-------------|
| `file` | Expected filename suffix. The response URI must end with this string (e.g. `SafeCast.sol`). |
| `line` | Expected 0-based line number. Checked against `range.start.line` (Location) or `targetRange.start.line` (LocationLink). |
| `count` | Expected exact count. The response array length must equal this value. Useful for `textDocument/references`, `textDocument/documentSymbol`, etc. |
| `minCount` | Expected minimum count. The response array length must be at least this value. Use when the exact count may vary but you want to assert a lower bound. |

All fields are optional — you can check any combination of file, line, count, and minCount.

**Precedence:** Per-snapshot `expect` overrides the method-level `expect`. If neither is set, that snapshot/iteration is skipped (not counted as pass or fail).

**Without didChange:** For non-snapshot benchmarks, the method-level `expect` is checked against the first iteration's response.

Valid benchmark names: `all`, `initialize`, `textDocument/diagnostic`, `textDocument/definition`, `textDocument/declaration`, `textDocument/typeDefinition`, `textDocument/implementation`, `textDocument/hover`, `textDocument/references`, `textDocument/completion`, `textDocument/signatureHelp`, `textDocument/rename`, `textDocument/prepareRename`, `textDocument/documentSymbol`, `textDocument/documentLink`, `textDocument/formatting`, `textDocument/foldingRange`, `textDocument/selectionRange`, `textDocument/codeLens`, `textDocument/inlayHint`, `textDocument/semanticTokens/full`, `textDocument/semanticTokens/range`, `textDocument/semanticTokens/full/delta`, `textDocument/documentColor`, `workspace/symbol`.

### Response truncation

The `response` field controls how much of each LSP response is stored in the JSON output. By default, responses are truncated to 80 characters.

```yaml
# Full response, no truncation
response: full

# Truncate to 200 characters
response: 200
```

When omitted, defaults to 80.

This affects both the per-iteration `response` field in JSON output and the top-level `response` summary. Use `response: true` when you need to inspect the full LSP response for correctness (e.g. verifying Go to Definition returns the right location).

### Server fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `label` | yes | -- | Short name shown in results (e.g. `solc`) |
| `description` | no | `""` | Longer description for the README |
| `link` | no | `""` | URL to the server's project page |
| `cmd` | yes | -- | Command to spawn the server (also the binary name when using `commit`) |
| `args` | no | `[]` | Command-line arguments passed to `cmd` |
| `commit` | no | -- | Git ref (branch, tag, or SHA) to checkout and build from |
| `repo` | no | -- | Path to the git repo to build from (required when `commit` is set) |

### Building from commit

When `commit` is set on a server, `lsp-bench` will:

1. `git checkout <commit>` in the `repo` directory
2. `cargo build --release`
3. Use the built binary at `<repo>/target/release/<cmd>`
4. Restore the repo to its original branch/ref afterward

This is useful for comparing performance across branches or commits without manually building each one.

```yaml
servers:
  - label: baseline
    cmd: solidity-language-server
    commit: main
    repo: /path/to/solidity-language-server

  - label: my-branch
    cmd: solidity-language-server
    commit: fix/position-encoding
    repo: /path/to/solidity-language-server
```

The `cmd` field is used as the binary name inside `target/release/`. The `repo` field must point to a Rust project with a `Cargo.toml`. Both servers can share the same repo — `lsp-bench` builds them sequentially and restores the original ref after each build.

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

The position should land on an identifier that LSP methods can act on -- a type name, function call, variable, etc. This is used by position-based benchmarks: `textDocument/definition`, `textDocument/declaration`, `textDocument/typeDefinition`, `textDocument/implementation`, `textDocument/hover`, `textDocument/references`, `textDocument/completion`, `textDocument/signatureHelp`, `textDocument/rename`, and `textDocument/prepareRename`. The `initialize`, `textDocument/diagnostic`, and document-level benchmarks (`textDocument/documentSymbol`, `textDocument/documentLink`, `textDocument/formatting`, `textDocument/foldingRange`, `textDocument/selectionRange`, `textDocument/codeLens`, `textDocument/inlayHint`, `textDocument/semanticTokens/full`, `textDocument/documentColor`) ignore the position.

### Example configs

**Minimal** -- single server, just initialize and diagnostics:

```yaml
project: examples
file: Counter.sol
line: 21
col: 8
benchmarks:
  - initialize
  - textDocument/diagnostic
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
benchmarks:
  - initialize
  - textDocument/hover
servers:
  - label: mmsaki
    cmd: solidity-language-server
```

**Full suite** -- all benchmarks against Uniswap V4-core:

```yaml
project: v4-core
file: src/libraries/Pool.sol
line: 102  # "TickMath" (editor line 103, col 16)
col: 15
iterations: 10
warmup: 2
output: benchmarks/v4-core
benchmarks:
  - all
readme:
  - benchmarks/v4-core/README.md
servers:
  - label: mmsaki
    cmd: solidity-language-server
  - label: solc
    cmd: solc
    args: ["--lsp"]
```

**Per-commit comparison** -- benchmark two branches of the same server:

```yaml
project: examples
file: Counter.sol
line: 21
col: 8
report: README.md
servers:
  - label: baseline
    cmd: solidity-language-server
    commit: main
    repo: /path/to/solidity-language-server
  - label: my-branch
    cmd: solidity-language-server
    commit: fix/position-encoding
    repo: /path/to/solidity-language-server
```

**Per-method positions** -- different cursor positions for different methods:

```yaml
project: examples
file: Counter.sol
line: 21
col: 8

methods:
  textDocument/completion:
    col: 9
    trigger: "."
  textDocument/hover:
    line: 10
    col: 15

benchmarks:
  - textDocument/completion
  - textDocument/hover
  - textDocument/definition

servers:
  - label: mmsaki
    cmd: solidity-language-server
```

Here `completion` uses line 21 col 9 with a `.` trigger, `hover` uses line 10 col 15, and `definition` uses the global line 21 col 8.

**Forward references (didOpen)** -- measure how reference counts grow as more files are opened:

```yaml
project: v4-core
file: src/types/Currency.sol
line: 6
col: 5
iterations: 1
warmup: 0
response: full

benchmarks:
  - textDocument/references

methods:
  textDocument/references:
    line: 6
    col: 5
    expect:
      minCount: 10
    didOpen:
      - file: src/PoolManager.sol
        expect:
          minCount: 30

servers:
  - label: mmsaki
    cmd: solidity-language-server
```

Here `Currency.sol` is opened first and a baseline `textDocument/references` request is sent (iteration 0). Then `PoolManager.sol` is opened — the server discovers new references to the symbol — and the request is re-sent on the original file (iteration 1). The `minCount` expectations verify that the reference count grows as expected.

**didChange snapshots** -- benchmark goto definition against edited buffer states:

```yaml
project: v4-core
file: src/libraries/Pool.sol
line: 102
col: 15
response: full
output: benchmarks/v4-core

benchmarks:
  - textDocument/definition

methods:
  textDocument/definition:
    line: 102
    col: 15
    didChange:
      - file: src/libraries/Pool.sol.snapshot0
        line: 102
        col: 15
      - file: src/libraries/Pool.sol.snapshot1
        line: 107
        col: 15

servers:
  - label: mmsaki
    cmd: solidity-language-server
```

Here `Pool.sol` is opened normally, then each snapshot is sent via `textDocument/didChange`. Snapshot0 has a minor edit (same cursor position), snapshot1 has 5 blank lines inserted at the top (cursor shifted to line 107). Each snapshot produces one iteration in the results.

**Verification** -- assert goto definition resolves correctly across dirty snapshots:

```yaml
project: v4-core
file: src/libraries/Pool.sol
line: 217
col: 26
response: full

benchmarks:
  - textDocument/definition

methods:
  textDocument/definition:
    line: 217
    col: 26
    expect:
      file: SafeCast.sol
      line: 39
    didChange:
      - file: src/libraries/Pool.sol.chain0
        line: 217
        col: 26
      - file: src/libraries/Pool.sol.dirty
        line: 216
        col: 22

servers:
  - label: mmsaki
    cmd: solidity-language-server
```

Run with `lsp-bench --config toInt128.yaml --verify` to check that all snapshots resolve to `SafeCast.sol:39`.

**Long timeouts** -- for slow servers that need more indexing time:

```yaml
project: v4-core
file: src/libraries/Pool.sol
line: 102
col: 15
timeout: 30
index_timeout: 60
benchmarks:
  - all
servers:
  - label: nomicfoundation
    description: Hardhat/Nomic Foundation Solidity Language Server
    link: https://github.com/NomicFoundation/hardhat-vscode
    cmd: nomicfoundation-solidity-language-server
    args: ["--stdio"]
```

### Running benchmarks

```sh
lsp-bench                            # uses benchmark.yaml in current directory
lsp-bench -c pool.yaml               # uses a different config file
lsp-bench -c configs/fast.yaml       # config can be in any path
```

### CLI options

| Flag | Description |
|------|-------------|
| `-c, --config <PATH>` | Config file path (default: `benchmark.yaml`) |
| `--verify` | Check responses against `expect` fields in config. Exits non-zero on mismatch. |
| `-V, --version` | Show version (includes commit hash, OS, and architecture) |
| `-h, --help` | Show help |

All benchmark settings (iterations, warmup, timeout, servers, etc.) are configured in the YAML file.

### Replay

The `replay` subcommand replays a JSON-RPC request from benchmark output against an LSP server. It handles the full LSP lifecycle — spawning the server, performing the `initialize`/`initialized` handshake, opening the target file via `textDocument/didOpen`, and sending the request with proper `Content-Length` framing.

```sh
lsp-bench replay \
  --server "solc --lsp" \
  --project v4-core \
  --input '{"id":1,"jsonrpc":"2.0","method":"textDocument/rename","params":{"newName":"NewName","position":{"character":15,"line":109},"textDocument":{"uri":"file:///path/to/Pool.sol"}}}'
```

The `--input` value is the `input` field from benchmark JSON output (see [JSON structure](#json-structure) below). Copy it directly from the benchmark results.

| Flag | Description |
|------|-------------|
| `-s, --server <CMD>` | Server command (e.g. `"solc --lsp"`, `"solidity-ls --stdio"`) |
| `-i, --input <JSON>` | JSON-RPC input string (from benchmark output's `"input"` field) |
| `-p, --project <DIR>` | Project root directory (defaults to current directory) |
| `-f, --file <PATH>` | File to open before sending the request (auto-detected from input URI if omitted) |
| `-t, --timeout <SECS>` | Timeout in seconds for the response (default: 30) |

**Example: reproducing an InternalCompilerError in solc**

```sh
# 1. Run a benchmark that captures the input
lsp-bench -c ice.yaml

# 2. Copy the "input" field from the JSON output for textDocument/rename
# 3. Replay it against solc to reproduce the crash
lsp-bench replay \
  --server "solc --lsp" \
  --project v4-core \
  --input '{"id":1,"jsonrpc":"2.0","method":"textDocument/rename","params":{"newName":"NewName","position":{"character":15,"line":109},"textDocument":{"uri":"file:///path/to/v4-core/src/libraries/Pool.sol"}}}'
```

Output:

```json
{
  "error": {
    "code": -32603,
    "message": "Unhandled exception: .../CompilerStack.cpp(1249): Throw in function ...\nDynamic exception type: boost::wrapexcept<solidity::langutil::InternalCompilerError>\nstd::exception::what: Parsing not yet performed.\n"
  },
  "id": 2,
  "jsonrpc": "2.0"
}
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

For `initialize` and `textDocument/diagnostic` benchmarks, a fresh server is started for every iteration, so warmup has less effect. For method benchmarks (`textDocument/definition`, `textDocument/hover`, etc.), the server stays alive across iterations, so warmup helps measure steady-state performance.

Set `warmup: 0` in your config (or `-w 0` on the CLI) to measure real-world "first call" performance.

### Benchmark types

Benchmarks are named after their official LSP method names:

**initialize**: Starts a fresh server process and performs the LSP `initialize`/`initialized` handshake. Measures cold-start time. A fresh server is spawned for every iteration.

**textDocument/diagnostic**: Starts a fresh server, opens the target file, and waits for the server to publish diagnostics. Measures how long the server takes to analyze the file. Uses `index_timeout`. A fresh server is spawned for every iteration.

**textDocument/definition**, **textDocument/declaration**, **textDocument/typeDefinition**, **textDocument/implementation**, **textDocument/hover**, **textDocument/references**, **textDocument/completion**, **textDocument/signatureHelp**, **textDocument/rename**, **textDocument/prepareRename**: Starts a single server, opens the target file, waits for diagnostics (using `index_timeout`), then sends repeated LSP method requests at the target position (`line`/`col`). Only the method request time is measured -- the indexing phase is not included in the timings.

**textDocument/documentSymbol**, **textDocument/documentLink**, **textDocument/formatting**, **textDocument/foldingRange**, **textDocument/selectionRange**, **textDocument/codeLens**, **textDocument/inlayHint**, **textDocument/semanticTokens/full**, **textDocument/documentColor**: Same as above but these are document-level methods that don't use the target position.

**workspace/symbol**: Sends a `workspace/symbol` request with an empty query string. This is a workspace-level method that doesn't use the target position or document.

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

### Memory measurement

Each benchmark measures the server's **Resident Set Size (RSS)** -- the amount of physical memory the process is using. RSS is sampled via `ps -o rss= -p <pid>` after the server finishes indexing (post-diagnostics).

Memory is measured in all outcomes:

| Scenario | When RSS is sampled |
|----------|---------------------|
| `textDocument/diagnostic` (success) | After diagnostics complete, before the server is killed. Peak RSS across all iterations is recorded. |
| `textDocument/diagnostic` (timeout/crash) | Right before returning the failure. The server is still alive, so RSS reflects memory consumed while stuck. |
| Method benchmarks (success) | Once after indexing completes, before the request loop begins. |
| Method benchmarks (timeout/crash) | Right before returning the failure. |
| `initialize` | Not measured (process is too short-lived). |

This means even servers that timeout or crash will have their memory usage recorded. For example, a Node.js server that times out after 15 seconds of indexing will show how much memory it consumed before giving up.

The value is stored as `rss_kb` (kilobytes) in the JSON output. Both `gen-readme` and `gen-analysis` display it in megabytes.

## Generate Report

Generate a competition report and session logs from benchmark results:

```sh
gen-report benchmarks/v4-core/results.json                     # write README.md, print to stdout
gen-report benchmarks/v4-core/results.json -o report.md        # custom output path
gen-report benchmarks/v4-core/results.json --session           # also generate session.txt and session.md
gen-report benchmarks/v4-core/results.json -q                  # write file only (quiet)
gen-report --help                                              # show help
```

The report includes:
- **Summary table** — p95 latency per server per method, fastest bolded
- **Scorecard** — win count per server
- **Per-method detail tables** — p95, RSS, human-readable result, responded (✓/✗)

With `--session`, two additional files are generated in the same directory as the output:
- **session.txt** — plain text input/output log with arrows (← →) showing what each server returned
- **session.md** — markdown version with collapsible raw JSON responses for GitHub rendering

To auto-generate after benchmarks, set `report: README.md` in your config.

### CLI options

| Flag | Description |
|------|-------------|
| `-o, --output <path>` | Output file path (default: `README.md`) |
| `--session` | Also generate session.txt and session.md |
| `-q, --quiet` | Don't print report to stdout |

## Output

`lsp-bench` writes results to the `output` directory (default `benchmarks/`):

- `<output>/results.json` -- overwritten on each run

During a run, partial results are saved to `<output>/partial/` after each benchmark completes. These are cleaned up automatically when the full run finishes.

If `report` is set in the config, the report is automatically generated from `results.json` along with session logs (session.txt, session.md).

### JSON structure

Each benchmark entry includes an `input` field containing the full JSON-RPC request that was sent to the server. This is a stringified JSON-RPC envelope with `jsonrpc`, `id`, `method`, and `params`:

```json
{
  "name": "textDocument/rename",
  "input": "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"textDocument/rename\",\"params\":{\"newName\":\"NewName\",\"position\":{\"character\":15,\"line\":109},\"textDocument\":{\"uri\":\"file:///path/to/Pool.sol\"}}}",
  "servers": [ ... ]
}
```

The `input` field can be passed directly to `lsp-bench replay --input` to reproduce the exact request against any server. For `initialize` and `textDocument/diagnostic` benchmarks, the `input` field is omitted.

Each result stores per-iteration data in an `iterations` array. For successful benchmarks (`status: "ok"`), the top-level `response` field contains the summary response (from the first iteration). Every iteration includes both its latency (`ms`) and its full `response`:

```json
{
  "server": "mmsaki",
  "status": "ok",
  "mean_ms": 8.8,
  "p50_ms": 8.8,
  "p95_ms": 10.1,
  "rss_kb": 40944,
  "response": { "uri": "file:///...TickMath.sol", "range": { "start": { "line": 9, "character": 4 }, "end": { "line": 9, "character": 12 } } },
  "iterations": [
    { "ms": 8.80, "response": { "uri": "file:///...TickMath.sol", "range": { "..." : "..." } } },
    { "ms": 8.45, "response": { "uri": "file:///...TickMath.sol", "range": { "..." : "..." } } },
    { "ms": 8.55, "response": { "uri": "file:///...TickMath.sol", "range": { "..." : "..." } } }
  ]
}
```

Responses are stored as native JSON values (objects, arrays, strings, or null) — not escaped strings. For `initialize` benchmarks, the response is `"ok"` for each iteration and `rss_kb` is omitted (process is too short-lived). For `textDocument/diagnostic` benchmarks, `rss_kb` is the peak RSS across all iterations (each iteration spawns a fresh server). For method benchmarks (`textDocument/definition`, `textDocument/hover`, etc.), `rss_kb` is measured once after indexing completes.

Failed or unsupported benchmarks (`status: "fail"` or `"invalid"`) have no `iterations` array:

```json
{
  "server": "solc",
  "status": "invalid",
  "response": []
}
```

The per-iteration data enables warmup curve analysis, response consistency checks across iterations, and detection of performance degradation over time. When using `didChange` snapshots, each snapshot produces one iteration — comparing responses across iterations shows how the server handles different buffer states.

`gen-readme` reads a JSON snapshot and writes `README.md` with:
- Summary results table with medals and trophy
- Medal tally and overall winner
- Feature support matrix
- Detailed per-benchmark latency tables (mean/p50/p95)
- Collapsible response details showing actual server responses

## Example files

The repo includes test resources in `examples/`:

- **`examples/Counter.sol`** -- A simple Solidity contract with NatSpec doc comments and intentional unused variables (`unused`, `owner`, `old`, `temp`) that trigger diagnostics warnings from LSP servers. Used as the default benchmark target by `lsp-bench init`.

For larger benchmarks, the repo also includes [Uniswap V4-core](https://github.com/Uniswap/v4-core) as a git submodule at `v4-core/` (618-line `Pool.sol`). Clone with `--recursive` to include it.
