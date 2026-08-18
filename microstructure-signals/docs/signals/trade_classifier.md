# Trade Classifier

**Implementation**: [src/trade_classifier.rs](../../src/trade_classifier.rs)

## What It Does

Determines whether a trade was buyer-initiated (aggressive buy) or seller-initiated (aggressive sell) using publicly available data. Essential for VSI calculation.

Two algorithms available:
1. **Tick Rule** - Classifies based on price movement
2. **Quote Rule (Lee-Ready)** - Classifies based on position relative to quotes (DEFAULT)

## Tick Rule

Classifies based on price change:
- **Uptick** (price rose) → Buy
- **Downtick** (price fell) → Sell
- **Zero tick** (no change) → Same as previous trade

**Pros**: Simple, doesn't need book data
**Cons**: Less accurate than the quote rule — price direction alone is a weaker signal of aggressor side than price relative to the prevailing quotes

```rust
use microstructure_signals::TickRuleClassifier;

let mut classifier = TickRuleClassifier::new();
let side = classifier.classify(&trade);
```

## Quote Rule (Lee-Ready)

Classifies based on trade price relative to mid-price:
- **Trade above mid** → Buy
- **Trade below mid** → Sell
- **Trade at mid** → Falls back to tick rule

**Pros**: More accurate than the tick rule, spread-aware
**Cons**: Requires book data

```rust
use microstructure_signals::QuoteRuleClassifier;

let mut classifier = QuoteRuleClassifier::new();
let side = classifier.classify(&trade, &book);
```

## ClassifierType

`ClassifierType` is an enum in `SignalEngineConfig`:

| Variant | Algorithm | Requires book? | Relative accuracy |
|---------|-----------|---------------|---------|
| `ClassifierType::QuoteRule` | Lee-Ready (quote rule) | Yes | Higher — uses price relative to quotes |
| `ClassifierType::TickRule`  | Tick rule (price direction) | No | Lower — uses price direction only |

`QuoteRule` is the default. Use `TickRule` only when book data is unavailable.

## Configuration

```rust
use microstructure_signals::{SignalEngineConfig, ClassifierType};

let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();

// Tick Rule (simpler, no book needed)
config.classifier = ClassifierType::TickRule;

// Quote Rule (better accuracy) - DEFAULT
config.classifier = ClassifierType::QuoteRule;
```

## When to Use Each

### Use Tick Rule when:
- No order book data available
- Building baseline/comparison
- Very liquid markets with tiny spreads

### Use Quote Rule when:
- Order book data available (recommended)
- Accuracy is critical (VSI depends on this)
- Industry-standard method needed

## Example

```rust
let mut classifier = QuoteRuleClassifier::new();

let book = BookSnapshot {
    bids: vec![PriceLevel { price: 100.0, quantity: 50.0 }],
    asks: vec![PriceLevel { price: 100.2, quantity: 50.0 }],
    timestamp_ns: 0,
};
// Mid = 100.1

let trade1 = Trade { price: 100.15, quantity: 30.0, timestamp_ns: 0 };
let side1 = classifier.classify(&trade1, &book);
// 100.15 > 100.1 → Buy

let trade2 = Trade { price: 100.05, quantity: 40.0, timestamp_ns: 1000 };
let side2 = classifier.classify(&trade2, &book);
// 100.05 < 100.1 → Sell
```

## Common Issues

**Stale book data**:
```rust
// Bad: book is 1000ns stale relative to the trade
let book = get_book_at_time(t - 1000);
let trade = get_trade_at_time(t);

// Good: book and trade synchronised to the same time
let book = get_book_at_time(t);
let trade = get_trade_at_time(t);
```

**Empty book**: Quote rule needs valid bid and ask. Handle edge case or fall back to tick rule.

**First-trade bias**: `TickRuleClassifier` has no previous price to compare against on the
very first trade it sees, so it unconditionally classifies that trade as `Buy` — an
arbitrary placeholder, not an inference from data. `QuoteRuleClassifier` falls back to the
tick rule (and inherits this same default) whenever the trade price equals the book's
mid-price, which is most likely on that same first trade if no book update has been
processed yet. This biases the very first VSI bucket and the first effective-spread reading
of a session. It also recurs every time you call `SignalEngine::reset()` to switch
instruments intraday, since `reset()` constructs a fresh classifier with no prior-price
memory. If your strategy is sensitive to the first few trades after start/reset (e.g. very
short warm-up windows), account for this bias rather than trusting the earliest
classifications.

## Accuracy

This crate does not measure classification accuracy against ground truth — doing so requires
exchange-provided aggressor flags this crate has no access to. What can be said without a
measurement:
- The quote rule uses more information (price relative to the prevailing book) than the tick
  rule (price direction only), so it is expected to outperform when quotes are fresh and
  reliable.
- Neither algorithm exactly recovers the true aggressor side — both are heuristics, and
  misclassification should be expected, particularly around the sources below.

Error sources:
- Hidden/iceberg orders
- Trades executed within spread
- Stale quotes in fast markets
