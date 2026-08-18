use approx::assert_relative_eq;
use microstructure_signals::types::{BookSnapshot, PriceLevel};
use microstructure_signals::{ConfigError, MultiLevelOfi, Ofi};

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
fn test_ofi_returns_none_before_two_snapshots() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    assert!(ofi.value().is_none());
}

#[test]
fn test_ofi_bid_increase_gives_positive() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 70.0, 101.0, 50.0));
    assert_relative_eq!(ofi.value().unwrap(), 20.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_ask_increase_gives_negative() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 50.0, 101.0, 70.0));
    assert_relative_eq!(ofi.value().unwrap(), -20.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_bid_price_improvement() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.5, 30.0, 101.0, 50.0));
    assert_relative_eq!(ofi.value().unwrap(), 30.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_bid_price_drops() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(99.5, 30.0, 101.0, 50.0));
    assert_relative_eq!(ofi.value().unwrap(), -50.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_window_eviction() {
    let mut ofi = Ofi::new(3).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 60.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 70.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 80.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 90.0, 101.0, 50.0));
    assert_relative_eq!(ofi.value().unwrap(), 30.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_symmetric_changes_cancel() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 60.0, 101.0, 60.0));
    assert_relative_eq!(ofi.value().unwrap(), 0.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_ewma_smoothing() {
    let mut ofi_raw = Ofi::new(10).unwrap();
    let mut ofi_smooth = Ofi::with_smoothing(10, 5.0).unwrap();

    ofi_raw.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi_smooth.update(&make_book(100.0, 50.0, 101.0, 50.0));

    ofi_raw.update(&make_book(100.0, 150.0, 101.0, 50.0));
    ofi_smooth.update(&make_book(100.0, 150.0, 101.0, 50.0));

    assert_relative_eq!(
        ofi_raw.value().unwrap(),
        ofi_smooth.value().unwrap(),
        epsilon = 1e-10
    );

    let prev_smooth = ofi_smooth.value().unwrap();

    ofi_raw.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi_smooth.update(&make_book(100.0, 50.0, 101.0, 50.0));

    let raw_val = ofi_raw.value().unwrap();
    let smooth_val = ofi_smooth.value().unwrap();

    let raw_change = (raw_val - prev_smooth).abs();
    let smooth_change = (smooth_val - prev_smooth).abs();
    assert!(
        smooth_change < raw_change,
        "EWMA must change less than raw: smooth_change={smooth_change}, raw_change={raw_change}"
    );
}

#[test]
fn test_ofi_no_smoothing_matches_original() {
    let mut ofi = Ofi::new(10).unwrap();
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ofi.update(&make_book(100.0, 70.0, 101.0, 50.0));
    assert_relative_eq!(ofi.value().unwrap(), 20.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_half_life_accessor() {
    let ofi = Ofi::new(10).unwrap();
    assert!(ofi.half_life().is_none());

    let ofi_smooth = Ofi::with_smoothing(10, 25.0).unwrap();
    assert_relative_eq!(ofi_smooth.half_life().unwrap(), 25.0, epsilon = 1e-10);
}

#[test]
fn test_ofi_is_ready() {
    let mut ofi = Ofi::new(10).unwrap();
    assert!(!ofi.is_ready());
    ofi.update(&make_book(100.0, 50.0, 101.0, 50.0));
    assert!(!ofi.is_ready());
    ofi.update(&make_book(100.0, 60.0, 101.0, 50.0));
    assert!(ofi.is_ready());
}

#[test]
fn test_ofi_new_zero_window_returns_err() {
    assert!(matches!(
        Ofi::new(0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_ofi_with_smoothing_zero_window_returns_err() {
    assert!(matches!(
        Ofi::with_smoothing(0, 10.0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_ofi_normalised_zero_window_returns_err() {
    assert!(matches!(
        Ofi::normalised(0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_multilevel_ofi_zero_window_returns_err() {
    assert!(matches!(
        MultiLevelOfi::new(0, 0.5).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_multilevel_ofi_nan_decay_returns_err() {
    assert!(matches!(
        MultiLevelOfi::new(10, f64::NAN).unwrap_err(),
        ConfigError::DecayInvalid(_)
    ));
}

#[test]
fn test_multilevel_ofi_negative_decay_returns_err() {
    assert!(matches!(
        MultiLevelOfi::new(10, -0.5).unwrap_err(),
        ConfigError::DecayInvalid(_)
    ));
}

#[test]
fn test_multilevel_ofi_infinity_decay_returns_err() {
    assert!(matches!(
        MultiLevelOfi::new(10, f64::INFINITY).unwrap_err(),
        ConfigError::DecayInvalid(_)
    ));
}

#[test]
fn test_multilevel_ofi_zero_decay_ok() {
    let _ = MultiLevelOfi::new(10, 0.0).unwrap();
}

#[test]
fn test_multilevel_ofi_with_smoothing_negative_decay_returns_err() {
    assert!(matches!(
        MultiLevelOfi::with_smoothing(10, -1.0, 5.0).unwrap_err(),
        ConfigError::DecayInvalid(_)
    ));
}

#[allow(clippy::too_many_arguments)]
fn make_book_2levels(
    bid0_p: f64,
    bid0_q: f64,
    bid1_p: f64,
    bid1_q: f64,
    ask0_p: f64,
    ask0_q: f64,
    ask1_p: f64,
    ask1_q: f64,
) -> BookSnapshot {
    use microstructure_signals::types::PriceLevel;
    BookSnapshot {
        bids: vec![
            PriceLevel {
                price: bid0_p,
                quantity: bid0_q,
            },
            PriceLevel {
                price: bid1_p,
                quantity: bid1_q,
            },
        ],
        asks: vec![
            PriceLevel {
                price: ask0_p,
                quantity: ask0_q,
            },
            PriceLevel {
                price: ask1_p,
                quantity: ask1_q,
            },
        ],
        timestamp_ns: 0,
    }
}

#[test]
fn test_multilevel_ofi_zero_decay_equal_weights() {
    let mut ml = MultiLevelOfi::new(10, 0.0).unwrap();

    let book1 = make_book_2levels(100.0, 50.0, 99.0, 30.0, 101.0, 50.0, 102.0, 20.0);
    let book2 = make_book_2levels(100.0, 60.0, 99.0, 35.0, 101.0, 50.0, 102.0, 20.0);

    ml.update(&book1);
    ml.update(&book2);

    let delta_bid0 = 60.0 - 50.0;
    let delta_ask0 = 0.0;
    let delta_bid1 = 35.0 - 30.0;
    let delta_ask1 = 0.0;
    let expected = (delta_bid0 - delta_ask0) + (delta_bid1 - delta_ask1);
    assert_relative_eq!(ml.value().unwrap(), expected, epsilon = 1e-10);
}

#[test]
fn test_multilevel_ofi_decay_reduces_deep_level_contribution() {
    let mut ml_decayed = MultiLevelOfi::new(10, 1.0).unwrap();
    let mut ml_nodecay = MultiLevelOfi::new(10, 0.0).unwrap();

    let book1 = make_book_2levels(100.0, 50.0, 99.0, 50.0, 101.0, 50.0, 102.0, 50.0);
    let book2 = make_book_2levels(100.0, 60.0, 99.0, 60.0, 101.0, 50.0, 102.0, 50.0);

    ml_decayed.update(&book1);
    ml_decayed.update(&book2);
    ml_nodecay.update(&book1);
    ml_nodecay.update(&book2);

    assert!(
        ml_decayed.value().unwrap() < ml_nodecay.value().unwrap(),
        "decayed OFI must be less than equal-weight OFI for positive signal"
    );
}

#[test]
fn test_multilevel_ofi_window_eviction() {
    let mut ml = MultiLevelOfi::new(2, 0.0).unwrap();

    ml.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ml.update(&make_book(100.0, 80.0, 101.0, 50.0));
    ml.update(&make_book(100.0, 90.0, 101.0, 50.0));
    ml.update(&make_book(100.0, 95.0, 101.0, 50.0));

    assert_relative_eq!(ml.value().unwrap(), 15.0, epsilon = 1e-10);
}

#[test]
fn test_multilevel_ofi_ewma_smoothing_changes_less_than_raw() {
    let mut ml_raw = MultiLevelOfi::new(10, 0.0).unwrap();
    let mut ml_smooth = MultiLevelOfi::with_smoothing(10, 0.0, 5.0).unwrap();

    ml_raw.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ml_smooth.update(&make_book(100.0, 50.0, 101.0, 50.0));

    ml_raw.update(&make_book(100.0, 150.0, 101.0, 50.0));
    ml_smooth.update(&make_book(100.0, 150.0, 101.0, 50.0));

    let prev_smooth = ml_smooth.value().unwrap();

    ml_raw.update(&make_book(100.0, 50.0, 101.0, 50.0));
    ml_smooth.update(&make_book(100.0, 50.0, 101.0, 50.0));

    let raw_val = ml_raw.value().unwrap();
    let smooth_val = ml_smooth.value().unwrap();

    let raw_change = (raw_val - prev_smooth).abs();
    let smooth_change = (smooth_val - prev_smooth).abs();
    assert!(
        smooth_change < raw_change,
        "EWMA must change less than raw: smooth_change={smooth_change}, raw_change={raw_change}"
    );
}

#[test]
fn test_multilevel_ofi_fewer_book_levels_than_previous() {
    use microstructure_signals::types::PriceLevel;
    let mut ml = MultiLevelOfi::new(10, 0.0).unwrap();

    let book_2levels = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 99.0,
                quantity: 30.0,
            },
        ],
        asks: vec![
            PriceLevel {
                price: 101.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 102.0,
                quantity: 20.0,
            },
        ],
        timestamp_ns: 0,
    };
    let book_1level = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 60.0,
        }],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 55.0,
        }],
        timestamp_ns: 1,
    };

    ml.update(&book_2levels);
    ml.update(&book_1level);

    let delta_bid0 = 60.0 - 50.0;
    let delta_ask0 = 55.0 - 50.0;
    let expected = delta_bid0 - delta_ask0;
    assert_relative_eq!(ml.value().unwrap(), expected, epsilon = 1e-10);
}
