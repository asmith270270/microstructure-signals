//! [`ConfigError`] returned by all constructors that validate their parameters.

use core::fmt;

/// Error returned when a signal component, normaliser, or engine is constructed
/// with invalid parameters.
///
/// Constructors that validate their parameters return `Result<T, ConfigError>`,
/// letting callers handle misconfigured engines without `catch_unwind`.
///
/// # Example
///
/// ```
/// use microstructure_signals::{EwmaNormaliser, ConfigError};
///
/// let err = EwmaNormaliser::new(0.0, 50).unwrap_err();
/// assert!(matches!(err, ConfigError::HalfLifeInvalid(_)));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ConfigError {
    /// A half-life parameter must be finite and positive.
    HalfLifeInvalid(f64),
    /// A window size, depth level, bucket count, or warm-up count must be > 0.
    ZeroSizeParameter(&'static str),
    /// A bucket volume or ADV fraction must be finite and positive.
    VolumeInvalid(f64),
    /// A decay-per-level parameter must be finite and non-negative.
    DecayInvalid(f64),
    /// A composite weight was not finite.
    WeightNotFinite {
        /// Zero-based index of the offending weight.
        index: usize,
        /// The non-finite value that was supplied.
        value: f64,
    },
    /// All composite weights are zero; at least one non-zero weight is required.
    AllWeightsZero,
    /// `fast_half_life` must be strictly less than `base_half_life`.
    FastHalfLifeNotLessThanBase {
        /// The fast half-life that was supplied.
        fast: f64,
        /// The base half-life that was supplied.
        base: f64,
    },
    /// A regime detection threshold must be finite and positive.
    RegimeThresholdInvalid(f64),
    /// The cooldown period must be greater than zero.
    CooldownPeriodZero,
    /// The seeded variance must be finite and non-negative.
    VarianceInvalid(f64),
    /// An exit hysteresis value must be finite and positive.
    ExitHysteresisInvalid(f64),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::HalfLifeInvalid(v) => {
                write!(f, "half_life must be finite and positive, got {v}")
            }
            ConfigError::ZeroSizeParameter(name) => write!(f, "{name} must be greater than zero"),
            ConfigError::VolumeInvalid(v) => {
                write!(f, "volume must be finite and positive, got {v}")
            }
            ConfigError::DecayInvalid(v) => write!(
                f,
                "decay_per_level must be finite and non-negative, got {v}"
            ),
            ConfigError::WeightNotFinite { index, value } => {
                write!(f, "weight[{index}] must be finite, got {value}")
            }
            ConfigError::AllWeightsZero => {
                write!(f, "at least one composite weight must be non-zero")
            }
            ConfigError::FastHalfLifeNotLessThanBase { fast, base } => write!(
                f,
                "fast_half_life ({fast}) must be less than base_half_life ({base})"
            ),
            ConfigError::RegimeThresholdInvalid(v) => {
                write!(f, "regime_threshold must be finite and positive, got {v}")
            }
            ConfigError::CooldownPeriodZero => {
                write!(f, "cooldown_period must be greater than zero")
            }
            ConfigError::VarianceInvalid(v) => write!(
                f,
                "seeded variance must be finite and non-negative, got {v}"
            ),
            ConfigError::ExitHysteresisInvalid(v) => {
                write!(f, "exit_hysteresis must be finite and positive, got {v}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ConfigError {}
