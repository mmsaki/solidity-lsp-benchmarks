# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against Uniswap V4-core (`Pool.sol`, 618 lines).

## Settings

| Setting | Value |
|---------|-------|
| File | `src/libraries/Pool.sol` |
| Target position | line 102, col 15 |
| Iterations | 10 |
| Warmup | 2 |
| Request timeout | 10s |
| Index timeout | 15s |

## Servers

| Server | Description | Version |
|--------|-------------|---------|
| [mmsaki](https://github.com/mmsaki/solidity-language-server) | Solidity Language Server by mmsaki | `solidity-language-server 0.1.13+commit.843bd50.macos.aarch64` |
| [solc](https://docs.soliditylang.org) | Official Solidity compiler LSP | `0.8.33+commit.64118f21.Darwin.appleclang` |
| [nomicfoundation](https://github.com/NomicFoundation/hardhat-vscode) | Hardhat/Nomic Foundation Solidity Language Server | `@nomicfoundation/solidity-language-server 0.8.25` |
| [juanfranblanco](https://github.com/juanfranblanco/vscode-solidity) | VSCode Solidity by Juan Blanco | `vscode-solidity-server 0.0.187` |
| [qiuxiang](https://github.com/qiuxiang/solidity-ls) | Solidity Language Server by qiuxiang | `solidity-ls 0.5.4` |

## Results

| Benchmark | mmsaki 🏆 | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|-------------|------|-----------------|----------------|----------|
| [Spawn + Init](#spawn--init) | 3.9ms 🥇 | 114.3ms 🥉 | 853.4ms | 514.9ms | 69.1ms 🥈 |
| [Diagnostics](#diagnostics) | 440.4ms 🥈 | 134.2ms 🥇 | timeout | FAIL | timeout |
| [Go to Definition](#go-to-definition) | 8.7ms 🥇 | - | timeout | FAIL | timeout |
| [Go to Declaration](#go-to-declaration) | 8.6ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Hover](#hover) | 13.6ms 🥇 | - | timeout | FAIL | timeout |
| [Find References](#find-references) | 10.0ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Symbols](#document-symbols) | 8.4ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Links](#document-links) | 63.1ms 🥇 | unsupported | timeout | FAIL | timeout |

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
| Spawn + Init | ✅ | ✅ | ✅ | ✅ | ✅ |
| Diagnostics | ✅ | ✅ | ⏳ | ❌ | ⏳ |
| Go to Definition | ✅ | ❌ | ⏳ | ❌ | ⏳ |
| Go to Declaration | ✅ | ❌ | ⏳ | ❌ | ⏳ |
| Hover | ✅ | ❌ | ⏳ | ❌ | ⏳ |
| Find References | ✅ | ❌ | ⏳ | ❌ | ⏳ |
| Document Symbols | ✅ | ❌ | ⏳ | ❌ | ⏳ |
| Document Links | ✅ | ❌ | ⏳ | ❌ | ⏳ |

> ✅ = valid response   ⚠️ = empty/null result   ⏳ = timeout   ❌ = unsupported / failed

---

## Detailed Results

### Spawn + Init

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | ✅ ok | 3.9ms | 4.0ms | 4.4ms |
| **solc** | ✅ ok | 114.3ms | 114.6ms | 115.7ms |
| **nomicfoundation** | ✅ ok | 853.4ms | 854.0ms | 862.2ms |
| **juanfranblanco** | ✅ ok | 514.9ms | 515.3ms | 519.0ms |
| **qiuxiang** | ✅ ok | 69.1ms | 68.9ms | 70.7ms |

<details>
<summary>Response details</summary>

**mmsaki**

```json
"ok"
```

**solc**

```json
"ok"
```

**nomicfoundation**

```json
"ok"
```

**juanfranblanco**

```json
"ok"
```

**qiuxiang**

```json
"ok"
```

</details>

### Diagnostics

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | ✅ ok | 440.4ms | 442.0ms | 443.6ms |
| **solc** | ✅ ok | 134.2ms | 134.4ms | 136.0ms |
| **nomicfoundation** | ❌ timeout | - | - | - |
| **juanfranblanco** | ❌ EOF | - | - | - |
| **qiuxiang** | ❌ timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "diagnostics": [
    {
      "code": "mixed-case-function",
      "message": "[forge lint] function names should use mixedCase",
      "range": {
        "end": {
          "character": 21,...
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
| **mmsaki** | ✅ ok | 8.7ms | 8.7ms | 9.6ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "range": {
    "end": {
      "character": 8,
      "line": 9
    },
    "start": {
      "character": 8,
      "line": 9
    }
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
| **mmsaki** | ✅ ok | 8.6ms | 8.5ms | 9.3ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "range": {
    "end": {
      "character": 8,
      "line": 9
    },
    "start": {
      "character": 8,
      "line": 9
    }
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
| **mmsaki** | ✅ ok | 13.6ms | 13.6ms | 13.9ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

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
| **mmsaki** | ✅ ok | 10.0ms | 10.0ms | 10.6ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "range": {
      "end": {
        "character": 38,
        "line": 434
      },
      "start": {
        "character": 30,
        "line": 434
      }
    },...
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
| **mmsaki** | ✅ ok | 8.4ms | 8.4ms | 8.7ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "kind": 15,
    "name": "solidity ^0.8.0",
    "range": {
      "end": {
        "character": 23,
        "line": 1
      },
      "start": {
        "character": 0,
        "line": 1...
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
| **mmsaki** | ✅ ok | 63.1ms | 63.2ms | 63.5ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "range": {
      "end": {
        "character": 16,
        "line": 3
      },
      "start": {
        "character": 8,
        "line": 3
      }
    },...
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

*Generated from [`benchmarks/2026-02-13T04-55-35Z.json`](benchmarks/2026-02-13T04-55-35Z.json) — benchmark run: 2026-02-13T04:55:35Z*

See [DOCS.md](./DOCS.md) for usage and installation.
