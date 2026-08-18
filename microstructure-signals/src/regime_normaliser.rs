//! Regime-change aware normaliser. See [`RegimeNormaliser`] and `docs/signals/normaliser.md`.

use crate::config_error::ConfigError;
use crate::normaliser::EwmaNormaliser;

const EPSILON: f64 = 1e-12;
const FAST_WARM_UP_CAP: usize = 10;

/// Number of consecutive ticks the fast/slow variance ratio must stay above
/// `regime_threshold` before a regime change is declared. See `docs/signals/normaliser.md`
/// ("Known limitation") for why this doesn't fully eliminate false positives.
const ENTRY_CONFIRMATION_TICKS: usize = 3;

/// Regime-change aware EWMA normaliser.
///
/// Monitors the ratio of fast-to-slow EWMA variance. When the ratio exceeds
/// `regime_threshold`, the normaliser switches to a faster EWMA and suspends
/// z-score output for at least `cooldown_period` observations. On regime exit it
/// seeds the slow normaliser from the fast one's state.
///
/// Prefer [`EwmaNormaliser`] unless you specifically
/// need regime detection. Configure via [`RegimeNormaliserParams`](crate::engine::RegimeNormaliserParams)
/// in [`SignalEngineConfig`](crate::engine::SignalEngineConfig).
///
/// [`update_and_normalise`](Self::update_and_normalise) never returns a z-score beyond the
/// shared [`EwmaNormaliser`] clamp (`±10.0`), and never returns `Some` while
/// [`is_in_regime_change`](Self::is_in_regime_change) is `true`. Regime-entry detection
/// itself is not statistically reliable — see `docs/signals/normaliser.md`
/// ("Known limitation") before gating risk logic on `is_in_regime_change()`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[must_use]
pub struct RegimeNormaliser {
    inner: EwmaNormaliser,
    fast: EwmaNormaliser,
    base_half_life: f64,
    fast_half_life: f64,
    regime_threshold: f64,
    exit_hysteresis: f64,
    warm_up: usize,
    in_regime_change: bool,
    regime_entry_streak: usize,
    regime_cooldown: usize,
    cooldown_period: usize,
}

impl RegimeNormaliser {
    /// Create a new `RegimeNormaliser`.
    ///
    /// `base_half_life` is the slow EWMA half-life used for normal z-score output.
    /// `fast_half_life` must be strictly less than `base_half_life`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if any parameter is non-positive or non-finite, `cooldown_period` is zero,
    /// or `fast_half_life >= base_half_life`.
    pub fn new(
        base_half_life: f64,
        fast_half_life: f64,
        warm_up: usize,
        regime_threshold: f64,
        cooldown_period: usize,
    ) -> Result<Self, ConfigError> {
        if !(base_half_life > 0.0 && base_half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(base_half_life));
        }
        if !(fast_half_life > 0.0 && fast_half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(fast_half_life));
        }
        if !(regime_threshold > 0.0 && regime_threshold.is_finite()) {
            return Err(ConfigError::RegimeThresholdInvalid(regime_threshold));
        }
        if cooldown_period == 0 {
            return Err(ConfigError::CooldownPeriodZero);
        }
        if fast_half_life >= base_half_life {
            return Err(ConfigError::FastHalfLifeNotLessThanBase {
                fast: fast_half_life,
                base: base_half_life,
            });
        }
        Ok(Self {
            inner: EwmaNormaliser::new(base_half_life, warm_up)
                .expect("seeded called post-construction with valid params"),
            fast: EwmaNormaliser::new(fast_half_life, warm_up.min(FAST_WARM_UP_CAP))
                .expect("seeded called post-construction with valid params"),
            base_half_life,
            fast_half_life,
            regime_threshold,
            exit_hysteresis: 0.5,
            warm_up,
            in_regime_change: false,
            regime_entry_streak: 0,
            regime_cooldown: 0,
            cooldown_period,
        })
    }

    /// Normalise `value` using the current state, then update the EWMA.
    ///
    /// Returns `None` during warm-up, for every tick while in a regime change (not just the
    /// entry tick), or when variance is negligible.
    pub fn update_and_normalise(&mut self, value: f64) -> Option<f64> {
        let z = self.inner.normalise(value);

        self.fast.update(value);
        self.inner.update(value);

        let slow_var = self.inner.variance();
        let fast_var = self.fast.variance();

        if !self.in_regime_change {
            let breached = self.fast.is_warmup_complete()
                && slow_var > EPSILON
                && fast_var / slow_var > self.regime_threshold;

            self.regime_entry_streak = if breached {
                self.regime_entry_streak + 1
            } else {
                0
            };

            if self.regime_entry_streak >= ENTRY_CONFIRMATION_TICKS {
                self.in_regime_change = true;
                self.regime_entry_streak = 0;
                self.regime_cooldown = 0;
                self.inner =
                    EwmaNormaliser::new(self.fast_half_life, self.warm_up.min(FAST_WARM_UP_CAP))
                        .expect("seeded called post-construction with valid params");
                return None;
            }
        }

        if self.in_regime_change {
            self.regime_cooldown += 1;

            let current_inner_var = self.inner.variance();
            let exit_ratio = if current_inner_var > EPSILON {
                fast_var / current_inner_var
            } else {
                0.0
            };

            if self.regime_cooldown >= self.cooldown_period
                && exit_ratio < self.regime_threshold * self.exit_hysteresis
            {
                self.in_regime_change = false;
                let current_mean = self.inner.mean();
                let current_var = self.inner.variance();
                self.inner = EwmaNormaliser::new_seeded(
                    self.base_half_life,
                    self.warm_up,
                    current_mean,
                    current_var,
                )
                .expect("seeded called post-construction with valid params");
            }
        }

        if self.in_regime_change {
            None
        } else {
            z
        }
    }

    /// Create a regime normaliser pre-seeded with a known slow-EWMA mean and variance.
    ///
    /// The slow (`inner`) normaliser starts with `count = warm_up` so z-scores are
    /// immediately available if variance is sufficient. The fast normaliser starts fresh.
    /// Regime-change state is reset to "not in a regime change".
    ///
    /// Useful for restoring persisted EWMA state after a process restart. Pair with
    /// [`mean`](Self::mean) and [`variance`](Self::variance) to capture the state before shutdown.
    ///
    /// # Errors
    ///
    /// Same conditions as [`new`](Self::new), plus `Err(ConfigError::VarianceInvalid)` if
    /// `variance` is negative or non-finite.
    pub fn new_seeded(
        base_half_life: f64,
        fast_half_life: f64,
        warm_up: usize,
        regime_threshold: f64,
        cooldown_period: usize,
        mean: f64,
        variance: f64,
    ) -> Result<Self, ConfigError> {
        if !(variance >= 0.0 && variance.is_finite()) {
            return Err(ConfigError::VarianceInvalid(variance));
        }
        let mut s = Self::new(
            base_half_life,
            fast_half_life,
            warm_up,
            regime_threshold,
            cooldown_period,
        )?;
        s.inner = EwmaNormaliser::new_seeded(base_half_life, warm_up, mean, variance)
            .expect("seeded called post-construction with valid params");
        Ok(s)
    }

    /// Re-seed the slow (`inner`) EWMA in place and reset the fast EWMA and regime-change
    /// state, without recomputing either EWMA's `lambda`/`alpha` (both half-lives are
    /// unchanged since construction). Used to restore persisted state onto an
    /// already-constructed normaliser — see [`EwmaNormaliser::reseed`].
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::VarianceInvalid)` if `variance` is negative or non-finite.
    pub(crate) fn reseed(&mut self, mean: f64, variance: f64) -> Result<(), ConfigError> {
        self.inner.reseed(mean, variance)?;
        self.fast.reset_fresh();
        self.in_regime_change = false;
        self.regime_entry_streak = 0;
        self.regime_cooldown = 0;
        Ok(())
    }

    /// The current mean of the slow (base) EWMA.
    ///
    /// Use together with [`variance`](Self::variance) to persist normaliser state for
    /// later restoration via [`new_seeded`](Self::new_seeded).
    #[inline]
    pub fn mean(&self) -> f64 {
        self.inner.mean()
    }

    /// The current variance of the slow (base) EWMA.
    #[inline]
    pub fn variance(&self) -> f64 {
        self.inner.variance()
    }

    /// Return a fresh normaliser with the same parameters but no accumulated state.
    pub fn fresh_copy(&self) -> Self {
        let mut copy = Self::new(
            self.base_half_life,
            self.fast_half_life,
            self.warm_up,
            self.regime_threshold,
            self.cooldown_period,
        )
        .expect("seeded called post-construction with valid params");
        copy.exit_hysteresis = self.exit_hysteresis;
        copy
    }

    /// Override the exit hysteresis factor (default `0.5`).
    ///
    /// The normaliser exits a regime when the fast/slow variance ratio drops below
    /// `regime_threshold * exit_hysteresis`. Smaller values make regime exit easier.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::ExitHysteresisInvalid)` if `h` is not finite and positive.
    pub fn set_exit_hysteresis(&mut self, h: f64) -> Result<(), ConfigError> {
        if !(h > 0.0 && h.is_finite()) {
            return Err(ConfigError::ExitHysteresisInvalid(h));
        }
        self.exit_hysteresis = h;
        Ok(())
    }

    /// The current exit hysteresis factor.
    #[inline]
    pub fn exit_hysteresis(&self) -> f64 {
        self.exit_hysteresis
    }

    /// Returns `true` while the normaliser is in a regime-change state (z-scores suppressed).
    #[inline]
    pub fn is_in_regime_change(&self) -> bool {
        self.in_regime_change
    }

    /// Returns `true` if the slow normaliser has completed warm-up and has sufficient variance.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Returns `true` if the slow normaliser has seen at least `warm_up` observations.
    #[inline]
    pub fn is_warmup_complete(&self) -> bool {
        self.inner.is_warmup_complete()
    }

    /// The slow EWMA half-life used for normal z-score output.
    #[inline]
    pub fn base_half_life(&self) -> f64 {
        self.base_half_life
    }

    /// The fast EWMA half-life used for regime detection.
    #[inline]
    pub fn fast_half_life(&self) -> f64 {
        self.fast_half_life
    }

    /// The fast/slow variance ratio that triggers a regime change.
    #[inline]
    pub fn regime_threshold(&self) -> f64 {
        self.regime_threshold
    }

    /// Minimum number of observations before considering regime exit.
    #[inline]
    pub fn cooldown_period(&self) -> usize {
        self.cooldown_period
    }

    /// The warm-up count passed at construction.
    #[inline]
    pub fn warm_up(&self) -> usize {
        self.warm_up
    }
}
