use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade, TradeSide};
use microstructure_signals::EffectiveSpread;

fn make_book(best_bid: f64, best_ask: f64) -> BookSnapshot {
    BookSnapshot::new(
        &[PriceLevel {
            price: best_bid,
            quantity: 10.0,
        }],
        &[PriceLevel {
            price: best_ask,
            quantity: 10.0,
        }],
        0,
    )
}

fn make_trade(price: f64) -> Trade {
    Trade {
        price,
        quantity: 1.0,
        timestamp_ns: 0,
    }
}

#[test]
fn test_effective_spread_buy_at_ask() {
    let mut es = EffectiveSpread::new();
    let book = make_book(100.0, 101.0);
    let trade = make_trade(101.0);
    let mid = 100.5;

    es.update(&trade, &book, TradeSide::Buy);

    let spread = es.effective_spread().unwrap();
    assert!((spread - 1.0).abs() < 1e-10);

    let relative = es.relative_effective_spread().unwrap();
    assert!((relative - 1.0 / mid).abs() < 1e-10);
}

#[test]
fn test_effective_spread_sell_at_bid() {
    let mut es = EffectiveSpread::new();
    let book = make_book(100.0, 101.0);
    let trade = make_trade(100.0);

    es.update(&trade, &book, TradeSide::Sell);

    let spread = es.effective_spread().unwrap();
    assert!((spread - 1.0).abs() < 1e-10);
}

#[test]
fn test_effective_spread_buy_inside_spread() {
    let mut es = EffectiveSpread::new();
    let book = make_book(100.0, 102.0);
    let trade = make_trade(101.5);

    es.update(&trade, &book, TradeSide::Buy);

    let spread = es.effective_spread().unwrap();
    assert!((spread - 1.0).abs() < 1e-10);
}

#[test]
fn test_effective_spread_negative_when_price_improvement() {
    let mut es = EffectiveSpread::new();
    let book = make_book(100.0, 102.0);
    let trade = make_trade(100.5);

    es.update(&trade, &book, TradeSide::Buy);

    let spread = es.effective_spread().unwrap();
    assert!((spread - (-1.0)).abs() < 1e-10);
}

#[test]
fn test_effective_spread_updates_overwrite() {
    let mut es = EffectiveSpread::new();
    let book = make_book(100.0, 101.0);

    es.update(&make_trade(101.0), &book, TradeSide::Buy);
    assert!(es.effective_spread().is_some());

    let book2 = make_book(200.0, 202.0);
    es.update(&make_trade(202.0), &book2, TradeSide::Buy);

    let spread = es.effective_spread().unwrap();
    assert!((spread - 2.0).abs() < 1e-10);
}

#[test]
fn test_effective_spread_none_before_update() {
    let es = EffectiveSpread::new();
    assert!(es.effective_spread().is_none());
    assert!(es.relative_effective_spread().is_none());
}

#[test]
fn test_effective_spread_empty_book() {
    let mut es = EffectiveSpread::new();
    let book = BookSnapshot::new(&[], &[], 0);
    let trade = make_trade(100.0);

    es.update(&trade, &book, TradeSide::Buy);
    assert!(es.effective_spread().is_none());
}

#[test]
fn test_effective_spread_empty_book_clears_stale_value() {
    let mut es = EffectiveSpread::new();

    es.update(&make_trade(101.0), &make_book(100.0, 101.0), TradeSide::Buy);
    assert!(es.effective_spread().is_some());

    let empty_book = BookSnapshot::new(&[], &[], 0);
    es.update(&make_trade(101.0), &empty_book, TradeSide::Buy);
    assert!(
        es.effective_spread().is_none(),
        "empty-book update must clear stale effective spread"
    );
    assert!(
        es.relative_effective_spread().is_none(),
        "empty-book update must clear stale relative effective spread"
    );
}
