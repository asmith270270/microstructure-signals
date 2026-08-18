# Usage Guide

## Quick Start

```rust
use microstructure_signals::{SignalEngine, SignalEngineConfig};

let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
let mut engine = SignalEngine::new(config).unwrap();

let snapshot = engine.on_book_update(&book);
```

## Processing Events

### Streaming (per-event)

Process events one at a time — lowest latency, suitable for live trading:

```rust
let snapshot = engine.on_book_update(&book);

let snapshot = engine.on_trade(&trade, &book);
```

### Batch processing

Process a pre-recorded event slice and collect all snapshots:

```rust
use microstructure_signals::types::MarketEvent;

let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
let mut engine = SignalEngine::new(config).unwrap();

let events: Vec<MarketEvent> = load_historical_data("2024-01-15.bin");
let results = engine.process_events(&events);

for (timestamp, snapshot) in results {
    analyse(timestamp, snapshot);
}
```

Use `process_events_with` to stream results through a callback without allocating the output `Vec`:

```rust
engine.process_events_with(&events, |timestamp, snapshot| {
    record(timestamp, snapshot);
});
```

Use `process_events_try_with` when the callback can fail — processing stops on the first error:

```rust
engine.process_events_try_with(&events, |timestamp, snapshot| -> Result<(), WriteError> {
    writer.write(timestamp, snapshot)?;
    Ok(())
})?;
```

## Signal Selection

Start from all-off and enable only the signals you need:

```rust
use microstructure_signals::SignalSelection;

let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();

let mut sel = SignalSelection::none();
sel.ofi = true;
sel.ofi_z = true;
sel.depth_imbalance = true;
sel.depth_imbalance_z = true;
sel.composite_toxicity = true;
config.signals = sel;

let engine = SignalEngine::new(config).unwrap();
```

Or start from all-on and disable what you don't need:

```rust
let mut sel = SignalSelection::default();
sel.vsi = false;
sel.vsi_z = false;
sel.adverse_selection = false;
config.signals = sel;
```

## Workflows

### Market Making with Dynamic Spreads

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
let mut engine = SignalEngine::new(config).unwrap();

let base_spread = 0.01;

loop {
    let book = get_next_book();
    let snapshot = engine.on_book_update(&book);

    if !snapshot.toxicity.is_nan() {
        let toxicity = snapshot.toxicity;
        let spread = base_spread * (1.0 + toxicity.abs() * 0.1);

        if toxicity.abs() > 3.0 {
            cancel_all_quotes();
            continue;
        }

        let mid = snapshot.mid_price;
        update_quotes(mid - spread / 2.0, mid + spread / 2.0);
    }
}
```

### Directional Trading

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
config.adverse_selection_weights = CompositeWeights { ofi: 1.5, vsi: -0.3, depth_imbalance: 0.5, microprice_deviation: 1.0 };
let mut engine = SignalEngine::new(config).unwrap();

loop {
    let book = get_next_book();
    let snapshot = engine.on_book_update(&book);

    if !snapshot.adverse_selection.is_nan() {
        let target_position = snapshot.adverse_selection.clamp(-1.0, 1.0);

        if (target_position - current_position).abs() > 0.1 {
            execute_trade(target_position - current_position);
        }
    }
}
```

### Backtesting

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
let mut engine = SignalEngine::new(config).unwrap();

let events = load_historical_data("2024-01-15.bin");
let results = engine.process_events(&events);

let mut pnl = 0.0;
for (_, snapshot) in &results {
    if !snapshot.adverse_selection.is_nan() && !snapshot.mid_price.is_nan() {
        let position = snapshot.adverse_selection.clamp(-1.0, 1.0);
        pnl += position * snapshot.mid_price;
    }
}
```

## Resetting an Engine

`engine.reset()` clears all signal history and restarts normaliser warm-up, while keeping the original configuration intact. Use it when switching to a new symbol or session:

```rust
// End of session for symbol A
engine.reset();

// Now reuse the same engine for symbol B
engine.process_events_with(&symbol_b_events, |timestamp, snapshot| {
    record(timestamp, snapshot);
});
```

`reset()` is equivalent to constructing a fresh `SignalEngine` with the same config. All EWMA statistics, ring buffers, trade classifiers, and the signal snapshot are cleared. The engine's compiled configuration (half-lives, depth levels, bucket sizes) is preserved.

## Performance Optimisation

```rust
config.signals.microprice = false;
config.signals.adverse_selection = false;
config.ofi_window = 50;
config.normalisation_warm_up = 20;
```

**Performance (release mode):** single-event latency is roughly 56–106 ns p50 depending on
which signals are enabled, giving multi-million updates/sec throughput. See
[docs/BENCHMARKS.md](BENCHMARKS.md) for current measured figures per configuration — those
numbers move as the engine changes, so this guide doesn't restate them.

## Cargo Features

Control binary size and dependencies by selecting only needed signals at compile time:

```toml
# Cargo.toml
[dependencies]
microstructure-signals = { version = "0.1", default-features = false, features = ["minimal"] }
```

**Available features:**
- `default`: `std` + all signals enabled
- `std`: Standard library support (includes `alloc`) — **enabled by default**
- `alloc`: Heap allocator support for `no_std` environments
- `minimal`: Only OFI + Depth Imbalance (**~40% smaller binary**)
- `ofi`: Order Flow Imbalance signal
- `depth-imbalance`: Depth Imbalance signal
- `microprice`: Microprice calculator
- `vsi`: Volume-Synchronised Imbalance (requires `trade-classifier`)
- `normaliser`: Z-score normalisation (required for composite signals)
- `regime-normaliser`: Regime-change aware normaliser (requires `normaliser`)
- `composite`: Composite toxicity signal (requires `normaliser`)
- `adverse-selection`: Adverse selection signal (requires `normaliser`)
- `trade-classifier`: Trade classification (Tick/Lee-Ready)
- `effective-spread`: Effective/relative spread signal (requires `trade-classifier`)
- `serde`: `Serialize`/`Deserialize` on config and state types (optional, not in `default`)

**Example configurations:**
```toml
# Minimal deployment
features = ["minimal"]

# Market making
features = ["ofi", "depth-imbalance", "vsi", "normaliser", "composite", "trade-classifier"]

# All signals
features = ["all-signals"]
```

## no_std Support (Embedded/FPGA Deployment)

The crate supports three allocation modes for different deployment environments:

### 1. Standard Mode (default)
Full `std` library with heap allocation:
```toml
[dependencies]
microstructure-signals = "0.1"  # std enabled by default
```

### 2. Allocator Mode (`alloc` feature)
`no_std` with custom allocator support (e.g., embedded Linux, bare metal with heap):
```toml
[dependencies]
microstructure-signals = { version = "0.1", default-features = false, features = ["alloc", "minimal"] }
```

### 3. Pure no_std Mode (no heap)
Fixed-size arrays for true embedded/FPGA deployment:
```toml
[dependencies]
microstructure-signals = { version = "0.1", default-features = false, features = ["minimal"] }
```

Uses compile-time constants:
- `MAX_BOOK_DEPTH = 10` price levels per side
- `MAX_WINDOW_SIZE = 1000` for ring buffers
- `MAX_COMPOSITE_SIGNALS = 4` for toxicity weights

API differences in pure no_std:
```rust
// With alloc: batch processing
let results = engine.process_events(&events);

// Without alloc: single-event processing
let (timestamp, snapshot) = engine.process_event(&event);
```

**Latency benefits of no_std:**
- **Deterministic timing**: No heap allocator locks or GC pauses
- **Cache-efficient**: Fixed-size arrays on stack (L1 cache hits)
- **Kernel bypass**: Deploy directly on smart NICs or FPGAs

## See Also

- [Signal Reference](signals/) - Individual signal documentation
- [README](../README.md) - Crate overview
