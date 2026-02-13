# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against `v4-core` (`src/libraries/Pool.sol`).

## Settings

| Setting | Value |
|---------|-------|
| Project | `v4-core` |
| File | `src/libraries/Pool.sol` |
| Target position | line 102, col 15 |
| Iterations | 10 |
| Warmup | 2 |
| Request timeout | 10s |
| Index timeout | 15s |

## Servers

| Server | Description | Version |
|--------|-------------|---------|
| [mmsaki](https://github.com/mmsaki/solidity-language-server) | Solidity Language Server by mmsaki | `solidity-language-server 0.1.14+commit.3d6a3d1.macos.aarch64` |
| [solc](https://docs.soliditylang.org) | Official Solidity compiler LSP | `0.8.33+commit.64118f21.Darwin.appleclang` |
| [nomicfoundation](https://github.com/NomicFoundation/hardhat-vscode) | Hardhat/Nomic Foundation Solidity Language Server | `@nomicfoundation/solidity-language-server 0.8.25` |
| [juanfranblanco](https://github.com/juanfranblanco/vscode-solidity) | VSCode Solidity by Juan Blanco | `vscode-solidity-server 0.0.187` |
| [qiuxiang](https://github.com/qiuxiang/solidity-ls) | Solidity Language Server by qiuxiang | `solidity-ls 0.5.4` |

## Results

| Benchmark | mmsaki 🏆 | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|-------------|------|-----------------|----------------|----------|
| [Spawn + Init](#spawn--init) | 4.00ms 🥇 | 113.40ms 🥉 | 862.90ms | 514.70ms | 67.90ms 🥈 |
| [Diagnostics](#diagnostics) | 443.20ms 🥈 | 133.30ms 🥇 | timeout | FAIL | timeout |
| [Go to Definition](#go-to-definition) | 8.90ms 🥇 | - | timeout | FAIL | timeout |
| [Go to Declaration](#go-to-declaration) | 8.90ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Hover](#hover) | 13.70ms 🥇 | - | timeout | FAIL | timeout |
| [Find References](#find-references) | 10.50ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Symbols](#document-symbols) | 8.40ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Links](#document-links) | 62.70ms 🥇 | unsupported | timeout | FAIL | timeout |

> **🏆 Overall Winner: mmsaki** — 7 🥇 out of 8 benchmarks

### Medal Tally

| Server | 🥇 Gold | 🥈 Silver | 🥉 Bronze | Score |
|--------|------|----------|----------|-------|
| **mmsaki** 🏆 | 7 | 1 | 0 | 23 |
| **solc** | 1 | 0 | 1 | 4 |
| **qiuxiang** | 0 | 1 | 0 | 2 |
| **nomicfoundation** | 0 | 0 | 0 | 0 |
| **juanfranblanco** | 0 | 0 | 0 | 0 |

## Feature Support

| Feature | mmsaki | solc | nomicfoundation | juanfranblanco | qiuxiang |
|---------|--------|------|-----------------|----------------|----------|
| Spawn + Init | yes | yes | yes | yes | yes |
| Diagnostics | yes | yes | timeout | crash | timeout |
| Go to Definition | yes | empty | timeout | crash | timeout |
| Go to Declaration | yes | no | timeout | crash | timeout |
| Hover | yes | empty | timeout | crash | timeout |
| Find References | yes | no | timeout | crash | timeout |
| Document Symbols | yes | no | timeout | crash | timeout |
| Document Links | yes | no | timeout | crash | timeout |

> yes = supported   no = unsupported   timeout = server timed out   crash = server crashed   empty = returned null/empty

## Memory Usage

Peak resident set size (RSS) measured after indexing.

| Server | Peak RSS | Measured During |
|--------|----------|-----------------|
| **mmsaki** | 39.7 MB | Diagnostics |
| **solc** | 26.2 MB | Diagnostics |
| **nomicfoundation** | 513.1 MB | Document Symbols |
| **juanfranblanco** | 0.0 MB | Diagnostics |
| **qiuxiang** | 70.2 MB | Document Symbols |

---

## Detailed Results

### Spawn + Init

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 4.00ms | 4.00ms | 4.40ms |
| **solc** | 🥉 | 113.40ms | 113.30ms | 116.10ms |
| **nomicfoundation** | ok | 862.90ms | 864.90ms | 881.10ms |
| **juanfranblanco** | ok | 514.70ms | 513.70ms | 518.90ms |
| **qiuxiang** | 🥈 | 67.90ms | 67.60ms | 69.40ms |

<details>
<summary>Response details</summary>

**mmsaki**

```json
ok
```

**solc**

```json
ok
```

**nomicfoundation**

```json
ok
```

**juanfranblanco**

```json
ok
```

**qiuxiang**

```json
ok
```

</details>

### Diagnostics

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥈 | 443.20ms | 443.40ms | 452.20ms |
| **solc** | 🥇 | 133.30ms | 133.90ms | 135.10ms |
| **nomicfoundation** | timeout | - | - | - |
| **juanfranblanco** | EOF | - | - | - |
| **qiuxiang** | timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "diagnostics": [
    {
      "code": "mixed-case-function",...
```

**solc**

```json
{
  "diagnostics": [
    {
      "code": 6275,...
```

**nomicfoundation**

Error: `timeout`

**juanfranblanco**

Error: `EOF`

**qiuxiang**

Error: `timeout`

</details>

### Go to Definition

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 8.90ms | 8.90ms | 9.80ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "range": {
    "end": {
      "character": 16,
      "line": 9
    },...
```

**solc**

```json
[]
```

**nomicfoundation**

Error: `wait_for_diagnostics: timeout`

**juanfranblanco**

Error: `wait_for_diagnostics: EOF`

**qiuxiang**

Error: `wait_for_diagnostics: timeout`

</details>

### Go to Declaration

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 8.90ms | 8.80ms | 9.70ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "range": {
    "end": {
      "character": 16,
      "line": 9
    },...
```

**solc**

```json
error: Unknown method textDocument/declaration
```

**nomicfoundation**

Error: `wait_for_diagnostics: timeout`

**juanfranblanco**

Error: `wait_for_diagnostics: EOF`

**qiuxiang**

Error: `wait_for_diagnostics: timeout`

</details>

### Hover

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 13.70ms | 13.80ms | 14.30ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "contents": {
    "kind": "markdown",...
```

**solc**

```json
null
```

**nomicfoundation**

Error: `wait_for_diagnostics: timeout`

**juanfranblanco**

Error: `wait_for_diagnostics: EOF`

**qiuxiang**

Error: `wait_for_diagnostics: timeout`

</details>

### Find References

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 10.50ms | 10.40ms | 11.50ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "range": {
      "end": {
        "character": 58,...
```

**solc**

```json
error: Unknown method textDocument/references
```

**nomicfoundation**

Error: `wait_for_diagnostics: timeout`

**juanfranblanco**

Error: `wait_for_diagnostics: EOF`

**qiuxiang**

Error: `wait_for_diagnostics: timeout`

</details>

### Document Symbols

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 8.40ms | 8.30ms | 8.80ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "kind": 15,
    "name": "solidity ^0.8.0",
    "range": {...
```

**solc**

```json
error: Unknown method textDocument/documentSymbol
```

**nomicfoundation**

Error: `wait_for_diagnostics: timeout`

**juanfranblanco**

Error: `wait_for_diagnostics: EOF`

**qiuxiang**

Error: `wait_for_diagnostics: timeout`

</details>

### Document Links

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 62.70ms | 62.70ms | 64.50ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "range": {
      "end": {
        "character": 16,...
```

**solc**

```json
error: Unknown method textDocument/documentLink
```

**nomicfoundation**

Error: `wait_for_diagnostics: timeout`

**juanfranblanco**

Error: `wait_for_diagnostics: EOF`

**qiuxiang**

Error: `wait_for_diagnostics: timeout`

</details>

---

*Generated from [`benchmarks/v4-core/2026-02-13T09-53-03Z.json`](benchmarks/v4-core/2026-02-13T09-53-03Z.json) — benchmark run: 2026-02-13T09:53:03Z*

See [DOCS.md](./DOCS.md) for usage and installation.
