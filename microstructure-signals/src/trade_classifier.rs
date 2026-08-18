//! Trade side classifiers (tick rule and quote rule / Lee-Ready).
//! See [`TickRuleClassifier`], [`QuoteRuleClassifier`], and `docs/signals/trade_classifier.md`.

use crate::types::{BookSnapshot, Trade, TradeSide};

/// Tick-rule trade classifier.
///
/// Classifies a trade as a buy if the price ticked up from the previous trade,
/// a sell if it ticked down, or repeats the previous classification on an unchanged price.
/// Accuracy is approximately 65–75%. Does not require book data.
///
/// See `docs/signals/trade_classifier.md` for accuracy benchmarks and trade-offs.
#[derive(Debug, Clone)]
#[must_use]
pub struct TickRuleClassifier {
    last_price: Option<f64>,
    last_side: TradeSide,
}

impl TickRuleClassifier {
    /// Create a new `TickRuleClassifier`. The first trade is always classified as `Buy`.
    pub fn new() -> Self {
        Self {
            last_price: None,
            last_side: TradeSide::Buy,
        }
    }

    /// Classify a trade and update internal state. Returns the inferred [`TradeSide`].
    pub fn classify(&mut self, trade: &Trade) -> TradeSide {
        let side = match self.last_price {
            None => TradeSide::Buy,
            Some(prev_price) => {
                if trade.price > prev_price {
                    TradeSide::Buy
                } else if trade.price < prev_price {
                    TradeSide::Sell
                } else {
                    self.last_side
                }
            }
        };

        self.last_price = Some(trade.price);
        self.last_side = side;
        side
    }

    #[inline]
    pub(crate) fn set_state(&mut self, price: f64, side: TradeSide) {
        self.last_price = Some(price);
        self.last_side = side;
    }
}

impl Default for TickRuleClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Quote-rule (Lee-Ready) trade classifier.
///
/// Classifies a trade by its position relative to the mid-price: above mid → Buy,
/// below mid → Sell, at mid → falls back to tick rule. Accuracy is approximately
/// 75–85%. Requires a current [`BookSnapshot`].
///
/// See `docs/signals/trade_classifier.md` for accuracy benchmarks and trade-offs.
#[derive(Debug, Clone)]
#[must_use]
pub struct QuoteRuleClassifier {
    tick_rule: TickRuleClassifier,
}

impl QuoteRuleClassifier {
    /// Create a new `QuoteRuleClassifier`.
    pub fn new() -> Self {
        Self {
            tick_rule: TickRuleClassifier::new(),
        }
    }

    /// Classify a trade using the quote rule, falling back to tick rule at mid. Returns the inferred [`TradeSide`].
    pub fn classify(&mut self, trade: &Trade, book: &BookSnapshot) -> TradeSide {
        if let Some(mid) = book.mid_price() {
            if trade.price > mid {
                self.tick_rule.set_state(trade.price, TradeSide::Buy);
                return TradeSide::Buy;
            } else if trade.price < mid {
                self.tick_rule.set_state(trade.price, TradeSide::Sell);
                return TradeSide::Sell;
            }
        }

        self.tick_rule.classify(trade)
    }
}

impl Default for QuoteRuleClassifier {
    fn default() -> Self {
        Self::new()
    }
}
