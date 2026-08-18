//! Streaming microstructure signal engine for order flow toxicity and adverse selection risk.
//!
//! # Quick Start
//!
//! ```
//! use microstructure_signals::{SignalEngine, SignalEngineConfig};
//! use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade};
//!
//! let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
//! let mut engine = SignalEngine::new(config).unwrap();
//!
//! let book = BookSnapshot::new(
//!     &[PriceLevel { price: 100.0, quantity: 50.0 }],
//!     &[PriceLevel { price: 100.1, quantity: 50.0 }],
//!     0,
//! );
//! let snapshot = engine.on_book_update(&book);
//! ```
//!
//! # Signal Overview
//!
//! | Signal | Field | Updated by | Feature flag |
//! |--------|-------|-----------|--------------|
//! | Order Flow Imbalance | `ofi`, `ofi_z` | `on_book_update` | `ofi` |
//! | Depth Imbalance | `depth_imbalance`, `depth_imbalance_z` | `on_book_update` | `depth-imbalance` |
//! | Microprice | `microprice`, `microprice_deviation` | `on_book_update` | `microprice` |
//! | Volume-Sync Imbalance | `vsi`, `vsi_z` | `on_trade` | `vsi` |
//! | Composite Toxicity | `toxicity` | `on_book_update` / `on_trade` | `composite` |
//! | Adverse Selection | `adverse_selection` | `on_book_update` / `on_trade` | `adverse-selection` |
//! | Effective Spread | `effective_spread` | `on_trade` | `effective-spread` |
//!
//! # Missing Values
//!
//! All signal fields in [`SignalSnapshot`] use `f64::NAN` as a sentinel for "not yet available".
//! Check availability with `.is_nan()` — never with `Option` methods.
//!
//! ```
//! # use microstructure_signals::{SignalEngine, SignalEngineConfig};
//! # use microstructure_signals::types::{BookSnapshot, PriceLevel};
//! # let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
//! # let mut engine = SignalEngine::new(config).unwrap();
//! # let book = BookSnapshot::new(&[PriceLevel { price: 100.0, quantity: 50.0 }], &[PriceLevel { price: 100.1, quantity: 50.0 }], 0);
//! let snapshot = engine.on_book_update(&book);
//!
//! if !snapshot.ofi.is_nan() {
//!     // safe to use snapshot.ofi
//! }
//! ```
//!
//! # Threading
//!
//! [`SignalEngine`] is `Send + Sync` but is **not** designed for concurrent mutation.
//! The expected model is a single writer thread calling `on_book_update` / `on_trade`,
//! with read-only access to the returned [`SignalSnapshot`] shared across threads.
//! Concurrent calls to any `&mut self` method on the same engine are a data race.
//!
//! # Feature Flags
//!
//! Individual signals can be excluded at compile time:
//!
//! ```toml
//! [dependencies]
//! microstructure-signals = { version = "*", default-features = false, features = ["ofi", "depth-imbalance"] }
//! ```
//!
//! See `Cargo.toml` for the full feature list. The `minimal` feature enables only `ofi`
//! and `depth-imbalance`. The `all-signals` feature enables everything.
//!
//! # Performance
//!
//! Benchmarked throughput figures assume a release build with `target-cpu=native`.
//! Add a `.cargo/config.toml` at the workspace root:
//!
//! ```toml
//! [build]
//! rustflags = ["-C", "target-cpu=native"]
//! ```
//!
//! Without this, release builds may be 40–70% slower than the documented figures.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "adverse-selection")]
pub mod adverse_selection;
#[cfg(feature = "composite")]
pub mod composite;
#[cfg(feature = "depth-imbalance")]
pub mod depth_imbalance;
#[cfg(feature = "effective-spread")]
pub mod effective_spread;
pub mod engine;
#[cfg(feature = "microprice")]
pub mod microprice;
#[cfg(feature = "normaliser")]
pub mod normaliser;
#[cfg(feature = "ofi")]
pub mod ofi;
#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
pub mod regime_normaliser;
#[cfg(feature = "trade-classifier")]
pub mod trade_classifier;
pub mod types;
#[cfg(feature = "vsi")]
pub mod vsi;

pub mod config_error;
mod ewma;
mod math;
mod ring_buffer;

#[cfg(feature = "adverse-selection")]
pub use adverse_selection::AdverseSelectionSignal;
#[cfg(feature = "composite")]
pub use composite::CompositeToxicity;
pub use config_error::ConfigError;
#[cfg(feature = "depth-imbalance")]
pub use depth_imbalance::DepthImbalance;
#[cfg(feature = "effective-spread")]
pub use effective_spread::EffectiveSpread;
#[cfg(feature = "normaliser")]
pub use engine::NormaliserSnapshot;
#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
pub use engine::RegimeNormaliserParams;
pub use engine::{
    ClassifierType, CompositeWeights, SignalEngine, SignalEngineConfig, SignalSelection,
};
#[cfg(feature = "microprice")]
pub use microprice::MicropriceCalculator;
#[cfg(feature = "normaliser")]
pub use normaliser::EwmaNormaliser;
#[cfg(feature = "ofi")]
pub use ofi::{MultiLevelOfi, Ofi};
#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
pub use regime_normaliser::RegimeNormaliser;
#[cfg(feature = "trade-classifier")]
pub use trade_classifier::{QuoteRuleClassifier, TickRuleClassifier};
#[cfg(not(feature = "alloc"))]
pub use types::MAX_BOOK_DEPTH;
pub use types::{
    BookSnapshot, ClassifiedTrade, MarketEvent, PriceLevel, SignalSnapshot, Trade, TradeSide,
};
#[cfg(feature = "vsi")]
pub use vsi::Vsi;
