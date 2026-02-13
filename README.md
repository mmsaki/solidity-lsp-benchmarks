# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against Uniswap V4-core (`Pool.sol`, 618 lines).

## Settings

| Setting | Value |
|---------|-------|
| File | `src/libraries/Pool.sol` |
| Target position | line 0, col 0 |
| Iterations | 10 |
| Warmup | 2 |
| Request timeout | 10s |
| Index timeout | 15s |

## Servers

| Server | Version |
|--------|---------|
| mmsaki | `solidity-language-server 0.1.13+commit.843bd50.macos.aarch64` |
| solc | `0.8.33+commit.64118f21.Darwin.appleclang` |
| nomicfoundation | `@nomicfoundation/solidity-language-server 0.8.25` |
| juanfranblanco | `vscode-solidity-server 0.0.187` |
| qiuxiang | `solidity-ls 0.5.4` |

## Results

| Benchmark | mmsaki 🏆 | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|-------------|------|-----------------|----------------|----------|
| [Spawn + Init](#spawn--init) | 3.9ms 🥇 | 112.3ms 🥉 | 865.6ms | 518.6ms | 70.2ms 🥈 |
| [Diagnostics](#diagnostics) | 445.1ms 🥈 | 135.4ms 🥇 | timeout | FAIL | timeout |
| [Go to Definition](#go-to-definition) | 8.6ms 🥇 | - | timeout | FAIL | timeout |
| [Go to Declaration](#go-to-declaration) | 8.7ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Hover](#hover) | 13.6ms 🥇 | - | timeout | FAIL | timeout |
| [Find References](#find-references) | 10.4ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Symbols](#document-symbols) | 8.4ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Links](#document-links) | 64.1ms 🥇 | unsupported | timeout | FAIL | timeout |

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
| **mmsaki** | ✅ ok | 3.9ms | 3.8ms | 4.8ms |
| **solc** | ✅ ok | 112.3ms | 112.3ms | 114.0ms |
| **nomicfoundation** | ✅ ok | 865.6ms | 866.2ms | 891.3ms |
| **juanfranblanco** | ✅ ok | 518.6ms | 520.0ms | 521.9ms |
| **qiuxiang** | ✅ ok | 70.2ms | 70.4ms | 71.7ms |

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
| **mmsaki** | ✅ ok | 445.1ms | 443.7ms | 457.4ms |
| **solc** | ✅ ok | 135.4ms | 135.4ms | 137.1ms |
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
| **mmsaki** | ✅ ok | 8.6ms | 8.5ms | 9.5ms |
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
| **mmsaki** | ✅ ok | 8.7ms | 8.7ms | 9.4ms |
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
| **mmsaki** | ✅ ok | 13.6ms | 13.6ms | 14.1ms |
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
| **mmsaki** | ✅ ok | 10.4ms | 10.2ms | 11.2ms |
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
        "character": 33,
        "line": 572
      },
      "start": {
        "character": 25,
        "line": 572
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
| **mmsaki** | ✅ ok | 8.4ms | 8.4ms | 8.8ms |
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
| **mmsaki** | ✅ ok | 64.1ms | 64.3ms | 65.0ms |
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

*Generated from [`benchmarks/2026-02-13T04-17-56Z.json`](benchmarks/2026-02-13T04-17-56Z.json) — benchmark run: 2026-02-13T04:17:56Z*

See [DOCS.md](./DOCS.md) for usage and installation.
