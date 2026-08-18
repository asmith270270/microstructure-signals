# Volume-Synchronised Imbalance (VSI)

**Implementation**: [src/vsi.rs](../../src/vsi.rs)

## What It Does

Measures directional imbalance of executed trades within fixed-volume "buckets". Related to VPIN (Volume-Synchronised Probability of Informed Trading), but uses a signed formulation rather than VPIN's unsigned probability.

**Output**: [-1.0, 1.0]
- +1.0 = All volume was buying (maximum buy-side pressure)
- 0.0 = Perfectly balanced buy and sell volume
- -1.0 = All volume was selling (maximum sell-side pressure)

## How It Works

1. Accumulate trades into buckets of fixed volume
2. When bucket fills, calculate: `(buy_vol - sell_vol) / total_vol`
3. Average the signed imbalance over last N completed buckets

## Configuration

```rust
let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();

// Or via with_adv_vsi_bucket for ADV-relative sizing:
let config = SignalEngineConfig::with_adv_vsi_bucket(
    1_000_000.0,  // Average daily volume
    50,           // Number of buckets
    0.05,         // Fraction of ADV per day covered by the window
).unwrap();
```

## Parameters

### Bucket Volume
**Small (100-1000)**:
- More responsive
- Noisier signal
- Good for HFT

**Medium (1000-10000)** - recommended:
- Balanced responsiveness/stability
- Typical for most strategies

**Large (10000+)**:
- Very stable
- Slower to react
- Good for slower strategies

Rule of thumb: 0.5-2% of daily volume per bucket

### Number of Buckets
How many completed buckets to average over:
- **Small (10-20)**: Recent flow only
- **Medium (50)**: Default, balanced
- **Large (100+)**: Long-term average

## Usage

```rust
let snapshot = engine.on_trade(&trade, &book);

if !snapshot.vsi.is_nan() {
    let vsi = snapshot.vsi;
    if vsi > 0.6 {
        // Strongly buy-dominated flow
        widen_ask();
    } else if vsi < -0.6 {
        // Strongly sell-dominated flow
        widen_bid();
    }
}
```

### Spread Adjustment

```rust
fn spread_multiplier(vsi: f64) -> f64 {
    let base = 1.0;
    let stress = vsi.abs();
    base + stress * 0.5
}
```

## When to Use

**Use VSI when**:
- Detecting toxic flow for market-making risk
- Measuring directional order flow over time
- Want volume-normalised signal (not time-dependent)
- Have classified trade data (buy vs sell)

**Don't use when**:
- No trade classification available
- Market has very sparse trades
- Need ultra-low-latency (VSI updates only on bucket completion)

## Why Volume Synchronisation?

Unlike time-based sampling, volume buckets:
- **Adjust to activity**: Fast markets → faster completion
- **Normalise intensity**: Busy vs quiet periods comparable
- **Align with microstructure**: Liquidity provision in volume space
- **Reduce noise**: Filters low-activity periods

## Example

```rust
use microstructure_signals::Vsi;

let mut vsi = Vsi::new(1000.0, 50).unwrap(); // 1000 volume per bucket, 50-bucket window

for classified_trade in trades {
    vsi.update(&classified_trade);

    if let Some(vsi_value) = vsi.value() {
        println!("VSI: {:.3}", vsi_value);
    }
}
```

## Typical Values

| Range        | Interpretation                              |
|--------------|---------------------------------------------|
| [-0.2, 0.2]  | Balanced, uninformed trading                |
| [0.2, 0.5]   | Moderate buy-side imbalance                 |
| [-0.5, -0.2] | Moderate sell-side imbalance                |
| [0.5, 0.8]   | Strong directional buy flow                 |
| [-0.8, -0.5] | Strong directional sell flow                |
| > 0.8        | Extreme buy-side, potentially very toxic    |
| < -0.8       | Extreme sell-side, potentially very toxic   |

## Trade Classification Dependency

VSI requires trades classified as Buy or Sell:
- Uses `classifier` setting (TickRule or QuoteRule)
- Quote Rule recommended for better accuracy (see [trade_classifier.md](trade_classifier.md#accuracy))
- Misclassification reduces VSI accuracy

## Edge Cases

- **No buckets completed**: Returns `None` until at least 1 bucket closes
- **Partial bucket**: Not counted in VSI until it closes
- **Overflow handling**: Trades split across bucket boundaries automatically
- **Capped trades**: A single trade that spans more than 100 bucket boundaries is capped at 100 bucket closures; the remaining volume is absorbed into the current partial bucket without closing it further. This prevents one enormous trade from clearing the entire rolling window. `SignalSnapshot.vsi_capped_trades` counts how many such trades have occurred since the engine was created or last reset. A non-zero count means VSI may underweight very large trades relative to their true directional impact — recalibrate `bucket_volume` upward if this occurs frequently.
