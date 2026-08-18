# Effective Spread

**Implementation**: [src/effective_spread.rs](../../src/effective_spread.rs)

## What It Does

Measures the realised transaction cost of a trade relative to the mid-price at the time of execution. Captures the actual price impact a market participant incurs, as opposed to the quoted spread which only reflects the nominal cost.

**Output**: Unbounded f64
- Positive = trade was costly relative to mid (typical for market orders)
- Negative = trade was executed at a favourable price (e.g., liquidity rebate)
- Zero = trade executed exactly at mid

## Formulas

**Effective spread** (absolute):
```
effective_spread = 2 × sign × (trade_price − mid_price)
```
where `sign = +1` for buyer-initiated trades, `−1` for seller-initiated.

**Relative effective spread** (proportional):
```
relative_effective_spread = effective_spread / mid_price
```

## Configuration

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
config.signals.effective_spread = true;
```

Requires trade classification (`trade-classifier` feature) to determine trade side.

## Usage

```rust
let snapshot = engine.on_trade(&trade, &book);

if !snapshot.effective_spread.is_nan() {
    println!("Effective spread: {:.4}", snapshot.effective_spread);
    println!("Relative spread:  {:.4}", snapshot.relative_effective_spread);
}
```

## Availability

`effective_spread` is always populated if the signal is enabled and a trade has occurred.

`relative_effective_spread` is additionally guarded against a non-positive mid-price. If `mid_price <= 0.0`, the field is `f64::NAN` even when `effective_spread` is valid. This can occur on synthetic or near-zero-price instruments. Always check `is_nan()` on both fields independently — do not assume that a valid `effective_spread` implies a valid `relative_effective_spread`.

## When to Use

**Use when**:
- Measuring execution quality against a mid-price benchmark
- Estimating adverse selection costs per trade
- Comparing realised spread to quoted spread (price improvement)

**Don't use when**:
- No trade classifier is available (requires buy/sell assignment)
- Comparing across instruments of very different price levels (use relative spread instead)
