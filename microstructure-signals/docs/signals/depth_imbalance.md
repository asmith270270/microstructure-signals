# Depth Imbalance

**Implementation**: [src/depth_imbalance.rs](../../src/depth_imbalance.rs)

## What It Does

Measures relative liquidity difference between bid and ask sides across multiple price levels. Snapshot measure of current book state (not changes like OFI).

**Formula**: `(bid_volume - ask_volume) / (bid_volume + ask_volume)`

**Output**: [-1.0, 1.0]
- +1.0 = All liquidity on bid side
- 0.0 = Balanced book
- -1.0 = All liquidity on ask side

## Configuration

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
config.depth_levels = 3; // Use top 3 levels
config.signals.depth_imbalance = true;

// Optional: EWMA-smooth the raw imbalance value itself before it reaches the normaliser
config.depth_imbalance_smoothing_half_life = Some(20.0);
```

## Usage

```rust
let snapshot = engine.on_book_update(&book);

if !snapshot.depth_imbalance.is_nan() {
    let di = snapshot.depth_imbalance;
    if di > 0.7 {
        // Heavy bids - strong support
    } else if di < -0.7 {
        // Heavy asks - strong resistance
    }
}
```

## Depth Levels Parameter

- **1 level**: Top-of-book only (similar to OFI but snapshot)
- **3-5 levels**: Typical, balances signal quality vs noise
- **10+ levels**: Very liquid markets, full book state

Trade-off:
- More levels → smoother signal, includes less relevant liquidity
- Fewer levels → noisier, more actionable (closer to mid)

## Interpretation

| DI Value | Book State | Typical Action |
|----------|------------|----------------|
| > 0.7 | Very heavy bids | Strong support, potential reversal |
| 0.3 to 0.7 | Moderate bid imbalance | Mild buy-side pressure |
| -0.3 to 0.3 | Balanced | No clear directional signal |
| -0.7 to -0.3 | Moderate ask imbalance | Mild sell-side pressure |
| < -0.7 | Very heavy asks | Strong resistance, potential reversal |

## When to Use

**Use depth imbalance when**:
- Need slower-moving signal than OFI
- Assessing support/resistance levels
- Gauging patient order flow (limit orders away from top)
- Building market-making strategy using deeper book
- Care about spoofing detection (sudden depth changes)

**Don't use when**:
- Need ultra-low-latency signals (use OFI)
- Book very thin (< 3 levels populated)
- Depth unreliable (dark pools, iceberg orders)
- Only have access to top-of-book

## Depth Imbalance vs OFI

| Aspect | Depth Imbalance | OFI |
|--------|----------------|-----|
| **Type** | Snapshot/state | Flow/change |
| **Levels** | Multiple (1-10+) | Top-of-book only |
| **Update freq** | Every snapshot | Every snapshot |
| **Interpretation** | Where liquidity sits | Where liquidity flows |
| **Predictive** | Medium-term (100ms-1s) | Short-term (10-100ms) |
| **Signal** | Static pressure | Dynamic pressure |

## Example

```rust
let book = BookSnapshot {
    bids: vec![
        PriceLevel { price: 100.0, quantity: 500.0 },
        PriceLevel { price: 99.9, quantity: 400.0 },
        PriceLevel { price: 99.8, quantity: 300.0 },
    ],
    asks: vec![
        PriceLevel { price: 100.1, quantity: 100.0 },
        PriceLevel { price: 100.2, quantity: 100.0 },
        PriceLevel { price: 100.3, quantity: 100.0 },
    ],
    timestamp_ns: 0,
};

let snapshot = engine.on_book_update(&book);

// bid_total = 500 + 400 + 300 = 1200
// ask_total = 100 + 100 + 100 = 300
// DI = (1200 - 300) / (1200 + 300) = 0.6
```

## Combining with OFI

Depth Imbalance and OFI are complementary:

```rust
if di > 0.5 && ofi > 0.0 {
    // Strong buy: heavy bids + buying flow
} else if di > 0.5 && ofi < 0.0 {
    // Uncertain: heavy bids but selling flow (potential absorption)
} else if di < -0.5 && ofi < 0.0 {
    // Strong sell: light bids + selling flow
}
```

## Edge Cases

- **Fewer levels than configured**: Uses available levels
- **Empty side, liquidity on the other**: Returns ±1.0 (fully imbalanced), not NaN
- **Zero total quantity (both sides empty)**: `snapshot.depth_imbalance` is `f64::NAN`
