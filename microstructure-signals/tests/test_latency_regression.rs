//! Latency regression tests.
//!
//! Run with:
//!   cargo test --release latency_regression -- --nocapture
//!
//! Each test measures p50 and p99 latency for a key operation and asserts it
//! stays within a hard ceiling. The ceilings are intentionally conservative
//! (~5–8× the documented native-build figures) so the tests pass on any modern
//! x86-64 machine regardless of whether `target-cpu=native` is set.
//!
//! # Updating limits after an intentional performance change
//!
//! 1. Run the benchmarks with `cargo bench --release`.
//! 2. Note the new steady-state figures in `docs/BENCHMARKS.md`.
//! 3. Update the `LIMIT_*` constants below (keep the ~5× safety margin).

use std::time::Instant;

use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade};
use microstructure_signals::{
    ClassifierType, DepthImbalance, EwmaNormaliser, MultiLevelOfi, Ofi, RegimeNormaliser,
    RegimeNormaliserParams, SignalEngine, SignalEngineConfig, SignalSelection,
};

const LIMIT_BOOK_RAW_P50: u64 = 200;
const LIMIT_BOOK_NORMALISED_P50: u64 = 350;
const LIMIT_BOOK_ALL_SIGNALS_P50: u64 = 500;
const LIMIT_TRADE_NO_VSI_P50: u64 = 300;
const LIMIT_TRADE_WITH_VSI_P50: u64 = 400;
const LIMIT_OFI_COMPONENT_P50: u64 = 100;
const LIMIT_MULTI_OFI_P50: u64 = 150;
const LIMIT_DI_COMPONENT_P50: u64 = 80;
const LIMIT_EWMA_NORM_P50: u64 = 100;
const LIMIT_REGIME_NORM_P50: u64 = 150;
const LIMIT_STATE_CAPTURE_P50: u64 = 80;
const LIMIT_STATE_RESTORE_P50: u64 = 80;
const LIMIT_STATE_COLD_START_P50: u64 = 2500;

const P99_MULTIPLIER: u64 = 10;

/// Measure the latency of a closure over `measure` iterations after `warmup`.
///
/// To amortise the ~20 ns `Instant` overhead, each sample times `BATCH`
/// consecutive calls and divides by `BATCH`. Returns `(p50, p99)` in nanoseconds.
const BATCH: usize = 50;

fn measure_ns<F: FnMut()>(warmup: usize, measure: usize, mut f: F) -> (u64, u64) {
    for _ in 0..(warmup * BATCH) {
        f();
    }
    let mut samples: Vec<u64> = (0..measure)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..BATCH {
                f();
            }
            (t.elapsed().as_nanos() as u64) / BATCH as u64
        })
        .collect();
    samples.sort_unstable();
    let n = samples.len();
    let p50 = samples[n / 2];
    let p99 = samples[n * 99 / 100];
    (p50, p99)
}

fn assert_latency(label: &str, p50: u64, p99: u64, limit_p50: u64) {
    let limit_p99 = limit_p50 * P99_MULTIPLIER;
    eprintln!(
        "  {label:<42}  p50 = {p50:>5} ns   p99 = {p99:>6} ns   \
         (limits: {limit_p50} / {limit_p99})",
    );
    assert!(
        p50 <= limit_p50,
        "{label}: p50 {p50} ns exceeded limit {limit_p50} ns"
    );
    assert!(
        p99 <= limit_p99,
        "{label}: p99 {p99} ns exceeded limit {limit_p99} ns"
    );
}

fn lcg(s: &mut u64) -> f64 {
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*s >> 11) as f64) * (1.0 / (1u64 << 53) as f64)
}

fn gen_book(depth: usize, s: &mut u64, ts: u64) -> BookSnapshot {
    let mid = 100.0 + (lcg(s) - 0.5) * 0.4;
    let half = 0.05 + lcg(s) * 0.05;
    let bids: Vec<PriceLevel> = (0..depth)
        .map(|k| PriceLevel {
            price: mid - half - k as f64 * 0.05,
            quantity: 100.0 + lcg(s) * 900.0,
        })
        .collect();
    let asks: Vec<PriceLevel> = (0..depth)
        .map(|k| PriceLevel {
            price: mid + half + k as f64 * 0.05,
            quantity: 100.0 + lcg(s) * 900.0,
        })
        .collect();
    BookSnapshot::new(&bids, &asks, ts)
}

fn gen_books(depth: usize, n: usize) -> Vec<BookSnapshot> {
    let mut s = 0xdeadbeef_u64;
    (0..n).map(|i| gen_book(depth, &mut s, i as u64)).collect()
}

fn gen_trade(s: &mut u64, bid: f64, ask: f64, ts: u64) -> Trade {
    Trade {
        price: bid + (ask - bid) * lcg(s),
        quantity: 10.0 + lcg(s) * 90.0,
        timestamp_ns: ts,
    }
}

fn gen_f64_values(n: usize) -> Vec<f64> {
    let mut s = 0xf00d_u64;
    (0..n).map(|_| (lcg(&mut s) - 0.5) * 1000.0).collect()
}

fn cfg_raw_only() -> SignalEngineConfig {
    let mut cfg = SignalEngineConfig::default();
    cfg.signals = SignalSelection::raw_only();
    cfg.signals.vsi = false;
    cfg.signals.vsi_z = false;
    cfg
}

fn cfg_no_composite() -> SignalEngineConfig {
    let mut cfg = SignalEngineConfig::default();
    cfg.signals.composite_toxicity = false;
    cfg.signals.adverse_selection = false;
    cfg
}

#[test]
fn latency_regression_book_update() {
    eprintln!("\n=== book update latency ===");

    let books = gen_books(5, 1024);
    let warm = gen_books(5, 512);

    {
        let mut engine = SignalEngine::new(cfg_raw_only()).unwrap();
        for b in &warm {
            engine.on_book_update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let snap = engine.on_book_update(&books[idx % books.len()]);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        });
        assert_latency("book_update/raw_only", p50, p99, LIMIT_BOOK_RAW_P50);
    }

    {
        let mut engine = SignalEngine::new(cfg_no_composite()).unwrap();
        for b in &warm {
            engine.on_book_update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let snap = engine.on_book_update(&books[idx % books.len()]);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        });
        assert_latency(
            "book_update/normalised_no_composite",
            p50,
            p99,
            LIMIT_BOOK_NORMALISED_P50,
        );
    }

    {
        let mut engine = SignalEngine::new(SignalEngineConfig::default()).unwrap();
        for b in &warm {
            engine.on_book_update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let snap = engine.on_book_update(&books[idx % books.len()]);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        });
        assert_latency(
            "book_update/all_signals",
            p50,
            p99,
            LIMIT_BOOK_ALL_SIGNALS_P50,
        );
    }
}

#[test]
fn latency_regression_book_depth() {
    eprintln!("\n=== book depth scaling ===");

    let depth_limits: &[(usize, u64)] = &[
        (1, LIMIT_BOOK_RAW_P50),
        (5, LIMIT_BOOK_RAW_P50),
        (10, LIMIT_BOOK_RAW_P50 * 2),
        (20, LIMIT_BOOK_RAW_P50 * 4),
    ];
    for &(depth, limit) in depth_limits {
        let books = gen_books(depth, 1024);
        let warm = gen_books(depth, 512);
        let mut cfg = cfg_raw_only();
        cfg.depth_levels = depth;
        let mut engine = SignalEngine::new(cfg).unwrap();
        for b in &warm {
            engine.on_book_update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let snap = engine.on_book_update(&books[idx % books.len()]);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        });
        assert_latency(&format!("book_update/depth_{depth}"), p50, p99, limit);
    }
}

#[test]
fn latency_regression_trade_path() {
    eprintln!("\n=== trade path latency ===");

    let mut s = 0xc0ffee_u64;
    let depth = 5;
    let warm_books: Vec<BookSnapshot> = (0..300).map(|i| gen_book(depth, &mut s, i)).collect();
    let data_books: Vec<BookSnapshot> =
        (0..512).map(|i| gen_book(depth, &mut s, 300 + i)).collect();
    let bid = data_books[0].bids()[0].price;
    let ask = data_books[0].asks()[0].price;
    let trades: Vec<Trade> = (0..512)
        .map(|i| gen_trade(&mut s, bid, ask, 812 + i))
        .collect();

    {
        let mut cfg = SignalEngineConfig::default();
        cfg.classifier = ClassifierType::QuoteRule;
        cfg.signals.vsi = false;
        cfg.signals.vsi_z = false;
        let mut engine = SignalEngine::new(cfg).unwrap();
        for b in &warm_books {
            engine.on_book_update(b);
        }
        let anchor = warm_books.last().unwrap();
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let snap = engine.on_trade(&trades[idx % trades.len()], anchor);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        });
        assert_latency(
            "on_trade/quote_rule_no_vsi",
            p50,
            p99,
            LIMIT_TRADE_NO_VSI_P50,
        );
    }

    {
        let config = SignalEngineConfig::with_vsi_bucket_volume(500.0).unwrap();
        let mut engine = SignalEngine::new(config).unwrap();
        for b in &warm_books {
            engine.on_book_update(b);
        }
        let anchor = warm_books.last().unwrap();
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let snap = engine.on_trade(&trades[idx % trades.len()], anchor);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        });
        assert_latency("on_trade/with_vsi", p50, p99, LIMIT_TRADE_WITH_VSI_P50);
    }
}

#[test]
fn latency_regression_ofi_window() {
    eprintln!("\n=== OFI window size scaling ===");

    let books = gen_books(5, 1024);
    let warm = gen_books(5, 512);

    let window_limits: &[(usize, u64)] = &[
        (10, LIMIT_OFI_COMPONENT_P50),
        (100, LIMIT_OFI_COMPONENT_P50),
        (500, LIMIT_OFI_COMPONENT_P50 * 2),
        (1000, LIMIT_OFI_COMPONENT_P50 * 3),
    ];
    for &(window, limit) in window_limits {
        let mut ofi = Ofi::new(window).unwrap();
        for b in &warm {
            ofi.update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            ofi.update(&books[idx % books.len()]);
            std::hint::black_box(ofi.value());
            idx = idx.wrapping_add(1);
        });
        assert_latency(&format!("ofi/window_{window}"), p50, p99, limit);
    }
}

#[test]
fn latency_regression_components() {
    eprintln!("\n=== individual signal components ===");

    let books_5 = gen_books(5, 1024);
    let books_10 = gen_books(10, 1024);
    let warm_5 = gen_books(5, 512);
    let warm_10 = gen_books(10, 512);
    let values = gen_f64_values(1024);

    {
        let mut ofi = MultiLevelOfi::new(100, 0.5).unwrap();
        for b in &warm_5 {
            ofi.update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            ofi.update(&books_5[idx % books_5.len()]);
            std::hint::black_box(ofi.value());
            idx = idx.wrapping_add(1);
        });
        assert_latency("multi_level_ofi/depth_5", p50, p99, LIMIT_MULTI_OFI_P50);
    }

    {
        let mut ofi = MultiLevelOfi::new(100, 0.5).unwrap();
        for b in &warm_10 {
            ofi.update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            ofi.update(&books_10[idx % books_10.len()]);
            std::hint::black_box(ofi.value());
            idx = idx.wrapping_add(1);
        });
        assert_latency(
            "multi_level_ofi/depth_10",
            p50,
            p99,
            LIMIT_MULTI_OFI_P50 * 2,
        );
    }

    {
        let mut di = DepthImbalance::new(5).unwrap();
        for b in &warm_5 {
            di.update(b);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let v = di.update(&books_5[idx % books_5.len()]);
            std::hint::black_box(v);
            idx = idx.wrapping_add(1);
        });
        assert_latency("depth_imbalance/depth_5", p50, p99, LIMIT_DI_COMPONENT_P50);
    }

    {
        let mut norm = EwmaNormaliser::new(200.0, 50).unwrap();
        for &v in &values {
            norm.update_and_normalise(v);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let z = norm.update_and_normalise(values[idx % values.len()]);
            std::hint::black_box(z);
            idx = idx.wrapping_add(1);
        });
        assert_latency("ewma_normaliser/warm", p50, p99, LIMIT_EWMA_NORM_P50);
    }

    {
        let mut norm = RegimeNormaliser::new(200.0, 20.0, 50, 4.0, 50).unwrap();
        for &v in &values {
            norm.update_and_normalise(v);
        }
        let mut idx = 0usize;
        let (p50, p99) = measure_ns(200, 2000, || {
            let z = norm.update_and_normalise(values[idx % values.len()]);
            std::hint::black_box(z);
            idx = idx.wrapping_add(1);
        });
        assert_latency(
            "regime_normaliser/warm_no_regime",
            p50,
            p99,
            LIMIT_REGIME_NORM_P50,
        );
    }
}

#[test]
fn latency_regression_normaliser_type() {
    eprintln!("\n=== normaliser type overhead ===");

    let books = gen_books(5, 1024);
    let warm = gen_books(5, 512);

    let ewma_config = cfg_no_composite();
    let (ewma_p50, ewma_p99) = {
        let mut engine = SignalEngine::new(ewma_config).unwrap();
        for b in &warm {
            engine.on_book_update(b);
        }
        let mut idx = 0usize;
        measure_ns(200, 2000, || {
            let snap = engine.on_book_update(&books[idx % books.len()]);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        })
    };
    assert_latency(
        "book_update/ewma_normaliser",
        ewma_p50,
        ewma_p99,
        LIMIT_BOOK_NORMALISED_P50,
    );

    let mut regime_config = cfg_no_composite();
    regime_config.regime_normaliser_params = Some(RegimeNormaliserParams::default());
    let (regime_p50, regime_p99) = {
        let mut engine = SignalEngine::new(regime_config).unwrap();
        for b in &warm {
            engine.on_book_update(b);
        }
        let mut idx = 0usize;
        measure_ns(200, 2000, || {
            let snap = engine.on_book_update(&books[idx % books.len()]);
            std::hint::black_box(snap);
            idx = idx.wrapping_add(1);
        })
    };
    assert_latency(
        "book_update/regime_normaliser",
        regime_p50,
        regime_p99,
        LIMIT_BOOK_NORMALISED_P50 * 2,
    );

    eprintln!(
        "  RegimeNormaliser overhead vs EWMA: +{} ns p50",
        regime_p50.saturating_sub(ewma_p50)
    );
}

#[test]
fn latency_regression_state_management() {
    eprintln!("\n=== normaliser state capture / restore ===");

    let warm = gen_books(5, 512);
    let config = SignalEngineConfig::default();

    let mut engine = SignalEngine::new(config.clone()).unwrap();
    for b in &warm {
        engine.on_book_update(b);
    }
    let snapshot = engine.capture_normaliser_state();

    let (p50, p99) = measure_ns(500, 5000, || {
        let snap = engine.capture_normaliser_state();
        std::hint::black_box(snap);
    });
    assert_latency(
        "state/capture_normaliser_state",
        p50,
        p99,
        LIMIT_STATE_CAPTURE_P50,
    );

    let mut fresh = SignalEngine::new(config.clone()).unwrap();
    let (p50, p99) = measure_ns(200, 2000, || {
        fresh.restore_normaliser_state(&snapshot);
        std::hint::black_box(fresh.capture_normaliser_state());
    });
    assert_latency(
        "state/restore_normaliser_state",
        p50,
        p99,
        LIMIT_STATE_RESTORE_P50,
    );

    let (p50, p99) = measure_ns(200, 2000, || {
        let mut fresh = SignalEngine::new(config.clone()).unwrap();
        fresh.restore_normaliser_state(&snapshot);
        std::hint::black_box(fresh.capture_normaliser_state());
    });
    assert_latency(
        "state/cold_start_and_restore",
        p50,
        p99,
        LIMIT_STATE_COLD_START_P50,
    );
}
