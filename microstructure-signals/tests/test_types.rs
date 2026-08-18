use approx::assert_relative_eq;
use microstructure_signals::types::{BookSnapshot, PriceLevel, SignalSnapshot, Trade, TradeSide};

#[test]
fn test_book_snapshot_mid_price() {
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
    assert_relative_eq!(book.mid_price().unwrap(), 100.5, epsilon = 1e-10);
}

#[test]
fn test_book_snapshot_spread() {
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
    assert_relative_eq!(book.spread().unwrap(), 1.0, epsilon = 1e-10);
}

#[test]
fn test_book_snapshot_empty_bids_returns_none() {
    let book = BookSnapshot {
        bids: vec![],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    assert!(book.best_bid().is_none());
    assert!(book.mid_price().is_none());
    assert!(book.spread().is_none());
    assert!(!book.is_valid());
}

#[test]
fn test_book_snapshot_empty_asks_returns_none() {
    let book = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        asks: vec![],
        timestamp_ns: 0,
    };
    assert!(book.best_ask().is_none());
    assert!(book.mid_price().is_none());
    assert!(book.spread().is_none());
    assert!(!book.is_valid());
}

#[test]
fn test_book_snapshot_crossed_book_is_invalid() {
    let locked = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        asks: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    assert!(!locked.is_valid());

    let crossed = BookSnapshot {
        bids: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        asks: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    assert!(!crossed.is_valid());
}

#[test]
fn test_book_snapshot_valid_book() {
    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 99.5,
                quantity: 100.0,
            },
        ],
        asks: vec![
            PriceLevel {
                price: 101.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 101.5,
                quantity: 100.0,
            },
        ],
        timestamp_ns: 1_000_000_000,
    };
    assert!(book.is_valid());
}

#[test]
fn test_book_snapshot_zero_quantity_is_invalid() {
    let book = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 0.0,
        }],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    assert!(!book.is_valid());
}

#[test]
fn test_trade_construction() {
    let trade = Trade {
        price: 100.5,
        quantity: 25.0,
        timestamp_ns: 1_000_000_000,
    };
    assert_relative_eq!(trade.price, 100.5, epsilon = 1e-10);
    assert_relative_eq!(trade.quantity, 25.0, epsilon = 1e-10);
}

#[test]
fn test_trade_side_enum() {
    assert_eq!(TradeSide::Buy, TradeSide::Buy);
    assert_ne!(TradeSide::Buy, TradeSide::Sell);
}

#[test]
fn test_book_snapshot_misordered_bids_is_invalid() {
    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 99.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 100.0,
                quantity: 50.0,
            },
        ],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    assert!(!book.is_valid());
}

#[test]
fn test_book_snapshot_misordered_asks_is_invalid() {
    let book = BookSnapshot {
        bids: vec![PriceLevel {
            price: 100.0,
            quantity: 50.0,
        }],
        asks: vec![
            PriceLevel {
                price: 102.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 101.0,
                quantity: 50.0,
            },
        ],
        timestamp_ns: 0,
    };
    assert!(!book.is_valid());
}

#[test]
fn test_book_snapshot_duplicate_bid_price_is_invalid() {
    let book = BookSnapshot {
        bids: vec![
            PriceLevel {
                price: 100.0,
                quantity: 50.0,
            },
            PriceLevel {
                price: 100.0,
                quantity: 30.0,
            },
        ],
        asks: vec![PriceLevel {
            price: 101.0,
            quantity: 50.0,
        }],
        timestamp_ns: 0,
    };
    assert!(!book.is_valid());
}

#[test]
fn test_signal_snapshot_default() {
    let snapshot = SignalSnapshot::default();
    assert!(snapshot.ofi.is_nan());
    assert!(snapshot.toxicity.is_nan());
}
