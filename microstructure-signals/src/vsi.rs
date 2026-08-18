//! Volume-Synchronised Imbalance (VSI) signal. See [`Vsi`] and `docs/signals/vsi.md`.

use crate::config_error::ConfigError;
use crate::ring_buffer::RingBuffer;
use crate::types::{ClassifiedTrade, TradeSide};

const MAX_BUCKETS_PER_TRADE: usize = 100;

/// Volume-Synchronised Imbalance (VSI) signal.
///
/// Partitions trade flow into fixed-volume buckets and computes the mean
/// buy/sell imbalance across the rolling window of completed buckets.
/// Output is in `[-1, 1]`. Positive values indicate buy-side dominance.
///
/// See `docs/signals/vsi.md` for bucket sizing guidance and interpretation.
#[derive(Debug, Clone)]
#[must_use]
pub struct Vsi {
    bucket_volume: f64,
    buffer: RingBuffer,
    current_buy_volume: f64,
    current_sell_volume: f64,
    current_total_volume: f64,
    buckets_completed_count: usize,
    capped_trades: u64,
}

impl Vsi {
    /// Create a new VSI calculator.
    ///
    /// `bucket_volume` is the total traded volume that closes a bucket.
    /// `n_buckets` is the rolling window length (number of completed buckets).
    ///
    /// # Errors
    ///
    /// Returns `Err(ConfigError::VolumeInvalid)` if `bucket_volume` is not finite and positive,
    /// or `Err(ConfigError::ZeroSizeParameter)` if `n_buckets` is zero.
    pub fn new(bucket_volume: f64, n_buckets: usize) -> Result<Self, ConfigError> {
        if !(bucket_volume > 0.0 && bucket_volume.is_finite()) {
            return Err(ConfigError::VolumeInvalid(bucket_volume));
        }
        if n_buckets == 0 {
            return Err(ConfigError::ZeroSizeParameter("n_buckets"));
        }
        Ok(Self {
            bucket_volume,
            buffer: RingBuffer::new(n_buckets),
            current_buy_volume: 0.0,
            current_sell_volume: 0.0,
            current_total_volume: 0.0,
            buckets_completed_count: 0,
            capped_trades: 0,
        })
    }

    /// Process a classified trade and update the VSI value.
    ///
    /// A single trade may close zero or more buckets. If a very large trade would close
    /// more than `MAX_BUCKETS_PER_TRADE` buckets, the remaining volume is placed into the
    /// current open bucket without closing it, and [`capped_trade_count`](Self::capped_trade_count)
    /// is incremented.
    ///
    /// # Panics
    ///
    /// Panics if `trade.quantity` is not finite and positive.
    #[inline]
    pub fn update(&mut self, trade: &ClassifiedTrade) {
        assert!(
            trade.trade.quantity > 0.0 && trade.trade.quantity.is_finite(),
            "trade quantity must be finite and positive, got {}",
            trade.trade.quantity
        );
        let mut remaining_qty = trade.trade.quantity;
        let side = trade.side;
        let mut buckets_closed = 0;

        while remaining_qty > 0.0 {
            let space_in_bucket = self.bucket_volume - self.current_total_volume;

            if remaining_qty >= space_in_bucket {
                match side {
                    TradeSide::Buy => self.current_buy_volume += space_in_bucket,
                    TradeSide::Sell => self.current_sell_volume += space_in_bucket,
                }
                self.current_total_volume = self.bucket_volume;
                remaining_qty -= space_in_bucket;

                self.close_bucket();
                buckets_closed += 1;

                if buckets_closed >= MAX_BUCKETS_PER_TRADE && remaining_qty > 0.0 {
                    self.capped_trades += 1;
                    match side {
                        TradeSide::Buy => self.current_buy_volume += remaining_qty,
                        TradeSide::Sell => self.current_sell_volume += remaining_qty,
                    }
                    self.current_total_volume += remaining_qty;
                    break;
                }
            } else {
                match side {
                    TradeSide::Buy => self.current_buy_volume += remaining_qty,
                    TradeSide::Sell => self.current_sell_volume += remaining_qty,
                }
                self.current_total_volume += remaining_qty;
                remaining_qty = 0.0;
            }
        }
    }

    fn close_bucket(&mut self) {
        let total = self.current_buy_volume + self.current_sell_volume;
        let imbalance = if total > 0.0 {
            (self.current_buy_volume - self.current_sell_volume) / total
        } else {
            0.0
        };

        self.buffer.push(imbalance);
        self.buckets_completed_count += 1;

        self.current_buy_volume = 0.0;
        self.current_sell_volume = 0.0;
        self.current_total_volume = 0.0;
    }

    /// Mean imbalance across the rolling window of completed buckets, or `None` if no bucket
    /// has completed yet.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        self.buffer.mean()
    }

    /// Total number of buckets completed since construction or last reset.
    #[inline]
    pub fn buckets_completed(&self) -> usize {
        self.buckets_completed_count
    }

    /// Number of trades whose volume exceeded `MAX_BUCKETS_PER_TRADE * bucket_volume`.
    ///
    /// A non-zero value indicates that very large block trades were partially absorbed without
    /// closing the expected number of buckets. See `docs/signals/vsi.md` for handling guidance.
    #[inline]
    pub fn capped_trade_count(&self) -> u64 {
        self.capped_trades
    }

    /// The volume threshold that closes a single bucket.
    #[inline]
    pub fn bucket_volume(&self) -> f64 {
        self.bucket_volume
    }

    /// The rolling window length (maximum number of completed buckets retained).
    #[inline]
    pub fn n_buckets(&self) -> usize {
        self.buffer.capacity()
    }
}
