use microstructure_signals::types::{BookSnapshot, MarketEvent, PriceLevel, Trade};
use microstructure_signals::{SignalEngine, SignalEngineConfig, SignalSelection};
use std::time::Instant;

fn make_book(bid_price: f64, bid_qty: f64, ask_price: f64, ask_qty: f64) -> BookSnapshot {
    BookSnapshot {
        bids: vec![PriceLevel {
            price: bid_price,
            quantity: bid_qty,
        }],
        asks: vec![PriceLevel {
            price: ask_price,
            quantity: ask_qty,
        }],
        timestamp_ns: 0,
    }
}

fn make_book_with_time(
    bid_price: f64,
    bid_qty: f64,
    ask_price: f64,
    ask_qty: f64,
    timestamp_ns: u64,
) -> BookSnapshot {
    BookSnapshot {
        bids: vec![PriceLevel {
            price: bid_price,
            quantity: bid_qty,
        }],
        asks: vec![PriceLevel {
            price: ask_price,
            quantity: ask_qty,
        }],
        timestamp_ns,
    }
}

fn make_trade_with_time(price: f64, qty: f64, timestamp_ns: u64) -> Trade {
    Trade {
        price,
        quantity: qty,
        timestamp_ns,
    }
}

#[test]
fn test_sustained_throughput_live_mode() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let iterations = 100_000;
    let mut books = Vec::with_capacity(iterations);

    for i in 0..iterations {
        let price = 100.0 + (i % 100) as f64 * 0.01;
        let bid_qty = 50.0 + (i % 50) as f64;
        let ask_qty = 50.0 + ((i + 25) % 50) as f64;
        books.push(make_book(price, bid_qty, price + 1.0, ask_qty));
    }

    let start = Instant::now();
    for book in &books {
        let _ = engine.on_book_update(book);
    }
    let elapsed = start.elapsed();

    let throughput = iterations as f64 / elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() / iterations as u128;

    println!("Sustained throughput: {:.0} updates/sec", throughput);
    println!("Average latency: {}ns", avg_latency_ns);

    #[cfg(debug_assertions)]
    {
        assert!(
            throughput > 50_000.0,
            "Throughput {}Hz below 50kHz threshold in debug",
            throughput
        );
        assert!(
            avg_latency_ns < 20_000,
            "Average latency {}ns exceeds 20µs in debug",
            avg_latency_ns
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert!(
            throughput > 500_000.0,
            "Throughput {}Hz below 500kHz threshold",
            throughput
        );
        assert!(
            avg_latency_ns < 2_000,
            "Average latency {}ns exceeds 2µs",
            avg_latency_ns
        );
    }

    let snap = engine.snapshot();
    assert!(!snap.ofi.is_nan());
    assert!(!snap.depth_imbalance.is_nan());
}

#[test]
fn test_sustained_throughput_with_trades() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let iterations = 50_000;
    let book = make_book(100.0, 50.0, 101.0, 50.0);

    let start = Instant::now();
    for i in 0..iterations {
        let _ = engine.on_book_update(&book);
        if i % 10 == 0 {
            let trade = Trade {
                price: 100.5,
                quantity: 5.0,
                timestamp_ns: (i * 1000) as u64,
            };
            let _ = engine.on_trade(&trade, &book);
        }
    }
    let elapsed = start.elapsed();

    let total_events = iterations + (iterations / 10);
    let throughput = total_events as f64 / elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() / total_events as u128;

    println!(
        "Mixed throughput (90% book, 10% trade): {:.0} events/sec",
        throughput
    );
    println!("Average latency: {}ns", avg_latency_ns);

    #[cfg(debug_assertions)]
    {
        assert!(
            throughput > 40_000.0,
            "Mixed throughput {}Hz below 40kHz in debug",
            throughput
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert!(
            throughput > 400_000.0,
            "Mixed throughput {}Hz below 400kHz",
            throughput
        );
    }
}

#[test]
fn test_historical_mode_batch_throughput() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let num_events = 100_000;
    let mut events = Vec::with_capacity(num_events);

    for i in 0..num_events {
        if i % 5 == 0 {
            events.push(MarketEvent::Trade(make_trade_with_time(
                100.5,
                10.0,
                (i * 1000) as u64,
            )));
        } else {
            let price = 100.0 + (i % 100) as f64 * 0.01;
            events.push(MarketEvent::BookUpdate(make_book_with_time(
                price,
                50.0 + (i % 50) as f64,
                price + 1.0,
                50.0,
                (i * 1000) as u64,
            )));
        }
    }

    let start = Instant::now();
    let results = engine.process_events(&events);
    let elapsed = start.elapsed();

    let throughput = num_events as f64 / elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() / num_events as u128;

    println!("Historical batch throughput: {:.0} events/sec", throughput);
    println!("Average latency per event: {}ns", avg_latency_ns);

    assert_eq!(results.len(), num_events);

    #[cfg(debug_assertions)]
    {
        assert!(
            throughput > 100_000.0,
            "Batch throughput {}Hz below 100kHz in debug",
            throughput
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert!(
            throughput > 1_000_000.0,
            "Batch throughput {}Hz below 1MHz",
            throughput
        );
    }
}

#[test]
fn test_memory_footprint() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    let engine = SignalEngine::new(config).unwrap();

    let size = std::mem::size_of_val(&engine);
    println!("SignalEngine size: {} bytes ({} KB)", size, size / 1024);

    assert!(
        size < 10_000,
        "Engine size {} bytes exceeds 10KB limit",
        size
    );
}

#[test]
fn test_minimal_config_memory() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    config.signals = {
        let mut s = SignalSelection::none();
        s.ofi = true;
        s
    };

    let engine = SignalEngine::new(config).unwrap();
    let size = std::mem::size_of_val(&engine);

    println!("Minimal engine size: {} bytes", size);

    assert!(
        size < 5_000,
        "Minimal engine size {} bytes exceeds 5KB",
        size
    );
}

#[test]
fn test_scalability_with_data_volume() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();

    let test_sizes = [1_000, 10_000, 50_000, 100_000];
    let mut throughputs = Vec::new();

    for &size in &test_sizes {
        let mut engine = SignalEngine::new(config.clone()).unwrap();
        let book = make_book(100.0, 50.0, 101.0, 50.0);

        let start = Instant::now();
        for _ in 0..size {
            let _ = engine.on_book_update(&book);
        }
        let elapsed = start.elapsed();

        let throughput = size as f64 / elapsed.as_secs_f64();
        throughputs.push(throughput);

        println!("Size {}: {:.0} updates/sec", size, throughput);
    }

    #[cfg(not(debug_assertions))]
    for i in 1..throughputs.len() {
        let ratio = throughputs[i] / throughputs[i - 1];
        assert!(
            ratio > 0.5,
            "Throughput degraded significantly: {} vs {} (ratio {})",
            throughputs[i - 1],
            throughputs[i],
            ratio
        );
    }

    println!("Scalability maintained across data volumes");
}

#[test]
fn test_varying_book_depths() {
    let depths = [1, 5, 10, 20];
    let iterations = 10_000;

    for &depth in &depths {
        let mut config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
        config.depth_levels = depth;
        let mut engine = SignalEngine::new(config).unwrap();

        let mut bids = Vec::new();
        let mut asks = Vec::new();
        for i in 0..depth {
            bids.push(PriceLevel {
                price: 100.0 - i as f64 * 0.01,
                quantity: 50.0,
            });
            asks.push(PriceLevel {
                price: 101.0 + i as f64 * 0.01,
                quantity: 50.0,
            });
        }

        let book = BookSnapshot {
            bids,
            asks,
            timestamp_ns: 0,
        };

        let start = Instant::now();
        for _ in 0..iterations {
            let _ = engine.on_book_update(&book);
        }
        let elapsed = start.elapsed();

        let throughput = iterations as f64 / elapsed.as_secs_f64();
        let avg_latency_ns = elapsed.as_nanos() / iterations;

        println!(
            "Depth {}: {:.0} updates/sec, {}ns latency",
            depth, throughput, avg_latency_ns
        );

        #[cfg(not(debug_assertions))]
        {
            assert!(
                throughput > 200_000.0,
                "Throughput with depth {} is {}Hz, below 200kHz",
                depth,
                throughput
            );
        }
    }
}

#[test]
fn test_normalisation_overhead() {
    let iterations = 50_000;

    let mut config_with_norm = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    config_with_norm.normalisation_warm_up = 10;

    let mut config_without_norm = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    config_without_norm.signals.ofi_z = false;
    config_without_norm.signals.depth_imbalance_z = false;
    config_without_norm.signals.vsi_z = false;
    config_without_norm.signals.composite_toxicity = false;
    config_without_norm.signals.adverse_selection = false;

    let book = make_book(100.0, 50.0, 101.0, 50.0);

    let mut engine_with = SignalEngine::new(config_with_norm).unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = engine_with.on_book_update(&book);
    }
    let elapsed_with = start.elapsed();

    let mut engine_without = SignalEngine::new(config_without_norm).unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = engine_without.on_book_update(&book);
    }
    let elapsed_without = start.elapsed();

    let throughput_with = iterations as f64 / elapsed_with.as_secs_f64();
    let throughput_without = iterations as f64 / elapsed_without.as_secs_f64();

    println!("With normalisation: {:.0} updates/sec", throughput_with);
    println!(
        "Without normalisation: {:.0} updates/sec",
        throughput_without
    );

    let overhead = (elapsed_with.as_nanos() as i128 - elapsed_without.as_nanos() as i128) as f64
        / iterations as f64;
    println!("Normalisation overhead: {:.1}ns per update", overhead);

    #[cfg(not(debug_assertions))]
    {
        assert!(
            overhead < 500.0,
            "Normalisation overhead {}ns exceeds 500ns",
            overhead
        );
    }
}

#[test]
fn test_realistic_market_data_simulation() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(50.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let num_seconds = 10;
    let updates_per_second = 1000;
    let total_updates = num_seconds * updates_per_second;

    let start = Instant::now();

    let mut price = 100.0;
    let mut rng_state = 12345u64;

    for i in 0..total_updates {
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let rand = (rng_state >> 16) & 0x7FFF;
        let price_change = (rand as f64 / 32768.0 - 0.5) * 0.1;
        price += price_change;

        let bid_qty = 50.0 + (rand % 100) as f64;
        let ask_qty = 50.0 + ((rand + 50) % 100) as f64;

        let book = make_book(price, bid_qty, price + 1.0, ask_qty);
        let _ = engine.on_book_update(&book);

        if i % 50 == 0 {
            let trade_price = price + 0.5;
            let trade = Trade {
                price: trade_price,
                quantity: 10.0,
                timestamp_ns: (i * 1000) as u64,
            };
            let _ = engine.on_trade(&trade, &book);
        }
    }

    let elapsed = start.elapsed();
    let throughput = total_updates as f64 / elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() / total_updates as u128;

    println!(
        "Simulated {}s market data at {}Hz: {:.0} updates/sec processed",
        num_seconds, updates_per_second, throughput
    );
    println!("Average latency: {}ns", avg_latency_ns);

    let snap = engine.snapshot();
    assert!(!snap.ofi.is_nan());
    assert!(!snap.depth_imbalance.is_nan());
    assert!(!snap.microprice.is_nan());

    #[cfg(debug_assertions)]
    {
        assert!(
            throughput > 100_000.0,
            "Realistic simulation throughput {}Hz below 100kHz in debug",
            throughput
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert!(
            throughput > 1_000_000.0,
            "Realistic simulation throughput {}Hz below 1MHz",
            throughput
        );
    }
}

#[test]
fn test_tail_latency_p99_p999() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let iterations = 50_000;
    let mut latencies_ns = Vec::with_capacity(iterations);

    let warmup_book = make_book(100.0, 50.0, 101.0, 50.0);
    for _ in 0..100 {
        let _ = engine.on_book_update(&warmup_book);
    }

    for i in 0..iterations {
        let price = 100.0 + (i % 100) as f64 * 0.01;
        let book = make_book(price, 50.0 + (i % 50) as f64, price + 1.0, 50.0);

        let start = Instant::now();
        let _ = engine.on_book_update(&book);
        let elapsed = start.elapsed().as_nanos() as u64;
        latencies_ns.push(elapsed);
    }

    latencies_ns.sort_unstable();

    let p50 = latencies_ns[iterations / 2];
    let p99 = latencies_ns[iterations * 99 / 100];
    let p999 = latencies_ns[iterations * 999 / 1000];
    let max = latencies_ns[iterations - 1];
    let avg = latencies_ns.iter().sum::<u64>() / iterations as u64;

    println!("Tail latency distribution ({} samples):", iterations);
    println!("  avg:  {}ns", avg);
    println!("  p50:  {}ns", p50);
    println!("  p99:  {}ns", p99);
    println!("  p999: {}ns", p999);
    println!("  max:  {}ns", max);

    #[cfg(not(debug_assertions))]
    {
        assert!(p99 < 5_000, "p99 latency {}ns exceeds 5µs", p99);
        assert!(p999 < 10_000, "p999 latency {}ns exceeds 10µs", p999);
    }
}

#[test]
fn test_tail_latency_mixed_book_and_trade() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let iterations = 20_000;
    let mut latencies_ns = Vec::with_capacity(iterations);

    let book = make_book(100.0, 50.0, 101.0, 50.0);
    for _ in 0..100 {
        let _ = engine.on_book_update(&book);
    }

    for i in 0..iterations {
        let price = 100.0 + (i % 100) as f64 * 0.01;
        let book = make_book(price, 50.0, price + 1.0, 50.0);

        let start = Instant::now();
        if i % 5 == 0 {
            let trade = Trade {
                price: price + 0.5,
                quantity: 10.0,
                timestamp_ns: (i * 1000) as u64,
            };
            let _ = engine.on_trade(&trade, &book);
        } else {
            let _ = engine.on_book_update(&book);
        }
        let elapsed = start.elapsed().as_nanos() as u64;
        latencies_ns.push(elapsed);
    }

    latencies_ns.sort_unstable();

    let p99 = latencies_ns[iterations * 99 / 100];
    let p999 = latencies_ns[iterations * 999 / 1000];

    println!("Mixed book+trade tail latency ({} samples):", iterations);
    println!("  p99:  {}ns", p99);
    println!("  p999: {}ns", p999);

    #[cfg(not(debug_assertions))]
    {
        assert!(p99 < 5_000, "Mixed p99 latency {}ns exceeds 5µs", p99);
    }
}

#[test]
fn test_historical_mode_million_events() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.normalisation_warm_up = 100;

    let mut engine = SignalEngine::new(config).unwrap();

    let num_events = 1_000_000;
    let mut events = Vec::with_capacity(num_events);

    println!("Generating {} events...", num_events);

    for i in 0..num_events {
        let price = 100.0 + (i % 1000) as f64 * 0.01;
        if i % 10 == 0 {
            events.push(MarketEvent::Trade(make_trade_with_time(
                price + 0.5,
                10.0,
                (i * 100) as u64,
            )));
        } else {
            events.push(MarketEvent::BookUpdate(make_book_with_time(
                price,
                50.0 + (i % 100) as f64,
                price + 1.0,
                50.0,
                (i * 100) as u64,
            )));
        }
    }

    println!("Processing {} events...", num_events);
    let start = Instant::now();
    let results = engine.process_events(&events);
    let elapsed = start.elapsed();

    assert_eq!(results.len(), num_events);

    let throughput = num_events as f64 / elapsed.as_secs_f64();
    let total_time_ms = elapsed.as_millis();

    println!(
        "Processed {} events in {}ms ({:.0} events/sec)",
        num_events, total_time_ms, throughput
    );

    let final_snap = &results.last().unwrap().1;
    assert!(!final_snap.ofi.is_nan());
    assert!(!final_snap.depth_imbalance.is_nan());
    assert!(!final_snap.ofi_z.is_nan());
    assert!(!final_snap.toxicity.is_nan());

    #[cfg(debug_assertions)]
    {
        assert!(
            throughput > 100_000.0,
            "1M event throughput {}Hz below 100kHz in debug",
            throughput
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert!(
            throughput > 1_000_000.0,
            "1M event throughput {}Hz below 1MHz",
            throughput
        );
        assert!(
            total_time_ms < 2000,
            "1M events took {}ms, exceeds 2s limit",
            total_time_ms
        );
    }
}
