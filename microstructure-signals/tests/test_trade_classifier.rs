use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade, TradeSide};
use microstructure_signals::{QuoteRuleClassifier, TickRuleClassifier};

fn make_trade(price: f64) -> Trade {
    Trade {
        price,
        quantity: 10.0,
        timestamp_ns: 0,
    }
}

fn make_book(bid: f64, ask: f64) -> BookSnapshot {
    BookSnapshot {
        bids: vec![PriceLevel {
            price: bid,
            quantity: 50.0,
        }],
        asks: vec![PriceLevel {
            price: ask,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    }
}

#[test]
fn test_tick_rule_uptick_is_buy() {
    let mut classifier = TickRuleClassifier::new();
    classifier.classify(&make_trade(100.0));
    assert_eq!(classifier.classify(&make_trade(101.0)), TradeSide::Buy);
}

#[test]
fn test_tick_rule_downtick_is_sell() {
    let mut classifier = TickRuleClassifier::new();
    classifier.classify(&make_trade(100.0));
    assert_eq!(classifier.classify(&make_trade(99.0)), TradeSide::Sell);
}

#[test]
fn test_tick_rule_zero_tick_carries_forward() {
    let mut classifier = TickRuleClassifier::new();
    classifier.classify(&make_trade(100.0));
    classifier.classify(&make_trade(101.0));
    assert_eq!(classifier.classify(&make_trade(101.0)), TradeSide::Buy);

    classifier.classify(&make_trade(100.0));
    assert_eq!(classifier.classify(&make_trade(100.0)), TradeSide::Sell);
}

#[test]
fn test_tick_rule_first_trade_defaults_to_buy() {
    let mut classifier = TickRuleClassifier::new();
    assert_eq!(classifier.classify(&make_trade(100.0)), TradeSide::Buy);
}

#[test]
fn test_tick_rule_sequence() {
    let mut classifier = TickRuleClassifier::new();
    assert_eq!(classifier.classify(&make_trade(100.0)), TradeSide::Buy);
    assert_eq!(classifier.classify(&make_trade(101.0)), TradeSide::Buy);
    assert_eq!(classifier.classify(&make_trade(101.0)), TradeSide::Buy);
    assert_eq!(classifier.classify(&make_trade(100.5)), TradeSide::Sell);
    assert_eq!(classifier.classify(&make_trade(100.5)), TradeSide::Sell);
    assert_eq!(classifier.classify(&make_trade(101.0)), TradeSide::Buy);
}

#[test]
fn test_quote_rule_above_mid_is_buy() {
    let mut classifier = QuoteRuleClassifier::new();
    let book = make_book(100.0, 101.0);
    assert_eq!(
        classifier.classify(&make_trade(100.6), &book),
        TradeSide::Buy
    );
}

#[test]
fn test_quote_rule_below_mid_is_sell() {
    let mut classifier = QuoteRuleClassifier::new();
    let book = make_book(100.0, 101.0);
    assert_eq!(
        classifier.classify(&make_trade(100.4), &book),
        TradeSide::Sell
    );
}

#[test]
fn test_quote_rule_at_mid_falls_back_to_tick() {
    let mut classifier = QuoteRuleClassifier::new();
    let book = make_book(100.0, 101.0);

    classifier.classify(&make_trade(100.4), &book);
    assert_eq!(
        classifier.classify(&make_trade(100.5), &book),
        TradeSide::Buy
    );

    classifier.classify(&make_trade(100.6), &book);
    assert_eq!(
        classifier.classify(&make_trade(100.5), &book),
        TradeSide::Sell
    );
}
