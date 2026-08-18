use approx::assert_relative_eq;
use microstructure_signals::types::{BookSnapshot, ClassifiedTrade, PriceLevel, Trade, TradeSide};
use microstructure_signals::{
    DepthImbalance, EwmaNormaliser, MicropriceCalculator, Ofi, SignalEngine, SignalEngineConfig,
    Vsi,
};

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

#[test]
fn scenario_1_simple_ofi_computation() {
    let mut ofi = Ofi::new(100).unwrap();

    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 70.0, 101.0, 50.0));

    assert_relative_eq!(ofi.value().unwrap(), 20.0, epsilon = 1e-10);
}

#[test]
fn scenario_2_bid_price_improvement_ofi() {
    let mut ofi = Ofi::new(100).unwrap();

    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.5, 30.0, 101.0, 50.0));

    assert_relative_eq!(ofi.value().unwrap(), 30.0, epsilon = 1e-10);
}

#[test]
fn scenario_3_microprice_deviation() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 80.0, 102.0, 20.0));

    let mid = calc.mid_price().unwrap();
    assert_relative_eq!(mid, 101.0, epsilon = 1e-10);

    let microprice = calc.microprice().unwrap();
    let expected_microprice = (100.0 * 20.0 + 102.0 * 80.0) / 100.0;
    assert_relative_eq!(microprice, expected_microprice, epsilon = 1e-10);
    assert_relative_eq!(microprice, 101.6, epsilon = 1e-10);

    let deviation = calc.deviation().unwrap();
    assert_relative_eq!(deviation, 0.6, epsilon = 1e-10);
}

#[test]
fn scenario_4_vsi_bucket_completion() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();

    vsi.update(&ClassifiedTrade {
        trade: Trade {
            price: 100.0,
            quantity: 40.0,
            timestamp_ns: 0,
        },
        side: TradeSide::Buy,
    });

    vsi.update(&ClassifiedTrade {
        trade: Trade {
            price: 100.0,
            quantity: 30.0,
            timestamp_ns: 0,
        },
        side: TradeSide::Buy,
    });

    vsi.update(&ClassifiedTrade {
        trade: Trade {
            price: 100.0,
            quantity: 40.0,
            timestamp_ns: 0,
        },
        side: TradeSide::Sell,
    });

    assert_eq!(vsi.buckets_completed(), 1);

    let expected_imbalance = (70.0_f64 - 30.0) / 100.0;
    assert_relative_eq!(vsi.value().unwrap(), expected_imbalance, epsilon = 1e-10);
}

#[test]
fn scenario_5_ewma_normaliser() {
    let mut norm = EwmaNormaliser::new(2.0, 3).unwrap();

    norm.update(10.0);
    norm.update(12.0);
    norm.update(14.0);
    let z = norm.update_and_normalise(16.0);

    assert!(z.is_some());
    assert!(z.unwrap() > 0.0);
}

#[test]
fn scenario_6_full_engine_integration() {
    let mut config = SignalEngineConfig::with_vsi_bucket_volume(50.0).unwrap();
    config.normalisation_warm_up = 5;
    config.ofi_window = 20;

    let mut engine = SignalEngine::new(config).unwrap();

    for i in 0..20 {
        let bid_qty = 50.0 + (i as f64) * 2.0;
        let ask_qty = 50.0;
        let book = make_book(100.0, bid_qty, 101.0, ask_qty);

        let _ = engine.on_book_update(&book);

        let trade = Trade {
            price: 100.5 + (i % 3) as f64 * 0.1,
            quantity: 25.0,
            timestamp_ns: i as u64,
        };
        let _ = engine.on_trade(&trade, &book);
    }

    let snapshot = engine.snapshot();

    assert!(!snapshot.ofi.is_nan());
    assert!(!snapshot.depth_imbalance.is_nan());
    assert!(!snapshot.microprice.is_nan());
    assert!(!snapshot.vsi.is_nan());

    assert!(snapshot.ofi > 0.0);
    assert!(snapshot.depth_imbalance > 0.0);
}

#[test]
fn scenario_depth_imbalance_calculation() {
    let mut di = DepthImbalance::new(5).unwrap();

    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 100.0,
            },
            PriceLevel {
                price: 99.0,
                quantity: 50.0,
            },
        ],
        asks: vec![
            PriceLevel {
                price: 101.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 102.0,
                quantity: 50.0,
            },
        ],
        timestamp_ns: 0,
    };

    let result = di.update(&book).unwrap();
    let expected = (150.0 - 100.0) / 250.0;
    assert_relative_eq!(result, expected, epsilon = 1e-10);
}

#[test]
fn scenario_ofi_price_level_changes() {
    let mut ofi = Ofi::new(100).unwrap();

    ofi.update(&make_book(100.0, 100.0, 102.0, 100.0));
    ofi.update(&make_book(101.0, 50.0, 102.0, 100.0));

    let result = ofi.value().unwrap();
    assert_relative_eq!(result, 50.0, epsilon = 1e-10);
}

#[test]
fn scenario_vsi_multiple_bucket_overflow() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();

    vsi.update(&ClassifiedTrade {
        trade: Trade {
            price: 100.0,
            quantity: 250.0,
            timestamp_ns: 0,
        },
        side: TradeSide::Buy,
    });

    assert_eq!(vsi.buckets_completed(), 2);
    assert_relative_eq!(vsi.value().unwrap(), 1.0, epsilon = 1e-10);
}
