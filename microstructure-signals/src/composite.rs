//! Composite toxicity and adverse selection signals.
//!
//! [`CompositeToxicity`] combines normalised z-scores from multiple signals into a single
//! weighted score. `AdverseSelectionSignal` is a type alias for the same type with
//! different default weights. See `docs/signals/composite.md`.

use crate::config_error::ConfigError;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

#[cfg(not(feature = "alloc"))]
const MAX_SIGNALS: usize = 4;

#[derive(Debug, Clone, Copy)]
struct SmoothingParams {
    lambda: f64,
    alpha: f64,
}

/// Weighted composite of z-score signals, optionally smoothed with EWMA.
///
/// Input z-scores use `f64::NAN` as a sentinel for "not available"; present signals
/// contribute proportionally to their absolute weight. Returns `None` when all inputs
/// are NaN.
///
/// `AdverseSelectionSignal` is a type alias for `CompositeToxicity` with different weights.
/// See `docs/signals/composite.md` for weight guidance.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
#[must_use]
pub struct CompositeToxicity {
    weights: Vec<f64>,
    smoothing: Option<SmoothingParams>,
    smoothed_value: Option<f64>,
    last_output_valid: bool,
}

#[cfg(not(feature = "alloc"))]
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct CompositeToxicity {
    weights: [f64; MAX_SIGNALS],
    smoothing: Option<SmoothingParams>,
    smoothed_value: Option<f64>,
    last_output_valid: bool,
}

#[cfg(feature = "alloc")]
impl CompositeToxicity {
    /// Create a composite with equal weight `1.0` for each of the four z-score inputs.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `smoothing_half_life` is present but not finite and positive.
    pub fn with_equal_weights(smoothing_half_life: Option<f64>) -> Result<Self, ConfigError> {
        Self::new(alloc::vec![1.0, 1.0, 1.0, 1.0], smoothing_half_life)
    }

    /// Create a composite with custom weights.
    ///
    /// Weights are normalised by their absolute sum so only relative magnitudes matter.
    /// At least one weight must be non-zero and all must be finite.
    ///
    /// `smoothing_half_life` applies EWMA smoothing to the composite output.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::WeightNotFinite)` if any weight is non-finite,
    /// `Err(ConfigError::AllWeightsZero)` if all weights are zero, or
    /// `Err(ConfigError::HalfLifeInvalid)` if `smoothing_half_life` is present but not finite and positive.
    pub fn new(weights: Vec<f64>, smoothing_half_life: Option<f64>) -> Result<Self, ConfigError> {
        for (i, &w) in weights.iter().enumerate() {
            if !w.is_finite() {
                return Err(ConfigError::WeightNotFinite { index: i, value: w });
            }
        }
        if !weights.is_empty() && !weights.iter().any(|&w| w != 0.0) {
            return Err(ConfigError::AllWeightsZero);
        }
        let smoothing = if let Some(hl) = smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
            let lambda = crate::math::exp(-crate::math::ln(2.0) / hl);
            Some(SmoothingParams {
                lambda,
                alpha: 1.0 - lambda,
            })
        } else {
            None
        };
        Ok(Self {
            weights,
            smoothing,
            smoothed_value: None,
            last_output_valid: false,
        })
    }

    /// Return a fresh composite with the same weights and smoothing config but no accumulated state.
    pub fn fresh_copy(&self) -> Self {
        Self {
            weights: self.weights.clone(),
            smoothing: self.smoothing,
            smoothed_value: None,
            last_output_valid: false,
        }
    }

    /// Update the composite with a slice of z-scores (using `f64::NAN` for absent signals).
    ///
    /// Returns `None` if all inputs are NaN or all weights for present inputs sum to zero.
    #[inline]
    pub fn update(&mut self, z_scores: &[f64]) -> Option<f64> {
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        let mut any_present = false;

        for (i, &z) in z_scores.iter().enumerate() {
            if !z.is_nan() {
                let Some(&weight) = self.weights.get(i) else {
                    continue;
                };
                weighted_sum += weight * z;
                total_weight += weight.abs();
                any_present = true;
            }
        }

        if !any_present || total_weight == 0.0 {
            if let (Some(s), Some(prev)) = (self.smoothing, self.smoothed_value) {
                self.smoothed_value = Some(s.lambda * prev);
            }
            self.last_output_valid = false;
            return None;
        }

        let raw_value = weighted_sum / total_weight;

        let result = match (self.smoothing, self.smoothed_value) {
            (Some(s), Some(prev)) => {
                let smoothed = s.lambda * prev + s.alpha * raw_value;
                self.smoothed_value = Some(smoothed);
                smoothed
            }
            _ => {
                self.smoothed_value = Some(raw_value);
                raw_value
            }
        };

        self.last_output_valid = true;
        Some(result)
    }

    /// The composite value from the most recent [`update`](Self::update) that had valid inputs.
    ///
    /// Returns `None` if no valid update has occurred yet, or if the last update had all-NaN inputs.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        if self.last_output_valid {
            self.smoothed_value
        } else {
            None
        }
    }
}

#[cfg(not(feature = "alloc"))]
impl CompositeToxicity {
    /// Create a composite with equal weight `1.0` for each of the four z-score inputs
    /// (no-alloc variant).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `smoothing_half_life` is present but not finite and positive.
    pub fn with_equal_weights(smoothing_half_life: Option<f64>) -> Result<Self, ConfigError> {
        Self::new([1.0; MAX_SIGNALS], smoothing_half_life)
    }

    /// Create a composite with custom weights (no-alloc variant).
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::WeightNotFinite)` if any weight is non-finite,
    /// `Err(ConfigError::AllWeightsZero)` if all weights are zero, or
    /// `Err(ConfigError::HalfLifeInvalid)` if `smoothing_half_life` is present but not finite and positive.
    pub fn new(
        weights: [f64; MAX_SIGNALS],
        smoothing_half_life: Option<f64>,
    ) -> Result<Self, ConfigError> {
        for (i, &w) in weights.iter().enumerate() {
            if !w.is_finite() {
                return Err(ConfigError::WeightNotFinite { index: i, value: w });
            }
        }
        if !weights.iter().any(|&w| w != 0.0) {
            return Err(ConfigError::AllWeightsZero);
        }
        let smoothing = if let Some(hl) = smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
            let lambda = crate::math::exp(-crate::math::ln(2.0) / hl);
            Some(SmoothingParams {
                lambda,
                alpha: 1.0 - lambda,
            })
        } else {
            None
        };
        Ok(Self {
            weights,
            smoothing,
            smoothed_value: None,
            last_output_valid: false,
        })
    }

    /// Return a fresh composite with the same weights and smoothing config but no accumulated state.
    pub fn fresh_copy(&self) -> Self {
        Self {
            weights: self.weights,
            smoothing: self.smoothing,
            smoothed_value: None,
            last_output_valid: false,
        }
    }

    /// Update the composite with a slice of z-scores (using `f64::NAN` for absent signals).
    #[inline]
    pub fn update(&mut self, z_scores: &[f64]) -> Option<f64> {
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        let mut any_present = false;

        for (i, &z) in z_scores.iter().enumerate().take(MAX_SIGNALS) {
            if !z.is_nan() {
                let weight = self.weights[i];
                weighted_sum += weight * z;
                total_weight += weight.abs();
                any_present = true;
            }
        }

        if !any_present || total_weight == 0.0 {
            if let (Some(s), Some(prev)) = (self.smoothing, self.smoothed_value) {
                self.smoothed_value = Some(s.lambda * prev);
            }
            self.last_output_valid = false;
            return None;
        }

        let raw_value = weighted_sum / total_weight;

        let result = match (self.smoothing, self.smoothed_value) {
            (Some(s), Some(prev)) => {
                let smoothed = s.lambda * prev + s.alpha * raw_value;
                self.smoothed_value = Some(smoothed);
                smoothed
            }
            _ => {
                self.smoothed_value = Some(raw_value);
                raw_value
            }
        };

        self.last_output_valid = true;
        Some(result)
    }

    /// The composite value from the most recent update that had valid inputs.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        if self.last_output_valid {
            self.smoothed_value
        } else {
            None
        }
    }
}
