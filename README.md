# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against Uniswap V4-core (`Pool.sol`, 618 lines).

## Settings

| Setting | Value |
|---------|-------|
| Iterations | 10 |
| Warmup | 2 |
| Timeout | 30s |

## Servers

| Server | Version |
|--------|---------|
| Our LSP | `solidity-language-server 0.1.13+commit.843bd50.macos.aarch64` |
| solc | `0.8.33+commit.64118f21.Darwin.appleclang` |
| nomicfoundation | `@nomicfoundation/solidity-language-server 0.8.25` |
| juanfranblanco | `vscode-solidity-server 0.0.187` |
| qiuxiang | `solidity-ls 0.5.4` |

## Results

| Benchmark | Our LSP 🏆 | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|--------------|------|-----------------|----------------|----------|
| [Spawn + Init](#spawn--init) | 4.5ms 🥇 | 113.3ms 🥉 | 857.4ms | 508.4ms | 66.7ms 🥈 |
| [Diagnostics](#diagnostics) | 440.3ms 🥈 | 131.9ms 🥇 | timeout | FAIL | timeout |
| [Go to Definition](#go-to-definition) | 8.6ms 🥇 | - | timeout | FAIL | timeout |
| [Go to Declaration](#go-to-declaration) | 8.6ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Hover](#hover) | 13.6ms 🥇 | - | timeout | FAIL | timeout |
| [Find References](#find-references) | 10.4ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Symbols](#document-symbols) | 8.5ms 🥇 | unsupported | timeout | FAIL | timeout |
| [Document Links](#document-links) | 63.3ms 🥇 | unsupported | timeout | FAIL | timeout |

> **🏆 Overall Winner: Our LSP** — 7 🥇 out of 8 benchmarks

### Medal Tally

| Server | 🥇 Gold | 🥈 Silver | 🥉 Bronze | Score |
|--------|------|----------|----------|-------|
| **Our LSP** 🏆 | 7 | 1 | 0 | 23 |
| **solc** | 1 | 0 | 1 | 4 |
| **qiuxiang** | 0 | 1 | 0 | 2 |
| **nomicfoundation** | 0 | 0 | 0 | 0 |
| **juanfranblanco** | 0 | 0 | 0 | 0 |

## Feature Support

| Feature | Our LSP | solc | nomicfoundation | juanfranblanco | qiuxiang |
|---------|---------|------|-----------------|----------------|----------|
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
| **Our LSP** | ✅ ok | 4.5ms | 4.3ms | 5.6ms |
| **solc** | ✅ ok | 113.3ms | 113.6ms | 115.8ms |
| **nomicfoundation** | ✅ ok | 857.4ms | 858.1ms | 875.5ms |
| **juanfranblanco** | ✅ ok | 508.4ms | 507.7ms | 512.3ms |
| **qiuxiang** | ✅ ok | 66.7ms | 66.2ms | 70.3ms |

<details>
<summary>Response details</summary>

**Our LSP**

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
| **Our LSP** | ✅ ok | 440.3ms | 441.4ms | 447.2ms |
| **solc** | ✅ ok | 131.9ms | 131.9ms | 134.3ms |
| **nomicfoundation** | ❌ timeout | - | - | - |
| **juanfranblanco** | ❌ EOF | - | - | - |
| **qiuxiang** | ❌ timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
{
  "diagnostics": [
    {
      "code": "mixed-case-function",
      "message":...
```

**solc**

```json
{
  "diagnostics": [
    {
      "code": 6275,
      "message": "ParserError: So...
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
| **Our LSP** | ✅ ok | 8.6ms | 8.9ms | 9.0ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
{
  "range": {
    "end": {
      "character": 8,
      "line": 9
    },
    "st...
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
| **Our LSP** | ✅ ok | 8.6ms | 8.7ms | 8.9ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
{
  "range": {
    "end": {
      "character": 8,
      "line": 9
    },
    "st...
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
| **Our LSP** | ✅ ok | 13.6ms | 13.7ms | 13.9ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
{
  "contents": {
    "kind": "markdown",
    "value": "```solidity\nlibrary Tic...
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
| **Our LSP** | ✅ ok | 10.4ms | 10.5ms | 11.2ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
[
  {
    "range": {
      "end": {
        "character": 38,
        "line": 434...
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
| **Our LSP** | ✅ ok | 8.5ms | 8.5ms | 8.7ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
[
  {
    "kind": 15,
    "name": "solidity ^0.8.0",
    "range": {
      "end":...
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
| **Our LSP** | ✅ ok | 63.3ms | 63.4ms | 65.3ms |
| **solc** | ⚠️ invalid | - | - | - |
| **nomicfoundation** | ❌ wait_for_diagnostics: timeout | - | - | - |
| **juanfranblanco** | ❌ wait_for_diagnostics: EOF | - | - | - |
| **qiuxiang** | ❌ wait_for_diagnostics: timeout | - | - | - |

<details>
<summary>Response details</summary>

**Our LSP**

```json
[
  {
    "range": {
      "end": {
        "character": 16,
        "line": 3
 ...
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

*Generated from [`benchmarks/2026-02-13T02-43-22Z.json`](benchmarks/2026-02-13T02-43-22Z.json) — benchmark run: 2026-02-13T02:43:22Z*

See [DOCS.md](./DOCS.md) for usage and installation.
