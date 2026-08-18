# Order Flow Imbalance (OFI)

**Implementation**: [src/ofi.rs](../../src/ofi.rs)

## What It Does

Measures net change in liquidity at **top of book** (best bid and ask) between consecutive snapshots. Quantifies directional pressure from limit order arrivals, cancellations, and executions.

**Formula**: `Δbid - Δask`

**Output**: Unbounded f64
- Positive = Net buying pressure
- Negative = Net selling pressure
- Magnitude = Aggressiveness of flow

## Configuration

```rust
let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
config.ofi_window = 100; // Rolling window size
config.signals.ofi = true;

// Optional: EWMA-smooth the raw OFI value itself before it reaches the normaliser
config.ofi_smoothing_half_life = Some(20.0);
```

## How It Calculates Deltas

For each side (bid or ask):
- **Price improved**: Delta = new quantity
- **Price deteriorated**: Delta = -old quantity
- **Price unchanged**: Delta = new quantity - old quantity

OFI = Δbid - Δask (rolling sum over window)

## Usage

```rust
let snapshot = engine.on_book_update(&book);

if !snapshot.ofi.is_nan() {
    let ofi = snapshot.ofi;
    if ofi > 100.0 {
        // Strong buying pressure - price likely to tick up
        widen_ask();
    } else if ofi < -100.0 {
        // Strong selling pressure - price likely to tick down
        widen_bid();
    }
}
```

## Interpretation

| OFI Value | Interpretation | Typical Market Response |
|-----------|----------------|------------------------|
| >> 0 | Strong buying pressure, aggressive bids | Price likely to tick up |
| > 0 | Moderate buying interest | Slight upward bias |
| ≈ 0 | Balanced flow | No directional signal |
| < 0 | Moderate selling pressure | Slight downward bias |
| << 0 | Strong selling pressure, aggressive asks | Price likely to tick down |

## When to Use

**Use OFI when**:
- Need leading indicator of short-term price movement
- Measuring toxic flow for market-making risk
- Want to detect information arrival via limit orders
- Building predictive signal for microsecond-level trading

**Don't use when**:
- Need longer-term trend prediction (OFI is ultra-short-term)
- Market has wide spreads or thin liquidity (noisy)
- Only have trade data without full book depth
- Care about liquidity beyond best bid/offer

## Window Size Parameter

- **Small (10-50 events)**: More responsive, noisier, good for HFT
- **Medium (100-200 events)**: Balanced signal, typical for market-making
- **Large (500+ events)**: Smoother but lagging, less actionable

## Example

```rust
let book1 = BookSnapshot {
    bids: vec![PriceLevel { price: 100.0, quantity: 50.0 }],
    asks: vec![PriceLevel { price: 100.1, quantity: 50.0 }],
    timestamp_ns: 0,
};

let book2 = BookSnapshot {
    bids: vec![PriceLevel { price: 100.0, quantity: 75.0 }], // +25 bid qty
    asks: vec![PriceLevel { price: 100.1, quantity: 50.0 }],
    timestamp_ns: 1000,
};

let snapshot1 = engine.on_book_update(&book1);
let snapshot2 = engine.on_book_update(&book2);

// OFI = Δbid - Δask = 25 - 0 = 25.0
```

## What OFI Measures

- Limit order activity (aggressive market makers joining)
- Order cancellations (passive liquidity withdrawing)
- Market order impact (consumption of liquidity)

## What OFI Does NOT Measure

- Liquidity deeper in the book (only top-of-book)
- Trade execution directly (measures book changes)
- Causation of price movements (only correlation)

## Empirical Properties

- **Leading indicator**: reacts to book activity (orders, cancellations) rather than executed trades, so it tends to move ahead of the price changes it anticipates
- **Mean reversion**: large OFI readings tend to partially revert once the pressure they reflect has been absorbed by the market
- **Correlation with trades**: positively associated with the direction of subsequent trades, though imperfectly — order flow does not always convert into executions
- **Persistence**: autocorrelation decays over a short window rather than persisting indefinitely

These are qualitative tendencies described in the market microstructure literature (e.g. Cont,
Kukanov & Stoikov), not measurements taken from this codebase — validate against your own
instrument and feed before relying on them.

## Edge Cases

- **Empty book**: Returns `None` until two snapshots available
- **First snapshot**: No delta calculated, returns `None`
- **Crossed book**: Assumes valid books (bid < ask)
