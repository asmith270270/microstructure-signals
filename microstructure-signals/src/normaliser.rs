//! EWMA-based z-score normaliser. See [`EwmaNormaliser`] and `docs/signals/normaliser.md`.

use crate::config_error::ConfigError;

const EPSILON: f64 = 1e-12;

/// Hard ceiling on the magnitude of any returned z-score.
///
/// `EPSILON` guards against dividing by exactly zero, but does not stop the variance
/// estimate from being *near* zero — in that regime `1 / sqrt(variance)` can be large enough
/// that an ordinary-sized deviation produces a z-score of several hundred or thousand, which
/// is a numerical artifact of the variance floor, not a real extreme observation. Clamping
/// keeps every consumer (composites, regime detection, downstream thresholds) from seeing
/// values that look like an outsized real event.
const MAX_ABS_Z: f64 = 10.0;

/// EWMA-based z-score normaliser.
///
/// Maintains an exponentially weighted mean and variance, and returns
/// `(value - mean) / std_dev` once the warm-up period has elapsed and variance
/// is non-negligible. Output is `None` during warm-up or on constant input.
///
/// See `docs/signals/normaliser.md` for half-life calibration guidance.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[must_use]
pub struct EwmaNormaliser {
    half_life: f64,
    lambda: f64,
    alpha: f64,
    ewma_mean: f64,
    ewma_var: f64,
    ewma_inv_std_dev: f64,
    count: usize,
    warm_up: usize,
}

impl EwmaNormaliser {
    /// Create a new normaliser.
    ///
    /// `half_life` controls how quickly old observations are forgotten (in the same units as
    /// the input event stream — book updates for OFI/Depth Imbalance/Microprice, completed
    /// buckets for VSI). `warm_up` is the minimum number of observations before z-scores
    /// are returned.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::HalfLifeInvalid)` if `half_life` is not finite and positive.
    pub fn new(half_life: f64, warm_up: usize) -> Result<Self, ConfigError> {
        if !(half_life > 0.0 && half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(half_life));
        }
        let lambda = crate::math::exp(-crate::math::ln(2.0) / half_life);
        Ok(Self {
            half_life,
            lambda,
            alpha: 1.0 - lambda,
            ewma_mean: 0.0,
            ewma_var: 0.0,
            ewma_inv_std_dev: 0.0,
            count: 0,
            warm_up,
        })
    }

    /// Create a normaliser pre-seeded with a known mean and variance.
    ///
    /// Starts with `count = warm_up` so z-scores are immediately available if variance is
    /// sufficient. Useful for restoring persisted EWMA state after a process restart.
    ///
    /// # Note
    ///
    /// If `variance < 1e-12`, the normaliser starts in the "constant input" state and will
    /// not return z-scores until variance accumulates. Use [`is_count_ready`](Self::is_warmup_complete)
    /// vs [`is_ready`](Self::is_ready) to distinguish this case.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::HalfLifeInvalid)` if `half_life` is not finite and positive,
    /// or `Err(ConfigError::VarianceInvalid)` if `variance` is negative or non-finite.
    pub fn new_seeded(
        half_life: f64,
        warm_up: usize,
        mean: f64,
        variance: f64,
    ) -> Result<Self, ConfigError> {
        if !(half_life > 0.0 && half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(half_life));
        }
        if !(variance >= 0.0 && variance.is_finite()) {
            return Err(ConfigError::VarianceInvalid(variance));
        }
        let lambda = crate::math::exp(-crate::math::ln(2.0) / half_life);
        Ok(Self {
            half_life,
            lambda,
            alpha: 1.0 - lambda,
            ewma_mean: mean,
            ewma_var: variance,
            ewma_inv_std_dev: if variance > EPSILON {
                1.0 / crate::math::sqrt(variance)
            } else {
                0.0
            },
            count: warm_up,
            warm_up,
        })
    }

    /// Re-seed this normaliser's mean/variance in place, without recomputing `lambda`/`alpha`
    /// from `half_life` (unchanged since construction). Used to restore persisted state onto
    /// an already-constructed normaliser — `exp`/`ln` are libm calls, not free, and `half_life`
    /// never changes across a restore.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::VarianceInvalid)` if `variance` is negative or non-finite.
    pub(crate) fn reseed(&mut self, mean: f64, variance: f64) -> Result<(), ConfigError> {
        if !(variance >= 0.0 && variance.is_finite()) {
            return Err(ConfigError::VarianceInvalid(variance));
        }
        self.ewma_mean = mean;
        self.ewma_var = variance;
        self.ewma_inv_std_dev = if variance > EPSILON {
            1.0 / crate::math::sqrt(variance)
        } else {
            0.0
        };
        self.count = self.warm_up;
        Ok(())
    }

    /// Reset mean/variance/count to their just-constructed state in place, without
    /// recomputing `lambda`/`alpha`. Used by [`RegimeNormaliser`](crate::RegimeNormaliser)'s
    /// fast EWMA on restore, which always starts fresh rather than seeded.
    pub(crate) fn reset_fresh(&mut self) {
        self.ewma_mean = 0.0;
        self.ewma_var = 0.0;
        self.ewma_inv_std_dev = 0.0;
        self.count = 0;
    }

    /// Update the EWMA mean and variance with a new observation.
    ///
    /// # Panics
    ///
    /// Panics if `value` is non-finite (NaN or infinite).
    #[inline]
    pub fn update(&mut self, value: f64) {
        assert!(
            value.is_finite(),
            "EwmaNormaliser::update: value must be finite, got {value}"
        );
        if self.count == 0 {
            self.ewma_mean = value;
            self.ewma_var = 0.0;
            self.ewma_inv_std_dev = 0.0;
        } else {
            let prev_mean = self.ewma_mean;
            self.ewma_mean = self.lambda * self.ewma_mean + self.alpha * value;
            let diff = value - prev_mean;
            self.ewma_var = self.lambda * self.ewma_var + self.alpha * diff * diff;
            self.ewma_inv_std_dev = if self.ewma_var > EPSILON {
                1.0 / crate::math::sqrt(self.ewma_var)
            } else {
                0.0
            };
        }
        self.count += 1;
    }

    /// Compute `(value - mean) / std_dev` without updating internal state.
    ///
    /// Returns `None` if the warm-up period is incomplete or variance is below `1e-12`.
    /// The result is clamped to `[-10.0, 10.0]` — a near-zero variance estimate can otherwise
    /// produce an arbitrarily large "z-score" that is a numerical artifact, not a real
    /// extreme observation.
    #[inline]
    pub fn normalise(&self, value: f64) -> Option<f64> {
        if self.count < self.warm_up || self.ewma_var < EPSILON {
            return None;
        }
        let z = (value - self.ewma_mean) * self.ewma_inv_std_dev;
        Some(z.clamp(-MAX_ABS_Z, MAX_ABS_Z))
    }

    /// Normalise `value` using the current state, then update the EWMA with `value`.
    ///
    /// Normalisation uses the pre-update mean/variance so the z-score is not biased
    /// by the current observation. The first call that satisfies warm-up returns `Some`.
    #[inline]
    pub fn update_and_normalise(&mut self, value: f64) -> Option<f64> {
        let z = self.normalise(value);
        self.update(value);
        z
    }

    /// The current EWMA mean.
    #[inline]
    pub fn mean(&self) -> f64 {
        self.ewma_mean
    }

    /// The current EWMA variance.
    #[inline]
    pub fn variance(&self) -> f64 {
        self.ewma_var
    }

    /// The current EWMA standard deviation (`sqrt(variance)`).
    #[inline]
    pub fn std_dev(&self) -> f64 {
        crate::math::sqrt(self.ewma_var)
    }

    /// Returns `true` if the warm-up period is complete **and** variance is sufficient.
    ///
    /// When `false`, [`update_and_normalise`](Self::update_and_normalise) returns `None`.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.count >= self.warm_up && self.ewma_var >= EPSILON
    }

    /// Returns `true` if at least `warm_up` observations have been seen.
    ///
    /// May be `true` while [`is_ready`](Self::is_ready) is `false` when the input signal
    /// has been constant (zero variance).
    #[inline]
    pub fn is_warmup_complete(&self) -> bool {
        self.count >= self.warm_up
    }

    /// The EWMA half-life passed at construction.
    #[inline]
    pub fn half_life(&self) -> f64 {
        self.half_life
    }

    /// The warm-up observation count passed at construction.
    #[inline]
    pub fn warm_up(&self) -> usize {
        self.warm_up
    }
}
