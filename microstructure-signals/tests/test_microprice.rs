use approx::assert_relative_eq;
use microstructure_signals::types::{BookSnapshot, PriceLevel};
use microstructure_signals::MicropriceCalculator;

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
fn test_mid_price_calculation() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 50.0, 102.0, 50.0));
    assert_relative_eq!(calc.mid_price().unwrap(), 101.0, epsilon = 1e-10);
}

#[test]
fn test_microprice_balanced_qty() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 50.0, 102.0, 50.0));
    assert_relative_eq!(calc.microprice().unwrap(), 101.0, epsilon = 1e-10);
}

#[test]
fn test_microprice_thin_ask() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 80.0, 102.0, 20.0));
    let microprice = calc.microprice().unwrap();
    let mid = calc.mid_price().unwrap();
    assert!(microprice > mid);
}

#[test]
fn test_microprice_thin_bid() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 20.0, 102.0, 80.0));
    let microprice = calc.microprice().unwrap();
    let mid = calc.mid_price().unwrap();
    assert!(microprice < mid);
}

#[test]
fn test_microprice_deviation_sign() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 80.0, 104.0, 20.0));
    let deviation = calc.deviation().unwrap();
    let microprice = calc.microprice().unwrap();
    let mid = calc.mid_price().unwrap();
    let half_spread = (104.0_f64 - 100.0_f64) / 2.0;
    assert_relative_eq!(deviation, (microprice - mid) / half_spread, epsilon = 1e-10);
}

#[test]
fn test_microprice_formula() {
    let mut calc = MicropriceCalculator::new();
    calc.update(&make_book(100.0, 80.0, 102.0, 20.0));
    let expected = (100.0 * 20.0 + 102.0 * 80.0) / 100.0;
    assert_relative_eq!(calc.microprice().unwrap(), expected, epsilon = 1e-10);
}
