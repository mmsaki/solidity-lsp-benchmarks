# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against Uniswap V4-core (`Pool.sol`, 618 lines).

## Servers

- **Our LSP** — [solidity-language-server](https://github.com/mmsaki/solidity-language-server) (Rust)
- **solc** — Solidity compiler built-in LSP (C++)
- **nomicfoundation** — nomicfoundation-solidity-language-server (Node.js)

## Results

| Benchmark | Our LSP | solc | nomicfoundation |
|-----------|---------|------------|---------------|
| Spawn + Init | 3ms ⚡ | 123ms | 867ms |
| Diagnostics | 435ms | 133ms ⚡ | 911ms |
| Go to Definition | 8.8ms ⚡ | - | timeout |
| Go to Declaration | 8.9ms ⚡ | unsupported | timeout |
| Find References | 10.2ms ⚡ | unsupported | timeout |
| Document Symbols | 9.0ms ⚡ | unsupported | timeout |

Detailed results per benchmark in [results/](./results).

## Prerequisites

- [solidity-language-server](https://github.com/mmsaki/solidity-language-server): `cargo install solidity-language-server`
- [solc](https://docs.soliditylang.org/en/latest/installing-solidity.html)
- [nomicfoundation-solidity-language-server](https://github.com/NomicFoundation/hardhat-vscode)

## Run

```sh
git clone --recursive https://github.com/mmsaki/solidity-lsp-benchmarks.git
cd solidity-lsp-benchmarks
cargo build --release
./target/release/bench <subcommand>
```

Subcommands: `spawn`, `diagnostics`, `definition`, `declaration`, `hover`, `references`, `documentSymbol`

## Methodology

- 10 iterations + 2 warmup per server per benchmark
- Reports p50, p95, mean latency
- Spawns each server as a child process, communicates via JSON-RPC over stdio
- Waits for `publishDiagnostics` before sending feature requests
- ⚡ marks the fastest server per benchmark
