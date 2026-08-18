# Signal Documentation Index

Practical reference documentation for each signal in the microstructure-signals crate.

## Core Signals

### [Order Flow Imbalance (OFI)](ofi.md)
**Type**: Book-based flow signal
**Update**: Every book snapshot

Measures net liquidity change at top-of-book. Primary indicator of short-term price pressure.

**File**: [src/ofi.rs](../../src/ofi.rs)

---

### [Depth Imbalance](depth_imbalance.md)
**Type**: Book-based state signal
**Update**: Every book snapshot

Measures relative liquidity across multiple price levels. Slower-moving alternative to OFI.

**File**: [src/depth_imbalance.rs](../../src/depth_imbalance.rs)

---

### [Microprice](microprice.md)
**Type**: Price estimate signal
**Update**: Every book snapshot

Volume-weighted fair mid-price accounting for liquidity imbalance. Provides sub-tick price resolution.

**File**: [src/microprice.rs](../../src/microprice.rs)

---

### [Volume-Synchronised Imbalance (VSI)](vsi.md)
**Type**: Trade-based flow signal
**Update**: Bucket completion

Measures directional trade imbalance within fixed-volume buckets. Related to VPIN toxicity measure.

**File**: [src/vsi.rs](../../src/vsi.rs)

---

## Signal Processing

### [EWMA Z-Score Normaliser](normaliser.md)
**Type**: Normalisation utility
**Update**: Per observation

Converts raw signals to z-scores using exponentially weighted moving average statistics.

**File**: [src/normaliser.rs](../../src/normaliser.rs)

---

### [Trade Classifier](trade_classifier.md)
**Type**: Trade direction classification
**Update**: Per trade

Classifies trades as buyer-initiated or seller-initiated using tick rule or Lee-Ready algorithm.

**File**: [src/trade_classifier.rs](../../src/trade_classifier.rs)

---

## Composite Signals

### [Composite Toxicity](composite.md)
**Type**: Aggregate risk signal
**Update**: When component signals update

Combines normalised signals into single toxicity measure for market-making risk management.

**File**: [src/composite.rs](../../src/composite.rs)

---

### [Adverse Selection](adverse_selection.md)
**Type**: Contemporaneous directional signal
**Update**: When component signals update

Characterises whether current flow is skewed toward informed buying or selling. Same inputs as
toxicity, different weights — not a price forecast.

**File**: [src/adverse_selection.rs](../../src/adverse_selection.rs)

---

## Signal Relationships

```
Raw Book Data → OFI ────────┐
             → Depth Imb ───┤
             → Microprice   │
                            ├→ EWMA Normaliser → Z-Scores ┬→ Composite Toxicity
Trades → Classifier → VSI ──┘                              └→ Adverse Selection
```

## Quick Reference

| Signal | Primary Use | Range | Interpretation |
|--------|------------|-------|----------------|
| OFI | Flow pressure | Unbounded | +: buying, -: selling |
| Depth Imb | Liquidity skew | [-1, 1] | +: heavy bids, -: heavy asks |
| Microprice | Fair value | [bid, ask] | Deviation predicts direction |
| VSI | Trade toxicity | [-1, 1] | +: buy-dominant, -: sell-dominant |
| Z-Scores | Normalised signals | ~[-3, 3] | \|z\| > 2 is unusual |
| Toxicity | Risk level | Unbounded | Higher = more toxic |
| Adverse Sel | Directional skew (contemporaneous) | Unbounded | +: informed buying, -: informed selling |
