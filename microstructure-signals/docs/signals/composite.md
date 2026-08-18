# Composite Toxicity

**Implementation**: [src/composite.rs](../../src/composite.rs)

## What It Does

Combines multiple normalised signals (z-scores) into a single aggregate measure of order flow toxicity. Provides unified view of market risk by weighting different information sources.

**Output**: Unbounded f64 toxicity score
- Positive: Buying pressure across signals
- Negative: Selling pressure across signals
- Near zero: Balanced market

## Configuration

```rust
use microstructure_signals::CompositeWeights;

let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();

config.toxicity_weights = CompositeWeights {
    ofi: 1.5,
    vsi: 1.0,
    depth_imbalance: 0.5,
    microprice_deviation: 0.3,
};
config.composite_smoothing_half_life = Some(100.0); // Optional EWMA smoothing
config.signals.composite_toxicity = true;
```

## Why Combine Signals?

Different signals capture different aspects:
- **OFI**: Book flow changes (fast, predictive)
- **VSI**: Trade flow imbalance (slow, stable)
- **Depth Imbalance**: Book state (moderate, broad)

Combining them:
- Reduces false positives (multiple signals agreeing)
- Increases coverage (different toxic event types)
- Improves robustness (less single-signal noise)

## Usage

```rust
let snapshot = engine.on_book_update(&book);

if !snapshot.toxicity.is_nan() {
    let toxicity = snapshot.toxicity;
    if toxicity.abs() > 3.0 {
        // High toxicity - cancel quotes
        cancel_all_quotes();
    } else if toxicity.abs() > 2.0 {
        // Moderate toxicity - widen spreads
        let spread = base_spread * (1.0 + toxicity.abs() * 0.1);
        update_quotes(spread);
    }
    // Normal market making below threshold
}
```

### Market Making with Dynamic Spreads

```rust
let base_spread = 0.01;

if !snapshot.toxicity.is_nan() {
    let toxicity = snapshot.toxicity;
    let spread = base_spread * (1.0 + toxicity.abs() * 0.1);

    if toxicity > 0.0 {
        // Buying pressure - widen ask more
        update_quotes(bid_spread: spread, ask_spread: spread * 1.2);
    } else {
        // Selling pressure - widen bid more
        update_quotes(bid_spread: spread * 1.2, ask_spread: spread);
    }
}
```

## When to Use

**Use composite toxicity when**:
- Need single aggregate measure for decisions
- Building market-making strategy
- Want to combine multiple information sources
- Comparing overall market health across instruments
- Need threshold-based trigger

**Don't use when**:
- Need granular signal-specific information
- Individual signal interpretation more actionable
- Still calibrating individual signal weights

## Weights

Default: `CompositeWeights::default()` — equal weight of 1.0 for each signal.

Weights are specified by name via `CompositeWeights`, so there is no risk of assigning them in the wrong order:

```rust
config.toxicity_weights = CompositeWeights {
    ofi: 2.0,
    vsi: 1.0,
    depth_imbalance: 0.5,
    microprice_deviation: 0.25,
};
```

**Choosing weights**:
1. **Equal weights**: Simple, good starting point
2. **Predictive power**: Weight by historical IC² (information coefficient)
3. **Risk-based**: Weight by adverse selection cost
4. **Empirical**: Optimise on backtest performance

Weights are normalised by their absolute sum before combining, so only relative magnitudes matter. Negative weights invert that signal's contribution.

Inputs (in internal computation order): `ofi_z`, `vsi_z`, `depth_imbalance_z`, `microprice_deviation_z`.

## Optional Smoothing

```rust
config.composite_smoothing_half_life = Some(100.0);
```

Applies EWMA smoothing to reduce noise. Use when:
- Market very noisy
- Want slower toxicity adaptation
- Building longer-horizon strategies

Don't smooth when:
- Need fastest possible signals
- Already using smoothed inputs

## Missing Values

When a z-score is NaN (disabled or not ready), weights are automatically rescaled over available signals.

Example:
- Weights: `[ofi: 1.0, vsi: 1.0, depth_imbalance: 1.0, microprice_deviation: 1.0]`
- VSI not ready: `[2.0, NaN, 1.0, NaN]`
- Result: `(1.0*2.0 + 1.0*1.0) / (1.0 + 1.0) = 1.5`

**This rescaling changes the composite's effective scale, not just its value.** With all
four signals present, `toxicity` is a blend of four (roughly) unit-variance inputs. If three
of them are momentarily unavailable — routine right after a VSI bucket boundary, or during
any single signal's own warm-up — the composite silently becomes a one-signal statistic with
a different effective variance, using the exact same weight formula. A fixed threshold like
`toxicity.abs() > 3.0` does not mean the same thing in both cases: it will trip more or less
often purely because of which components happened to be present, not because the market
changed. If you're thresholding `toxicity` or `adverse_selection` for a trading decision,
check `snapshot.normalisers_ready` (or the per-signal `*_normaliser_ready` fields) first, or
be aware that your effective false-positive rate varies with signal availability.

## Interpretation

| Toxicity | Interpretation | Action |
|----------|---------------|--------|
| > 3.0 | Very high buying pressure | Cancel asks or widen significantly |
| 2.0-3.0 | High buying pressure | Widen ask more than bid |
| -1.0 to 1.0 | Normal | Standard market making |
| -3.0 to -2.0 | High selling pressure | Widen bid more than ask |
| < -3.0 | Very high selling pressure | Cancel bids or widen significantly |
