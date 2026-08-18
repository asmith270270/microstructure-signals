//! Microprice and microprice deviation signal. See [`MicropriceCalculator`] and `docs/signals/microprice.md`.

use crate::types::BookSnapshot;

/// Calculates the Microprice and its deviation from mid.
///
/// The Microprice is a quantity-weighted mid-price:
/// `(bid_price * ask_qty + ask_price * bid_qty) / (bid_qty + ask_qty)`.
/// The deviation is `(microprice - mid) / half_spread`, normalised to `[-1, 1]`
/// when both sides have positive quantity.
///
/// See `docs/signals/microprice.md` for interpretation and calibration notes.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct MicropriceCalculator {
    last_mid: Option<f64>,
    last_microprice: Option<f64>,
    last_deviation: Option<f64>,
}

impl MicropriceCalculator {
    /// Create a new `MicropriceCalculator`. All values are `None` until the first update.
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a new book snapshot and update microprice and deviation.
    ///
    /// Does nothing if the book has no best bid, no best ask, or total quantity is zero.
    #[inline]
    pub fn update(&mut self, book: &BookSnapshot) {
        let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) else {
            return;
        };

        let mid = (bid.price + ask.price) / 2.0;
        let total_qty = bid.quantity + ask.quantity;

        if total_qty <= 0.0 {
            return;
        }

        let microprice = (bid.price * ask.quantity + ask.price * bid.quantity) / total_qty;

        let half_spread = (ask.price - bid.price) / 2.0;
        let deviation = if half_spread > 0.0 {
            (microprice - mid) / half_spread
        } else {
            0.0
        };

        self.last_mid = Some(mid);
        self.last_microprice = Some(microprice);
        self.last_deviation = Some(deviation);
    }

    /// The simple mid-price `(bid + ask) / 2` from the most recent update, or `None`.
    #[inline]
    pub fn mid_price(&self) -> Option<f64> {
        self.last_mid
    }

    /// The quantity-weighted microprice from the most recent update, or `None`.
    #[inline]
    pub fn microprice(&self) -> Option<f64> {
        self.last_microprice
    }

    /// The microprice deviation from the most recent update, or `None`.
    ///
    /// Computed as `(microprice - mid) / half_spread`. Range `[-1, 1]` when
    /// both sides have positive quantity and a positive spread.
    #[inline]
    pub fn deviation(&self) -> Option<f64> {
        self.last_deviation
    }
}
