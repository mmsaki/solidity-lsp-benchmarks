# Solidity LSP Benchmark Analysis

Analysis of `v4-core` (`src/libraries/Pool.sol`) — 10 iterations per benchmark.

## Servers

| Server | Description | Version |
|--------|-------------|---------|
| [mmsaki](https://github.com/mmsaki/solidity-language-server) | Solidity Language Server by mmsaki | `solidity-language-server 0.1.14+commit.3d6a3d1.macos.aarch64` |
| [solc](https://docs.soliditylang.org) | Official Solidity compiler LSP | `0.8.33+commit.64118f21.Darwin.appleclang` |
| [nomicfoundation](https://github.com/NomicFoundation/hardhat-vscode) | Hardhat/Nomic Foundation Solidity Language Server | `@nomicfoundation/solidity-language-server 0.8.25` |
| [juanfranblanco](https://github.com/juanfranblanco/vscode-solidity) | VSCode Solidity by Juan Blanco | `vscode-solidity-server 0.0.187` |
| [qiuxiang](https://github.com/qiuxiang/solidity-ls) | Solidity Language Server by qiuxiang | `solidity-ls 0.5.4` |

## Capability Matrix

| Benchmark | mmsaki | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|--------|------|-----------------|----------------|----------|
| initialize | ok | ok | ok | ok | ok |
| textDocument/diagnostic | ok | ok | timeout | crash | timeout |
| textDocument/definition | ok | empty | timeout | crash | timeout |
| textDocument/declaration | ok | no | timeout | crash | timeout |
| textDocument/hover | ok | empty | timeout | crash | timeout |
| textDocument/references | ok | no | timeout | crash | timeout |
| textDocument/documentSymbol | ok | no | timeout | crash | timeout |
| textDocument/documentLink | ok | no | timeout | crash | timeout |

| Server | Working | Failed | Success Rate |
|--------|---------|--------|--------------|
| mmsaki | 8/8 | 0/8 | 100% |
| solc | 2/8 | 6/8 | 25% |
| nomicfoundation | 1/8 | 7/8 | 12% |
| juanfranblanco | 1/8 | 7/8 | 12% |
| qiuxiang | 1/8 | 7/8 | 12% |

## initialize

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | Overhead | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|----------|-----------|
| mmsaki | ok | 4.13ms | 4.2ms | 4.6ms | 0.4ms | 1.09x | 3.77ms | 4.59ms | 0.82ms | **1.0x (fastest)** | - |
| solc | ok | 116.04ms | 116.5ms | 117.8ms | 1.3ms | 1.01x | 114.28ms | 117.81ms | 3.53ms | **28.1x** | **28.1x slower** |
| nomicfoundation | ok | 882.34ms | 877.4ms | 902.3ms | **25.0ms** | 1.03x | 869.44ms | 902.32ms | **32.88ms** | **213.6x** | **213.6x slower** |
| juanfranblanco | ok | 524.35ms | 524.8ms | 526.9ms | 2.1ms | 1.00x | 520.96ms | 526.85ms | 5.89ms | **127.0x** | **127.0x slower** |
| qiuxiang | ok | 70.60ms | 71.0ms | 71.9ms | 1.0ms | 1.01x | 69.11ms | 71.93ms | 2.82ms | **17.1x** | **17.1x slower** |

## textDocument/diagnostic

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | Overhead | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|----------|-----------|
| mmsaki | ok | 39.7 MB | 454.45ms | 452.1ms | 473.6ms | **21.5ms** | 1.05x | 449.46ms | 473.56ms | **24.10ms** | 3.3x | - |
| solc | ok | 26.2 MB | 136.80ms | 136.7ms | 138.3ms | 1.6ms | 1.01x | 135.92ms | 138.35ms | 2.43ms | **1.0x (fastest)** | 3.3x faster |
| nomicfoundation | timeout | 509.9 MB | - | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 69.7 MB | - | - | - | - | - | - | - | - | - | timeout |

## textDocument/definition

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|-----------|
| mmsaki | ok | 26.4 MB | 8.95ms | 9.2ms | 9.5ms | 0.3ms | 1.04x | 8.51ms | 9.52ms | 1.01ms | - |
| solc | empty | 26.2 MB | - | - | - | - | - | - | - | - | empty |
| nomicfoundation | timeout | 509.7 MB | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 68.7 MB | - | - | - | - | - | - | - | - | timeout |

## textDocument/declaration

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|-----------|
| mmsaki | ok | 35.7 MB | 9.04ms | 8.9ms | 9.9ms | 1.0ms | 1.11x | 8.58ms | 9.86ms | 1.28ms | - |
| solc | no | 26.1 MB | - | - | - | - | - | - | - | - | empty |
| nomicfoundation | timeout | 510.8 MB | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 69.4 MB | - | - | - | - | - | - | - | - | timeout |

## textDocument/hover

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|-----------|
| mmsaki | ok | 30.0 MB | 14.01ms | 14.0ms | 14.7ms | 0.7ms | 1.05x | 13.51ms | 14.67ms | 1.16ms | - |
| solc | empty | 26.0 MB | - | - | - | - | - | - | - | - | empty |
| nomicfoundation | timeout | 511.8 MB | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 69.9 MB | - | - | - | - | - | - | - | - | timeout |

## textDocument/references

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|-----------|
| mmsaki | ok | 30.3 MB | 11.06ms | 10.6ms | 14.4ms | 3.8ms | 1.36x | 10.24ms | 14.40ms | 4.16ms | - |
| solc | no | 26.1 MB | - | - | - | - | - | - | - | - | empty |
| nomicfoundation | timeout | 511.7 MB | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 70.1 MB | - | - | - | - | - | - | - | - | timeout |

## textDocument/documentSymbol

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|-----------|
| mmsaki | ok | 25.8 MB | 8.72ms | 8.8ms | 9.2ms | 0.3ms | 1.04x | 8.26ms | 9.17ms | 0.91ms | - |
| solc | no | 26.1 MB | - | - | - | - | - | - | - | - | empty |
| nomicfoundation | timeout | 513.5 MB | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 70.0 MB | - | - | - | - | - | - | - | - | timeout |

## textDocument/documentLink

| Server | Status | Mem | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | vs mmsaki |
|--------|--------|-----|------|-----|-----|--------|-------|-----|-----|-------|-----------|
| mmsaki | ok | 26.7 MB | 64.32ms | 64.5ms | 65.2ms | 0.7ms | 1.01x | 63.34ms | 65.24ms | 1.90ms | - |
| solc | no | 25.7 MB | - | - | - | - | - | - | - | - | empty |
| nomicfoundation | timeout | 511.9 MB | - | - | - | - | - | - | - | - | timeout |
| juanfranblanco | crash | 0.0 MB | - | - | - | - | - | - | - | - | crash |
| qiuxiang | timeout | 69.5 MB | - | - | - | - | - | - | - | - | timeout |

## Peak Memory (RSS)

| mmsaki | solc | nomicfoundation | juanfranblanco | qiuxiang |
|--------|------|-----------------|----------------|----------|
| 39.7 MB | 26.2 MB | 513.5 MB | 0.0 MB | 70.1 MB |

---

*Generated from [`benchmarks/v4-core/2026-02-13T10-31-12Z.json`](benchmarks/v4-core/2026-02-13T10-31-12Z.json) — benchmark run: 2026-02-13T10:31:12Z*
