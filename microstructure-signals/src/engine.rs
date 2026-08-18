//! [`SignalEngine`] and its configuration types. See `docs/USAGE.md`.

#[cfg(feature = "adverse-selection")]
use crate::adverse_selection::AdverseSelectionSignal;
#[cfg(feature = "composite")]
use crate::composite::CompositeToxicity;
use crate::config_error::ConfigError;
#[cfg(feature = "depth-imbalance")]
use crate::depth_imbalance::DepthImbalance;
#[cfg(feature = "effective-spread")]
use crate::effective_spread::EffectiveSpread;
#[cfg(feature = "microprice")]
use crate::microprice::MicropriceCalculator;
#[cfg(feature = "normaliser")]
use crate::normaliser::EwmaNormaliser;
#[cfg(feature = "ofi")]
use crate::ofi::Ofi;
#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
use crate::regime_normaliser::RegimeNormaliser;
#[cfg(feature = "trade-classifier")]
use crate::trade_classifier::{QuoteRuleClassifier, TickRuleClassifier};
use crate::types::{BookSnapshot, ClassifiedTrade, MarketEvent, SignalSnapshot, Trade};
#[cfg(feature = "vsi")]
use crate::vsi::Vsi;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::vec::Vec;

macro_rules! update_book_signal {
    ($self:expr, $signal:ident, $field:ident, $book:expr) => {
        if let Some(sig) = &mut $self.$signal {
            sig.update($book);
            $self.current_snapshot.$field = sig.value().unwrap_or(f64::NAN);
        } else {
            $self.current_snapshot.$field = f64::NAN;
        }
    };
}

macro_rules! normalise_signal {
    ($self:expr, $raw_field:ident, $z_field:ident, $normaliser:ident) => {{
        let raw = $self.current_snapshot.$raw_field;
        if !raw.is_nan() {
            if let Some(norm) = &mut $self.$normaliser {
                $self.current_snapshot.$z_field =
                    norm.update_and_normalise(raw).unwrap_or(f64::NAN);
            } else {
                $self.current_snapshot.$z_field = f64::NAN;
            }
        } else {
            $self.current_snapshot.$z_field = f64::NAN;
        }
    }};
}

macro_rules! update_composite {
    ($self:expr, $signal:ident, $field:ident, $z_scores:expr) => {
        if let Some(sig) = &mut $self.$signal {
            $self.current_snapshot.$field = sig.update($z_scores).unwrap_or(f64::NAN);
        } else {
            $self.current_snapshot.$field = f64::NAN;
        }
    };
}

/// Which trade classification algorithm the engine uses for VSI and Effective Spread.
///
/// See `docs/signals/trade_classifier.md` for accuracy figures and trade-offs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClassifierType {
    /// Tick rule: classifies by price movement direction. ~65–75% accurate. Does not require book data.
    TickRule,
    /// Quote rule (Lee-Ready): classifies by position relative to mid-price. ~75–85% accurate.
    /// Falls back to tick rule when the trade price equals the mid. This is the default.
    #[default]
    QuoteRule,
}

/// Controls which signals the engine computes.
///
/// Disabling a signal skips its computation entirely — its [`SignalSnapshot`] field will be `NAN`.
/// Use [`SignalSelection::none`] as a starting point and enable only what you need,
/// or use [`SignalSelection::raw_only`] to enable all raw signals without z-scores or composites.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalSelection {
    /// Enable raw OFI (rolling window sum). Required for `ofi_z`.
    pub ofi: bool,
    /// Enable Depth Imbalance. Required for `depth_imbalance_z`.
    pub depth_imbalance: bool,
    /// Enable Microprice and Microprice Deviation. Required for `microprice_deviation_z`.
    pub microprice: bool,
    /// Enable VSI. Requires the `vsi` and `trade-classifier` features. Required for `vsi_z`.
    pub vsi: bool,
    /// Enable OFI z-score normalisation. Requires `ofi = true`.
    pub ofi_z: bool,
    /// Enable Depth Imbalance z-score normalisation. Requires `depth_imbalance = true`.
    pub depth_imbalance_z: bool,
    /// Enable Microprice Deviation z-score normalisation. Requires `microprice = true`.
    pub microprice_deviation_z: bool,
    /// Enable VSI z-score normalisation. Requires `vsi = true`.
    pub vsi_z: bool,
    /// Enable Composite Toxicity. Requires at least one z-score to be enabled.
    pub composite_toxicity: bool,
    /// Enable Adverse Selection signal. Requires at least one z-score to be enabled.
    pub adverse_selection: bool,
    /// Enable Effective Spread. Requires the `effective-spread` and `trade-classifier` features.
    pub effective_spread: bool,
}

impl Default for SignalSelection {
    fn default() -> Self {
        Self {
            ofi: true,
            depth_imbalance: true,
            microprice: true,
            vsi: true,
            ofi_z: true,
            depth_imbalance_z: true,
            microprice_deviation_z: true,
            vsi_z: true,
            composite_toxicity: true,
            adverse_selection: true,
            effective_spread: true,
        }
    }
}

impl SignalSelection {
    /// All signals disabled. Use as a base and enable only what you need.
    pub fn none() -> Self {
        Self {
            ofi: false,
            depth_imbalance: false,
            microprice: false,
            vsi: false,
            ofi_z: false,
            depth_imbalance_z: false,
            microprice_deviation_z: false,
            vsi_z: false,
            composite_toxicity: false,
            adverse_selection: false,
            effective_spread: false,
        }
    }

    /// All raw signals enabled; z-scores, composites, and adverse selection disabled.
    ///
    /// Useful for low-latency paths where normalisation overhead is unacceptable,
    /// or when calibrating bucket/window parameters before enabling normalisers.
    pub fn raw_only() -> Self {
        Self {
            ofi: true,
            depth_imbalance: true,
            microprice: true,
            vsi: true,
            ofi_z: false,
            depth_imbalance_z: false,
            microprice_deviation_z: false,
            vsi_z: false,
            composite_toxicity: false,
            adverse_selection: false,
            effective_spread: true,
        }
    }
}

#[cfg(not(feature = "alloc"))]
pub const MAX_COMPOSITE_SIGNALS: usize = 4;

/// Named weights for the composite toxicity and adverse selection signals.
///
/// Weights are normalised by their absolute sum before combining, so only relative magnitudes
/// matter. Negative weights invert a signal's contribution. At least one weight must be non-zero.
///
/// Default: equal weight of `1.0` for each component.
///
/// See `docs/signals/composite.md` for guidance on choosing weights.
#[derive(Debug, Clone, Copy)]
pub struct CompositeWeights {
    /// Weight applied to the OFI z-score.
    pub ofi: f64,
    /// Weight applied to the VSI z-score.
    pub vsi: f64,
    /// Weight applied to the Depth Imbalance z-score.
    pub depth_imbalance: f64,
    /// Weight applied to the Microprice Deviation z-score.
    pub microprice_deviation: f64,
}

impl Default for CompositeWeights {
    fn default() -> Self {
        Self {
            ofi: 1.0,
            vsi: 1.0,
            depth_imbalance: 1.0,
            microprice_deviation: 1.0,
        }
    }
}

/// Configuration for [`SignalEngine`].
///
/// Use [`SignalEngineConfig::with_vsi_bucket_volume`] or [`SignalEngineConfig::with_adv_vsi_bucket`]
/// as constructors when VSI is needed. Use [`SignalEngineConfig::default`] for a configuration
/// with all book signals enabled and VSI disabled (VSI requires a non-zero bucket volume).
///
/// All parameters are validated by [`SignalEngineConfig::validate`], which is called automatically
/// by [`SignalEngine::new`].
///
/// See `docs/USAGE.md` for full parameter guidance.
#[non_exhaustive]
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub struct SignalEngineConfig {
    /// Number of order book levels used by Depth Imbalance. Default: `5`.
    pub depth_levels: usize,
    /// Optional EWMA smoothing half-life (in book updates) for Depth Imbalance. Default: `None`.
    pub depth_imbalance_smoothing_half_life: Option<f64>,
    /// Rolling window size (number of book events) for OFI. Default: `100`.
    pub ofi_window: usize,
    /// Optional EWMA smoothing half-life (in book updates) for OFI. Default: `None`.
    pub ofi_smoothing_half_life: Option<f64>,
    /// If `true`, each OFI event is divided by total top-of-book liquidity. Default: `false`.
    pub ofi_normalise_by_liquidity: bool,
    /// Volume per VSI bucket. Must be positive when `signals.vsi = true`. Default: `0.0` (VSI off).
    pub vsi_bucket_volume: f64,
    /// Number of completed VSI buckets in the rolling window. Default: `50`.
    pub vsi_n_buckets: usize,
    /// EWMA half-life (in observations) for all signal normalisers. Default: `1000.0`.
    ///
    /// Smaller values adapt faster to regime changes; larger values give a more stable baseline.
    /// See `docs/signals/normaliser.md` for per-signal recommendations.
    pub normalisation_half_life: f64,
    /// Minimum number of observations before normalisers return z-scores. Default: `50`.
    ///
    /// VSI counts completed buckets, not individual trades.
    pub normalisation_warm_up: usize,
    /// Weights for the composite toxicity signal. Default: equal weight `1.0` for all four components.
    pub toxicity_weights: CompositeWeights,
    /// Weights for the adverse selection signal. Default: equal weight `1.0` for all four components.
    pub adverse_selection_weights: CompositeWeights,
    /// Optional EWMA smoothing half-life for composite outputs. Default: `None`.
    pub composite_smoothing_half_life: Option<f64>,
    /// Trade classification algorithm. Default: [`ClassifierType::QuoteRule`].
    pub classifier: ClassifierType,
    /// Which signals to compute. Default: all signals enabled (VSI disabled unless bucket volume set).
    pub signals: SignalSelection,
    /// Optional regime-change normaliser parameters. `None` uses the standard EWMA normaliser.
    #[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
    pub regime_normaliser_params: Option<RegimeNormaliserParams>,
}

/// Configuration for [`SignalEngine`] (no-alloc variant).
#[non_exhaustive]
#[cfg(not(feature = "alloc"))]
#[derive(Debug, Clone, Copy)]
pub struct SignalEngineConfig {
    /// Number of order book levels used by Depth Imbalance. Default: `5`.
    pub depth_levels: usize,
    /// Optional EWMA smoothing half-life (in book updates) for Depth Imbalance.
    pub depth_imbalance_smoothing_half_life: Option<f64>,
    /// Rolling window size (number of book events) for OFI. Default: `100`.
    pub ofi_window: usize,
    /// Optional EWMA smoothing half-life (in book updates) for OFI.
    pub ofi_smoothing_half_life: Option<f64>,
    /// If `true`, each OFI event is divided by total top-of-book liquidity.
    pub ofi_normalise_by_liquidity: bool,
    /// Volume per VSI bucket. Must be positive when `signals.vsi = true`.
    pub vsi_bucket_volume: f64,
    /// Number of completed VSI buckets in the rolling window. Default: `50`.
    pub vsi_n_buckets: usize,
    /// EWMA half-life (in observations) for all signal normalisers. Default: `1000.0`.
    pub normalisation_half_life: f64,
    /// Minimum number of observations before normalisers return z-scores. Default: `50`.
    pub normalisation_warm_up: usize,
    /// Weights for the composite toxicity signal.
    pub toxicity_weights: CompositeWeights,
    /// Weights for the adverse selection signal.
    pub adverse_selection_weights: CompositeWeights,
    /// Optional EWMA smoothing half-life for composite outputs.
    pub composite_smoothing_half_life: Option<f64>,
    /// Trade classification algorithm.
    pub classifier: ClassifierType,
    /// Which signals to compute.
    pub signals: SignalSelection,
    /// Optional regime-change normaliser parameters.
    #[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
    pub regime_normaliser_params: Option<RegimeNormaliserParams>,
}

#[cfg(feature = "alloc")]
impl Default for SignalEngineConfig {
    fn default() -> Self {
        Self {
            depth_levels: 5,
            depth_imbalance_smoothing_half_life: None,
            ofi_window: 100,
            ofi_smoothing_half_life: None,
            ofi_normalise_by_liquidity: false,
            vsi_bucket_volume: 0.0,
            vsi_n_buckets: 50,
            normalisation_half_life: 1000.0,
            normalisation_warm_up: 50,
            toxicity_weights: CompositeWeights::default(),
            adverse_selection_weights: CompositeWeights::default(),
            composite_smoothing_half_life: None,
            classifier: ClassifierType::QuoteRule,
            signals: SignalSelection {
                vsi: false,
                vsi_z: false,
                ..SignalSelection::default()
            },
            #[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
            regime_normaliser_params: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl SignalEngineConfig {
    /// Create a config with VSI enabled using an explicit bucket volume.
    ///
    /// All other fields take their [`Default`] values. VSI and `vsi_z` are enabled;
    /// the remaining signals default to on.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::VolumeInvalid)` if `vsi_bucket_volume` is not finite and positive.
    pub fn with_vsi_bucket_volume(vsi_bucket_volume: f64) -> Result<Self, ConfigError> {
        if !(vsi_bucket_volume > 0.0 && vsi_bucket_volume.is_finite()) {
            return Err(ConfigError::VolumeInvalid(vsi_bucket_volume));
        }
        let mut config = Self {
            vsi_bucket_volume,
            ..Self::default()
        };
        config.signals.vsi = true;
        config.signals.vsi_z = true;
        Ok(config)
    }

    /// Create a config with VSI bucket volume derived from average daily volume (ADV).
    ///
    /// Sets `vsi_bucket_volume = adv * fraction / n_buckets`. This is the recommended
    /// constructor when calibrating VSI relative to a known ADV figure — see
    /// `docs/signals/vsi.md` for guidance on choosing `fraction` and `n_buckets`.
    ///
    /// `fraction` is the share of ADV consumed by the **entire** `n_buckets`-bucket rolling
    /// window, not by a single bucket — e.g. `fraction = 0.1, n_buckets = 50` means the full
    /// window of 50 buckets together represent 10% of ADV, so each individual bucket is
    /// `0.1 / 50 = 0.2%` of ADV. A larger `n_buckets` for the same `fraction` gives smaller,
    /// more frequent buckets, not a bigger window in volume terms.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `adv` or `fraction` are not finite and positive, if `n_buckets` is zero,
    /// or if the resulting `bucket_volume` underflows to zero or infinity.
    pub fn with_adv_vsi_bucket(
        adv: f64,
        n_buckets: usize,
        fraction: f64,
    ) -> Result<Self, ConfigError> {
        if !(adv > 0.0 && adv.is_finite()) {
            return Err(ConfigError::VolumeInvalid(adv));
        }
        if !(fraction > 0.0 && fraction.is_finite()) {
            return Err(ConfigError::VolumeInvalid(fraction));
        }
        if n_buckets == 0 {
            return Err(ConfigError::ZeroSizeParameter("n_buckets"));
        }
        let bucket_volume = adv * fraction / n_buckets as f64;
        if !(bucket_volume > 0.0 && bucket_volume.is_finite()) {
            return Err(ConfigError::VolumeInvalid(bucket_volume));
        }
        let mut config = Self {
            vsi_bucket_volume: bucket_volume,
            vsi_n_buckets: n_buckets,
            ..Self::default()
        };
        config.signals.vsi = true;
        config.signals.vsi_z = true;
        Ok(config)
    }

    /// Validate the configuration.
    ///
    /// Called automatically by [`SignalEngine::new`]. Call explicitly when building a config
    /// incrementally and you want early feedback before constructing the engine.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::...)` describing the first invalid parameter found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(self.normalisation_half_life > 0.0 && self.normalisation_half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(self.normalisation_half_life));
        }
        if self.ofi_window == 0 {
            return Err(ConfigError::ZeroSizeParameter("ofi_window"));
        }
        if self.depth_levels == 0 {
            return Err(ConfigError::ZeroSizeParameter("depth_levels"));
        }
        if self.vsi_n_buckets == 0 {
            return Err(ConfigError::ZeroSizeParameter("vsi_n_buckets"));
        }
        #[cfg(feature = "vsi")]
        if self.signals.vsi && !(self.vsi_bucket_volume > 0.0 && self.vsi_bucket_volume.is_finite())
        {
            return Err(ConfigError::VolumeInvalid(self.vsi_bucket_volume));
        }
        if let Some(hl) = self.ofi_smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        if let Some(hl) = self.depth_imbalance_smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        if let Some(hl) = self.composite_smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        #[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
        if let Some(rp) = &self.regime_normaliser_params {
            if rp.fast_half_life >= self.normalisation_half_life {
                return Err(ConfigError::FastHalfLifeNotLessThanBase {
                    fast: rp.fast_half_life,
                    base: self.normalisation_half_life,
                });
            }
        }
        Ok(())
    }
}

#[cfg(not(feature = "alloc"))]
impl Default for SignalEngineConfig {
    fn default() -> Self {
        Self {
            depth_levels: 5,
            depth_imbalance_smoothing_half_life: None,
            ofi_window: 100,
            ofi_smoothing_half_life: None,
            ofi_normalise_by_liquidity: false,
            vsi_bucket_volume: 0.0,
            vsi_n_buckets: 50,
            normalisation_half_life: 1000.0,
            normalisation_warm_up: 50,
            toxicity_weights: CompositeWeights::default(),
            adverse_selection_weights: CompositeWeights::default(),
            composite_smoothing_half_life: None,
            classifier: ClassifierType::QuoteRule,
            signals: SignalSelection {
                vsi: false,
                vsi_z: false,
                ..SignalSelection::default()
            },
            #[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
            regime_normaliser_params: None,
        }
    }
}

#[cfg(not(feature = "alloc"))]
impl SignalEngineConfig {
    /// Create a config with VSI enabled using an explicit bucket volume (no-alloc variant).
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::VolumeInvalid)` if `vsi_bucket_volume` is not finite and positive.
    pub fn with_vsi_bucket_volume(vsi_bucket_volume: f64) -> Result<Self, ConfigError> {
        if !(vsi_bucket_volume > 0.0 && vsi_bucket_volume.is_finite()) {
            return Err(ConfigError::VolumeInvalid(vsi_bucket_volume));
        }
        let mut config = Self {
            vsi_bucket_volume,
            ..Self::default()
        };
        config.signals.vsi = true;
        config.signals.vsi_z = true;
        Ok(config)
    }

    /// Create a config with VSI bucket volume derived from average daily volume (ADV)
    /// (no-alloc variant).
    ///
    /// Sets `vsi_bucket_volume = adv * fraction / n_buckets`. `fraction` is the share of ADV
    /// consumed by the **entire** `n_buckets`-bucket rolling window, not by a single bucket —
    /// e.g. `fraction = 0.1, n_buckets = 50` means the full window of 50 buckets together
    /// represent 10% of ADV, so each individual bucket is `0.1 / 50 = 0.2%` of ADV.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `adv` or `fraction` are not finite and positive, if `n_buckets` is zero,
    /// or if the resulting `bucket_volume` underflows to zero or infinity.
    pub fn with_adv_vsi_bucket(
        adv: f64,
        n_buckets: usize,
        fraction: f64,
    ) -> Result<Self, ConfigError> {
        if !(adv > 0.0 && adv.is_finite()) {
            return Err(ConfigError::VolumeInvalid(adv));
        }
        if !(fraction > 0.0 && fraction.is_finite()) {
            return Err(ConfigError::VolumeInvalid(fraction));
        }
        if n_buckets == 0 {
            return Err(ConfigError::ZeroSizeParameter("n_buckets"));
        }
        let bucket_volume = adv * fraction / n_buckets as f64;
        if !(bucket_volume > 0.0 && bucket_volume.is_finite()) {
            return Err(ConfigError::VolumeInvalid(bucket_volume));
        }
        let mut config = Self {
            vsi_bucket_volume: bucket_volume,
            vsi_n_buckets: n_buckets,
            ..Self::default()
        };
        config.signals.vsi = true;
        config.signals.vsi_z = true;
        Ok(config)
    }

    /// Validate the configuration (no-alloc variant).
    ///
    /// Called automatically by [`SignalEngine::new`].
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::...)` describing the first invalid parameter found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(self.normalisation_half_life > 0.0 && self.normalisation_half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(self.normalisation_half_life));
        }
        if self.ofi_window == 0 {
            return Err(ConfigError::ZeroSizeParameter("ofi_window"));
        }
        if self.depth_levels == 0 {
            return Err(ConfigError::ZeroSizeParameter("depth_levels"));
        }
        if self.vsi_n_buckets == 0 {
            return Err(ConfigError::ZeroSizeParameter("vsi_n_buckets"));
        }
        #[cfg(feature = "vsi")]
        if self.signals.vsi && !(self.vsi_bucket_volume > 0.0 && self.vsi_bucket_volume.is_finite())
        {
            return Err(ConfigError::VolumeInvalid(self.vsi_bucket_volume));
        }
        if let Some(hl) = self.ofi_smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        if let Some(hl) = self.depth_imbalance_smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        if let Some(hl) = self.composite_smoothing_half_life {
            if !(hl > 0.0 && hl.is_finite()) {
                return Err(ConfigError::HalfLifeInvalid(hl));
            }
        }
        #[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
        if let Some(rp) = &self.regime_normaliser_params {
            if rp.fast_half_life >= self.normalisation_half_life {
                return Err(ConfigError::FastHalfLifeNotLessThanBase {
                    fast: rp.fast_half_life,
                    base: self.normalisation_half_life,
                });
            }
        }
        Ok(())
    }
}

/// Parameters for the regime-change normaliser.
///
/// The regime normaliser uses a fast EWMA to detect when volatility spikes beyond
/// `regime_threshold` standard deviations, then suspends normalisation for
/// `cooldown_period` observations. See `docs/signals/normaliser.md` for details.
///
/// Construct via [`RegimeNormaliserParams::new`] for validated parameters, or use
/// [`Default`] for the defaults (`fast_half_life=20`, `regime_threshold=4.0`,
/// `cooldown_period=50`, `exit_hysteresis=0.5`).
#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[derive(Debug, Clone, Copy)]
pub struct RegimeNormaliserParams {
    /// Half-life (in observations) of the fast EWMA used to detect volatility spikes.
    /// Must be less than `SignalEngineConfig::normalisation_half_life`. Default: `20.0`.
    pub fast_half_life: f64,
    /// Number of standard deviations above which a regime change is declared. Default: `4.0`.
    pub regime_threshold: f64,
    /// Minimum number of observations to wait after a regime entry before re-evaluating exit.
    /// Default: `50`.
    pub cooldown_period: usize,
    /// Fast-EWMA z-score must fall below `regime_threshold - exit_hysteresis` before the
    /// normaliser exits a regime. Prevents rapid regime toggling. Default: `0.5`.
    pub exit_hysteresis: f64,
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
impl Default for RegimeNormaliserParams {
    fn default() -> Self {
        Self {
            fast_half_life: 20.0,
            regime_threshold: 4.0,
            cooldown_period: 50,
            exit_hysteresis: 0.5,
        }
    }
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
impl RegimeNormaliserParams {
    /// Construct validated [`RegimeNormaliserParams`].
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::...)` if any parameter is non-positive or non-finite,
    /// or if `cooldown_period` is zero.
    pub fn new(
        fast_half_life: f64,
        regime_threshold: f64,
        cooldown_period: usize,
        exit_hysteresis: f64,
    ) -> Result<Self, ConfigError> {
        if !(fast_half_life > 0.0 && fast_half_life.is_finite()) {
            return Err(ConfigError::HalfLifeInvalid(fast_half_life));
        }
        if !(regime_threshold > 0.0 && regime_threshold.is_finite()) {
            return Err(ConfigError::RegimeThresholdInvalid(regime_threshold));
        }
        if cooldown_period == 0 {
            return Err(ConfigError::CooldownPeriodZero);
        }
        if !(exit_hysteresis > 0.0 && exit_hysteresis.is_finite()) {
            return Err(ConfigError::ExitHysteresisInvalid(exit_hysteresis));
        }
        Ok(Self {
            fast_half_life,
            regime_threshold,
            cooldown_period,
            exit_hysteresis,
        })
    }
}

#[cfg(feature = "normaliser")]
#[derive(Debug, Clone)]
enum SignalNormaliser {
    Ewma(EwmaNormaliser),
    #[cfg(feature = "regime-normaliser")]
    Regime(RegimeNormaliser),
}

#[cfg(feature = "normaliser")]
impl SignalNormaliser {
    #[inline]
    fn update_and_normalise(&mut self, value: f64) -> Option<f64> {
        match self {
            Self::Ewma(n) => n.update_and_normalise(value),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => n.update_and_normalise(value),
        }
    }

    #[inline]
    fn is_ready(&self) -> bool {
        match self {
            Self::Ewma(n) => n.is_ready(),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => n.is_ready(),
        }
    }

    #[inline]
    fn is_warmup_complete(&self) -> bool {
        match self {
            Self::Ewma(n) => n.is_warmup_complete(),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => n.is_warmup_complete(),
        }
    }

    fn fresh(&self) -> Self {
        match self {
            Self::Ewma(n) => Self::Ewma(
                EwmaNormaliser::new(n.half_life(), n.warm_up())
                    .expect("config already validated at construction time"),
            ),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => Self::Regime(n.fresh_copy()),
        }
    }

    fn mean(&self) -> f64 {
        match self {
            Self::Ewma(n) => n.mean(),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => n.mean(),
        }
    }

    fn variance(&self) -> f64 {
        match self {
            Self::Ewma(n) => n.variance(),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => n.variance(),
        }
    }

    /// Re-seed in place. `half_life` (and, for `Regime`, `fast_half_life`) never change across
    /// a restore, so this skips `EwmaNormaliser::new_seeded`'s `exp`/`ln` recomputation of
    /// `lambda`/`alpha` — see [`EwmaNormaliser::reseed`].
    fn seeded(&mut self, mean: f64, variance: f64) {
        match self {
            Self::Ewma(n) => n
                .reseed(mean, variance)
                .expect("config already validated at construction time"),
            #[cfg(feature = "regime-normaliser")]
            Self::Regime(n) => n
                .reseed(mean, variance)
                .expect("config already validated at construction time"),
        }
    }
}

/// Captured EWMA state for all active normalisers in a [`SignalEngine`].
///
/// Produced by [`SignalEngine::capture_normaliser_state`] and consumed by
/// [`SignalEngine::restore_normaliser_state`]. Each field is `None` when the
/// corresponding normaliser is disabled or not yet initialised (no observations seen).
///
/// # Example — persist and restore across a process restart
///
/// ```
/// use microstructure_signals::{SignalEngine, SignalEngineConfig};
/// use microstructure_signals::types::{BookSnapshot, PriceLevel};
///
/// // Session 1: warm up the engine.
/// let config = SignalEngineConfig::default();
/// let mut engine = SignalEngine::new(config.clone()).unwrap();
///
/// let book = BookSnapshot::new(
///     &[PriceLevel { price: 100.0, quantity: 10.0 }],
///     &[PriceLevel { price: 100.1, quantity: 10.0 }],
///     0,
/// );
/// for _ in 0..100 { engine.on_book_update(&book); }
/// let state = engine.capture_normaliser_state();
///
/// // Session 2: restore state so warm-up is not repeated.
/// let mut engine2 = SignalEngine::new(config).unwrap();
/// engine2.restore_normaliser_state(&state);
/// ```
#[cfg(feature = "normaliser")]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NormaliserSnapshot {
    /// `(mean, variance)` of the OFI normaliser, or `None` if disabled / unseen.
    pub ofi: Option<(f64, f64)>,
    /// `(mean, variance)` of the Depth Imbalance normaliser, or `None` if disabled / unseen.
    pub depth_imbalance: Option<(f64, f64)>,
    /// `(mean, variance)` of the Microprice Deviation normaliser, or `None` if disabled / unseen.
    pub microprice_deviation: Option<(f64, f64)>,
    /// `(mean, variance)` of the VSI normaliser, or `None` if disabled / unseen.
    pub vsi: Option<(f64, f64)>,
}

#[cfg(feature = "trade-classifier")]
#[derive(Debug, Clone)]
enum Classifier {
    Tick(TickRuleClassifier),
    Quote(QuoteRuleClassifier),
}

/// Streaming microstructure signal engine.
///
/// Call [`SignalEngine::on_book_update`] on each order book snapshot and
/// [`SignalEngine::on_trade`] on each trade event. Both methods return a reference to the
/// current [`SignalSnapshot`], which is updated in-place.
///
/// # Threading
///
/// `SignalEngine` is `Send + Sync` but is **not** designed for concurrent mutation.
/// The expected model is a single writer thread calling `on_book_update` / `on_trade`,
/// with read-only access to the returned [`SignalSnapshot`] shared across threads.
/// Concurrent calls to any `&mut self` method on the same engine are a data race.
///
/// # Example
///
/// ```
/// use microstructure_signals::{SignalEngine, SignalEngineConfig};
/// use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade};
///
/// let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
/// let mut engine = SignalEngine::new(config).unwrap();
///
/// let book = BookSnapshot::new(
///     &[PriceLevel { price: 100.0, quantity: 50.0 }],
///     &[PriceLevel { price: 100.1, quantity: 50.0 }],
///     0,
/// );
/// let snapshot = engine.on_book_update(&book);
/// if !snapshot.ofi.is_nan() {
///     println!("OFI: {}", snapshot.ofi);
/// }
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct SignalEngine {
    #[cfg(feature = "ofi")]
    ofi: Option<Ofi>,
    #[cfg(feature = "depth-imbalance")]
    depth_imbalance: Option<DepthImbalance>,
    #[cfg(feature = "microprice")]
    microprice: Option<MicropriceCalculator>,
    #[cfg(feature = "vsi")]
    vsi: Option<Vsi>,
    #[cfg(all(feature = "ofi", feature = "normaliser"))]
    ofi_normaliser: Option<SignalNormaliser>,
    #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
    depth_imbalance_normaliser: Option<SignalNormaliser>,
    #[cfg(all(feature = "microprice", feature = "normaliser"))]
    microprice_deviation_normaliser: Option<SignalNormaliser>,
    #[cfg(all(feature = "vsi", feature = "normaliser"))]
    vsi_normaliser: Option<SignalNormaliser>,
    #[cfg(feature = "composite")]
    composite: Option<CompositeToxicity>,
    #[cfg(feature = "adverse-selection")]
    adverse_selection: Option<AdverseSelectionSignal>,
    #[cfg(feature = "effective-spread")]
    effective_spread: Option<EffectiveSpread>,
    #[cfg(feature = "trade-classifier")]
    classifier: Option<Classifier>,
    #[cfg(not(feature = "alloc"))]
    last_book: Option<BookSnapshot>,
    current_snapshot: SignalSnapshot,
}

impl SignalEngine {
    /// Create a new engine from `config`.
    ///
    /// Calls [`SignalEngineConfig::validate`] and returns `Err` on invalid configuration.
    /// The `signals` field of `config` controls which computations are active.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::...)` if the configuration is invalid.
    pub fn new(config: SignalEngineConfig) -> Result<Self, ConfigError> {
        let selection = config.signals;
        Self::with_selection(config, selection)
    }

    /// Create a new engine, overriding the signal selection in `config` with `selection`.
    ///
    /// Useful for constructing multiple engines from the same base config with different
    /// active signal subsets without modifying the config struct.
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::...)` if the configuration is invalid.
    pub fn with_selection(
        config: SignalEngineConfig,
        selection: SignalSelection,
    ) -> Result<Self, ConfigError> {
        config.validate()?;
        let s = &selection;

        #[cfg(feature = "ofi")]
        let ofi = s.ofi.then(|| {
            match (
                config.ofi_normalise_by_liquidity,
                config.ofi_smoothing_half_life,
            ) {
                (true, Some(hl)) => Ofi::normalised_with_smoothing(config.ofi_window, hl)
                    .expect("config validated by SignalEngineConfig::validate"),
                (true, None) => Ofi::normalised(config.ofi_window)
                    .expect("config validated by SignalEngineConfig::validate"),
                (false, Some(hl)) => Ofi::with_smoothing(config.ofi_window, hl)
                    .expect("config validated by SignalEngineConfig::validate"),
                (false, None) => Ofi::new(config.ofi_window)
                    .expect("config validated by SignalEngineConfig::validate"),
            }
        });
        #[cfg(feature = "depth-imbalance")]
        let depth_imbalance =
            s.depth_imbalance
                .then(|| match config.depth_imbalance_smoothing_half_life {
                    Some(hl) => DepthImbalance::with_smoothing(config.depth_levels, hl)
                        .expect("config validated by SignalEngineConfig::validate"),
                    None => DepthImbalance::new(config.depth_levels)
                        .expect("config validated by SignalEngineConfig::validate"),
                });
        #[cfg(feature = "microprice")]
        let microprice = s.microprice.then(MicropriceCalculator::new);
        #[cfg(feature = "vsi")]
        let vsi = (s.vsi && config.vsi_bucket_volume > 0.0 && config.vsi_bucket_volume.is_finite())
            .then(|| {
                Vsi::new(config.vsi_bucket_volume, config.vsi_n_buckets)
                    .expect("config validated by SignalEngineConfig::validate")
            });

        #[cfg(feature = "normaliser")]
        let hl = config.normalisation_half_life;
        #[cfg(feature = "normaliser")]
        let wu = config.normalisation_warm_up;

        #[cfg(feature = "normaliser")]
        let make_normaliser = |hl: f64, wu: usize| -> SignalNormaliser {
            #[cfg(feature = "regime-normaliser")]
            if let Some(rp) = &config.regime_normaliser_params {
                let mut rn = RegimeNormaliser::new(
                    hl,
                    rp.fast_half_life,
                    wu,
                    rp.regime_threshold,
                    rp.cooldown_period,
                )
                .expect("config validated by SignalEngineConfig::validate");
                rn.set_exit_hysteresis(rp.exit_hysteresis)
                    .expect("config validated by SignalEngineConfig::validate");
                return SignalNormaliser::Regime(rn);
            }
            SignalNormaliser::Ewma(
                EwmaNormaliser::new(hl, wu)
                    .expect("config validated by SignalEngineConfig::validate"),
            )
        };

        #[cfg(all(feature = "ofi", feature = "normaliser"))]
        let ofi_normaliser = (s.ofi && s.ofi_z).then(|| make_normaliser(hl, wu));
        #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
        let depth_imbalance_normaliser =
            (s.depth_imbalance && s.depth_imbalance_z).then(|| make_normaliser(hl, wu));
        #[cfg(all(feature = "microprice", feature = "normaliser"))]
        let microprice_deviation_normaliser =
            (s.microprice && s.microprice_deviation_z).then(|| make_normaliser(hl, wu));
        #[cfg(all(feature = "vsi", feature = "normaliser"))]
        let vsi_normaliser = (s.vsi && s.vsi_z).then(|| make_normaliser(hl, wu));

        #[cfg(feature = "normaliser")]
        let any_z = {
            let mut has_z = false;
            #[cfg(feature = "ofi")]
            {
                has_z = has_z || (s.ofi && s.ofi_z);
            }
            #[cfg(feature = "depth-imbalance")]
            {
                has_z = has_z || (s.depth_imbalance && s.depth_imbalance_z);
            }
            #[cfg(feature = "microprice")]
            {
                has_z = has_z || (s.microprice && s.microprice_deviation_z);
            }
            #[cfg(feature = "vsi")]
            {
                has_z = has_z || (s.vsi && s.vsi_z);
            }
            has_z
        };

        #[cfg(feature = "composite")]
        let composite = {
            #[cfg(feature = "normaliser")]
            {
                (s.composite_toxicity && any_z).then(|| {
                    let w = config.toxicity_weights;
                    #[cfg(feature = "alloc")]
                    let weights =
                        Vec::from([w.ofi, w.vsi, w.depth_imbalance, w.microprice_deviation]);
                    #[cfg(not(feature = "alloc"))]
                    let weights = [w.ofi, w.vsi, w.depth_imbalance, w.microprice_deviation];
                    CompositeToxicity::new(weights, config.composite_smoothing_half_life)
                        .expect("config validated by SignalEngineConfig::validate")
                })
            }
            #[cfg(not(feature = "normaliser"))]
            {
                None
            }
        };

        #[cfg(feature = "adverse-selection")]
        let adverse_selection = {
            #[cfg(feature = "normaliser")]
            {
                (s.adverse_selection && any_z).then(|| {
                    let w = config.adverse_selection_weights;
                    #[cfg(feature = "alloc")]
                    let weights =
                        Vec::from([w.ofi, w.vsi, w.depth_imbalance, w.microprice_deviation]);
                    #[cfg(not(feature = "alloc"))]
                    let weights = [w.ofi, w.vsi, w.depth_imbalance, w.microprice_deviation];
                    AdverseSelectionSignal::new(weights, config.composite_smoothing_half_life)
                        .expect("config validated by SignalEngineConfig::validate")
                })
            }
            #[cfg(not(feature = "normaliser"))]
            {
                None
            }
        };

        #[cfg(feature = "effective-spread")]
        let effective_spread = s.effective_spread.then(EffectiveSpread::new);

        #[cfg(feature = "trade-classifier")]
        let needs_classifier = {
            let mut needed = s.vsi;
            #[cfg(feature = "effective-spread")]
            {
                needed = needed || s.effective_spread;
            }
            needed
        };
        #[cfg(feature = "trade-classifier")]
        let classifier = needs_classifier.then(|| match config.classifier {
            ClassifierType::TickRule => Classifier::Tick(TickRuleClassifier::new()),
            ClassifierType::QuoteRule => Classifier::Quote(QuoteRuleClassifier::new()),
        });

        let engine = Self {
            #[cfg(feature = "ofi")]
            ofi,
            #[cfg(feature = "depth-imbalance")]
            depth_imbalance,
            #[cfg(feature = "microprice")]
            microprice,
            #[cfg(feature = "vsi")]
            vsi,
            #[cfg(all(feature = "ofi", feature = "normaliser"))]
            ofi_normaliser,
            #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
            depth_imbalance_normaliser,
            #[cfg(all(feature = "microprice", feature = "normaliser"))]
            microprice_deviation_normaliser,
            #[cfg(all(feature = "vsi", feature = "normaliser"))]
            vsi_normaliser,
            #[cfg(feature = "composite")]
            composite,
            #[cfg(feature = "adverse-selection")]
            adverse_selection,
            #[cfg(feature = "effective-spread")]
            effective_spread,
            #[cfg(feature = "trade-classifier")]
            classifier,
            #[cfg(not(feature = "alloc"))]
            last_book: None,
            current_snapshot: SignalSnapshot::default(),
        };
        Ok(engine)
    }

    /// Process a new order book snapshot and return the updated signal snapshot.
    ///
    /// Updates OFI, Depth Imbalance, Microprice, and their z-scores, then
    /// recomputes composite signals. VSI and Effective Spread are **not** updated here;
    /// call [`SignalEngine::on_trade`] for those.
    ///
    /// The returned reference is valid until the next call to any `&mut self` method.
    ///
    /// # Crossed and locked books
    ///
    /// Crossed books (bid > ask) and locked books (bid == ask) are accepted — they occur
    /// normally during pre-open auctions and mid-session on some venues. However, the
    /// signals computed from them are economically unreliable:
    ///
    /// - **OFI** treats the crossed BBO the same as any other update; the resulting
    ///   flow imbalance values have no directional meaning.
    /// - **Depth Imbalance** uses raw quantities which remain valid, but the sign
    ///   interpretation (buy/sell pressure) is ambiguous in a crossed market.
    /// - **Microprice** may equal or exceed the ask (or fall below the bid) because
    ///   the formula does not clip to `[bid, ask]` when the book is crossed.
    /// - **`spread`** is `NaN` for crossed books (bid > ask) and `0.0` for locked
    ///   books (bid == ask). A crossed book's `NaN` is indistinguishable from an
    ///   engine that has not yet received any book update; use
    ///   `snapshot.book_update_count > 0` to tell the two apart.
    ///
    /// Consider filtering or tagging snapshots produced during auction periods
    /// using `snapshot.spread` before acting on them.
    ///
    /// # Panics (debug builds only)
    ///
    /// Asserts that all book prices are finite and all quantities are positive.
    /// Crossed or locked books are **not** rejected. In release builds the assertion is elided.
    #[inline]
    pub fn on_book_update(&mut self, book: &BookSnapshot) -> &SignalSnapshot {
        debug_assert!(
            book.bids()
                .iter()
                .all(|p| p.price.is_finite() && p.quantity > 0.0)
                && book
                    .asks()
                    .iter()
                    .all(|p| p.price.is_finite() && p.quantity > 0.0),
            "on_book_update: BookSnapshot contains NaN/infinite price or non-positive quantity"
        );

        #[cfg(not(feature = "alloc"))]
        {
            self.last_book = Some(*book);
        }

        #[cfg(feature = "ofi")]
        {
            update_book_signal!(self, ofi, ofi, book);
        }
        #[cfg(not(feature = "ofi"))]
        {
            self.current_snapshot.ofi = f64::NAN;
        }

        #[cfg(feature = "depth-imbalance")]
        {
            update_book_signal!(self, depth_imbalance, depth_imbalance, book);
        }
        #[cfg(not(feature = "depth-imbalance"))]
        {
            self.current_snapshot.depth_imbalance = f64::NAN;
        }

        #[cfg(feature = "microprice")]
        {
            if let Some(mp) = &mut self.microprice {
                mp.update(book);
                self.current_snapshot.mid_price = mp.mid_price().unwrap_or(f64::NAN);
                self.current_snapshot.microprice = mp.microprice().unwrap_or(f64::NAN);
                self.current_snapshot.microprice_deviation = mp.deviation().unwrap_or(f64::NAN);
            } else {
                self.current_snapshot.mid_price = f64::NAN;
                self.current_snapshot.microprice = f64::NAN;
                self.current_snapshot.microprice_deviation = f64::NAN;
            }
        }
        #[cfg(not(feature = "microprice"))]
        {
            self.current_snapshot.mid_price = f64::NAN;
            self.current_snapshot.microprice = f64::NAN;
            self.current_snapshot.microprice_deviation = f64::NAN;
        }

        self.current_snapshot.spread = book.spread().filter(|&s| s >= 0.0).unwrap_or(f64::NAN);

        self.current_snapshot.last_book_update_ns = book.timestamp_ns;
        self.current_snapshot.book_update_count += 1;
        self.normalise_signals();
        self.update_composites();

        &self.current_snapshot
    }

    /// Process a trade event and return the updated signal snapshot.
    ///
    /// Classifies the trade side (tick rule or quote rule), updates VSI and Effective Spread,
    /// and recomputes composite signals if a new VSI bucket has completed.
    /// Book-based signals (OFI, Depth Imbalance, Microprice) are **not** updated here.
    ///
    /// `book` should be the most recent [`BookSnapshot`] received before this trade.
    /// Pass an empty `BookSnapshot` if no book update has been seen yet — VSI and
    /// Effective Spread will remain `NAN` until a valid book is available.
    ///
    /// The returned reference is valid until the next call to any `&mut self` method.
    #[inline]
    pub fn on_trade(&mut self, trade: &Trade, book: &BookSnapshot) -> &SignalSnapshot {
        #[cfg(feature = "trade-classifier")]
        let classified_side = self.classifier.as_mut().map(|classifier| match classifier {
            Classifier::Tick(c) => c.classify(trade),
            Classifier::Quote(c) => c.classify(trade, book),
        });

        let prev_buckets = self.current_snapshot.vsi_buckets_completed;

        #[cfg(all(feature = "vsi", feature = "trade-classifier"))]
        {
            if let (Some(side), Some(vsi)) = (classified_side, &mut self.vsi) {
                let classified = ClassifiedTrade {
                    trade: *trade,
                    side,
                };
                vsi.update(&classified);
                self.current_snapshot.vsi = vsi.value().unwrap_or(f64::NAN);
                self.current_snapshot.vsi_buckets_completed = vsi.buckets_completed() as u64;
                self.current_snapshot.vsi_capped_trades = vsi.capped_trade_count();
            } else {
                self.current_snapshot.vsi = f64::NAN;
            }
        }
        #[cfg(not(all(feature = "vsi", feature = "trade-classifier")))]
        {
            self.current_snapshot.vsi = f64::NAN;
        }

        #[cfg(all(feature = "vsi", feature = "normaliser"))]
        {
            if self.current_snapshot.vsi_buckets_completed > prev_buckets {
                normalise_signal!(self, vsi, vsi_z, vsi_normaliser);
            }
            self.current_snapshot.vsi_normaliser_ready =
                self.vsi_normaliser.as_ref().is_some_and(|n| n.is_ready());
            self.current_snapshot.vsi_normaliser_warmup_complete = self
                .vsi_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
        #[cfg(not(all(feature = "vsi", feature = "normaliser")))]
        {
            self.current_snapshot.vsi_z = f64::NAN;
            self.current_snapshot.vsi_normaliser_ready = false;
            self.current_snapshot.vsi_normaliser_warmup_complete = false;
        }

        #[cfg(all(feature = "effective-spread", feature = "trade-classifier"))]
        {
            if let (Some(side), Some(es)) = (classified_side, &mut self.effective_spread) {
                es.update(trade, book, side);
                self.current_snapshot.effective_spread = es.effective_spread().unwrap_or(f64::NAN);
                self.current_snapshot.relative_effective_spread =
                    es.relative_effective_spread().unwrap_or(f64::NAN);
            } else {
                self.current_snapshot.effective_spread = f64::NAN;
                self.current_snapshot.relative_effective_spread = f64::NAN;
            }
        }
        #[cfg(not(all(feature = "effective-spread", feature = "trade-classifier")))]
        {
            self.current_snapshot.effective_spread = f64::NAN;
            self.current_snapshot.relative_effective_spread = f64::NAN;
        }

        self.current_snapshot.last_trade_ns = trade.timestamp_ns;
        self.current_snapshot.trade_count += 1;
        self.refresh_book_normaliser_ready_flags();
        self.current_snapshot.normalisers_ready = self.all_normalisers_ready();

        if self.current_snapshot.vsi_buckets_completed > prev_buckets {
            self.update_composites();
        }

        &self.current_snapshot
    }

    /// Recompute the book-signal normaliser ready/warm-up flags from current normaliser
    /// state, without feeding them new data.
    ///
    /// These flags are only *updated* (fed new observations) in [`Self::on_book_update`],
    /// but their boolean readiness can be recomputed cheaply at any time. Called from
    /// [`Self::on_trade`] so `ofi_normaliser_ready` and friends are never more than
    /// momentarily stale relative to the snapshot they're read from.
    #[inline]
    fn refresh_book_normaliser_ready_flags(&mut self) {
        #[cfg(all(feature = "ofi", feature = "normaliser"))]
        {
            self.current_snapshot.ofi_normaliser_ready =
                self.ofi_normaliser.as_ref().is_some_and(|n| n.is_ready());
            self.current_snapshot.ofi_normaliser_warmup_complete = self
                .ofi_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
        #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
        {
            self.current_snapshot.depth_imbalance_normaliser_ready = self
                .depth_imbalance_normaliser
                .as_ref()
                .is_some_and(|n| n.is_ready());
            self.current_snapshot
                .depth_imbalance_normaliser_warmup_complete = self
                .depth_imbalance_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
        #[cfg(all(feature = "microprice", feature = "normaliser"))]
        {
            self.current_snapshot.microprice_deviation_normaliser_ready = self
                .microprice_deviation_normaliser
                .as_ref()
                .is_some_and(|n| n.is_ready());
            self.current_snapshot
                .microprice_deviation_normaliser_warmup_complete = self
                .microprice_deviation_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
    }

    /// Process a batch of [`MarketEvent`]s and return one `(timestamp_ns, snapshot)` per event.
    ///
    /// Equivalent to calling [`on_book_update`](Self::on_book_update) /
    /// [`on_trade`](Self::on_trade) in sequence, but allocates a single result `Vec`
    /// rather than exposing intermediate references. For large batches without allocation,
    /// prefer [`process_events_with`](Self::process_events_with).
    #[cfg(feature = "alloc")]
    pub fn process_events(&mut self, events: &[MarketEvent]) -> Vec<(u64, SignalSnapshot)> {
        let empty_book = BookSnapshot::new(&[], &[], 0);
        let mut results = Vec::with_capacity(events.len());
        let mut last_book: Option<&BookSnapshot> = None;

        for event in events {
            let (timestamp, snapshot) = match event {
                MarketEvent::BookUpdate(book) => {
                    last_book = Some(book);
                    let snap = *self.on_book_update(book);
                    (book.timestamp_ns, snap)
                }
                MarketEvent::Trade(trade) => {
                    let book = last_book.unwrap_or(&empty_book);
                    let snap = *self.on_trade(trade, book);
                    (trade.timestamp_ns, snap)
                }
            };
            results.push((timestamp, snapshot));
        }

        results
    }

    /// Process a batch of [`MarketEvent`]s, invoking `callback` with `(timestamp_ns, snapshot)`
    /// after each event without allocating a result collection.
    ///
    /// Prefer this over [`process_events`](Self::process_events) when processing large
    /// historical datasets where allocating a result `Vec` would be expensive.
    #[cfg(feature = "alloc")]
    pub fn process_events_with<F>(&mut self, events: &[MarketEvent], mut callback: F)
    where
        F: FnMut(u64, &SignalSnapshot),
    {
        let empty_book = BookSnapshot::new(&[], &[], 0);
        let mut last_book: Option<&BookSnapshot> = None;

        for event in events {
            match event {
                MarketEvent::BookUpdate(book) => {
                    last_book = Some(book);
                    let _ = self.on_book_update(book);
                    callback(book.timestamp_ns, &self.current_snapshot);
                }
                MarketEvent::Trade(trade) => {
                    let book = last_book.unwrap_or(&empty_book);
                    let _ = self.on_trade(trade, book);
                    callback(trade.timestamp_ns, &self.current_snapshot);
                }
            }
        }
    }

    /// Process a batch of [`MarketEvent`]s with a fallible callback.
    ///
    /// Like [`process_events_with`](Self::process_events_with) but the callback may return
    /// `Err(E)` to abort processing early. Returns `Ok(())` if all events were processed.
    #[cfg(feature = "alloc")]
    pub fn process_events_try_with<F, E>(
        &mut self,
        events: &[MarketEvent],
        mut callback: F,
    ) -> Result<(), E>
    where
        F: FnMut(u64, &SignalSnapshot) -> Result<(), E>,
    {
        let empty_book = BookSnapshot::new(&[], &[], 0);
        let mut last_book: Option<&BookSnapshot> = None;

        for event in events {
            match event {
                MarketEvent::BookUpdate(book) => {
                    last_book = Some(book);
                    let _ = self.on_book_update(book);
                    callback(book.timestamp_ns, &self.current_snapshot)?;
                }
                MarketEvent::Trade(trade) => {
                    let book = last_book.unwrap_or(&empty_book);
                    let _ = self.on_trade(trade, book);
                    callback(trade.timestamp_ns, &self.current_snapshot)?;
                }
            }
        }

        Ok(())
    }

    /// Process a single [`MarketEvent`] and return `(timestamp_ns, snapshot)` (no-alloc variant).
    ///
    /// In the `alloc` build, use [`process_events`](Self::process_events) or
    /// [`process_events_with`](Self::process_events_with) instead.
    #[cfg(not(feature = "alloc"))]
    pub fn process_event(&mut self, event: &MarketEvent) -> (u64, SignalSnapshot) {
        match event {
            MarketEvent::BookUpdate(book) => {
                let snap = *self.on_book_update(book);
                (book.timestamp_ns, snap)
            }
            MarketEvent::Trade(trade) => {
                let book = self.last_book.unwrap_or(BookSnapshot::new(&[], &[], 0));
                let snap = *self.on_trade(trade, &book);
                (trade.timestamp_ns, snap)
            }
        }
    }

    /// Return a reference to the most recently computed [`SignalSnapshot`].
    ///
    /// Equivalent to the return value of the last `on_book_update` / `on_trade` call.
    /// Useful when you hold the engine by reference and cannot borrow the return value directly.
    #[inline]
    pub fn snapshot(&self) -> &SignalSnapshot {
        &self.current_snapshot
    }

    /// Capture the current EWMA state of all active normalisers.
    ///
    /// The returned [`NormaliserSnapshot`] can be serialised (with the `serde` feature) and
    /// stored, then passed to [`restore_normaliser_state`](Self::restore_normaliser_state)
    /// when constructing a fresh engine after a process restart. This eliminates the need
    /// to replay historical data purely to re-warm the normalisers.
    ///
    /// Signal accumulators (OFI rolling window, VSI bucket, etc.) are not captured — they
    /// warm up quickly and are not worth the complexity of persisting. The normalisers are
    /// the only state with a long warm-up (tens to thousands of observations).
    #[cfg(feature = "normaliser")]
    pub fn capture_normaliser_state(&self) -> NormaliserSnapshot {
        #[inline]
        fn capture(n: &Option<SignalNormaliser>) -> Option<(f64, f64)> {
            n.as_ref().map(|n| (n.mean(), n.variance()))
        }

        NormaliserSnapshot {
            #[cfg(feature = "ofi")]
            ofi: capture(&self.ofi_normaliser),
            #[cfg(not(feature = "ofi"))]
            ofi: None,
            #[cfg(feature = "depth-imbalance")]
            depth_imbalance: capture(&self.depth_imbalance_normaliser),
            #[cfg(not(feature = "depth-imbalance"))]
            depth_imbalance: None,
            #[cfg(feature = "microprice")]
            microprice_deviation: capture(&self.microprice_deviation_normaliser),
            #[cfg(not(feature = "microprice"))]
            microprice_deviation: None,
            #[cfg(feature = "vsi")]
            vsi: capture(&self.vsi_normaliser),
            #[cfg(not(feature = "vsi"))]
            vsi: None,
        }
    }

    /// Restore normaliser EWMA state captured by [`capture_normaliser_state`](Self::capture_normaliser_state).
    ///
    /// For each active normaliser, if the snapshot contains a `Some((mean, variance))` for
    /// that normaliser, the normaliser is re-seeded with that state and its observation
    /// count is set to `warm_up` (so z-scores are available immediately if variance is
    /// sufficient). Fields that are `None` in the snapshot leave the corresponding
    /// normaliser unchanged.
    ///
    /// Call this immediately after constructing a new engine with the same config, before
    /// processing any live events.
    #[cfg(feature = "normaliser")]
    pub fn restore_normaliser_state(&mut self, snapshot: &NormaliserSnapshot) {
        #[inline]
        fn restore(n: &mut Option<SignalNormaliser>, state: Option<(f64, f64)>) {
            if let (Some(normaliser), Some((mean, variance))) = (n.as_mut(), state) {
                normaliser.seeded(mean, variance);
            }
        }

        #[cfg(feature = "ofi")]
        restore(&mut self.ofi_normaliser, snapshot.ofi);
        #[cfg(feature = "depth-imbalance")]
        restore(
            &mut self.depth_imbalance_normaliser,
            snapshot.depth_imbalance,
        );
        #[cfg(feature = "microprice")]
        restore(
            &mut self.microprice_deviation_normaliser,
            snapshot.microprice_deviation,
        );
        #[cfg(feature = "vsi")]
        restore(&mut self.vsi_normaliser, snapshot.vsi);
    }

    /// Reset the engine to its initial state, preserving configuration.
    ///
    /// Clears all signal accumulators (OFI ring buffer, depth imbalance, VSI buckets),
    /// normaliser EWMA state, classifier last-price state, and the current snapshot
    /// (including `book_update_count` and `vsi_buckets_completed`). The engine behaves
    /// identically to a freshly constructed one with the same config after this call.
    ///
    /// Use before replaying a historical dataset from the beginning, or when switching
    /// instruments.
    pub fn reset(&mut self) {
        #[cfg(not(feature = "alloc"))]
        {
            self.last_book = None;
        }
        self.current_snapshot = SignalSnapshot::default();

        #[cfg(feature = "ofi")]
        if let Some(ofi) = &self.ofi {
            let window = ofi.window_size();
            self.ofi = Some(match (ofi.is_normalised(), ofi.half_life()) {
                (true, Some(hl)) => Ofi::normalised_with_smoothing(window, hl)
                    .expect("config already validated at construction time"),
                (true, None) => {
                    Ofi::normalised(window).expect("config already validated at construction time")
                }
                (false, Some(hl)) => Ofi::with_smoothing(window, hl)
                    .expect("config already validated at construction time"),
                (false, None) => {
                    Ofi::new(window).expect("config already validated at construction time")
                }
            });
        }
        #[cfg(feature = "depth-imbalance")]
        if let Some(di) = &self.depth_imbalance {
            let levels = di.depth_levels();
            self.depth_imbalance = Some(match di.half_life() {
                Some(hl) => DepthImbalance::with_smoothing(levels, hl)
                    .expect("config already validated at construction time"),
                None => DepthImbalance::new(levels)
                    .expect("config already validated at construction time"),
            });
        }
        #[cfg(feature = "microprice")]
        if self.microprice.is_some() {
            self.microprice = Some(MicropriceCalculator::new());
        }
        #[cfg(feature = "vsi")]
        if let Some(vsi) = &self.vsi {
            let (bv, nb) = (vsi.bucket_volume(), vsi.n_buckets());
            self.vsi =
                Some(Vsi::new(bv, nb).expect("config already validated at construction time"));
        }

        #[cfg(feature = "effective-spread")]
        if self.effective_spread.is_some() {
            self.effective_spread = Some(EffectiveSpread::new());
        }

        #[cfg(all(feature = "ofi", feature = "normaliser"))]
        {
            self.ofi_normaliser = self.ofi_normaliser.as_ref().map(|n| n.fresh());
        }
        #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
        {
            self.depth_imbalance_normaliser =
                self.depth_imbalance_normaliser.as_ref().map(|n| n.fresh());
        }
        #[cfg(all(feature = "microprice", feature = "normaliser"))]
        {
            self.microprice_deviation_normaliser = self
                .microprice_deviation_normaliser
                .as_ref()
                .map(|n| n.fresh());
        }
        #[cfg(all(feature = "vsi", feature = "normaliser"))]
        {
            self.vsi_normaliser = self.vsi_normaliser.as_ref().map(|n| n.fresh());
        }

        #[cfg(feature = "composite")]
        if let Some(c) = &self.composite {
            self.composite = Some(c.fresh_copy());
        }
        #[cfg(feature = "adverse-selection")]
        if let Some(a) = &self.adverse_selection {
            self.adverse_selection = Some(a.fresh_copy());
        }

        #[cfg(feature = "trade-classifier")]
        if let Some(classifier) = &mut self.classifier {
            *classifier = match classifier {
                Classifier::Tick(_) => Classifier::Tick(TickRuleClassifier::new()),
                Classifier::Quote(_) => Classifier::Quote(QuoteRuleClassifier::new()),
            };
        }
    }

    #[inline]
    fn normalise_signals(&mut self) {
        #[cfg(all(feature = "ofi", feature = "normaliser"))]
        {
            normalise_signal!(self, ofi, ofi_z, ofi_normaliser);
            self.current_snapshot.ofi_normaliser_ready =
                self.ofi_normaliser.as_ref().is_some_and(|n| n.is_ready());
            self.current_snapshot.ofi_normaliser_warmup_complete = self
                .ofi_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
        #[cfg(not(all(feature = "ofi", feature = "normaliser")))]
        {
            self.current_snapshot.ofi_z = f64::NAN;
            self.current_snapshot.ofi_normaliser_ready = false;
            self.current_snapshot.ofi_normaliser_warmup_complete = false;
        }

        #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
        {
            normalise_signal!(
                self,
                depth_imbalance,
                depth_imbalance_z,
                depth_imbalance_normaliser
            );
            self.current_snapshot.depth_imbalance_normaliser_ready = self
                .depth_imbalance_normaliser
                .as_ref()
                .is_some_and(|n| n.is_ready());
            self.current_snapshot
                .depth_imbalance_normaliser_warmup_complete = self
                .depth_imbalance_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
        #[cfg(not(all(feature = "depth-imbalance", feature = "normaliser")))]
        {
            self.current_snapshot.depth_imbalance_z = f64::NAN;
            self.current_snapshot.depth_imbalance_normaliser_ready = false;
            self.current_snapshot
                .depth_imbalance_normaliser_warmup_complete = false;
        }

        #[cfg(all(feature = "microprice", feature = "normaliser"))]
        {
            normalise_signal!(
                self,
                microprice_deviation,
                microprice_deviation_z,
                microprice_deviation_normaliser
            );
            self.current_snapshot.microprice_deviation_normaliser_ready = self
                .microprice_deviation_normaliser
                .as_ref()
                .is_some_and(|n| n.is_ready());
            self.current_snapshot
                .microprice_deviation_normaliser_warmup_complete = self
                .microprice_deviation_normaliser
                .as_ref()
                .is_some_and(|n| n.is_warmup_complete());
        }
        #[cfg(not(all(feature = "microprice", feature = "normaliser")))]
        {
            self.current_snapshot.microprice_deviation_z = f64::NAN;
            self.current_snapshot.microprice_deviation_normaliser_ready = false;
            self.current_snapshot
                .microprice_deviation_normaliser_warmup_complete = false;
        }

        self.current_snapshot.normalisers_ready = self.all_normalisers_ready();
    }

    #[inline]
    fn all_normalisers_ready(&self) -> bool {
        #[cfg(all(feature = "ofi", feature = "normaliser"))]
        if let Some(n) = &self.ofi_normaliser {
            if !n.is_ready() {
                return false;
            }
        }
        #[cfg(all(feature = "depth-imbalance", feature = "normaliser"))]
        if let Some(n) = &self.depth_imbalance_normaliser {
            if !n.is_ready() {
                return false;
            }
        }
        #[cfg(all(feature = "microprice", feature = "normaliser"))]
        if let Some(n) = &self.microprice_deviation_normaliser {
            if !n.is_ready() {
                return false;
            }
        }
        #[cfg(all(feature = "vsi", feature = "normaliser"))]
        if let Some(n) = &self.vsi_normaliser {
            if !n.is_ready() {
                return false;
            }
        }
        true
    }

    #[inline]
    fn update_composites(&mut self) {
        let z_scores = [
            self.current_snapshot.ofi_z,
            self.current_snapshot.vsi_z,
            self.current_snapshot.depth_imbalance_z,
            self.current_snapshot.microprice_deviation_z,
        ];

        #[cfg(feature = "composite")]
        {
            update_composite!(self, composite, toxicity, &z_scores);
        }
        #[cfg(not(feature = "composite"))]
        {
            self.current_snapshot.toxicity = f64::NAN;
        }

        #[cfg(feature = "adverse-selection")]
        {
            update_composite!(self, adverse_selection, adverse_selection, &z_scores);
        }
        #[cfg(not(feature = "adverse-selection"))]
        {
            self.current_snapshot.adverse_selection = f64::NAN;
        }
    }
}

#[allow(dead_code)]
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        assert_send_sync::<SignalEngine>();
    }
};
