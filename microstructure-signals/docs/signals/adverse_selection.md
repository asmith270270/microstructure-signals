# Adverse Selection Signal

**Implementation**: [src/adverse_selection.rs](../../src/adverse_selection.rs)

## What It Does

Combines normalised signals (z-scores) into a single weighted score that characterises the current adverse selection environment. This is a **contemporaneous** measure — it reflects the current state of the market, not a prediction of future price movement.

**Output**: Unbounded f64
- Positive = current environment skewed toward informed buying
- Negative = current environment skewed toward informed selling
- Near zero = no clear directional pressure

## Difference from Toxicity

Both `adverse_selection` and `toxicity` are weighted z-score sums computed from the same four inputs. The distinction is their weight vectors:

| Aspect            | `toxicity`                                  | `adverse_selection`                             |
|-------------------|---------------------------------------------|-------------------------------------------------|
| **Weights field** | `toxicity_weights`                          | `adverse_selection_weights`                     |
| **Typical use**   | Risk level — widen spreads, reduce size     | Directional intensity — skew quotes, lean inventory |
| **Default**       | `[1.0, 1.0, 1.0, 1.0]`                     | `[1.0, 1.0, 1.0, 1.0]`                         |

Both are contemporaneous. Neither is inherently predictive without calibration on historical data.

## Input Signals

Weight order: `[ofi_z, vsi_z, depth_imbalance_z, microprice_deviation_z]`

## Configuration

```rust
use microstructure_signals::CompositeWeights;

let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
config.adverse_selection_weights = CompositeWeights {
    ofi: 1.5,
    vsi: 1.0,
    depth_imbalance: 0.5,
    microprice_deviation: 0.3,
};
config.signals.adverse_selection = true;
```

Weights are normalised by their absolute sum before combining, so scale does not matter — only relative magnitudes. Negative weights are valid and invert the contribution of that signal.

## Usage

```rust
let snapshot = engine.on_book_update(&book);

if !snapshot.adverse_selection.is_nan() {
    let adv = snapshot.adverse_selection;
    if adv > 2.0 {
        // Strong buy-side adverse selection
        // Consider widening ask or leaning long
    } else if adv < -2.0 {
        // Strong sell-side adverse selection
        // Consider widening bid or leaning short
    }
}
```

### Quote Skewing

```rust
fn compute_quotes(mid: f64, adv: f64, base_spread: f64) -> (f64, f64) {
    let skew = adv * 0.01;
    let bid = mid - base_spread / 2.0 - skew;
    let ask = mid + base_spread / 2.0 - skew;
    (bid, ask)
}
```

## When to Use

**Use when**:
- Characterising the current directional pressure of flow
- Skewing quotes based on observed signal imbalance
- Adjusting inventory targets

**Don't use when**:
- Only care about risk level, not direction (use `toxicity` instead)
- Weights have not been calibrated for the target instrument

## Calibration

Default `CompositeWeights::default()` treats all four signals equally. To calibrate:

1. **Regression**: Regress short-horizon price changes on the four z-scores; use the coefficients as weights
2. **Ridge/Lasso**: Regularise to prevent overfitting
3. **Cross-validate**: Use multiple time periods to avoid look-ahead

Example calibrated:
```rust
CompositeWeights { ofi: 1.5, vsi: 1.0, depth_imbalance: -0.3, microprice_deviation: 0.5 }
// depth_imbalance inverted if your market exhibits mean-reversion in book state
```

## Limitations

- Weights must be re-calibrated per instrument and regime
- The signal reflects the present, not a forecast; any predictive power requires empirical validation
- May become unreliable during regime transitions (consider `regime_normaliser_params`)
