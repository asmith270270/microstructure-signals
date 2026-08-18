//! Core types: [`BookSnapshot`], [`Trade`], [`SignalSnapshot`], [`MarketEvent`], and related types.

/// Maximum number of price levels stored per side in no-std / no-alloc builds.
///
/// Feeds with deeper books must be truncated to this depth before constructing
/// a [`BookSnapshot`].
#[cfg(not(feature = "alloc"))]
pub const MAX_BOOK_DEPTH: usize = 10;

/// A single price level in the order book (price and resting quantity).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PriceLevel {
    /// Price of this level.
    pub price: f64,
    /// Resting quantity at this price level.
    pub quantity: f64,
}

/// Snapshot of the order book at a single point in time.
///
/// Bids must be sorted descending by price (best bid first).
/// Asks must be sorted ascending by price (best ask first).
/// Use [`BookSnapshot::is_valid`] to verify structure before feeding to the engine.
///
/// The engine calls `debug_assert!(book.is_valid())` on every
/// [`SignalEngine::on_book_update`](crate::engine::SignalEngine::on_book_update)
/// call, catching malformed books in debug builds without paying the cost in release.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BookSnapshot {
    /// Bid levels, best bid first (descending price order).
    pub bids: alloc::vec::Vec<PriceLevel>,
    /// Ask levels, best ask first (ascending price order).
    pub asks: alloc::vec::Vec<PriceLevel>,
    /// Nanosecond timestamp of this snapshot.
    pub timestamp_ns: u64,
}

/// Snapshot of the order book at a single point in time (no-alloc variant).
///
/// Bids must be sorted descending by price (best bid first).
/// Asks must be sorted ascending by price (best ask first).
/// Maximum depth is [`MAX_BOOK_DEPTH`] levels per side.
#[cfg(not(feature = "alloc"))]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BookSnapshot {
    /// Bid levels, best bid first (descending price order).
    pub bids: [PriceLevel; MAX_BOOK_DEPTH],
    /// Ask levels, best ask first (ascending price order).
    pub asks: [PriceLevel; MAX_BOOK_DEPTH],
    /// Number of valid entries in `bids`.
    pub bids_len: usize,
    /// Number of valid entries in `asks`.
    pub asks_len: usize,
    /// Nanosecond timestamp of this snapshot.
    pub timestamp_ns: u64,
}

impl BookSnapshot {
    /// Constructs a [`BookSnapshot`] from slices.
    #[cfg(feature = "alloc")]
    pub fn new(bids: &[PriceLevel], asks: &[PriceLevel], timestamp_ns: u64) -> Self {
        Self {
            bids: bids.to_vec(),
            asks: asks.to_vec(),
            timestamp_ns,
        }
    }

    /// Constructs a [`BookSnapshot`] from slices.
    ///
    /// The slices are copied into fixed arrays and must not exceed
    /// [`MAX_BOOK_DEPTH`] entries per side.
    #[cfg(not(feature = "alloc"))]
    pub fn new(bids: &[PriceLevel], asks: &[PriceLevel], timestamp_ns: u64) -> Self {
        assert!(
            bids.len() <= MAX_BOOK_DEPTH,
            "bids length {} exceeds MAX_BOOK_DEPTH ({})",
            bids.len(),
            MAX_BOOK_DEPTH
        );
        assert!(
            asks.len() <= MAX_BOOK_DEPTH,
            "asks length {} exceeds MAX_BOOK_DEPTH ({})",
            asks.len(),
            MAX_BOOK_DEPTH
        );
        let mut book = Self {
            bids: [PriceLevel {
                price: 0.0,
                quantity: 0.0,
            }; MAX_BOOK_DEPTH],
            asks: [PriceLevel {
                price: 0.0,
                quantity: 0.0,
            }; MAX_BOOK_DEPTH],
            bids_len: bids.len(),
            asks_len: asks.len(),
            timestamp_ns,
        };

        book.bids[..book.bids_len].copy_from_slice(bids);
        book.asks[..book.asks_len].copy_from_slice(asks);
        book
    }

    /// Returns the best bid level, or `None` if the bid side is empty.
    #[inline]
    #[cfg(feature = "alloc")]
    pub fn best_bid(&self) -> Option<&PriceLevel> {
        self.bids.first()
    }

    /// Returns the best bid level, or `None` if the bid side is empty.
    #[inline]
    #[cfg(not(feature = "alloc"))]
    pub fn best_bid(&self) -> Option<&PriceLevel> {
        if self.bids_len > 0 {
            Some(&self.bids[0])
        } else {
            None
        }
    }

    /// Returns the best ask level, or `None` if the ask side is empty.
    #[inline]
    #[cfg(feature = "alloc")]
    pub fn best_ask(&self) -> Option<&PriceLevel> {
        self.asks.first()
    }

    /// Returns the best ask level, or `None` if the ask side is empty.
    #[inline]
    #[cfg(not(feature = "alloc"))]
    pub fn best_ask(&self) -> Option<&PriceLevel> {
        if self.asks_len > 0 {
            Some(&self.asks[0])
        } else {
            None
        }
    }

    /// Arithmetic mid-price `(best_bid + best_ask) / 2`, or `None` if either side is empty.
    #[inline]
    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid.price + ask.price) / 2.0),
            _ => None,
        }
    }

    /// Quoted spread `best_ask - best_bid`, or `None` if either side is empty.
    ///
    /// A negative value indicates a crossed book.
    #[inline]
    pub fn spread(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask.price - bid.price),
            _ => None,
        }
    }

    /// Returns `true` if the book is structurally valid for signal computation.
    ///
    /// A valid book has:
    /// - At least one bid and one ask
    /// - Best bid strictly below best ask (not crossed)
    /// - Bids in strictly descending price order
    /// - Asks in strictly ascending price order
    /// - All quantities strictly positive
    ///
    /// Useful for pre-validating books before feeding them to [`SignalEngine`](crate::engine::SignalEngine).
    /// Note that the engine accepts crossed/locked books (which fail this check) as they
    /// occur normally during auctions; it only asserts finite prices and positive quantities.
    #[cfg(feature = "alloc")]
    pub fn is_valid(&self) -> bool {
        let (Some(best_bid), Some(best_ask)) = (self.best_bid(), self.best_ask()) else {
            return false;
        };

        if best_bid.price >= best_ask.price {
            return false;
        }

        self.bids.windows(2).all(|w| w[0].price > w[1].price)
            && self.asks.windows(2).all(|w| w[0].price < w[1].price)
            && self.bids.iter().all(|lvl| lvl.quantity > 0.0)
            && self.asks.iter().all(|lvl| lvl.quantity > 0.0)
    }

    /// Returns `true` if the book is structurally valid for signal computation.
    ///
    /// A valid book has:
    /// - At least one bid and one ask
    /// - Best bid strictly below best ask (not crossed)
    /// - Bids in strictly descending price order
    /// - Asks in strictly ascending price order
    /// - All quantities strictly positive
    ///
    /// Useful for pre-validating books before feeding them to [`SignalEngine`](crate::engine::SignalEngine).
    /// Note that the engine accepts crossed/locked books (which fail this check) as they
    /// occur normally during auctions; it only asserts finite prices and positive quantities.
    #[cfg(not(feature = "alloc"))]
    pub fn is_valid(&self) -> bool {
        let (Some(best_bid), Some(best_ask)) = (self.best_bid(), self.best_ask()) else {
            return false;
        };

        if best_bid.price >= best_ask.price {
            return false;
        }

        let bids = &self.bids[..self.bids_len];
        let asks = &self.asks[..self.asks_len];

        bids.windows(2).all(|w| w[0].price > w[1].price)
            && asks.windows(2).all(|w| w[0].price < w[1].price)
            && bids.iter().all(|lvl| lvl.quantity > 0.0)
            && asks.iter().all(|lvl| lvl.quantity > 0.0)
    }

    /// Bid levels as a slice (best bid first).
    #[inline]
    #[cfg(feature = "alloc")]
    pub fn bids(&self) -> &[PriceLevel] {
        &self.bids
    }

    /// Bid levels as a slice (best bid first).
    #[inline]
    #[cfg(not(feature = "alloc"))]
    pub fn bids(&self) -> &[PriceLevel] {
        &self.bids[..self.bids_len]
    }

    /// Ask levels as a slice (best ask first).
    #[inline]
    #[cfg(feature = "alloc")]
    pub fn asks(&self) -> &[PriceLevel] {
        &self.asks
    }

    /// Ask levels as a slice (best ask first).
    #[inline]
    #[cfg(not(feature = "alloc"))]
    pub fn asks(&self) -> &[PriceLevel] {
        &self.asks[..self.asks_len]
    }
}

/// A single trade (price, quantity, timestamp).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Trade {
    /// Execution price.
    pub price: f64,
    /// Executed quantity. Must be finite and positive.
    pub quantity: f64,
    /// Nanosecond timestamp of the trade.
    pub timestamp_ns: u64,
}

/// Direction of a trade (buyer-initiated or seller-initiated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TradeSide {
    /// Buyer-initiated (aggressive buy / market buy).
    Buy,
    /// Seller-initiated (aggressive sell / market sell).
    Sell,
}

/// A [`Trade`] with its classified [`TradeSide`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClassifiedTrade {
    /// The raw trade.
    pub trade: Trade,
    /// Classified direction.
    pub side: TradeSide,
}

/// Output of every [`SignalEngine`](crate::SignalEngine) call.
///
/// All signal fields use `f64::NAN` as a sentinel for "not yet available".
/// Check with `.is_nan()` before using any value.
///
/// ```
/// # use microstructure_signals::{SignalEngine, SignalEngineConfig};
/// # use microstructure_signals::types::{BookSnapshot, PriceLevel};
/// # let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
/// # let mut engine = SignalEngine::new(config).unwrap();
/// # let book = BookSnapshot::new(&[PriceLevel { price: 100.0, quantity: 50.0 }], &[PriceLevel { price: 100.1, quantity: 50.0 }], 0);
/// let snapshot = engine.on_book_update(&book);
///
/// if !snapshot.ofi.is_nan() {
///     println!("OFI: {:.1}", snapshot.ofi);
/// }
/// if !snapshot.toxicity.is_nan() {
///     println!("Toxicity z-score: {:.2}", snapshot.toxicity);
/// }
/// ```
///
/// [`SignalSnapshot`] implements `PartialEq` with NaN-equality semantics:
/// two NaN fields compare as equal (both "not available").
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalSnapshot {
    /// Raw Order Flow Imbalance (rolling window sum). `NAN` until two book snapshots received.
    ///
    /// See `docs/signals/ofi.md` for interpretation.
    pub ofi: f64,
    /// OFI z-score (standard deviations from EWMA mean). `NAN` during normaliser warm-up.
    pub ofi_z: f64,
    /// Depth Imbalance `(bid_vol - ask_vol) / (bid_vol + ask_vol)` in `[-1.0, 1.0]`.
    /// `NAN` when book is empty.
    ///
    /// See `docs/signals/depth_imbalance.md` for interpretation.
    pub depth_imbalance: f64,
    /// Depth Imbalance z-score. `NAN` during normaliser warm-up.
    pub depth_imbalance_z: f64,
    /// Arithmetic mid-price `(best_bid + best_ask) / 2`. `NAN` when book is empty.
    pub mid_price: f64,
    /// Quoted spread `best_ask - best_bid`. `NAN` when book is empty or spread is negative (crossed book).
    pub spread: f64,
    /// Volume-weighted microprice. `NAN` when book is empty or quantities are zero.
    ///
    /// See `docs/signals/microprice.md` for the formula and interpretation.
    pub microprice: f64,
    /// Microprice deviation from mid, normalised by the half-spread.
    /// Range approximately `[-1.0, 1.0]`. `NAN` when book is empty.
    pub microprice_deviation: f64,
    /// Microprice deviation z-score. `NAN` during normaliser warm-up.
    pub microprice_deviation_z: f64,
    /// Volume-Synchronised Imbalance in `[-1.0, 1.0]`. `NAN` until the first bucket completes.
    ///
    /// Updated only on [`SignalEngine::on_trade`](crate::engine::SignalEngine::on_trade) calls when a bucket closes.
    /// See `docs/signals/vsi.md`.
    pub vsi: f64,
    /// VSI z-score. `NAN` during normaliser warm-up.
    pub vsi_z: f64,
    /// Total number of VSI buckets that have completed since engine creation or last reset.
    pub vsi_buckets_completed: u64,
    /// Number of trades that spanned more than 100 bucket boundaries and were capped.
    ///
    /// A non-zero count means very large individual trades are underweighted in VSI.
    /// Consider increasing `vsi_bucket_volume` if this is non-zero in production.
    pub vsi_capped_trades: u64,
    /// Weighted composite of OFI, VSI, Depth Imbalance, and Microprice Deviation z-scores.
    /// `NAN` when all component z-scores are unavailable or when no z-scores are enabled.
    ///
    /// See `docs/signals/composite.md` for weights and interpretation.
    pub toxicity: f64,
    /// Adverse selection signal — same composite formula as `toxicity` with configurable weights.
    /// `NAN` when all component z-scores are unavailable.
    pub adverse_selection: f64,
    /// Effective spread for the most recent trade: `2 × sign × (trade_price − mid_price)`.
    /// `NAN` when no trade has occurred or the book had no valid mid at trade time.
    ///
    /// See `docs/signals/effective_spread.md`.
    pub effective_spread: f64,
    /// Relative effective spread: `effective_spread / mid_price`.
    /// `NAN` when `effective_spread` is `NAN` or `mid_price <= 0`.
    pub relative_effective_spread: f64,
    /// Nanosecond timestamp of the most recent book update (0 before any update).
    pub last_book_update_ns: u64,
    /// Nanosecond timestamp of the most recent trade (0 before any trade).
    pub last_trade_ns: u64,
    /// Total number of book updates processed since engine creation or last reset.
    pub book_update_count: u64,
    /// Total number of trades processed since engine creation or last reset.
    pub trade_count: u64,
    /// `true` when every enabled normaliser has passed warm-up and has sufficient variance.
    ///
    /// Safe guard before trusting composite signals: wait until this is `true`.
    pub normalisers_ready: bool,
    /// `true` when the OFI normaliser is ready (warm-up complete and variance above floor).
    pub ofi_normaliser_ready: bool,
    /// `true` when the Depth Imbalance normaliser is ready.
    pub depth_imbalance_normaliser_ready: bool,
    /// `true` when the Microprice Deviation normaliser is ready.
    pub microprice_deviation_normaliser_ready: bool,
    /// `true` when the VSI normaliser is ready.
    pub vsi_normaliser_ready: bool,
    /// `true` when the OFI normaliser has processed at least `normalisation_warm_up` observations,
    /// regardless of variance. Useful for distinguishing "not enough data" from "constant input".
    pub ofi_normaliser_warmup_complete: bool,
    /// `true` when the Depth Imbalance normaliser has processed enough observations.
    pub depth_imbalance_normaliser_warmup_complete: bool,
    /// `true` when the Microprice Deviation normaliser has processed enough observations.
    pub microprice_deviation_normaliser_warmup_complete: bool,
    /// `true` when the VSI normaliser has processed enough bucket observations.
    pub vsi_normaliser_warmup_complete: bool,
}

impl PartialEq for SignalSnapshot {
    fn eq(&self, other: &Self) -> bool {
        #[inline(always)]
        fn feq(a: f64, b: f64) -> bool {
            (a.is_nan() && b.is_nan()) || (a == b)
        }
        feq(self.ofi, other.ofi)
            && feq(self.ofi_z, other.ofi_z)
            && feq(self.depth_imbalance, other.depth_imbalance)
            && feq(self.depth_imbalance_z, other.depth_imbalance_z)
            && feq(self.mid_price, other.mid_price)
            && feq(self.spread, other.spread)
            && feq(self.microprice, other.microprice)
            && feq(self.microprice_deviation, other.microprice_deviation)
            && feq(self.microprice_deviation_z, other.microprice_deviation_z)
            && feq(self.vsi, other.vsi)
            && feq(self.vsi_z, other.vsi_z)
            && self.vsi_buckets_completed == other.vsi_buckets_completed
            && self.vsi_capped_trades == other.vsi_capped_trades
            && feq(self.toxicity, other.toxicity)
            && feq(self.adverse_selection, other.adverse_selection)
            && feq(self.effective_spread, other.effective_spread)
            && feq(
                self.relative_effective_spread,
                other.relative_effective_spread,
            )
            && self.last_book_update_ns == other.last_book_update_ns
            && self.last_trade_ns == other.last_trade_ns
            && self.book_update_count == other.book_update_count
            && self.trade_count == other.trade_count
            && self.normalisers_ready == other.normalisers_ready
            && self.ofi_normaliser_ready == other.ofi_normaliser_ready
            && self.depth_imbalance_normaliser_ready == other.depth_imbalance_normaliser_ready
            && self.microprice_deviation_normaliser_ready
                == other.microprice_deviation_normaliser_ready
            && self.vsi_normaliser_ready == other.vsi_normaliser_ready
            && self.ofi_normaliser_warmup_complete == other.ofi_normaliser_warmup_complete
            && self.depth_imbalance_normaliser_warmup_complete
                == other.depth_imbalance_normaliser_warmup_complete
            && self.microprice_deviation_normaliser_warmup_complete
                == other.microprice_deviation_normaliser_warmup_complete
            && self.vsi_normaliser_warmup_complete == other.vsi_normaliser_warmup_complete
    }
}

impl SignalSnapshot {
    /// Returns `true` if the snapshot is stale relative to `current_ns`.
    ///
    /// A snapshot is considered stale if:
    /// - No book update has ever been received (`book_update_count == 0`), or
    /// - More than `max_staleness_ns` nanoseconds have elapsed since the last book update.
    #[inline]
    pub fn is_stale(&self, current_ns: u64, max_staleness_ns: u64) -> bool {
        self.book_update_count == 0
            || current_ns.saturating_sub(self.last_book_update_ns) > max_staleness_ns
    }
}

impl Default for SignalSnapshot {
    fn default() -> Self {
        Self {
            ofi: f64::NAN,
            ofi_z: f64::NAN,
            depth_imbalance: f64::NAN,
            depth_imbalance_z: f64::NAN,
            mid_price: f64::NAN,
            spread: f64::NAN,
            microprice: f64::NAN,
            microprice_deviation: f64::NAN,
            microprice_deviation_z: f64::NAN,
            vsi: f64::NAN,
            vsi_z: f64::NAN,
            vsi_buckets_completed: 0,
            vsi_capped_trades: 0,
            toxicity: f64::NAN,
            adverse_selection: f64::NAN,
            effective_spread: f64::NAN,
            relative_effective_spread: f64::NAN,
            last_book_update_ns: 0,
            last_trade_ns: 0,
            book_update_count: 0,
            trade_count: 0,
            normalisers_ready: false,
            ofi_normaliser_ready: false,
            depth_imbalance_normaliser_ready: false,
            microprice_deviation_normaliser_ready: false,
            vsi_normaliser_ready: false,
            ofi_normaliser_warmup_complete: false,
            depth_imbalance_normaliser_warmup_complete: false,
            microprice_deviation_normaliser_warmup_complete: false,
            vsi_normaliser_warmup_complete: false,
        }
    }
}

/// A market event: either a book update or a trade.
///
/// Used with [`SignalEngine::process_events`](crate::engine::SignalEngine::process_events) for batch processing.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq)]
pub enum MarketEvent {
    /// An order book snapshot update.
    BookUpdate(BookSnapshot),
    /// An executed trade.
    Trade(Trade),
}

/// A market event: either a book update or a trade.
///
/// Used with [`SignalEngine::process_event`](crate::engine::SignalEngine::process_event) for single-event processing.
#[cfg(not(feature = "alloc"))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarketEvent {
    /// An order book snapshot update.
    BookUpdate(BookSnapshot),
    /// An executed trade.
    Trade(Trade),
}

impl MarketEvent {
    /// Returns the nanosecond timestamp of the event.
    #[inline]
    pub fn timestamp_ns(&self) -> u64 {
        match self {
            MarketEvent::BookUpdate(book) => book.timestamp_ns,
            MarketEvent::Trade(trade) => trade.timestamp_ns,
        }
    }

    /// Returns `true` if this event is a book update.
    #[inline]
    pub fn is_book_update(&self) -> bool {
        matches!(self, MarketEvent::BookUpdate(_))
    }

    /// Returns `true` if this event is a trade.
    #[inline]
    pub fn is_trade(&self) -> bool {
        matches!(self, MarketEvent::Trade(_))
    }
}
