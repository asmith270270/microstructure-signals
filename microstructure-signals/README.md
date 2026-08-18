# microstructure-signals

An engine that quantifies **order flow toxicity** and **adverse selection risk** from
order book and trade data, i.e. the risk that whoever you're trading against knows something you
don't.

## The Problem

Every fill carries two possible explanations: the counterparty needed liquidity for reasons
unrelated to information (noise trading), or the counterparty has better information about where
the price is headed (informed trading). A market maker who cannot tell these apart prices every
trade the same way and loses systematically to the informed side. This is **adverse selection**,
and the resulting pattern of losses is order flow
**toxicity**.

None of the individual signals below can confirm uninformed flow on their own. Each is a different lens on the same underlying question (is the current order flow more likely to be information-driven than usual), built from a different slice of the data: the resting book, executed trades, or the price path. Combined and compared against their own recent history, they give a continuously updated read on how dangerous it currently is to provide liquidity.

## Signals

### Order Flow Imbalance (OFI)

`Δbid_qty − Δask_qty` at the best bid/ask between consecutive book snapshots.

OFI reacts to *resting* liquidity: new limit orders, cancellations, and the liquidity consumed
by executions, not just trades that have already happened. That makes it a leading rather than
a confirming indicator: informed participants tend to reveal themselves in how the book is being
built and torn down before the trade tape catches up. Positive OFI is net buying pressure; negative is net selling pressure.

### Depth Imbalance

`(bid_volume − ask_volume) / (bid_volume + ask_volume)` across the top N book levels, in [-1, 1].

Where OFI measures the *rate of change* of top-of-book liquidity, depth imbalance measures the
*standing state* of the book several levels deep. A book that is persistently heavy on one side
reflects a standing belief among liquidity providers about where the risk lies, independent of
any single event.

### Microprice

`(bid_price × ask_qty + ask_price × bid_qty) / (bid_qty + ask_qty)`, a fair-price estimate
between the best bid and ask.

The simple mid-price treats both sides as equally likely to move. Microprice instead weights each
side's price by the *opposite* side's quantity: a thin ask means little volume is needed to
consume it and push price up, so the fair-price estimate should already lean toward the ask
before that happens (Stoikov). It gives sub-tick price resolution and reacts to imbalance before
a trade occurs.

### Volume-Synchronised Imbalance (VSI)

Trades are classified as buyer- or seller-initiated and accumulated into
fixed-*volume* buckets rather than fixed-time bars. Each bucket's signed imbalance,
`(buy_vol − sell_vol) / total_vol`, is averaged over the last N buckets, giving a value in
[-1, 1].

This is a signed variant of VPIN (Volume-Synchronised Probability of Informed Trading). Volume
buckets exist because clock time is a poor sampling unit for order flow: a bar at the open and a
bar at lunch can contain wildly different amounts of information, while a volume bucket always
represents a comparable amount of trading activity regardless of how long it took to fill. VPIN's
original formulation, `|buy − sell| / total`, discards direction to produce an unsigned "how
one-sided was this bucket" probability; VSI keeps the sign, because a market maker needs to know
*which way* to skew, not just that skewing is warranted.

### Trade Classification

Executed trades carry a price and size but not, in most feeds, an explicit side. VSI and
effective spread need to know whether each trade was buyer- or seller-initiated. Two algorithms are 
provided:

- **Tick Rule**: an uptick is a buy, a downtick is a sell, a zero-tick inherits the previous
  trade's side.
- **Quote Rule (Lee & Ready)**: a trade above the prevailing mid is a buy, below is a sell, at the
  mid falls back to the tick rule. More accurate when quotes are available, and the default.

### Effective Spread

`2 × sign × (trade_price − mid_price)` at the moment of execution.

The quoted spread is a nominal cost that no one necessarily pays. Effective spread measures what
a trade *actually* cost relative to the mid-price prevailing at execution, positive when the
trade paid through the spread and negative when it was filled favourably (e.g. a passive rebate
fill). It is the realised counterpart to the quoted spread.

### Normalisation (Z-Scores)

Raw signal magnitudes are not comparable across instruments or across time: OFI on an
index future and OFI on a thinly-traded small-cap live on entirely different scales, and the same
instrument's own scale shifts between a calm morning and a volatile close. Each raw signal is
converted to a z-score, `(value − EWMA mean) / EWMA std_dev`, using an exponentially-weighted
moving average rather than a simple average so that recent observations are weighted more heavily
and the estimate adapts as the market's baseline drifts. A |z| beyond 2 is a statistically unusual
reading for that signal, right now, on this instrument, not just a large raw number.

A companion regime-change detector watches for the EWMA statistics themselves spiking abruptly
(e.g. a volatility shock) and briefly suspends normalisation rather than letting one outlier drag
the running mean and variance. The alternative, a longer half-life, only trades getting dragged
by outliers for being slower to adapt to genuinely new conditions.

### Composite Toxicity & Adverse Selection

A weighted sum of the normalised (z-scored) signals above into a single number.

The two composites use the same underlying combination but different default weight vectors,
because they answer different questions from the same inputs: **toxicity** characterizes risk to
a passive liquidity provider (should quotes widen or pull back), while **adverse selection**
characterizes the directional skew of informed flow (is the informed money currently buying or
selling). Positive is buy-side pressure, negative is sell-side, magnitude reflects how many
standard deviations away from normal the aggregate picture currently sits.

## What This Does Not Do

This is a signal-computation library, not a trading system. It does not reconstruct an order book
from incremental feed updates, execute or manage orders, simulate a matching engine, backtest a
strategy, or forecast future prices. It produces contemporaneous measurements of the *current*
order flow environment for something else to act on.

## Getting Started

```rust
use microstructure_signals::{SignalEngine, SignalEngineConfig};

let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
let mut engine = SignalEngine::new(config).unwrap();

let snapshot = engine.on_book_update(&book);
println!("toxicity: {}", snapshot.toxicity);
```

For streaming vs. batch processing, signal selection, `no_std`/embedded deployment, and Cargo
feature flags, see **[docs/USAGE.md](docs/USAGE.md)**.

## Documentation

- **[docs/signals/](docs/signals/)**: formulas, parameters, worked examples, and empirical
  properties for every signal above
- **[docs/USAGE.md](docs/USAGE.md)**: API usage, configuration, and deployment modes
- **[docs/BENCHMARKS.md](docs/BENCHMARKS.md)**: measured throughput and latency
