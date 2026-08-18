use approx::assert_relative_eq;
use microstructure_signals::types::{BookSnapshot, PriceLevel};
use microstructure_signals::{ConfigError, DepthImbalance};

#[test]
fn test_depth_imbalance_balanced_book() {
    let mut di = DepthImbalance::new(5).unwrap();
    let book = BookSnapshot {
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
    assert_relative_eq!(di.update(&book).unwrap(), 0.0, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_heavy_bids() {
    let mut di = DepthImbalance::new(5).unwrap();
    let book = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 75.0,
        }],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 25.0,
        }],
        timestamp_ns: 0,
    };
    assert_relative_eq!(di.update(&book).unwrap(), 0.5, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_heavy_asks() {
    let mut di = DepthImbalance::new(5).unwrap();
    let book = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 25.0,
        }],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 75.0,
        }],
        timestamp_ns: 0,
    };
    assert_relative_eq!(di.update(&book).unwrap(), -0.5, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_single_level() {
    let mut di = DepthImbalance::new(1).unwrap();
    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 100.0,
            },
            PriceLevel {
                price: 99.0,
                quantity: 1000.0,
            },
        ],
        asks: vec![
            PriceLevel {
                price: 101.0,
                quantity: 100.0,
            },
            PriceLevel {
                price: 102.0,
                quantity: 1000.0,
            },
        ],
        timestamp_ns: 0,
    };
    assert_relative_eq!(di.update(&book).unwrap(), 0.0, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_fewer_levels_than_configured() {
    let mut di = DepthImbalance::new(5).unwrap();
    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 99.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 98.0,
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
            PriceLevel {
                price: 103.0,
                quantity: 50.0,
            },
        ],
        timestamp_ns: 0,
    };
    assert_relative_eq!(di.update(&book).unwrap(), 0.0, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_multiple_levels() {
    let mut di = DepthImbalance::new(3).unwrap();
    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 100.0,
            },
            PriceLevel {
                price: 99.0,
                quantity: 100.0,
            },
            PriceLevel {
                price: 98.0,
                quantity: 100.0,
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
            PriceLevel {
                price: 103.0,
                quantity: 50.0,
            },
        ],
        timestamp_ns: 0,
    };
    let expected = (300.0 - 150.0) / 450.0;
    assert_relative_eq!(di.update(&book).unwrap(), expected, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_ewma_smoothing() {
    let mut di = DepthImbalance::with_smoothing(1, 5.0).unwrap();

    let book_bid_heavy = BookSnapshot::new(
        &[PriceLevel {
            price: 100.0,
            quantity: 75.0,
        }],
        &[PriceLevel {
            price: 101.0,
            quantity: 25.0,
        }],
        0,
    );
    let v1 = di.update(&book_bid_heavy).unwrap();
    assert_relative_eq!(v1, 0.5, epsilon = 1e-10);

    let book_ask_heavy = BookSnapshot::new(
        &[PriceLevel {
            price: 100.0,
            quantity: 25.0,
        }],
        &[PriceLevel {
            price: 101.0,
            quantity: 75.0,
        }],
        0,
    );
    let v2 = di.update(&book_ask_heavy).unwrap();
    assert!(v2 > -0.5, "Smoothed value should lag behind raw: {v2}");
    assert!(
        v2 < 0.5,
        "Smoothed value should have moved from initial: {v2}"
    );
}

#[test]
fn test_depth_imbalance_ewma_converges() {
    let mut di = DepthImbalance::with_smoothing(1, 3.0).unwrap();

    let balanced = BookSnapshot::new(
        &[PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        &[PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        0,
    );

    for _ in 0..100 {
        di.update(&balanced);
    }
    assert_relative_eq!(di.value().unwrap(), 0.0, epsilon = 1e-6);
}

#[test]
fn test_depth_imbalance_no_smoothing_is_instantaneous() {
    let mut di = DepthImbalance::new(1).unwrap();

    let book1 = BookSnapshot::new(
        &[PriceLevel {
            price: 100.0,
            quantity: 75.0,
        }],
        &[PriceLevel {
            price: 101.0,
            quantity: 25.0,
        }],
        0,
    );
    di.update(&book1);

    let book2 = BookSnapshot::new(
        &[PriceLevel {
            price: 100.0,
            quantity: 25.0,
        }],
        &[PriceLevel {
            price: 101.0,
            quantity: 75.0,
        }],
        0,
    );
    let v = di.update(&book2).unwrap();
    assert_relative_eq!(v, -0.5, epsilon = 1e-10);
}

#[test]
fn test_depth_imbalance_half_life_accessor() {
    let di = DepthImbalance::with_smoothing(5, 20.0).unwrap();
    assert_relative_eq!(di.half_life().unwrap(), 20.0, epsilon = 1e-6);

    let di_plain = DepthImbalance::new(5).unwrap();
    assert!(di_plain.half_life().is_none());
}

#[test]
fn test_depth_imbalance_new_zero_levels_returns_err() {
    assert!(matches!(
        DepthImbalance::new(0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_depth_imbalance_with_smoothing_zero_levels_returns_err() {
    assert!(matches!(
        DepthImbalance::with_smoothing(0, 10.0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}
