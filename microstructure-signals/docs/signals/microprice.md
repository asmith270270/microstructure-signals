# Microprice

**Implementation**: [src/microprice.rs](../../src/microprice.rs)

## What It Does

Volume-weighted estimate of the "fair" mid-price that accounts for liquidity imbalance at top of book. Provides more refined price estimate than simple arithmetic mid-price.

**Formula**: `(bid_price * ask_qty + ask_price * bid_qty) / (bid_qty + ask_qty)`

**Output**: Price value between bid and ask
- Closer to ask when ask is thin (upward bias)
- Closer to bid when bid is thin (downward bias)
- Equals mid when quantities balanced

## Why Opposite-Side Weighting?

Weights each price by the **opposite** side's quantity because:
- Thin liquidity is fragile and likely to be consumed
- Fair value pulls toward the thin side
- Next trade more likely at price with less liquidity

## Configuration

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
config.signals.microprice = true;
config.signals.microprice_deviation_z = true;
```

## Usage

```rust
let snapshot = engine.on_book_update(&book);

if !snapshot.microprice.is_nan() {
    let mid = snapshot.mid_price;

    if snapshot.microprice > mid {
        // Upward bias - thin ask, likely to trade up
    } else if snapshot.microprice < mid {
        // Downward bias - thin bid, likely to trade down
    }
}
```

### Three Outputs Available

1. **`mid_price`**: Simple arithmetic mid `(bid + ask) / 2`
2. **`microprice`**: Volume-weighted microprice
3. **`microprice_deviation`**: `(microprice - mid_price) / half_spread` — normalised to `[-1, 1]`

## When to Use

**Use microprice when**:
- Need better mid-price estimate for valuation
- Pricing derivatives or mark-to-market
- Predicting next trade price direction
- Building market-making model with queue position
- Need sub-tick price resolution

**Don't use when**:
- Simple mid-price sufficient (balanced markets)
- Need long-term pricing (microprice is short-term)
- Spread is very wide (unstable)
- Only care about direction, not magnitude (use depth imbalance)

## Example

```rust
let book = BookSnapshot {
    bids: vec![PriceLevel { price: 100.00, quantity: 500.0 }], // Thick
    asks: vec![PriceLevel { price: 100.10, quantity: 100.0 }], // Thin
    timestamp_ns: 0,
};

let snapshot = engine.on_book_update(&book);

// mid = (100.00 + 100.10) / 2 = 100.05
// microprice = (100.00 * 100 + 100.10 * 500) / 600 = 100.083
// deviation = 100.083 - 100.05 = +0.033 (upward bias)
```

## Interpretation

| Deviation | Qty Imbalance | Interpretation |
|-----------|---------------|----------------|
| >> 0 | Very thin ask | Strong upward pressure, buy very likely |
| > 0 | Thin ask | Mild upward pressure, buy more likely |
| ≈ 0 | Balanced | No bias, neutral |
| < 0 | Thin bid | Mild downward pressure, sell more likely |
| << 0 | Very thin bid | Strong downward pressure, sell very likely |

## Empirical Properties

- **Directional tendency**: deviation from the simple mid tends to be associated with the direction of the next trade, though not deterministically
- **Mean reversion**: deviation tends to revert toward zero as the imbalance driving it is resolved
- **Magnitude**: bounded by construction to within the half-spread, and typically well inside it
- **Relationship to depth imbalance**: both are liquidity-imbalance measures and tend to move together, though microprice deviation only sees the top of book while depth imbalance sees multiple levels

These are qualitative tendencies described in the literature (Stoikov), not measurements taken
from this codebase — validate against your own instrument and feed before relying on them.

## Edge Cases

- **Zero quantity on both sides**: Values stay at their last computed state (no update)
- **Empty book (no best bid or ask)**: Values stay at their last computed state (no update)
- **Equal quantities**: Microprice equals mid-price (deviation = 0)
