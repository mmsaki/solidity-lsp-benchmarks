# Solidity LSP Benchmarks

Benchmarks comparing Solidity LSP servers against `examples` (`Counter.sol`).

## Settings

| Setting | Value |
|---------|-------|
| Project | `examples` |
| File | `Counter.sol` |
| Target position | line 21, col 8 |
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

| Benchmark | mmsaki | solc 🏆 | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|--------|-----------|-----------------|----------------|----------|
| [initialize](#initialize) | 4.60ms 🥇 | 113.80ms 🥉 | 867.70ms | 529.00ms | 69.80ms 🥈 |
| [textDocument/diagnostic](#textdocumentdiagnostic) | 136.00ms 🥈 | 0.90ms 🥇 | 373.00ms | 806.30ms | 153.80ms 🥉 |
| [textDocument/definition](#textdocumentdefinition) | 118.60ms | 0.10ms 🥇 | 0.40ms 🥉 | 0.40ms | 0.30ms 🥈 |
| [textDocument/hover](#textdocumenthover) | 128.00ms | 0.00ms 🥇 | 0.30ms 🥈 | 0.30ms 🥉 | - |
| [textDocument/references](#textdocumentreferences) | 0.40ms 🥉 | unsupported | 0.30ms 🥈 | 0.90ms | 0.20ms 🥇 |

> **🏆 Overall Winner: solc** — 3 🥇 out of 5 benchmarks

### Medal Tally

| Server | 🥇 Gold | 🥈 Silver | 🥉 Bronze | Score |
|--------|------|----------|----------|-------|
| **solc** 🏆 | 3 | 0 | 1 | 10 |
| **qiuxiang** | 1 | 2 | 1 | 8 |
| **mmsaki** | 1 | 1 | 1 | 6 |
| **nomicfoundation** | 0 | 2 | 1 | 5 |
| **juanfranblanco** | 0 | 0 | 1 | 1 |

## Feature Support

| Feature | mmsaki | solc | nomicfoundation | juanfranblanco | qiuxiang |
|---------|--------|------|-----------------|----------------|----------|
| initialize | yes | yes | yes | yes | yes |
| textDocument/diagnostic | yes | yes | yes | yes | yes |
| textDocument/definition | yes | yes | yes | yes | yes |
| textDocument/hover | yes | yes | yes | yes | empty |
| textDocument/references | yes | no | yes | yes | yes |

> yes = supported   no = unsupported   timeout = server timed out   crash = server crashed   empty = returned null/empty

## Memory Usage

Peak resident set size (RSS) measured after indexing.

| Server | Peak RSS | Measured During |
|--------|----------|-----------------|
| **mmsaki** | 4.9 MB | textDocument/diagnostic |
| **solc** | 26.2 MB | textDocument/references |
| **nomicfoundation** | 363.6 MB | textDocument/diagnostic |
| **juanfranblanco** | 381.1 MB | textDocument/diagnostic |
| **qiuxiang** | 60.4 MB | textDocument/diagnostic |

---

## Detailed Results

### initialize

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥇 | 4.60ms | 4.20ms | 8.60ms |
| **solc** | 🥉 | 113.80ms | 113.90ms | 117.50ms |
| **nomicfoundation** | ok | 867.70ms | 870.80ms | 880.00ms |
| **juanfranblanco** | ok | 529.00ms | 526.30ms | 578.10ms |
| **qiuxiang** | 🥈 | 69.80ms | 70.10ms | 71.50ms |

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
| **mmsaki** | 🥈 | 136.00ms | 138.40ms | 159.30ms |
| **solc** | 🥇 | 0.90ms | 0.90ms | 1.00ms |
| **nomicfoundation** | ok | 373.00ms | 374.30ms | 377.30ms |
| **juanfranblanco** | ok | 806.30ms | 802.00ms | 879.70ms |
| **qiuxiang** | 🥉 | 153.80ms | 153.40ms | 156.20ms |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "diagnostics": [
    {
      "code": "2072",...
```

**solc**

```json
{
  "diagnostics": [
    {
      "code": 2072,...
```

**nomicfoundation**

```json
{
  "diagnostics": [
    {
      "code": "2072",...
```

**juanfranblanco**

```json
{
  "diagnostics": [
    {
      "code": "2072",...
```

**qiuxiang**

```json
{
  "diagnostics": [
    {
      "code": "2072",...
```

</details>

### textDocument/definition

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | ok | 118.60ms | 117.90ms | 122.20ms |
| **solc** | 🥇 | 0.10ms | 0.10ms | 0.10ms |
| **nomicfoundation** | 🥉 | 0.40ms | 0.40ms | 0.70ms |
| **juanfranblanco** | ok | 0.40ms | 0.40ms | 0.50ms |
| **qiuxiang** | 🥈 | 0.30ms | 0.30ms | 0.40ms |

<details>
<summary>Response details</summary>

**mmsaki**

```json
{
  "range": {
    "end": {
      "character": 25,
      "line": 9
    },...
```

**solc**

```json
[
  {
    "range": {
      "end": {
        "character": 25,...
```

**nomicfoundation**

```json
{
  "range": {
    "end": {
      "character": 25,
      "line": 9
    },...
```

**juanfranblanco**

```json
[
  {
    "range": {
      "end": {
        "character": 26,...
```

**qiuxiang**

```json
{
  "range": {
    "end": {
      "character": 26,
      "line": 19
    },...
```

</details>

### textDocument/hover

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | ok | 128.00ms | 124.00ms | 178.50ms |
| **solc** | 🥇 | 0.00ms | 0.00ms | 0.10ms |
| **nomicfoundation** | 🥈 | 0.30ms | 0.30ms | 0.40ms |
| **juanfranblanco** | 🥉 | 0.30ms | 0.30ms | 0.40ms |
| **qiuxiang** | invalid | - | - | - |

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
{
  "contents": {
    "kind": "markdown",...
```

**nomicfoundation**

```json
{
  "contents": {
    "kind": "markdown",...
```

**juanfranblanco**

```json
{
  "contents": {
    "kind": "markdown",...
```

**qiuxiang**

```json
null
```

</details>

### textDocument/references

| Server | Status | Mean | P50 | P95 |
|--------|--------|------|-----|-----|
| **mmsaki** | 🥉 | 0.40ms | 0.40ms | 0.50ms |
| **solc** | invalid | - | - | - |
| **nomicfoundation** | 🥈 | 0.30ms | 0.30ms | 0.40ms |
| **juanfranblanco** | ok | 0.90ms | 0.70ms | 2.80ms |
| **qiuxiang** | 🥇 | 0.20ms | 0.20ms | 0.40ms |

<details>
<summary>Response details</summary>

**mmsaki**

```json
[
  {
    "range": {
      "end": {
        "character": 25,...
```

**solc**

```json
error: Unknown method textDocument/references
```

**nomicfoundation**

```json
[
  {
    "range": {
      "end": {
        "character": 25,...
```

**juanfranblanco**

```json
[
  {
    "range": {
      "end": {
        "character": 26,...
```

**qiuxiang**

```json
[
  {
    "range": {
      "end": {
        "character": 26,...
```

</details>

---

*Generated from [`benchmarks/counter/2026-02-13T10-03-59Z.json`](benchmarks/counter/2026-02-13T10-03-59Z.json) — benchmark run: 2026-02-13T10:03:59Z*

See [DOCS.md](./DOCS.md) for usage and installation.
