use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade};
use microstructure_signals::{
    ClassifierType, DepthImbalance, EwmaNormaliser, MultiLevelOfi, Ofi, RegimeNormaliser,
    RegimeNormaliserParams, SignalEngine, SignalEngineConfig, SignalSelection,
};

fn lcg(s: &mut u64) -> f64 {
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*s >> 11) as f64) * (1.0 / (1u64 << 53) as f64)
}

fn gen_book(depth: usize, s: &mut u64, ts: u64) -> BookSnapshot {
    let mid = 100.0 + (lcg(s) - 0.5) * 0.4;
    let half_spread = 0.05 + lcg(s) * 0.05;
    let bids: Vec<PriceLevel> = (0..depth)
        .map(|k| PriceLevel {
            price: mid - half_spread - k as f64 * 0.05,
            quantity: 100.0 + lcg(s) * 900.0,
        })
        .collect();
    let asks: Vec<PriceLevel> = (0..depth)
        .map(|k| PriceLevel {
            price: mid + half_spread + k as f64 * 0.05,
            quantity: 100.0 + lcg(s) * 900.0,
        })
        .collect();
    BookSnapshot::new(&bids, &asks, ts)
}

fn gen_book_with_mid(depth: usize, mid: f64, spread: f64, s: &mut u64, ts: u64) -> BookSnapshot {
    let half = spread / 2.0;
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

fn gen_trade(s: &mut u64, bid: f64, ask: f64, ts: u64) -> Trade {
    Trade {
        price: bid + (ask - bid) * lcg(s),
        quantity: 10.0 + lcg(s) * 90.0,
        timestamp_ns: ts,
    }
}

fn prebuilt_books(depth: usize, n: usize) -> Vec<BookSnapshot> {
    let mut s = 0xdeadbeef_u64;
    (0..n).map(|i| gen_book(depth, &mut s, i as u64)).collect()
}

fn warmup_engine(engine: &mut SignalEngine, books: &[BookSnapshot]) {
    for b in books {
        engine.on_book_update(b);
    }
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

fn cfg_signals_none_vsi() -> SignalSelection {
    let mut sel = SignalSelection::raw_only();
    sel.vsi = false;
    sel.vsi_z = false;
    sel
}

fn bench_config_latency(c: &mut Criterion) {
    let warm = prebuilt_books(5, 500);
    let data = prebuilt_books(5, 1000);

    let configs: Vec<(&str, SignalEngineConfig)> = vec![
        ("raw_only", cfg_raw_only()),
        ("normalised_no_composite", cfg_no_composite()),
        ("all_signals", SignalEngineConfig::default()),
    ];

    let mut group = c.benchmark_group("latency/config");
    for (name, config) in &configs {
        group.bench_function(*name, |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            warmup_engine(&mut engine, &warm);
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&data[idx % data.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }
    group.finish();
}

fn bench_book_depth(c: &mut Criterion) {
    let depths = [1usize, 5, 10, 20];
    let mut group = c.benchmark_group("latency/book_depth");

    for &depth in &depths {
        let warm = prebuilt_books(depth, 500);
        let data = prebuilt_books(depth, 1000);
        let mut config = cfg_raw_only();
        config.depth_levels = depth;

        group.bench_with_input(BenchmarkId::new("depth", depth), &depth, |b, _| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            warmup_engine(&mut engine, &warm);
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&data[idx % data.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }
    group.finish();
}

fn bench_ofi_window(c: &mut Criterion) {
    let windows = [10usize, 50, 100, 500, 1000];
    let warm = prebuilt_books(5, 500);
    let data = prebuilt_books(5, 1000);
    let mut group = c.benchmark_group("latency/ofi_window");

    for &window in &windows {
        let mut config = cfg_raw_only();
        config.ofi_window = window;
        group.bench_with_input(BenchmarkId::new("window", window), &window, |b, _| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            warmup_engine(&mut engine, &warm);
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&data[idx % data.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }
    group.finish();
}

fn bench_normaliser_half_life(c: &mut Criterion) {
    let half_lives = [50.0f64, 200.0, 1000.0];
    let warm = prebuilt_books(5, 500);
    let data = prebuilt_books(5, 1000);
    let mut group = c.benchmark_group("latency/normaliser_half_life");

    for &hl in &half_lives {
        let mut config = cfg_no_composite();
        config.normalisation_half_life = hl;
        config.normalisation_warm_up = 50;
        let label = format!("{}", hl as u64);
        group.bench_with_input(BenchmarkId::new("half_life", &label), &hl, |b, _| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            warmup_engine(&mut engine, &warm);
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&data[idx % data.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }
    group.finish();
}

fn bench_normaliser_type(c: &mut Criterion) {
    let warm = prebuilt_books(5, 500);
    let data = prebuilt_books(5, 1000);
    let mut group = c.benchmark_group("latency/normaliser_type");

    {
        let mut config = cfg_no_composite();
        config.normalisation_half_life = 200.0;
        config.normalisation_warm_up = 50;
        config.regime_normaliser_params = None;
        group.bench_function("ewma", |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            warmup_engine(&mut engine, &warm);
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&data[idx % data.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    {
        let mut config = cfg_no_composite();
        config.normalisation_half_life = 200.0;
        config.normalisation_warm_up = 50;
        config.regime_normaliser_params = Some(RegimeNormaliserParams::default());
        group.bench_function("regime", |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            warmup_engine(&mut engine, &warm);
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&data[idx % data.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    group.finish();
}

fn bench_trade_path(c: &mut Criterion) {
    let mut s = 0xc0ffee_u64;
    let depth = 5;

    let warm_books: Vec<BookSnapshot> = (0..200).map(|i| gen_book(depth, &mut s, i)).collect();
    let data_books: Vec<BookSnapshot> =
        (0..500).map(|i| gen_book(depth, &mut s, 200 + i)).collect();
    let bbo = &data_books[0];
    let bid = bbo.bids()[0].price;
    let ask = bbo.asks()[0].price;
    let trades: Vec<Trade> = (0..500)
        .map(|i| gen_trade(&mut s, bid, ask, 700 + i))
        .collect();

    let mut group = c.benchmark_group("latency/trade_path");

    {
        let mut config = SignalEngineConfig::default();
        config.classifier = ClassifierType::TickRule;
        config.signals = cfg_signals_none_vsi();
        group.bench_function("tick_rule", |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            for bk in &warm_books {
                engine.on_book_update(bk);
            }
            let current_book = warm_books.last().unwrap();
            let mut idx = 0usize;
            b.iter(|| {
                engine.on_book_update(&data_books[idx % data_books.len()]);
                let snap = engine.on_trade(&trades[idx % trades.len()], current_book);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    {
        let mut config = SignalEngineConfig::default();
        config.classifier = ClassifierType::QuoteRule;
        config.signals = cfg_signals_none_vsi();
        group.bench_function("quote_rule", |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            for bk in &warm_books {
                engine.on_book_update(bk);
            }
            let current_book = warm_books.last().unwrap();
            let mut idx = 0usize;
            b.iter(|| {
                engine.on_book_update(&data_books[idx % data_books.len()]);
                let snap = engine.on_trade(&trades[idx % trades.len()], current_book);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    {
        let config = SignalEngineConfig::with_vsi_bucket_volume(500.0).unwrap();
        group.bench_function("with_vsi", |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            for bk in &warm_books {
                engine.on_book_update(bk);
            }
            let current_book = warm_books.last().unwrap();
            let mut idx = 0usize;
            b.iter(|| {
                engine.on_book_update(&data_books[idx % data_books.len()]);
                let snap = engine.on_trade(&trades[idx % trades.len()], current_book);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    {
        let mut config = SignalEngineConfig::default();
        config.signals.vsi = true;
        config.signals.vsi_z = false;
        config.vsi_bucket_volume = 5.0;
        group.bench_function("vsi_frequent_bucket_close", |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            for bk in &warm_books {
                engine.on_book_update(bk);
            }
            let current_book = warm_books.last().unwrap();
            let mut idx = 0usize;
            b.iter(|| {
                engine.on_book_update(&data_books[idx % data_books.len()]);
                let snap = engine.on_trade(&trades[idx % trades.len()], current_book);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    group.finish();
}

fn bench_components(c: &mut Criterion) {
    let mut s = 0x1234abcd_u64;
    let books_1: Vec<BookSnapshot> = (0..1000).map(|i| gen_book(1, &mut s, i as u64)).collect();
    let books_5: Vec<BookSnapshot> = (0..1000)
        .map(|i| gen_book(5, &mut s, (1000 + i) as u64))
        .collect();
    let books_10: Vec<BookSnapshot> = (0..1000)
        .map(|i| gen_book(10, &mut s, (2000 + i) as u64))
        .collect();
    let books_20: Vec<BookSnapshot> = (0..1000)
        .map(|i| gen_book(20, &mut s, (3000 + i) as u64))
        .collect();

    let mut group = c.benchmark_group("components");

    for &window in &[50usize, 100, 500] {
        group.bench_with_input(BenchmarkId::new("ofi/window", window), &window, |b, &w| {
            let mut ofi = Ofi::new(w).unwrap();
            for bk in &books_5 {
                ofi.update(bk);
            }
            let mut idx = 0usize;
            b.iter(|| {
                ofi.update(&books_5[idx % books_5.len()]);
                criterion::black_box(ofi.value());
                idx = idx.wrapping_add(1);
            });
        });
    }

    for &depth in &[1usize, 5, 10] {
        let books: &Vec<BookSnapshot> = match depth {
            1 => &books_1,
            10 => &books_10,
            _ => &books_5,
        };
        group.bench_with_input(
            BenchmarkId::new("multi_level_ofi/depth", depth),
            &depth,
            |b, _| {
                let mut ofi = MultiLevelOfi::new(100, 0.5).unwrap();
                for bk in books {
                    ofi.update(bk);
                }
                let mut idx = 0usize;
                b.iter(|| {
                    ofi.update(&books[idx % books.len()]);
                    criterion::black_box(ofi.value());
                    idx = idx.wrapping_add(1);
                });
            },
        );
    }

    let di_cases: Vec<(usize, &Vec<BookSnapshot>)> = vec![
        (1, &books_1),
        (5, &books_5),
        (10, &books_10),
        (20, &books_20),
    ];
    for (depth, books) in &di_cases {
        let d = *depth;
        group.bench_with_input(
            BenchmarkId::new("depth_imbalance/depth", d),
            &d,
            |b, &dep| {
                let mut di = DepthImbalance::new(dep).unwrap();
                for bk in *books {
                    di.update(bk);
                }
                let mut idx = 0usize;
                b.iter(|| {
                    let v = di.update(&books[idx % books.len()]);
                    criterion::black_box(v);
                    idx = idx.wrapping_add(1);
                });
            },
        );
    }

    group.bench_function("normaliser/ewma", |b| {
        let mut norm = EwmaNormaliser::new(200.0, 50).unwrap();
        let values: Vec<f64> = {
            let mut sv = 0xaabb_u64;
            (0..1000).map(|_| (lcg(&mut sv) - 0.5) * 1000.0).collect()
        };
        for &v in &values {
            norm.update_and_normalise(v);
        }
        let mut idx = 0usize;
        b.iter(|| {
            let z = norm.update_and_normalise(values[idx % values.len()]);
            criterion::black_box(z);
            idx = idx.wrapping_add(1);
        });
    });

    group.bench_function("normaliser/regime", |b| {
        let mut norm = RegimeNormaliser::new(200.0, 20.0, 50, 4.0, 50).unwrap();
        let values: Vec<f64> = {
            let mut sv = 0xccdd_u64;
            (0..1000).map(|_| (lcg(&mut sv) - 0.5) * 1000.0).collect()
        };
        for &v in &values {
            norm.update_and_normalise(v);
        }
        let mut idx = 0usize;
        b.iter(|| {
            let z = norm.update_and_normalise(values[idx % values.len()]);
            criterion::black_box(z);
            idx = idx.wrapping_add(1);
        });
    });

    group.finish();
}

fn bench_state_management(c: &mut Criterion) {
    let warm = prebuilt_books(5, 500);
    let config = SignalEngineConfig::default();
    let mut group = c.benchmark_group("state");

    group.bench_function("capture_normaliser_state", |b| {
        let mut engine = SignalEngine::new(config.clone()).unwrap();
        warmup_engine(&mut engine, &warm);
        b.iter(|| {
            let snap = engine.capture_normaliser_state();
            criterion::black_box(snap);
        });
    });

    group.bench_function("restore_normaliser_state", |b| {
        let mut engine = SignalEngine::new(config.clone()).unwrap();
        warmup_engine(&mut engine, &warm);
        let snapshot = engine.capture_normaliser_state();
        b.iter(|| {
            let mut fresh = SignalEngine::new(config.clone()).unwrap();
            fresh.restore_normaliser_state(&snapshot);
            criterion::black_box(fresh.capture_normaliser_state());
        });
    });

    group.finish();
}

fn bench_market_scenarios(c: &mut Criterion) {
    let mut s = 0xf00dface_u64;
    let n = 1000usize;
    let depth = 5;

    let stable: Vec<BookSnapshot> = (0..n)
        .map(|i| {
            let noise = (lcg(&mut s) - 0.5) * 0.02;
            gen_book_with_mid(depth, 100.0 + noise, 0.10, &mut s, i as u64)
        })
        .collect();

    let trending: Vec<BookSnapshot> = (0..n)
        .map(|i| {
            let mid = 100.0 + i as f64 * 0.005;
            gen_book_with_mid(depth, mid, 0.10, &mut s, (n + i) as u64)
        })
        .collect();

    let volatile: Vec<BookSnapshot> = (0..n)
        .map(|i| {
            let mid = 100.0 + (lcg(&mut s) - 0.5) * 4.0;
            let spread = 0.05 + lcg(&mut s) * 0.95;
            gen_book_with_mid(depth, mid, spread, &mut s, (2 * n + i) as u64)
        })
        .collect();

    let mut config = SignalEngineConfig::default();
    config.normalisation_warm_up = 50;

    let mut group = c.benchmark_group("latency/market_scenario");

    for (name, books) in &[
        ("stable", &stable),
        ("trending", &trending),
        ("volatile", &volatile),
    ] {
        group.bench_function(*name, |b| {
            let mut engine = SignalEngine::new(config.clone()).unwrap();
            for bk in *books {
                engine.on_book_update(bk);
            }
            let mut idx = 0usize;
            b.iter(|| {
                let snap = engine.on_book_update(&books[idx % books.len()]);
                criterion::black_box(snap);
                idx = idx.wrapping_add(1);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_config_latency,
    bench_book_depth,
    bench_ofi_window,
    bench_normaliser_half_life,
    bench_normaliser_type,
    bench_trade_path,
    bench_components,
    bench_state_management,
    bench_market_scenarios,
);
criterion_main!(benches);
