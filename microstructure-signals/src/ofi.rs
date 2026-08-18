//! Order Flow Imbalance (OFI) and Multi-Level OFI signals.
//! See [`Ofi`], [`MultiLevelOfi`], and `docs/signals/ofi.md`.

use crate::config_error::ConfigError;
use crate::ewma::EwmaSmoothing;
use crate::ring_buffer::RingBuffer;
use crate::types::{BookSnapshot, PriceLevel};

#[cfg(not(feature = "alloc"))]
use crate::types::MAX_BOOK_DEPTH;

#[derive(Debug, Clone, Copy)]
struct PrevBbo {
    bid_price: f64,
    bid_qty: f64,
    ask_price: f64,
    ask_qty: f64,
}

/// Order Flow Imbalance (OFI) signal.
///
/// Measures the net pressure on the best bid/ask over a rolling window of book updates.
/// Positive values indicate buy-side pressure; negative indicates sell-side pressure.
///
/// # Price comparison and floating-point noise
///
/// OFI classifies each book update by comparing the new best bid/ask price to the previous
/// one using exact `f64` equality. On most exchange feeds prices are exact multiples of a
/// fixed tick size, so this works correctly. However, on feeds that reconstruct prices from
/// floating-point arithmetic (e.g., some crypto venues), a "stable" price level may arrive
/// with a tiny floating-point noise component (`100.0` vs `99.99999999999999`). In that case
/// OFI will misclassify the event as a price move rather than a quantity change, adding a
/// spurious `±quantity` impulse to the rolling window.
///
/// If you see unexpectedly large OFI spikes on a feed with sub-tick price noise, pre-round
/// prices to the instrument's tick size before constructing [`BookSnapshot`].
///
/// See `docs/signals/ofi.md` for the full formula and calibration guidance.
#[derive(Debug, Clone)]
#[must_use]
pub struct Ofi {
    buffer: RingBuffer,
    prev_bbo: Option<PrevBbo>,
    ewma: Option<EwmaSmoothing>,
    last_value: Option<f64>,
    normalise_by_liquidity: bool,
}

impl Ofi {
    /// Create an OFI calculator with a rolling window of `window_size` book updates.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::ZeroSizeParameter)` if `window_size` is zero.
    pub fn new(window_size: usize) -> Result<Self, ConfigError> {
        Self::new_inner(window_size, None, false)
    }

    /// Create an OFI calculator with an additional EWMA smoothing stage.
    ///
    /// The rolling-window sum is smoothed with an EWMA of the given `half_life` (in updates).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `window_size` is zero or `half_life` is not finite and positive.
    pub fn with_smoothing(window_size: usize, half_life: f64) -> Result<Self, ConfigError> {
        Self::new_inner(window_size, Some(half_life), false)
    }

    /// Create an OFI calculator that divides each event by total top-of-book liquidity.
    ///
    /// Normalising by liquidity makes OFI comparable across instruments and time periods
    /// with different typical order sizes.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::ZeroSizeParameter)` if `window_size` is zero.
    pub fn normalised(window_size: usize) -> Result<Self, ConfigError> {
        Self::new_inner(window_size, None, true)
    }

    /// Create an OFI calculator that normalises by liquidity and applies EWMA smoothing.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `window_size` is zero or `half_life` is not finite and positive.
    pub fn normalised_with_smoothing(
        window_size: usize,
        half_life: f64,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(window_size, Some(half_life), true)
    }

    fn new_inner(
        window_size: usize,
        half_life: Option<f64>,
        normalise_by_liquidity: bool,
    ) -> Result<Self, ConfigError> {
        if window_size == 0 {
            return Err(ConfigError::ZeroSizeParameter("window_size"));
        }
        if let Some(hl) = half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        Ok(Self {
            buffer: RingBuffer::new(window_size),
            prev_bbo: None,
            ewma: half_life.map(EwmaSmoothing::new),
            last_value: None,
            normalise_by_liquidity,
        })
    }

    /// Process a new book snapshot and update the OFI value.
    ///
    /// Does nothing if the book has no best bid or best ask.
    /// Returns `None` from [`value`](Self::value) until at least two updates have been seen.
    #[inline]
    pub fn update(&mut self, book: &BookSnapshot) {
        let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) else {
            return;
        };

        if let Some(prev) = self.prev_bbo {
            let delta_bid = if bid.price > prev.bid_price {
                bid.quantity
            } else if bid.price == prev.bid_price {
                bid.quantity - prev.bid_qty
            } else {
                -prev.bid_qty
            };

            let delta_ask = if ask.price < prev.ask_price {
                ask.quantity
            } else if ask.price == prev.ask_price {
                ask.quantity - prev.ask_qty
            } else {
                -prev.ask_qty
            };

            let mut ofi_event = delta_bid - delta_ask;

            if self.normalise_by_liquidity {
                let total_touch = bid.quantity + ask.quantity;
                if total_touch > 0.0 {
                    ofi_event /= total_touch;
                }
            }

            self.buffer.push(ofi_event);

            let raw = self.buffer.sum();
            self.last_value = Some(if let Some(ewma) = &mut self.ewma {
                ewma.update(raw)
            } else {
                raw
            });
        }

        self.prev_bbo = Some(PrevBbo {
            bid_price: bid.price,
            bid_qty: bid.quantity,
            ask_price: ask.price,
            ask_qty: ask.quantity,
        });
    }

    /// The current OFI value, or `None` if fewer than two book updates have been processed.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// Returns `true` if at least two book updates have been processed.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    /// The rolling window size (number of book events).
    #[inline]
    pub fn window_size(&self) -> usize {
        self.buffer.capacity()
    }

    /// The EWMA smoothing half-life in book updates, or `None` if smoothing is disabled.
    #[inline]
    pub fn half_life(&self) -> Option<f64> {
        self.ewma.as_ref().map(|e| e.half_life())
    }

    /// Returns `true` if each OFI event is divided by total top-of-book liquidity.
    #[inline]
    pub fn is_normalised(&self) -> bool {
        self.normalise_by_liquidity
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PrevLevel {
    bid_price: f64,
    bid_qty: f64,
    ask_price: f64,
    ask_qty: f64,
}

impl PrevLevel {
    fn from_levels(bid: &PriceLevel, ask: &PriceLevel) -> Self {
        Self {
            bid_price: bid.price,
            bid_qty: bid.quantity,
            ask_price: ask.price,
            ask_qty: ask.quantity,
        }
    }
}

/// Multi-level Order Flow Imbalance with exponential level weighting.
///
/// Extends [`Ofi`] by incorporating pressure signals from levels beyond the best bid/ask,
/// with each deeper level weighted by `exp(-decay_per_level * k)`.
/// `decay_per_level = 0.0` gives equal weight to all levels; larger values concentrate
/// weight near the top of book.
///
/// See `docs/signals/ofi.md` for the full formula and calibration guidance.
#[derive(Debug, Clone)]
#[must_use]
pub struct MultiLevelOfi {
    buffer: RingBuffer,
    #[cfg(feature = "alloc")]
    prev_levels: alloc::vec::Vec<PrevLevel>,
    #[cfg(not(feature = "alloc"))]
    prev_levels: [PrevLevel; MAX_BOOK_DEPTH],
    #[cfg(not(feature = "alloc"))]
    prev_levels_len: usize,
    decay_per_level: f64,
    ewma: Option<EwmaSmoothing>,
    last_value: Option<f64>,
}

impl MultiLevelOfi {
    /// Create a multi-level OFI calculator.
    ///
    /// `decay_per_level` is the exponential decay coefficient applied per book level.
    /// Use `0.0` for equal weighting across all levels.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::ZeroSizeParameter)` if `window_size` is zero, or
    /// `Err(ConfigError::DecayInvalid)` if `decay_per_level` is negative or non-finite.
    pub fn new(window_size: usize, decay_per_level: f64) -> Result<Self, ConfigError> {
        Self::new_inner(window_size, decay_per_level, None)
    }

    /// Create a multi-level OFI calculator with EWMA smoothing on the rolling-window sum.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `window_size` is zero, `decay_per_level` is negative or non-finite,
    /// or `half_life` is not finite and positive.
    pub fn with_smoothing(
        window_size: usize,
        decay_per_level: f64,
        half_life: f64,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(window_size, decay_per_level, Some(half_life))
    }

    fn new_inner(
        window_size: usize,
        decay_per_level: f64,
        half_life: Option<f64>,
    ) -> Result<Self, ConfigError> {
        if window_size == 0 {
            return Err(ConfigError::ZeroSizeParameter("window_size"));
        }
        if !(decay_per_level >= 0.0 && decay_per_level.is_finite()) {
            return Err(ConfigError::DecayInvalid(decay_per_level));
        }
        if let Some(hl) = half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        Ok(Self {
            buffer: RingBuffer::new(window_size),
            #[cfg(feature = "alloc")]
            prev_levels: alloc::vec::Vec::new(),
            #[cfg(not(feature = "alloc"))]
            prev_levels: [PrevLevel::default(); MAX_BOOK_DEPTH],
            #[cfg(not(feature = "alloc"))]
            prev_levels_len: 0,
            decay_per_level,
            ewma: half_life.map(EwmaSmoothing::new),
            last_value: None,
        })
    }

    /// Process a new book snapshot and update the multi-level OFI value.
    ///
    /// Does nothing if the book has no bids or asks.
    /// Returns `None` from [`value`](Self::value) until at least two updates have been seen.
    #[inline]
    pub fn update(&mut self, book: &BookSnapshot) {
        let bids = book.bids();
        let asks = book.asks();

        if bids.is_empty() || asks.is_empty() {
            return;
        }

        let has_prev = self.has_prev_state();

        if has_prev {
            let n_levels = bids.len().min(asks.len()).min(self.prev_len());
            let mut weighted_ofi = 0.0_f64;

            for k in 0..n_levels {
                let weight = crate::math::exp(-self.decay_per_level * k as f64);
                let prev = self.get_prev(k);

                let delta_bid = if bids[k].price > prev.bid_price {
                    bids[k].quantity
                } else if bids[k].price == prev.bid_price {
                    bids[k].quantity - prev.bid_qty
                } else {
                    -prev.bid_qty
                };

                let delta_ask = if asks[k].price < prev.ask_price {
                    asks[k].quantity
                } else if asks[k].price == prev.ask_price {
                    asks[k].quantity - prev.ask_qty
                } else {
                    -prev.ask_qty
                };

                weighted_ofi += weight * (delta_bid - delta_ask);
            }

            self.buffer.push(weighted_ofi);

            let raw = self.buffer.sum();
            self.last_value = Some(if let Some(ewma) = &mut self.ewma {
                ewma.update(raw)
            } else {
                raw
            });
        }

        self.store_prev(bids, asks);
    }

    #[cfg(feature = "alloc")]
    #[inline]
    fn has_prev_state(&self) -> bool {
        !self.prev_levels.is_empty()
    }

    #[cfg(not(feature = "alloc"))]
    #[inline]
    fn has_prev_state(&self) -> bool {
        self.prev_levels_len > 0
    }

    #[cfg(feature = "alloc")]
    #[inline]
    fn prev_len(&self) -> usize {
        self.prev_levels.len()
    }

    #[cfg(not(feature = "alloc"))]
    #[inline]
    fn prev_len(&self) -> usize {
        self.prev_levels_len
    }

    #[inline]
    fn get_prev(&self, k: usize) -> PrevLevel {
        self.prev_levels[k]
    }

    #[cfg(feature = "alloc")]
    fn store_prev(&mut self, bids: &[PriceLevel], asks: &[PriceLevel]) {
        let n = bids.len().min(asks.len());
        self.prev_levels.clear();
        self.prev_levels.extend(
            bids.iter()
                .zip(asks.iter())
                .take(n)
                .map(|(b, a)| PrevLevel::from_levels(b, a)),
        );
    }

    #[cfg(not(feature = "alloc"))]
    fn store_prev(&mut self, bids: &[PriceLevel], asks: &[PriceLevel]) {
        let n = bids.len().min(asks.len()).min(MAX_BOOK_DEPTH);
        for k in 0..n {
            self.prev_levels[k] = PrevLevel::from_levels(&bids[k], &asks[k]);
        }
        self.prev_levels_len = n;
    }

    /// The current multi-level OFI value, or `None` if fewer than two book updates have been processed.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// Returns `true` if at least two book updates have been processed.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    /// The rolling window size (number of book events).
    #[inline]
    pub fn window_size(&self) -> usize {
        self.buffer.capacity()
    }

    /// The exponential decay coefficient applied per book level (`0.0` = equal weighting).
    #[inline]
    pub fn decay_per_level(&self) -> f64 {
        self.decay_per_level
    }

    /// The EWMA smoothing half-life in book updates, or `None` if smoothing is disabled.
    #[inline]
    pub fn half_life(&self) -> Option<f64> {
        self.ewma.as_ref().map(|e| e.half_life())
    }
}
