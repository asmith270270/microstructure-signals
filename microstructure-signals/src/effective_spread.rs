//! Effective Spread signal. See [`EffectiveSpread`] and `docs/signals/effective_spread.md`.

use crate::types::{BookSnapshot, Trade, TradeSide};

/// Effective Spread signal: `2 * sign * (trade_price - mid_price)`.
///
/// Measures how far a trade executed from the mid-price, as a proxy for transaction costs.
/// Positive values mean the trader paid above mid (buy) or received below mid (sell).
/// Negative values indicate price improvement.
///
/// See `docs/signals/effective_spread.md` for interpretation guidance.
#[derive(Debug, Clone)]
#[must_use]
pub struct EffectiveSpread {
    last_effective_spread: Option<f64>,
    last_relative_effective_spread: Option<f64>,
}

impl EffectiveSpread {
    /// Create a new `EffectiveSpread`. Values are `None` until the first update.
    pub fn new() -> Self {
        Self {
            last_effective_spread: None,
            last_relative_effective_spread: None,
        }
    }

    /// Process a classified trade against the current book and update effective spread.
    ///
    /// Clears both spread values if the book has no valid mid-price (empty or one-sided book).
    /// [`relative_effective_spread`](Self::relative_effective_spread) is additionally cleared
    /// when `mid_price <= 0`.
    pub fn update(&mut self, trade: &Trade, book: &BookSnapshot, side: TradeSide) {
        let Some(mid) = book.mid_price() else {
            self.last_effective_spread = None;
            self.last_relative_effective_spread = None;
            return;
        };

        let sign = match side {
            TradeSide::Buy => 1.0,
            TradeSide::Sell => -1.0,
        };

        let effective = 2.0 * sign * (trade.price - mid);
        self.last_effective_spread = Some(effective);
        self.last_relative_effective_spread = if mid > 0.0 {
            Some(effective / mid)
        } else {
            None
        };
    }

    /// The effective spread from the most recent update, or `None`.
    #[inline]
    pub fn effective_spread(&self) -> Option<f64> {
        self.last_effective_spread
    }

    /// The effective spread relative to mid-price from the most recent update, or `None`.
    #[inline]
    pub fn relative_effective_spread(&self) -> Option<f64> {
        self.last_relative_effective_spread
    }
}

impl Default for EffectiveSpread {
    fn default() -> Self {
        Self::new()
    }
}
