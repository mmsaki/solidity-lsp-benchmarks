# Benchmark Analysis

Analysis of `v4-core` (`src/libraries/Pool.sol`) — 10 iterations per benchmark.

## Capability Matrix

| Benchmark | mmsaki | solc | nomicfoundation | juanfranblanco | qiuxiang |
|-----------|--------|------|-----------------|----------------|----------|
| Spawn + Init | ok | ok | ok | ok | ok |
| Diagnostics | ok | ok | timeout | crash | timeout |
| Go to Definition | ok | empty | timeout | crash | timeout |
| Go to Declaration | ok | no | timeout | crash | timeout |
| Hover | ok | empty | timeout | crash | timeout |
| Find References | ok | no | timeout | crash | timeout |
| Document Symbols | ok | no | timeout | crash | timeout |
| Document Links | ok | no | timeout | crash | timeout |

| Server | Working | Failed | Success Rate |
|--------|---------|--------|--------------|
| mmsaki | 8/8 | 0/8 | 100% |
| solc | 2/8 | 6/8 | 25% |
| nomicfoundation | 1/8 | 7/8 | 12% |
| juanfranblanco | 1/8 | 7/8 | 12% |
| qiuxiang | 1/8 | 7/8 | 12% |

## Spawn + Init

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | Overhead | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|----------|-----------|
| mmsaki | ok | 4.00ms | 4.0ms | 4.4ms | 0.4ms | 1.10x | 3.66ms | 4.42ms | 0.76ms | **1.0x (fastest)** | - |
| solc | ok | 113.40ms | 113.3ms | 116.1ms | 2.8ms | 1.02x | 112.13ms | 116.08ms | 3.95ms | **28.4x** | **28.4x slower** |
| nomicfoundation | ok | 862.90ms | 864.9ms | 881.1ms | **16.2ms** | 1.02x | 845.30ms | 881.14ms | **35.84ms** | **215.7x** | **215.7x slower** |
| juanfranblanco | ok | 514.70ms | 513.7ms | 518.9ms | 5.2ms | 1.01x | 510.59ms | 518.88ms | 8.29ms | **128.7x** | **128.7x slower** |
| qiuxiang | ok | 67.90ms | 67.6ms | 69.4ms | 1.8ms | 1.03x | 66.90ms | 69.44ms | 2.54ms | **17.0x** | **17.0x slower** |

## Diagnostics

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | Overhead | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|----------|-----|-----------|
| mmsaki | ok | 443.20ms | 443.4ms | 452.2ms | 8.8ms | 1.02x | 438.57ms | 452.23ms | **13.66ms** | 3.3x | 39.7 MB | - |
| solc | ok | 133.30ms | 133.9ms | 135.1ms | 1.2ms | 1.01x | 130.96ms | 135.10ms | 4.14ms | **1.0x (fastest)** | 26.2 MB | 3.3x faster |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | - | 511.5 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | - | 70.1 MB | timeout |

## Go to Definition

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|-----|-----------|
| mmsaki | ok | 8.90ms | 8.9ms | 9.8ms | 0.9ms | 1.10x | 8.35ms | 9.83ms | 1.48ms | 37.7 MB | - |
| solc | empty | - | - | - | - | - | - | - | - | 25.9 MB | empty |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | 511.6 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | 69.7 MB | timeout |

## Go to Declaration

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|-----|-----------|
| mmsaki | ok | 8.90ms | 8.8ms | 9.7ms | 0.9ms | 1.10x | 8.25ms | 9.72ms | 1.47ms | 39.5 MB | - |
| solc | no | - | - | - | - | - | - | - | - | 25.8 MB | empty |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | 513.0 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | 70.0 MB | timeout |

## Hover

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|-----|-----------|
| mmsaki | ok | 13.70ms | 13.8ms | 14.3ms | 0.5ms | 1.04x | 12.90ms | 14.35ms | 1.45ms | 29.5 MB | - |
| solc | empty | - | - | - | - | - | - | - | - | 25.6 MB | empty |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | 488.1 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | 70.0 MB | timeout |

## Find References

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|-----|-----------|
| mmsaki | ok | 10.50ms | 10.4ms | 11.5ms | 1.1ms | 1.11x | 9.96ms | 11.52ms | 1.56ms | 29.9 MB | - |
| solc | no | - | - | - | - | - | - | - | - | 25.9 MB | empty |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | 511.0 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | 70.1 MB | timeout |

## Document Symbols

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|-----|-----------|
| mmsaki | ok | 8.40ms | 8.3ms | 8.8ms | 0.5ms | 1.06x | 8.09ms | 8.79ms | 0.70ms | 37.7 MB | - |
| solc | no | - | - | - | - | - | - | - | - | 25.8 MB | empty |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | 513.1 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | 70.2 MB | timeout |

## Document Links

| Server | Status | Mean | p50 | p95 | Spread | Spike | Min | Max | Range | RSS | vs mmsaki |
|--------|--------|------|-----|-----|--------|-------|-----|-----|-------|-----|-----------|
| mmsaki | ok | 62.70ms | 62.7ms | 64.5ms | 1.8ms | 1.03x | 61.12ms | 64.50ms | 3.38ms | 29.0 MB | - |
| solc | no | - | - | - | - | - | - | - | - | 26.2 MB | empty |
| nomicfoundation | timeout | - | - | - | - | - | - | - | - | 512.5 MB | timeout |
| juanfranblanco | crash | - | - | - | - | - | - | - | - | 0.0 MB | crash |
| qiuxiang | timeout | - | - | - | - | - | - | - | - | 69.3 MB | timeout |

## Peak Memory (RSS)

| mmsaki | solc | nomicfoundation | juanfranblanco | qiuxiang |
|--------|------|-----------------|----------------|----------|
| 39.7 MB | 26.2 MB | 513.1 MB | 0.0 MB | 70.2 MB |

---

*Generated from [`benchmarks/v4-core/2026-02-13T09-53-03Z.json`](benchmarks/v4-core/2026-02-13T09-53-03Z.json) — benchmark run: 2026-02-13T09:53:03Z*
