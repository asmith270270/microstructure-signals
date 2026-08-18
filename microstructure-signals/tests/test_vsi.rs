use approx::assert_relative_eq;
use microstructure_signals::types::{ClassifiedTrade, Trade, TradeSide};
use microstructure_signals::{ConfigError, Vsi};

fn make_trade(qty: f64, side: TradeSide) -> ClassifiedTrade {
    ClassifiedTrade {
        trade: Trade {
            price: 100.0,
            quantity: qty,
            timestamp_ns: 0,
        },
        side,
    }
}

#[test]
fn test_vsi_returns_none_before_first_bucket() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(50.0, TradeSide::Buy));
    assert!(vsi.value().is_none());
}

#[test]
fn test_vsi_single_bucket_all_buys() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(100.0, TradeSide::Buy));
    assert_relative_eq!(vsi.value().unwrap(), 1.0, epsilon = 1e-10);
}

#[test]
fn test_vsi_single_bucket_balanced() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(50.0, TradeSide::Buy));
    vsi.update(&make_trade(50.0, TradeSide::Sell));
    assert_relative_eq!(vsi.value().unwrap(), 0.0, epsilon = 1e-10);
}

#[test]
fn test_vsi_bucket_overflow_handling() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(40.0, TradeSide::Buy));
    vsi.update(&make_trade(30.0, TradeSide::Buy));
    vsi.update(&make_trade(40.0, TradeSide::Sell));

    assert_eq!(vsi.buckets_completed(), 1);

    let expected = (70.0 - 30.0) / 100.0;
    assert_relative_eq!(vsi.value().unwrap(), expected, epsilon = 1e-10);
}

#[test]
fn test_vsi_rolling_window() {
    let mut vsi = Vsi::new(100.0, 3).unwrap();

    vsi.update(&make_trade(100.0, TradeSide::Buy));
    vsi.update(&make_trade(100.0, TradeSide::Sell));
    vsi.update(&make_trade(50.0, TradeSide::Buy));
    vsi.update(&make_trade(50.0, TradeSide::Sell));

    assert_eq!(vsi.buckets_completed(), 3);

    vsi.update(&make_trade(100.0, TradeSide::Buy));

    assert_eq!(vsi.buckets_completed(), 4);
}

#[test]
fn test_vsi_large_trade_multiple_buckets() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(350.0, TradeSide::Buy));

    assert_eq!(vsi.buckets_completed(), 3);
    assert_relative_eq!(vsi.value().unwrap(), 1.0, epsilon = 1e-10);
}

#[test]
fn test_vsi_monotonic_with_imbalance() {
    let mut vsi = Vsi::new(100.0, 5).unwrap();

    for _ in 0..5 {
        vsi.update(&make_trade(80.0, TradeSide::Buy));
        vsi.update(&make_trade(20.0, TradeSide::Sell));
    }

    assert!(vsi.value().unwrap() > 0.5);
}

#[test]
fn test_vsi_sell_dominated_bucket_is_negative() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(100.0, TradeSide::Sell));
    assert_relative_eq!(vsi.value().unwrap(), -1.0, epsilon = 1e-10);
}

#[test]
fn test_vsi_mixed_sell_dominant_is_negative() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(30.0, TradeSide::Buy));
    vsi.update(&make_trade(70.0, TradeSide::Sell));
    assert_relative_eq!(vsi.value().unwrap(), -0.4, epsilon = 1e-10);
}

#[test]
fn test_vsi_capped_bucket_fill() {
    let mut vsi = Vsi::new(1.0, 50).unwrap();

    let start = std::time::Instant::now();
    vsi.update(&make_trade(10_000.0, TradeSide::Buy));
    let elapsed = start.elapsed();

    assert_eq!(vsi.buckets_completed(), 100);

    assert!(
        elapsed.as_millis() < 10,
        "Capped VSI took {}ms — loop was not bounded",
        elapsed.as_millis()
    );

    assert!(vsi.value().is_some());
}

#[test]
#[should_panic(expected = "trade quantity must be finite and positive")]
fn test_vsi_update_infinity_quantity_panics() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(f64::INFINITY, TradeSide::Buy));
}

#[test]
#[should_panic(expected = "trade quantity must be finite and positive")]
fn test_vsi_update_nan_quantity_panics() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(f64::NAN, TradeSide::Buy));
}

#[test]
#[should_panic(expected = "trade quantity must be finite and positive")]
fn test_vsi_update_zero_quantity_panics() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(0.0, TradeSide::Buy));
}

#[test]
#[should_panic(expected = "trade quantity must be finite and positive")]
fn test_vsi_update_negative_quantity_panics() {
    let mut vsi = Vsi::new(100.0, 10).unwrap();
    vsi.update(&make_trade(-1.0, TradeSide::Buy));
}

#[test]
fn test_vsi_new_zero_n_buckets_returns_err() {
    assert!(matches!(
        Vsi::new(100.0, 0).unwrap_err(),
        ConfigError::ZeroSizeParameter(_)
    ));
}

#[test]
fn test_vsi_new_zero_bucket_volume_returns_err() {
    assert!(matches!(
        Vsi::new(0.0, 10).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_vsi_new_negative_bucket_volume_returns_err() {
    assert!(matches!(
        Vsi::new(-1.0, 10).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}

#[test]
fn test_vsi_new_nan_bucket_volume_returns_err() {
    assert!(matches!(
        Vsi::new(f64::NAN, 10).unwrap_err(),
        ConfigError::VolumeInvalid(_)
    ));
}
