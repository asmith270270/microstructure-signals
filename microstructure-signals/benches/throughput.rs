use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use microstructure_signals::types::{BookSnapshot, MarketEvent, PriceLevel, Trade};
use microstructure_signals::{SignalEngine, SignalEngineConfig};

fn lcg(s: &mut u64) -> f64 {
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*s >> 11) as f64) * (1.0 / (1u64 << 53) as f64)
}

fn gen_book(mid: f64, spread: f64, s: &mut u64, ts: u64) -> BookSnapshot {
    let depth = 5;
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

fn stable_events(n: usize) -> Vec<MarketEvent> {
    let mut s = 0xaabbccdd_u64;
    let mut events = Vec::with_capacity(n * 5 / 4);
    for i in 0..n {
        let noise = (lcg(&mut s) - 0.5) * 0.02;
        let mid = 100.0 + noise;
        let book = gen_book(mid, 0.10, &mut s, i as u64);
        let bid = book.bids()[0].price;
        let ask = book.asks()[0].price;
        events.push(MarketEvent::BookUpdate(book));
        if i % 4 == 0 {
            events.push(MarketEvent::Trade(gen_trade(&mut s, bid, ask, i as u64)));
        }
    }
    events
}

fn trending_events(n: usize) -> Vec<MarketEvent> {
    let mut s = 0x11223344_u64;
    let mut events = Vec::with_capacity(n * 5 / 4);
    for i in 0..n {
        let mid = 100.0 + i as f64 * 0.002;
        let book = gen_book(mid, 0.10, &mut s, (1_000_000 + i) as u64);
        let bid = book.bids()[0].price;
        let ask = book.asks()[0].price;
        events.push(MarketEvent::BookUpdate(book));
        if i % 4 == 0 {
            events.push(MarketEvent::Trade(gen_trade(
                &mut s,
                bid,
                ask,
                (1_000_000 + i) as u64,
            )));
        }
    }
    events
}

fn volatile_events(n: usize) -> Vec<MarketEvent> {
    let mut s = 0x55667788_u64;
    let mut events = Vec::with_capacity(n * 4 / 3);
    for i in 0..n {
        let mid = 100.0 + (lcg(&mut s) - 0.5) * 4.0;
        let spread = 0.05 + lcg(&mut s) * 0.95;
        let book = gen_book(mid, spread, &mut s, (2_000_000 + i) as u64);
        let bid = book.bids()[0].price;
        let ask = book.asks()[0].price;
        events.push(MarketEvent::BookUpdate(book));
        if i % 3 == 0 {
            events.push(MarketEvent::Trade(gen_trade(
                &mut s,
                bid,
                ask,
                (2_000_000 + i) as u64,
            )));
        }
    }
    events
}

fn mixed_events(n: usize) -> Vec<MarketEvent> {
    let third = n / 3;
    let mut events = stable_events(third);
    events.extend(trending_events(third));
    events.extend(volatile_events(third));
    events
}

fn default_config() -> SignalEngineConfig {
    SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap()
}

/// Uses `iter_batched` so each iteration gets a freshly constructed, pre-warmed engine —
/// construction and normaliser warm-up happen in the setup closure, not the timed body.
fn bench_process_events(c: &mut Criterion) {
    let sizes = [1_000usize, 10_000, 100_000];
    let warm_events = stable_events(500);
    let mut group = c.benchmark_group("throughput/process_events");

    for &n in &sizes {
        let events = stable_events(n);
        group.throughput(Throughput::Elements(events.len() as u64));
        group.bench_with_input(BenchmarkId::new("stable", n), &n, |b, _| {
            b.iter_batched(
                || {
                    let mut engine = SignalEngine::new(default_config()).unwrap();
                    engine.process_events_with(&warm_events, |_, _| {});
                    engine
                },
                |mut engine| {
                    let results = engine.process_events(&events);
                    criterion::black_box(results)
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_process_events_with(c: &mut Criterion) {
    let sizes = [1_000usize, 10_000, 100_000];
    let warm_events = stable_events(500);
    let mut group = c.benchmark_group("throughput/process_events_with");

    for &n in &sizes {
        let events = stable_events(n);
        group.throughput(Throughput::Elements(events.len() as u64));
        group.bench_with_input(BenchmarkId::new("stable", n), &n, |b, _| {
            b.iter_batched(
                || {
                    let mut engine = SignalEngine::new(default_config()).unwrap();
                    engine.process_events_with(&warm_events, |_, _| {});
                    engine
                },
                |mut engine| {
                    let mut count = 0u64;
                    engine.process_events_with(&events, |_ts, _snap| {
                        count = count.wrapping_add(1);
                    });
                    criterion::black_box(count)
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Engine is pre-warmed, then each Criterion iteration processes the full event slice on
/// a *continuing* engine (state accumulates across iterations) — sustained hot-path
/// throughput with a warm normaliser, not cold-start overhead.
fn bench_scenario_throughput(c: &mut Criterion) {
    let n = 10_000usize;
    let scenarios: Vec<(&str, Vec<MarketEvent>)> = vec![
        ("stable", stable_events(n)),
        ("trending", trending_events(n)),
        ("volatile", volatile_events(n)),
        ("mixed", mixed_events(n)),
    ];

    let warm_events = stable_events(500);

    let mut group = c.benchmark_group("throughput/scenario");
    for (name, events) in &scenarios {
        group.throughput(Throughput::Elements(events.len() as u64));
        group.bench_function(*name, |b| {
            let mut engine = SignalEngine::new(default_config()).unwrap();
            engine.process_events_with(&warm_events, |_, _| {});
            let mut count = 0u64;
            b.iter(|| {
                engine.process_events_with(events, |_ts, _snap| {
                    count = count.wrapping_add(1);
                });
                criterion::black_box(count)
            });
        });
    }
    group.finish();
}

fn bench_sustained_throughput(c: &mut Criterion) {
    let n = 10_000usize;
    let warm_events = stable_events(500);
    let events = stable_events(n);

    let configs: Vec<(&str, SignalEngineConfig)> = vec![
        ("book_only_raw", SignalEngineConfig::default()),
        ("book_only_normalised", {
            let mut cfg = SignalEngineConfig::default();
            cfg.normalisation_warm_up = 50;
            cfg
        }),
        (
            "with_vsi_all_signals",
            SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap(),
        ),
    ];

    let mut group = c.benchmark_group("throughput/sustained");

    for (name, config) in &configs {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(*name, |b| {
            b.iter_batched(
                || {
                    let mut engine = SignalEngine::new(config.clone()).unwrap();
                    engine.process_events_with(&warm_events, |_, _| {});
                    engine
                },
                |mut engine| {
                    let mut count = 0u64;
                    engine.process_events_with(&events, |_ts, _snap| {
                        count = count.wrapping_add(1);
                    });
                    criterion::black_box(count)
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_alloc_vs_callback(c: &mut Criterion) {
    let n = 10_000usize;
    let warm_events = stable_events(500);
    let events = stable_events(n);
    let mut group = c.benchmark_group("throughput/alloc_vs_callback");
    group.throughput(Throughput::Elements(events.len() as u64));

    group.bench_function("process_events_vec", |b| {
        b.iter_batched(
            || {
                let mut engine = SignalEngine::new(default_config()).unwrap();
                engine.process_events_with(&warm_events, |_, _| {});
                engine
            },
            |mut engine| {
                let results = engine.process_events(&events);
                criterion::black_box(results.len())
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("process_events_with_callback", |b| {
        b.iter_batched(
            || {
                let mut engine = SignalEngine::new(default_config()).unwrap();
                engine.process_events_with(&warm_events, |_, _| {});
                engine
            },
            |mut engine| {
                let mut last_ts = 0u64;
                engine.process_events_with(&events, |ts, _snap| {
                    last_ts = ts;
                });
                criterion::black_box(last_ts)
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_process_events,
    bench_process_events_with,
    bench_scenario_throughput,
    bench_sustained_throughput,
    bench_alloc_vs_callback,
);
criterion_main!(benches);
