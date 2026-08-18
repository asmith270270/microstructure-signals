//! Depth Imbalance signal. See [`DepthImbalance`] and `docs/signals/depth_imbalance.md`.

use crate::config_error::ConfigError;
use crate::ewma::EwmaSmoothing;
use crate::types::BookSnapshot;

/// Depth Imbalance signal: `(bid_qty - ask_qty) / (bid_qty + ask_qty)` over `N` levels.
///
/// Output is in `[-1, 1]`. Values near `+1` indicate heavy buy-side depth;
/// values near `-1` indicate heavy sell-side depth.
///
/// See `docs/signals/depth_imbalance.md` for calibration guidance and interpretation.
#[derive(Debug, Clone)]
#[must_use]
pub struct DepthImbalance {
    depth_levels: usize,
    last_value: Option<f64>,
    ewma: Option<EwmaSmoothing>,
}

impl DepthImbalance {
    /// Create a Depth Imbalance calculator using the top `depth_levels` price levels.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::ZeroSizeParameter)` if `depth_levels` is zero.
    pub fn new(depth_levels: usize) -> Result<Self, ConfigError> {
        if depth_levels == 0 {
            return Err(ConfigError::ZeroSizeParameter("depth_levels"));
        }
        Ok(Self {
            depth_levels,
            last_value: None,
            ewma: None,
        })
    }

    /// Create a Depth Imbalance calculator with EWMA smoothing applied to the raw imbalance.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::ZeroSizeParameter)` if `depth_levels` is zero, or
    /// `Err(ConfigError::HalfLifeInvalid)` if `half_life` is not finite and positive.
    pub fn with_smoothing(depth_levels: usize, half_life: f64) -> Result<Self, ConfigError> {
        if depth_levels == 0 {
            return Err(ConfigError::ZeroSizeParameter("depth_levels"));
        }
        if !(half_life > 0.0 && half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(half_life));
        }
        Ok(Self {
            depth_levels,
            last_value: None,
            ewma: Some(EwmaSmoothing::new(half_life)),
        })
    }

    /// Process a new book snapshot and return the updated depth imbalance value.
    ///
    /// Returns `None` if total depth across all sampled levels is zero (empty or one-sided book).
    /// The last computed value is retained and returned by [`value`](Self::value) until
    /// the next update.
    #[inline]
    pub fn update(&mut self, book: &BookSnapshot) -> Option<f64> {
        let bid_qty: f64 = book
            .bids()
            .iter()
            .take(self.depth_levels)
            .map(|lvl| lvl.quantity)
            .sum();

        let ask_qty: f64 = book
            .asks()
            .iter()
            .take(self.depth_levels)
            .map(|lvl| lvl.quantity)
            .sum();

        let total = bid_qty + ask_qty;
        if total <= 0.0 {
            return None;
        }

        let raw_imbalance = (bid_qty - ask_qty) / total;

        let result = if let Some(ewma) = &mut self.ewma {
            ewma.update(raw_imbalance)
        } else {
            raw_imbalance
        };

        self.last_value = Some(result);
        Some(result)
    }

    /// The depth imbalance from the most recent successful update, or `None` if no update
    /// has produced a valid value yet. Persists until the next update.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// The number of book levels used in the imbalance computation.
    #[inline]
    pub fn depth_levels(&self) -> usize {
        self.depth_levels
    }

    /// The EWMA smoothing half-life in book updates, or `None` if smoothing is disabled.
    #[inline]
    pub fn half_life(&self) -> Option<f64> {
        self.ewma.as_ref().map(|e| e.half_life())
    }
}
