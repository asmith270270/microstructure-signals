//! `std`/no_std dispatch for float functions not available in `core` at this crate's MSRV.
//!
//! `f64::exp`, `f64::ln`, and `f64::sqrt` require actual libm transcendental routines and
//! remain `std`-only as of this crate's `rust-version = "1.84"` MSRV (tracked by the
//! still-unstable `core_float_math`, rust-lang/rust#137578) — `core` has no libm and cannot
//! provide them. In `no_std` builds these dispatch to the [`libm`] crate instead, so
//! half-life-to-decay conversion (used by every EWMA-based signal), exponential level
//! weighting (used by [`crate::ofi::MultiLevelOfi`]), and composite weight normalisation all
//! work identically with or without `std`.
//!
//! `f64::abs` needs no libm — it's a single hardware instruction — and has been available in
//! `core` since Rust 1.84, so it's called directly rather than routed through this module.

#[cfg(feature = "std")]
#[inline]
pub(crate) fn exp(x: f64) -> f64 {
    x.exp()
}

#[cfg(feature = "std")]
#[inline]
pub(crate) fn ln(x: f64) -> f64 {
    x.ln()
}

#[cfg(feature = "std")]
#[inline]
pub(crate) fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[cfg(not(feature = "std"))]
#[inline]
pub(crate) fn exp(x: f64) -> f64 {
    libm::exp(x)
}

#[cfg(not(feature = "std"))]
#[inline]
pub(crate) fn ln(x: f64) -> f64 {
    libm::log(x)
}

#[cfg(not(feature = "std"))]
#[inline]
pub(crate) fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}
