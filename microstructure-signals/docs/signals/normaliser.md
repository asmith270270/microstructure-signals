# EWMA Z-Score Normaliser

**Implementation**: [src/normaliser.rs](../../src/normaliser.rs)

## What It Does

Converts raw signal values into z-scores using exponentially weighted moving average (EWMA) statistics. Allows comparison across instruments and detection of unusual market conditions.

**Output**: Z-score (standard deviations from mean)
- |z| < 1: Normal (68% of observations)
- 1 < |z| < 2: Moderately unusual (27%)
- |z| > 2: Statistically unusual (5%)
- |z| > 3: Very unusual (0.3%)

## Why Normalise?

Raw signals vary in magnitude across:
- **Instruments**: ES futures OFI ~1000s, small-cap equity OFI ~10s
- **Time**: Morning volatility vs afternoon
- **Regime**: Calm vs crisis periods

Z-scores standardise everything to the same scale for comparison and thresholds.

## Configuration

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();

config.normalisation_half_life = 200.0;  // EWMA half-life (observations)
config.normalisation_warm_up = 50;     // Min observations before returning z-scores

config.signals.ofi_z = true;           // Enable OFI z-score
config.signals.depth_imbalance_z = true;
config.signals.vsi_z = true;
```

## Parameters

### Half-Life
Number of observations for the influence of a past value to decay to 50% of its original weight. Smaller = adapts faster; larger = more stable baseline. Counterintuitively, a value of 1000 does **not** mean "normalise over 1000 ticks" — it means data from 1000 ticks ago still has 50% of its original influence on the mean estimate.

- **Small (10-50)**: Fast adaptation, tracks recent regime
- **Medium (100-500)**: Balanced, typical default
- **Large (1000+)**: Slow adaptation, stable baseline

Recommended by signal:
- OFI: 100-500 events
- Depth Imbalance: 500-1000 events
- VSI: 50-200 buckets

### Warm-Up Period
Z-scores unreliable with few observations. Returns `None` until warm-up complete.
- **Minimum**: 10 (very unstable)
- **Typical**: 50-100
- **Conservative**: 200+

## Usage

```rust
let snapshot = engine.on_book_update(&book);

if !snapshot.ofi_z.is_nan() {
    let ofi_z = snapshot.ofi_z;
    if ofi_z.abs() > 2.0 {
        // Unusual order flow - widen spreads or pause
    }
}
```

### Combining Normalised Signals

```rust
// All z-scores on same scale, can be compared
let ofi_z = if !snapshot.ofi_z.is_nan() { snapshot.ofi_z } else { 0.0 };
let vsi_z = if !snapshot.vsi_z.is_nan() { snapshot.vsi_z } else { 0.0 };
if ofi_z.abs() > vsi_z.abs() {
    // OFI signal stronger than VSI
}
```

## When to Use

**Use normalisation when**:
- Comparing signals across different instruments
- Combining multiple signals with different scales
- Detecting regime changes
- Building threshold-based strategies
- Need adaptive statistics

**Don't use when**:
- Raw magnitude is meaningful (e.g., "OFI = +100 contracts")
- Trading single instrument with stable characteristics
- Need fastest possible signals (normalisation adds lag)
- During warm-up period

## Calibrating to your feed

`normalisation_half_life` is measured in **observation counts**, not seconds. Its effective lookback window depends entirely on your feed's update rate and the instrument's activity. The same value of `1000` means very different things at different venues:

| Feed rate | `half_life = 1000` effective window |
|-----------|--------------------------------------|
| 1 update/sec | ~17 minutes |
| 10 updates/sec | ~100 seconds |
| 100 updates/sec | ~10 seconds |
| 1000 updates/sec | ~1 second |

**Recommended calibration approach:**

1. Measure your feed's average book update rate for the instrument and session you care about (e.g., `update_count / session_seconds`).
2. Choose a target lookback window in seconds (e.g., 5 minutes = 300 s for a position-holding strategy).
3. Compute `half_life = target_seconds × updates_per_second`.

For example, a 5-minute EWMA on a feed delivering 20 book updates per second:
```
half_life = 300 × 20 = 6000 book updates
```

**VSI is different.** For VSI, the observation unit is **completed volume buckets**, not individual trades or book updates. A typical liquid futures instrument completes 2–10 buckets per minute; an illiquid name might complete 1 bucket per hour. Calibrate `vsi` half-life independently:
```
vsi_half_life = target_minutes × avg_buckets_per_minute
```

**Session boundaries.** EWMA state accumulates across the session. If you leave the engine running overnight, stale mean/variance from the previous session will distort the next session's z-scores until the EWMA decays. Call `engine.reset()` at the start of each session to avoid this.

## EWMA vs Simple Moving Average

**EWMA** (used here):
- Smooth adaptation to regime changes
- Recent data weighted higher
- No sudden jumps
- Single parameter (half-life)

**Simple MA**:
- Fixed window
- Sudden jumps when old data exits
- Slow to adapt
- Equal weights in window

## Example

```rust
use microstructure_signals::EwmaNormaliser;

let half_life = 200.0_f64;
let warm_up: usize = 50;
let mut normaliser = EwmaNormaliser::new(half_life, warm_up).unwrap();

// Feed observations
for value in raw_ofi_values {
    if let Some(z_score) = normaliser.update_and_normalise(value) {
        println!("Z-score: {:.2}", z_score);
    }
}
```

## Edge Cases

- **Zero variance**: Returns `None` (avoid division by zero)
- **Warm-up period**: Returns `None` until enough observations
- **First observation**: Initialises mean, variance starts at zero

## Readiness States

`EwmaNormaliser` exposes two readiness checks:

- `is_ready()` — true when both the warm-up observation count has been reached **and** variance is above the numerical floor. This is the safe guard to check before trusting a z-score.
- `is_warmup_complete()` — true when only the observation count threshold has been reached, regardless of variance. Use this to detect when the normaliser has "seen enough data" even if the signal happens to be constant (zero variance) at that moment.

Both are mirrored as fields on `SignalSnapshot` (e.g., `ofi_normaliser_ready` and `ofi_normaliser_warmup_complete`).

## Regime Transitions (RegimeNormaliser)

When `regime_normaliser_params` is configured, the normaliser detects volatility regime changes and resets its EWMA statistics to a faster half-life to catch up quickly. `update_and_normalise` returns `None` for the entire duration `is_in_regime_change()` is `true` — not just the tick it enters — and every returned z-score (in or out of a regime change) is clamped to `±10.0`, so a near-zero variance estimate can never produce an extreme-looking value.

Use `RegimeNormaliser::is_in_regime_change()` to detect this state:

```rust
if let Some(params) = &config.regime_normaliser_params {
    // z-scores disappear entirely while a regime change is active
    // check snapshot.normalisers_ready before acting on composite signals
}
```

Composite signals (`toxicity`, `adverse_selection`) treat a suppressed input as absent and rescale weights over the remaining present signals (see [composite.md](composite.md#missing-values)) — they only stop updating entirely if *every* input is suppressed at once.

### Known limitation: entry detection is not statistically reliable

`is_in_regime_change()` becoming `true` is **not** a trustworthy signal that a real
volatility event occurred. The entry test compares the fast and slow EWMA variances, but
both are computed from the same input stream, so they are correlated rather than
independent — the standard statistical test for comparing two variances assumes
independence and does not apply here. Empirically, with the crate's own default half-lives,
a freshly constructed `RegimeNormaliser` fed nothing but calm, unchanging-variance data
still enters a "regime change" in essentially every run within a few hundred to a few
thousand ticks, and raising `regime_threshold` to suppress this also makes genuine
volatility spikes undetectable — there is no fixed threshold that cleanly separates the two
cases for this configuration.

What you *can* rely on: bounded z-scores and full suppression during a regime change (both
described above). What you should not rely on: treating `is_in_regime_change()` as evidence
that the market actually changed regime. Do not gate risk-off or quote-widening logic on it
without independently validating the false-positive rate for your own half-life
configuration — or prefer the plain `EwmaNormaliser` if you don't specifically need regime
detection.
