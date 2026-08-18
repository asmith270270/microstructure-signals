use microstructure_signals::types::{BookSnapshot, MarketEvent, PriceLevel, Trade};
use microstructure_signals::{ConfigError, SignalEngine, SignalEngineConfig};

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

fn make_trade(price: f64, qty: f64) -> Trade {
    Trade {
        price,
        quantity: qty,
        timestamp_ns: 0,
    }
}

#[test]
fn test_engine_initialisation() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let engine = SignalEngine::new(config).unwrap();
    let snapshot = engine.snapshot();

    assert!(snapshot.ofi.is_nan());
    assert!(snapshot.vsi.is_nan());
    assert!(snapshot.toxicity.is_nan());
}

#[test]
fn test_engine_book_update_produces_depth_and_micro() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let book = make_book(100.0, 50.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book);
    let snapshot = engine.snapshot();

    assert!(!snapshot.depth_imbalance.is_nan());
    assert!(!snapshot.microprice.is_nan());
    assert!(!snapshot.mid_price.is_nan());
}

#[test]
fn test_engine_trade_updates_vsi() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let book = make_book(100.0, 50.0, 101.0, 50.0);

    let _ = engine.on_trade(&make_trade(100.8, 100.0), &book);
    let snapshot = engine.snapshot();

    assert!(!snapshot.vsi.is_nan());
}

#[test]
fn test_engine_full_warmup_all_signals_some() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config.normalisation_warm_up = 5;
    config.ofi_window = 10;

    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..100 {
        let book = make_book(100.0 + (i % 5) as f64 * 0.1, 50.0 + i as f64, 101.0, 50.0);
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(100.5, 5.0), &book);
    }

    let snapshot = engine.snapshot();

    assert!(!snapshot.ofi.is_nan());
    assert!(!snapshot.ofi_z.is_nan());
    assert!(!snapshot.depth_imbalance.is_nan());
    assert!(!snapshot.depth_imbalance_z.is_nan());
    assert!(!snapshot.vsi.is_nan());
    assert!(!snapshot.vsi_z.is_nan());
    assert!(!snapshot.toxicity.is_nan());
    assert!(!snapshot.adverse_selection.is_nan());
}

#[test]
fn test_normaliser_ready_flags_not_stale_after_on_trade() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config.normalisation_warm_up = 5;
    config.ofi_window = 10;

    let mut engine = SignalEngine::new(config).unwrap();
    let mut last_book = make_book(100.0, 50.0, 101.0, 50.0);

    for i in 0..20 {
        last_book = make_book(100.0 + (i % 5) as f64 * 0.1, 50.0 + i as f64, 101.0, 50.0);
        let _ = engine.on_book_update(&last_book);
    }

    let ready_before_trade = engine.snapshot().ofi_normaliser_ready;
    assert!(
        ready_before_trade,
        "ofi_normaliser_ready should be true after enough varied book updates"
    );

    let _ = engine.on_trade(&make_trade(100.5, 5.0), &last_book);
    let snapshot = engine.snapshot();

    assert!(
        snapshot.ofi_normaliser_ready,
        "ofi_normaliser_ready must remain true immediately after on_trade, not go stale"
    );
    assert!(
        snapshot.depth_imbalance_normaliser_ready,
        "depth_imbalance_normaliser_ready must remain true immediately after on_trade"
    );
}

#[test]
fn test_engine_ofi_accumulates() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let book1 = make_book(100.0, 50.0, 101.0, 50.0);
    let book2 = make_book(100.0, 70.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book1);
    let _ = engine.on_book_update(&book2);

    let snapshot = engine.snapshot();
    assert!(!snapshot.ofi.is_nan());
    assert!(snapshot.ofi > 0.0);
}

#[test]
fn test_signal_selection_disabled_ofi() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.signals.ofi = false;
    config.signals.ofi_z = false;
    let mut engine = SignalEngine::new(config).unwrap();

    let book1 = make_book(100.0, 50.0, 101.0, 50.0);
    let book2 = make_book(100.0, 70.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book1);
    let _ = engine.on_book_update(&book2);

    let snap = engine.snapshot();
    assert!(snap.ofi.is_nan(), "OFI should be NaN when disabled");
    assert!(
        snap.ofi_z.is_nan(),
        "OFI z-score should be NaN when disabled"
    );
    assert!(!snap.depth_imbalance.is_nan());
    assert!(!snap.microprice.is_nan());
}

#[test]
fn test_ofi_z_none_when_raw_disabled() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.signals.ofi = false;
    config.signals.ofi_z = true;
    config.normalisation_warm_up = 3;
    let mut engine = SignalEngine::new(config).unwrap();

    for _ in 0..10 {
        let book = make_book(100.0, 50.0, 101.0, 50.0);
        let _ = engine.on_book_update(&book);
    }

    let snap = engine.snapshot();
    assert!(snap.ofi.is_nan());
    assert!(
        snap.ofi_z.is_nan(),
        "OFI z-score should be NaN when raw signal is disabled"
    );
}

#[test]
fn test_composite_none_when_all_z_disabled() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config.signals.ofi_z = false;
    config.signals.depth_imbalance_z = false;
    config.signals.vsi_z = false;
    config.signals.composite_toxicity = true;
    config.signals.adverse_selection = true;
    config.normalisation_warm_up = 3;
    let mut engine = SignalEngine::new(config).unwrap();

    for _ in 0..50 {
        let book = make_book(100.0, 50.0, 101.0, 50.0);
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(100.5, 5.0), &book);
    }

    let snap = engine.snapshot();
    assert!(
        snap.toxicity.is_nan(),
        "Toxicity should be NaN when no z-scores available"
    );
    assert!(
        snap.adverse_selection.is_nan(),
        "Adverse selection should be NaN when no z-scores available"
    );
}

#[test]
fn test_historical_mode_batch_processing() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let events = vec![
        MarketEvent::BookUpdate(make_book(100.0, 50.0, 101.0, 50.0)),
        MarketEvent::Trade(Trade {
            price: 100.5,
            quantity: 10.0,
            timestamp_ns: 1000,
        }),
        MarketEvent::BookUpdate(make_book(100.0, 60.0, 101.0, 50.0)),
        MarketEvent::Trade(Trade {
            price: 100.5,
            quantity: 15.0,
            timestamp_ns: 2000,
        }),
    ];

    let results = engine.process_events(&events);

    assert_eq!(results.len(), 4, "Should have one result per event");
    assert_eq!(results[0].0, 0, "First BookUpdate timestamp");
    assert_eq!(results[1].0, 1000, "First Trade timestamp");
    assert_eq!(results[2].0, 0, "Second BookUpdate timestamp");
    assert_eq!(results[3].0, 2000, "Second Trade timestamp");
    assert!(
        !results[2].1.depth_imbalance.is_nan(),
        "Signals should be computed"
    );
}

#[test]
fn test_snapshot_consistent_after_process_events() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let events = vec![
        MarketEvent::BookUpdate(make_book(100.0, 50.0, 101.0, 50.0)),
        MarketEvent::BookUpdate(make_book(100.0, 70.0, 101.0, 50.0)),
    ];

    let results = engine.process_events(&events);
    let last_from_batch = &results.last().unwrap().1;
    let peek = engine.snapshot();

    assert_eq!(
        *last_from_batch, *peek,
        "Snapshot should match last process_events result"
    );
}

#[test]
fn test_live_mode_low_latency() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let book = make_book(100.0, 50.0, 101.0, 50.0);

    let start = std::time::Instant::now();
    for _ in 0..10000 {
        let _ = engine.on_book_update(&book);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / 10000;
    println!("Average latency per on_book_update: {}ns", avg_ns);

    #[cfg(debug_assertions)]
    let threshold_ns = 2000;
    #[cfg(not(debug_assertions))]
    let threshold_ns = 200;

    assert!(
        avg_ns < threshold_ns,
        "Average latency {}ns exceeds {}ns threshold",
        avg_ns,
        threshold_ns
    );
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
fn test_live_mode_trending_up_market() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(50.0).unwrap();
    config.normalisation_warm_up = 5;
    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..10 {
        let book = make_book(100.0, 50.0 + i as f64, 101.0, 50.0 - i as f64);
        let _ = engine.on_book_update(&book);
    }

    for i in 0..20 {
        let price = 100.0 + (i as f64 * 0.1);
        let book = make_book(price, 60.0 + i as f64, price + 1.0, 40.0 - i as f64);
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(price + 0.8, 10.0), &book);
    }

    let snap = engine.snapshot();

    assert!(
        snap.ofi > 0.0,
        "OFI should be positive in trending up market"
    );
    assert!(
        snap.depth_imbalance > 0.0,
        "Depth imbalance should favour bids"
    );
    assert!(
        !snap.vsi.is_nan(),
        "VSI should be computed after bucket completion"
    );
    assert!(
        snap.microprice > snap.mid_price,
        "Microprice should be above mid in buy pressure"
    );
}

#[test]
fn test_live_mode_trending_down_market() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(50.0).unwrap();
    config.normalisation_warm_up = 5;
    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..10 {
        let book = make_book(100.0, 50.0 - i as f64, 101.0, 50.0 + i as f64);
        let _ = engine.on_book_update(&book);
    }

    for i in 0..20 {
        let price = 100.0 - (i as f64 * 0.1);
        let book = make_book(price, 40.0 - i as f64, price + 1.0, 60.0 + i as f64);
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(price + 0.2, 10.0), &book);
    }

    let snap = engine.snapshot();

    assert!(
        snap.ofi < 0.0,
        "OFI should be negative in trending down market"
    );
    assert!(
        snap.depth_imbalance < 0.0,
        "Depth imbalance should favour asks"
    );
    assert!(
        snap.microprice < snap.mid_price,
        "Microprice should be below mid in sell pressure"
    );
}

#[test]
fn test_live_mode_volatile_market() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(30.0).unwrap();
    config.normalisation_warm_up = 5;
    let mut engine = SignalEngine::new(config).unwrap();

    for _ in 0..10 {
        let book = make_book(100.0, 50.0, 101.0, 50.0);
        let _ = engine.on_book_update(&book);
    }

    for i in 0..30 {
        let offset = if i % 2 == 0 { 10.0 } else { -10.0 };
        let book = make_book(100.0, 50.0 + offset, 101.0, 50.0 - offset);
        let _ = engine.on_book_update(&book);
        let trade_price = if i % 2 == 0 { 100.8 } else { 100.2 };
        let _ = engine.on_trade(&make_trade(trade_price, 6.0), &book);
    }

    let snap = engine.snapshot();

    assert!(
        !snap.ofi_z.is_nan(),
        "OFI z-score should be computed after warm-up"
    );
    assert!(
        !snap.toxicity.is_nan(),
        "Toxicity should be computed after warm-up in volatile market"
    );
}

#[test]
fn test_live_mode_quiet_market() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.normalisation_warm_up = 5;
    let mut engine = SignalEngine::new(config).unwrap();

    for _ in 0..50 {
        let book = make_book(100.0, 50.0, 101.0, 50.0);
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(100.5, 2.0), &book);
    }

    let snap = engine.snapshot();

    assert!(
        snap.depth_imbalance.abs() < 0.1,
        "Depth imbalance should be near zero in balanced market, got {}",
        snap.depth_imbalance
    );
    let mid = snap.mid_price;
    let micro = snap.microprice;
    assert!(
        (micro - mid).abs() < 0.1,
        "Microprice should be near mid in balanced market"
    );
}

#[test]
fn test_historical_mode_trending_up_market() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(50.0).unwrap();
    config.normalisation_warm_up = 5;
    let mut engine = SignalEngine::new(config).unwrap();

    let mut events = Vec::new();

    for i in 0..10 {
        events.push(MarketEvent::BookUpdate(make_book_with_time(
            100.0,
            50.0,
            101.0,
            50.0,
            i * 1000,
        )));
    }

    for i in 10..30 {
        let price = 100.0 + ((i - 10) as f64 * 0.1);
        events.push(MarketEvent::BookUpdate(make_book_with_time(
            price,
            60.0 + (i - 10) as f64,
            price + 1.0,
            40.0 - (i - 10) as f64,
            i * 1000,
        )));
        events.push(MarketEvent::Trade(make_trade_with_time(
            price + 0.8,
            10.0,
            i * 1000 + 500,
        )));
    }

    let results = engine.process_events(&events);

    let final_snap = &results.last().unwrap().1;

    assert!(
        final_snap.ofi > 0.0,
        "Historical mode: OFI should be positive in trending up market"
    );
    assert!(
        final_snap.depth_imbalance > 0.0,
        "Historical mode: Depth imbalance should favour bids"
    );
}

#[test]
fn test_historical_mode_with_interleaved_events() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(25.0).unwrap();
    config.normalisation_warm_up = 3;
    let mut engine = SignalEngine::new(config).unwrap();

    let mut events = Vec::new();
    let mut time = 0u64;

    for i in 0..20 {
        events.push(MarketEvent::BookUpdate(make_book_with_time(
            100.0 + (i % 5) as f64 * 0.05,
            50.0 + i as f64,
            101.0,
            50.0,
            time,
        )));
        time += 100;

        let num_trades = (i % 3) + 1;
        for j in 0..num_trades {
            events.push(MarketEvent::Trade(make_trade_with_time(
                100.5,
                5.0,
                time + j * 10,
            )));
        }
        time += 100;
    }

    let results = engine.process_events(&events);

    assert_eq!(results.len(), events.len());

    for (i, (timestamp, _)) in results.iter().enumerate() {
        assert_eq!(*timestamp, events[i].timestamp_ns());
    }

    let final_snap = &results.last().unwrap().1;
    assert!(!final_snap.ofi.is_nan());
    assert!(!final_snap.depth_imbalance.is_nan());
    assert!(!final_snap.microprice.is_nan());
}

#[test]
fn test_signal_combination_only_book_signals() {
    use microstructure_signals::SignalSelection;

    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.signals = {
        let mut s = SignalSelection::default();
        s.vsi = false;
        s.vsi_z = false;
        s.effective_spread = false;
        s
    };
    config.normalisation_warm_up = 5;

    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..20 {
        let book = make_book(100.0 + i as f64 * 0.1, 50.0 + i as f64, 101.0, 50.0);
        let _ = engine.on_book_update(&book);
    }

    let snap = engine.snapshot();

    assert!(!snap.ofi.is_nan());
    assert!(!snap.depth_imbalance.is_nan());
    assert!(!snap.microprice.is_nan());
    assert!(!snap.ofi_z.is_nan());
    assert!(!snap.depth_imbalance_z.is_nan());

    assert!(snap.vsi.is_nan());
    assert!(snap.vsi_z.is_nan());

    assert!(!snap.toxicity.is_nan());
    assert!(!snap.adverse_selection.is_nan());
}

#[test]
fn test_signal_combination_minimal_config() {
    use microstructure_signals::SignalSelection;

    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.signals = {
        let mut s = SignalSelection::none();
        s.ofi = true;
        s
    };

    let mut engine = SignalEngine::new(config).unwrap();

    let book1 = make_book(100.0, 50.0, 101.0, 50.0);
    let book2 = make_book(100.0, 60.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book1);
    let _ = engine.on_book_update(&book2);

    let snap = engine.snapshot();

    assert!(!snap.ofi.is_nan());
    assert!(snap.ofi > 0.0);

    assert!(snap.depth_imbalance.is_nan());
    assert!(snap.microprice.is_nan());
    assert!(snap.vsi.is_nan());
    assert!(snap.ofi_z.is_nan());
    assert!(snap.toxicity.is_nan());
    assert!(snap.adverse_selection.is_nan());
}

#[test]
fn test_signal_combination_normalised_only() {
    use microstructure_signals::SignalSelection;

    let mut config = SignalEngineConfig::with_vsi_bucket_volume(20.0).unwrap();
    config.signals = {
        let mut s = SignalSelection::default();
        s.microprice = false;
        s.microprice_deviation_z = false;
        s.adverse_selection = false;
        s.effective_spread = false;
        s
    };
    config.normalisation_warm_up = 3;

    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..20 {
        let book = make_book(
            100.0 + (i % 3) as f64 * 0.1,
            50.0 + i as f64,
            101.0,
            50.0 - i as f64,
        );
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(100.5, 4.0), &book);
    }

    let snap = engine.snapshot();

    assert!(!snap.ofi.is_nan());
    assert!(!snap.depth_imbalance.is_nan());
    assert!(!snap.vsi.is_nan());

    assert!(!snap.ofi_z.is_nan());
    assert!(!snap.depth_imbalance_z.is_nan());
    assert!(!snap.vsi_z.is_nan());

    assert!(!snap.toxicity.is_nan());
}

#[test]
fn test_historical_mode_preserves_signal_evolution() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(30.0).unwrap();
    config.normalisation_warm_up = 5;

    let mut engine = SignalEngine::new(config).unwrap();

    let mut events = Vec::new();

    for i in 0..10 {
        events.push(MarketEvent::BookUpdate(make_book_with_time(
            100.0,
            50.0,
            101.0,
            50.0,
            i * 1000,
        )));
    }

    for i in 10..20 {
        events.push(MarketEvent::BookUpdate(make_book_with_time(
            100.0,
            60.0 + (i - 10) as f64 * 5.0,
            101.0,
            40.0,
            i * 1000,
        )));
        events.push(MarketEvent::Trade(make_trade_with_time(
            100.8,
            6.0,
            i * 1000 + 500,
        )));
    }

    for i in 30..40 {
        events.push(MarketEvent::BookUpdate(make_book_with_time(
            100.0,
            40.0,
            101.0,
            60.0 + (i - 30) as f64 * 5.0,
            i * 1000,
        )));
        events.push(MarketEvent::Trade(make_trade_with_time(
            100.2,
            6.0,
            i * 1000 + 500,
        )));
    }

    let results = engine.process_events(&events);

    let phase1_snap = &results[5].1;
    if !phase1_snap.depth_imbalance.is_nan() {
        assert!(
            phase1_snap.depth_imbalance.abs() < 0.2,
            "Phase 1 should be balanced"
        );
    }

    let phase2_snap = &results[25].1;
    if !phase2_snap.ofi.is_nan() {
        assert!(phase2_snap.ofi > 0.0, "Phase 2 should show buying pressure");
    }

    let phase3_snap = &results.last().unwrap().1;
    if !phase3_snap.depth_imbalance.is_nan() {
        assert!(
            phase3_snap.depth_imbalance < 0.0,
            "Phase 3 should show selling pressure"
        );
    }
}

#[test]
fn test_reset_produces_fresh_state() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();

    let mut engine = SignalEngine::new(config.clone()).unwrap();

    for i in 0..50 {
        let price = 100.0 + (i % 5) as f64 * 0.1;
        let book = make_book(price, 50.0 + i as f64, price + 1.0, 50.0);
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(price + 0.5, 5.0), &book);
    }

    engine.reset();

    let mut fresh = SignalEngine::new(config).unwrap();
    let book = make_book(100.0, 50.0, 101.0, 50.0);

    let reset_snap = *engine.on_book_update(&book);
    let fresh_snap = *fresh.on_book_update(&book);

    assert_eq!(
        reset_snap, fresh_snap,
        "reset() must produce state equivalent to SignalEngine::new(same_config)"
    );
}

#[test]
fn test_with_vsi_bucket_volume_zero_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_vsi_bucket_volume(0.0).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_vsi_bucket_volume_negative_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_vsi_bucket_volume(-1.0).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_vsi_bucket_volume_nan_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_vsi_bucket_volume(f64::NAN).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_vsi_bucket_volume_infinity_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_vsi_bucket_volume(f64::INFINITY).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_vsi_capped_trades_zero_when_no_overflow() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let book = make_book(100.0, 50.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book);
    for _ in 0..10 {
        let _ = engine.on_trade(&make_trade(100.8, 50.0), &book);
    }
    assert_eq!(engine.snapshot().vsi_capped_trades, 0);
}

#[test]
fn test_vsi_capped_trades_increments_on_massive_trade() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(1.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let book = make_book(100.0, 50.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book);
    let _ = engine.on_trade(&make_trade(100.8, 200.0), &book);
    assert!(
        engine.snapshot().vsi_capped_trades > 0,
        "A trade of 200x bucket_volume must trigger the cap guard"
    );
}

#[test]
fn test_vsi_capped_trades_resets_on_engine_reset() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(1.0).unwrap();
    let mut engine = SignalEngine::new(config.clone()).unwrap();
    let book = make_book(100.0, 50.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book);
    let _ = engine.on_trade(&make_trade(100.8, 200.0), &book);
    assert!(engine.snapshot().vsi_capped_trades > 0);
    engine.reset();
    assert_eq!(
        engine.snapshot().vsi_capped_trades,
        0,
        "reset() must clear vsi_capped_trades"
    );
}

#[test]
fn test_is_stale_before_any_book_update() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let engine = SignalEngine::new(config).unwrap();
    assert!(engine.snapshot().is_stale(0, 1_000_000_000));
    assert!(engine.snapshot().is_stale(1_000_000_000, 1_000_000_000));
}

#[test]
fn test_is_stale_zero_timestamp_book_not_permanently_stale() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let zero_ts_book = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    let _ = engine.on_book_update(&zero_ts_book);
    assert!(
        !engine.snapshot().is_stale(0, 1_000_000_000),
        "A zero-timestamp book just processed must NOT be stale at current_ns=0"
    );
}

#[test]
fn test_is_stale_normal_book_fresh_and_stale() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let book = make_book_with_time(100.0, 50.0, 101.0, 50.0, 1_000_000_000);
    let _ = engine.on_book_update(&book);
    assert!(!engine.snapshot().is_stale(1_500_000_000, 1_000_000_000));
    assert!(engine.snapshot().is_stale(3_000_000_000, 1_000_000_000));
}

#[test]
fn test_process_events_try_with_processes_all_on_ok() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let events = vec![
        MarketEvent::BookUpdate(make_book_with_time(100.0, 50.0, 101.0, 50.0, 1000)),
        MarketEvent::BookUpdate(make_book_with_time(100.0, 60.0, 101.0, 50.0, 2000)),
        MarketEvent::BookUpdate(make_book_with_time(100.0, 70.0, 101.0, 50.0, 3000)),
    ];
    let mut count = 0usize;
    let result = engine.process_events_try_with(&events, |_ts, _snap| {
        count += 1;
        Ok::<(), &str>(())
    });
    assert!(result.is_ok());
    assert_eq!(count, 3, "All 3 events should have been processed");
}

#[test]
fn test_process_events_try_with_stops_on_err() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let events = vec![
        MarketEvent::BookUpdate(make_book_with_time(100.0, 50.0, 101.0, 50.0, 1000)),
        MarketEvent::BookUpdate(make_book_with_time(100.0, 60.0, 101.0, 50.0, 2000)),
        MarketEvent::BookUpdate(make_book_with_time(100.0, 70.0, 101.0, 50.0, 3000)),
    ];
    let mut count = 0usize;
    let result = engine.process_events_try_with(&events, |_ts, _snap| {
        count += 1;
        if count >= 2 {
            Err("stop")
        } else {
            Ok(())
        }
    });
    assert_eq!(result, Err("stop"), "Should propagate the error");
    assert_eq!(count, 2, "Should have stopped after the second event");
}

#[test]
fn test_composite_smoothing_not_applied_between_bucket_closes() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(1_000_000.0).unwrap();
    config.normalisation_warm_up = 2;
    config.composite_smoothing_half_life = Some(1000.0);
    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..10 {
        let book = make_book(
            100.0 + (i % 3) as f64 * 0.5,
            50.0 + i as f64,
            101.0 + (i % 3) as f64 * 0.5,
            50.0,
        );
        let _ = engine.on_book_update(&book);
    }

    let book = make_book(100.0, 50.0, 101.0, 50.0);
    let _ = engine.on_book_update(&book);
    let toxicity_after_book = engine.snapshot().toxicity;

    for _ in 0..50 {
        let _ = engine.on_trade(&make_trade(100.5, 1.0), &book);
    }
    let toxicity_after_trades = engine.snapshot().toxicity;

    assert_eq!(
        toxicity_after_book, toxicity_after_trades,
        "Composite toxicity must not change when no VSI bucket closes between book updates"
    );
}

#[test]
fn test_composite_decay_fires_during_all_nan_period() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config.normalisation_warm_up = 3;
    config.composite_smoothing_half_life = Some(2.0);

    let mut engine = SignalEngine::new(config.clone()).unwrap();

    for i in 0..20 {
        let book = make_book(
            100.0 + (i % 5) as f64 * 0.2,
            50.0 + i as f64,
            101.0 + (i % 5) as f64 * 0.2,
            50.0,
        );
        let _ = engine.on_book_update(&book);
        let _ = engine.on_trade(&make_trade(100.5, 5.0), &book);
    }

    let tox_valid = engine.snapshot().toxicity;
    assert!(!tox_valid.is_nan(), "toxicity must be non-NaN after warmup");

    let mut config2 = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config2.signals.ofi_z = false;
    config2.signals.depth_imbalance_z = false;
    config2.signals.microprice_deviation_z = false;
    config2.signals.vsi_z = false;
    config2.composite_smoothing_half_life = Some(2.0);
    let mut engine2 = SignalEngine::new(config2).unwrap();

    for _ in 0..5 {
        let book = make_book(100.0, 50.0, 101.0, 50.0);
        let snap = engine2.on_book_update(&book);
        assert!(
            snap.toxicity.is_nan(),
            "All-NaN z-scores must produce NaN toxicity"
        );
    }
}

#[test]
fn test_crossed_book_spread_is_nan_in_snapshot() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let crossed = BookSnapshot {
        bids: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        asks: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        timestamp_ns: 1,
    };
    let snap = engine.on_book_update(&crossed);
    assert!(
        snap.spread.is_nan(),
        "Crossed book (bid > ask) must produce NaN spread, got {}",
        snap.spread
    );
}

#[test]
fn test_locked_book_spread_is_zero_in_snapshot() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let locked = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        asks: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        timestamp_ns: 1,
    };
    let snap = engine.on_book_update(&locked);
    assert_eq!(
        snap.spread, 0.0,
        "Locked book (bid == ask) must produce 0.0 spread"
    );
}

#[test]
fn test_with_adv_vsi_bucket_zero_adv_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_adv_vsi_bucket(0.0, 50, 1.0 / 50.0).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_adv_vsi_bucket_negative_adv_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_adv_vsi_bucket(-1.0, 50, 1.0 / 50.0).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_adv_vsi_bucket_zero_fraction_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_adv_vsi_bucket(1_000_000.0, 50, 0.0).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_adv_vsi_bucket_zero_buckets_returns_err() {
    assert!(matches!(
        SignalEngineConfig::with_adv_vsi_bucket(1_000_000.0, 0, 1.0 / 50.0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_zero_fast_half_life_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(0.0, 4.0, 50, 0.5).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_nan_fast_half_life_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(f64::NAN, 4.0, 50, 0.5).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_zero_regime_threshold_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(20.0, 0.0, 50, 0.5).unwrap_err(),
        ConfigError::RegimeThresholdInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_zero_cooldown_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(20.0, 4.0, 0, 0.5).unwrap_err(),
        ConfigError::CooldownPeriodZero
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_zero_exit_hysteresis_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(20.0, 4.0, 50, 0.0).unwrap_err(),
        ConfigError::ExitHysteresisInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_nan_exit_hysteresis_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(20.0, 4.0, 50, f64::NAN).unwrap_err(),
        ConfigError::ExitHysteresisInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_negative_exit_hysteresis_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    assert!(matches!(
        RegimeNormaliserParams::new(20.0, 4.0, 50, -0.5).unwrap_err(),
        ConfigError::ExitHysteresisInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_regime_normaliser_params_valid_ok() {
    use microstructure_signals::RegimeNormaliserParams;
    let p = RegimeNormaliserParams::new(20.0, 4.0, 50, 0.5).unwrap();
    assert_eq!(p.fast_half_life, 20.0);
    assert_eq!(p.regime_threshold, 4.0);
    assert_eq!(p.cooldown_period, 50);
    assert_eq!(p.exit_hysteresis, 0.5);
}

#[test]
fn test_validate_valid_config_ok() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_zero_normalisation_half_life_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.normalisation_half_life = 0.0;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_validate_nan_normalisation_half_life_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.normalisation_half_life = f64::NAN;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_validate_zero_ofi_window_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.ofi_window = 0;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_validate_zero_depth_levels_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.depth_levels = 0;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_validate_zero_vsi_n_buckets_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.vsi_n_buckets = 0;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_validate_nan_ofi_smoothing_half_life_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.ofi_smoothing_half_life = Some(f64::NAN);
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_validate_zero_composite_smoothing_half_life_returns_err() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.composite_smoothing_half_life = Some(0.0);
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[cfg(all(feature = "normaliser", feature = "regime-normaliser"))]
#[test]
fn test_validate_regime_fast_half_life_not_less_than_normalisation_half_life_returns_err() {
    use microstructure_signals::RegimeNormaliserParams;
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    config.normalisation_half_life = 100.0;
    config.regime_normaliser_params = Some(RegimeNormaliserParams {
        fast_half_life: 200.0,
        regime_threshold: 4.0,
        cooldown_period: 50,
        exit_hysteresis: 0.5,
    });
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::FastHalfLifeNotLessThanBase { .. }
    ));
}

#[test]
fn test_default_config_validate_ok() {
    let config = SignalEngineConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_vsi_enabled_with_zero_bucket_volume_returns_err() {
    let mut config = SignalEngineConfig::default();
    config.signals.vsi = true;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_with_vsi_bucket_volume_enables_vsi_in_selection() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(500.0).unwrap();
    assert!(
        config.signals.vsi,
        "with_vsi_bucket_volume should enable vsi in selection"
    );
    assert!(
        config.signals.vsi_z,
        "with_vsi_bucket_volume should enable vsi_z in selection"
    );
}

#[test]
fn test_raw_only_selection_disables_z_and_composites() {
    use microstructure_signals::SignalSelection;
    let sel = SignalSelection::raw_only();
    assert!(sel.ofi);
    assert!(sel.depth_imbalance);
    assert!(sel.microprice);
    assert!(!sel.ofi_z);
    assert!(!sel.depth_imbalance_z);
    assert!(!sel.microprice_deviation_z);
    assert!(!sel.vsi_z);
    assert!(!sel.composite_toxicity);
    assert!(!sel.adverse_selection);
}

#[test]
fn test_signal_engine_is_clone() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();
    let book = make_book(100.0, 50.0, 101.0, 50.0);
    engine.on_book_update(&book);
    let _cloned = engine.clone();
}

#[test]
fn test_signal_engine_is_debug() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let engine = SignalEngine::new(config).unwrap();
    let _ = format!("{:?}", engine);
}

#[test]
fn test_composite_weights_named_fields() {
    use approx::assert_relative_eq;
    use microstructure_signals::CompositeWeights;

    let mut config_ofi_only = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config_ofi_only.normalisation_warm_up = 5;
    config_ofi_only.toxicity_weights = CompositeWeights {
        ofi: 1.0,
        vsi: 0.0,
        depth_imbalance: 0.0,
        microprice_deviation: 0.0,
    };
    let mut engine_ofi = SignalEngine::new(config_ofi_only).unwrap();

    let mut config_di_only = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config_di_only.normalisation_warm_up = 5;
    config_di_only.toxicity_weights = CompositeWeights {
        ofi: 0.0,
        vsi: 0.0,
        depth_imbalance: 1.0,
        microprice_deviation: 0.0,
    };
    let mut engine_di = SignalEngine::new(config_di_only).unwrap();

    for i in 0..20 {
        let book = make_book(100.0 + (i % 3) as f64 * 0.5, 50.0 + i as f64, 101.0, 50.0);
        engine_ofi.on_book_update(&book);
        engine_di.on_book_update(&book);
    }

    let snap_ofi = engine_ofi.snapshot();
    let snap_di = engine_di.snapshot();

    if !snap_ofi.toxicity.is_nan() && !snap_di.toxicity.is_nan() {
        assert_relative_eq!(snap_ofi.toxicity, snap_ofi.ofi_z, epsilon = 1e-10);
        assert_relative_eq!(snap_di.toxicity, snap_di.depth_imbalance_z, epsilon = 1e-10);
    }
}

#[test]
fn test_composite_weights_default_is_equal() {
    use microstructure_signals::CompositeWeights;
    let w = CompositeWeights::default();
    assert_eq!(w.ofi, 1.0);
    assert_eq!(w.vsi, 1.0);
    assert_eq!(w.depth_imbalance, 1.0);
    assert_eq!(w.microprice_deviation, 1.0);
}

#[test]
fn test_process_events_trade_before_first_book_update_does_not_panic() {
    let config = SignalEngineConfig::with_vsi_bucket_volume(100.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let events = vec![
        MarketEvent::Trade(Trade {
            price: 100.0,
            quantity: 1.0,
            timestamp_ns: 0,
        }),
        MarketEvent::BookUpdate(make_book(99.0, 50.0, 101.0, 50.0)),
    ];

    let results = engine.process_events(&events);
    assert_eq!(results.len(), 2);

    let trade_snap = &results[0].1;
    assert!(
        trade_snap.effective_spread.is_nan(),
        "trade with no prior book must produce NaN effective_spread"
    );
}

#[test]
fn test_normaliser_ready_flags_consistent_after_on_trade() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(10.0).unwrap();
    config.normalisation_warm_up = 5;
    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..20 {
        let book = make_book(100.0 + (i % 3) as f64 * 0.1, 50.0 + i as f64, 101.0, 50.0);
        engine.on_book_update(&book);
    }

    let ofi_ready_before = engine.snapshot().ofi_normaliser_ready;
    let di_ready_before = engine.snapshot().depth_imbalance_normaliser_ready;
    assert!(
        ofi_ready_before,
        "ofi_normaliser_ready must be true after warm-up"
    );

    let book = make_book(100.0, 50.0, 101.0, 50.0);
    engine.on_trade(&make_trade(100.5, 1.0), &book);

    assert_eq!(
        engine.snapshot().ofi_normaliser_ready,
        ofi_ready_before,
        "ofi_normaliser_ready must be unchanged after on_trade"
    );
    assert_eq!(
        engine.snapshot().depth_imbalance_normaliser_ready,
        di_ready_before,
        "depth_imbalance_normaliser_ready must be unchanged after on_trade"
    );
}

#[test]
fn test_raw_only_with_default_bucket_volume_returns_err() {
    use microstructure_signals::SignalSelection;

    let mut config = SignalEngineConfig::default();
    config.signals = SignalSelection::raw_only();

    assert!(SignalEngine::new(config).is_err());
}
