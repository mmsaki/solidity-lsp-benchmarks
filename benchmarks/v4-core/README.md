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
| [initialize](#initialize) | 4.13ms 🥇 | 116.04ms 🥉 | 882.34ms | 524.35ms | 70.60ms 🥈 |
| [textDocument/diagnostic](#textdocumentdiagnostic) | 454.45ms 🥈 | 136.80ms 🥇 | timeout | FAIL | timeout |
| [textDocument/definition](#textdocumentdefinition) | 8.95ms 🥇 | - | timeout | FAIL | timeout |
| [textDocument/declaration](#textdocumentdeclaration) | 9.04ms 🥇 | unsupported | timeout | FAIL | timeout |
| [textDocument/hover](#textdocumenthover) | 14.01ms 🥇 | - | timeout | FAIL | timeout |
| [textDocument/references](#textdocumentreferences) | 11.06ms 🥇 | unsupported | timeout | FAIL | timeout |
| [textDocument/documentSymbol](#textdocumentdocumentsymbol) | 8.72ms 🥇 | unsupported | timeout | FAIL | timeout |
| [textDocument/documentLink](#textdocumentdocumentlink) | 64.32ms 🥇 | unsupported | timeout | FAIL | timeout |

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
| initialize | yes | yes | yes | yes | yes |
| textDocument/diagnostic | yes | yes | timeout | crash | timeout |
| textDocument/definition | yes | empty | timeout | crash | timeout |
| textDocument/declaration | yes | no | timeout | crash | timeout |
| textDocument/hover | yes | empty | timeout | crash | timeout |
| textDocument/references | yes | no | timeout | crash | timeout |
| textDocument/documentSymbol | yes | no | timeout | crash | timeout |
| textDocument/documentLink | yes | no | timeout | crash | timeout |

> yes = supported   no = unsupported   timeout = server timed out   crash = server crashed   empty = returned null/empty

## Memory Usage

Peak resident set size (RSS) measured after indexing.

| Server | Peak RSS | Measured During |
|--------|----------|-----------------|
| **mmsaki** | 39.7 MB | textDocument/diagnostic |
| **solc** | 26.2 MB | textDocument/diagnostic |
| **nomicfoundation** | 513.5 MB | textDocument/documentSymbol |
| **juanfranblanco** | 0.0 MB | textDocument/diagnostic |
| **qiuxiang** | 70.1 MB | textDocument/references |

---

## Detailed Results

### initialize

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 4.13ms | 4.20ms | 4.59ms |
| **solc** | 🥉 | 116.04ms | 116.51ms | 117.81ms |
| **nomicfoundation** | ok | 882.34ms | 877.35ms | 902.32ms |
| **juanfranblanco** | ok | 524.35ms | 524.75ms | 526.85ms |
| **qiuxiang** | 🥈 | 70.60ms | 70.98ms | 71.93ms |

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

### textDocument/diagnostic

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥈 | 454.45ms | 452.05ms | 473.56ms |
| **solc** | 🥇 | 136.80ms | 136.74ms | 138.35ms |
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

### textDocument/definition

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 8.95ms | 9.18ms | 9.52ms |
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

### textDocument/declaration

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 9.04ms | 8.86ms | 9.86ms |
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

### textDocument/hover

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 14.01ms | 13.99ms | 14.67ms |
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

### textDocument/references

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 11.06ms | 10.62ms | 14.40ms |
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
        "character": 41,...
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

### textDocument/documentSymbol

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 8.72ms | 8.84ms | 9.17ms |
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

### textDocument/documentLink

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 64.32ms | 64.55ms | 65.24ms |
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

*Generated from [`benchmarks/v4-core/2026-02-13T10-31-12Z.json`](benchmarks/v4-core/2026-02-13T10-31-12Z.json) — benchmark run: 2026-02-13T10:31:12Z*

See [DOCS.md](./DOCS.md) for usage and installation.
