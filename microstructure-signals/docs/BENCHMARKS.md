# Performance Benchmarks

Measured on Windows 11, release build (`target-cpu=native`), single-threaded. Latency figures
are p50/p99 in nanoseconds via `cargo bench`; throughput is derived as `1000 / p50_ns` (M
events/sec) unless stated otherwise.

## Running full Criterion benchmarks

```
cargo bench                             # HTML report in target/criterion/
cargo bench --bench signals             # latency benchmarks only
cargo bench --bench throughput          # batch throughput benchmarks only

# Save a named baseline, then compare future runs against it:
cargo bench -- --save-baseline approved
cargo bench -- --baseline approved
```

---

## Latency

### Book update

| Configuration | p50 (ns) | p99 (ns) | Throughput (M/s) |
|---|---|---|---|
| raw_only (OFI + DI + microprice, no normalisation) | 56 | 76 | ~17.9 |
| normalised (all z-scores, no composite) | 80 | 128 | ~12.5 |
| all_signals (z-scores + composite + adverse selection) | 106 | 168 | ~9.4 |

### Book depth scaling (raw_only, OFI window 100)

| Depth levels | p50 (ns) | p99 (ns) |
|---|---|---|
| 1 | 52 | 58 |
| 5 | 46 | 114 |
| 10 | 34 | 60 |
| 20 | 44 | 70 |

Latency is roughly flat across depths 1–20 rather than scaling with depth. At these magnitudes
(tens of ns) branch prediction and loop overhead dominate the measurement.

### Trade path

| Configuration | p50 (ns) | p99 (ns) |
|---|---|---|
| quote_rule, no VSI | 30 | 38 |
| quote_rule, with VSI (bucket_volume = 500) | 42 | 70 |

### OFI window size scaling

| Window | p50 (ns) | p99 (ns) |
|---|---|---|
| 10 | 12 | 18 |
| 100 | 10 | 14 |
| 500 | 8 | 10 |
| 1000 | 8 | 10 |

OFI uses a ring buffer with a running sum, so window size has negligible latency impact.

### Individual signal components (not through engine)

| Component | p50 (ns) | p99 (ns) |
|---|---|---|
| `Ofi::update` (window 100) | 10 | 14 |
| `MultiLevelOfi::update` (depth 5, decay 0.5) | 74 | 110 |
| `MultiLevelOfi::update` (depth 10, decay 0.5) | 128 | 212 |
| `DepthImbalance::update` (depth 5) | 8 | 16 |
| `EwmaNormaliser::update_and_normalise` (warm) | 12 | 14 |
| `RegimeNormaliser::update_and_normalise` (warm) | 14 | 18 |

### Normaliser comparison (book_update, no composite)

| Normaliser | p50 (ns) | p99 (ns) | Overhead vs EWMA |
|---|---|---|---|
| EWMA | 80 | 116 | — |
| RegimeNormaliser | 106 | 150 | +26 ns p50 |

`RegimeNormaliser` runs two EWMAs (fast and slow) and computes a variance-ratio check on every
update; see `src/regime_normaliser.rs` for the statistical limitations of that check.

### State management

| Operation | p50 (ns) | p99 (ns) |
|---|---|---|
| `capture_normaliser_state` | 4 | 6 |
| `restore_normaliser_state` (re-seed an existing engine) | 24 | 46 |
| `cold_start_and_restore` (`SignalEngine::new()` + restore) | 420 | 970 |

`capture` reads a handful of struct fields. `restore` mutates the cached EWMA mean/variance
directly without recomputing the decay constant, so its cost is independent of half-life.
`cold_start_and_restore` includes `SignalEngine::new()`'s allocations (composite/adverse-selection
weight vectors, the VSI ring buffer) and is dominated by that construction cost rather than the
restore itself.

---

## Throughput (batch processing)

Measured via `cargo bench --bench throughput`. All benchmarks use a pre-warmed engine (500 events
consumed before timing begins) so normaliser warm-up overhead is excluded. All scenarios use VSI
(`bucket_volume = 1000.0`) and the full signal set. Event mix: ~80% book updates, ~20% trades
(stable/trending), ~75%/25% (volatile).

Each scenario replays a fixed 10k–100k event slice hundreds of times, which keeps the data
L2/L3-cache warm after the first pass; real streaming data arrives fresh from a network buffer and
will be somewhat slower at large volumes. The 100k-event batches (~17 MB) overflow L3 cache and
give a more realistic memory-bandwidth-limited figure. **For production sizing, use the
single-event latency figures above** (throughput = 1 / latency_per_event).

### Market scenario throughput (pre-warmed engine, 10k-book-iteration batch)

| Scenario | Median (M events/s) | 95% CI |
|---|---|---|
| stable (mid ±0.01, spread 0.10) | 13.4 | 12.3–14.2 |
| trending (mid +0.002/update, spread 0.10) | 15.7 | 15.2–16.0 |
| volatile (mid ±2.0, spread 0.05–1.0) | 15.3 | 14.6–15.8 |
| mixed (equal thirds of the above) | 15.0 | 14.0–15.8 |

Trending is faster than stable because larger price moves let OFI take a simpler branch (no
`== prev_price` subtraction), saving a few ns per update.

### Sustained throughput (pre-warmed engine, 10k events, stable scenario, book-only)

| Configuration | Median (M events/s) | Notes |
|---|---|---|
| book_only_raw (no normalisation) | 12.5 | all book signals, no z-scores |
| book_only_normalised (EWMA, all z-scores) | 12.2 | includes composite + adverse selection |
| with_vsi_all_signals (full stack) | 10.7 | VSI bucket_volume = 1000, includes trades |

### Batch size effect: `process_events` (returns `Vec`) vs `process_events_with` (callback)

Both use a pre-warmed engine and the stable event sequence.

| Batch size | `process_events` Vec (M/s) | `process_events_with` callback (M/s) | Callback speedup |
|---|---|---|---|
| 1,000 events | 13.9 | 15.8 | +14% |
| 10,000 events | 9.5 | 15.1 | +60% |
| 100,000 events | 7.0 | 7.3 | +4% |

The Vec path allocates a `Vec<(u64, SignalSnapshot)>` proportional to the event count and writes
every snapshot into it. At 10k events this is ~10k × 176 bytes ≈ 1.7 MB, which overflows L2 cache
and drives the gap; at 100k events both paths are bottlenecked on memory bandwidth and converge.

**Use `process_events_with` whenever you don't need to retain all snapshots simultaneously.**

---

## Updating this file

After any deliberate performance change, run `cargo bench -- --save-baseline approved` and copy
the updated figures into the tables above.
