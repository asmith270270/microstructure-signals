use microstructure_signals::types::{BookSnapshot, ClassifiedTrade, PriceLevel, Trade, TradeSide};
use microstructure_signals::{DepthImbalance, EwmaNormaliser, MicropriceCalculator, Ofi, Vsi};
use proptest::prelude::*;

fn valid_price_level(price: f64, qty: f64) -> PriceLevel {
    PriceLevel {
        price,
        quantity: qty,
    }
}

fn two_level_book(bid: f64, bid_qty: f64, ask: f64, ask_qty: f64) -> BookSnapshot {
    BookSnapshot::new(
        &[valid_price_level(bid, bid_qty)],
        &[valid_price_level(ask, ask_qty)],
        0,
    )
}

proptest! {
    #[test]
    fn prop_depth_imbalance_in_range(
        bid_qty in 0.001f64..1_000_000.0,
        ask_qty in 0.001f64..1_000_000.0,
        bid_price in 1.0f64..100_000.0,
    ) {
        let ask_price = bid_price + 0.01;
        let book = two_level_book(bid_price, bid_qty, ask_price, ask_qty);
        let mut di = DepthImbalance::new(5).unwrap();
        if let Some(v) = di.update(&book) {
            prop_assert!((-1.0..=1.0).contains(&v),
                "depth imbalance {v} out of [-1, 1]");
        }
    }

    #[test]
    fn prop_vsi_in_range(
        qty in 0.001f64..10_000.0,
        n_trades in 1usize..20,
        buy_fraction in 0.0f64..1.0,
    ) {
        let mut vsi = Vsi::new(100.0, 10).unwrap();
        let n_buys = ((n_trades as f64) * buy_fraction) as usize;
        for i in 0..n_trades {
            let side = if i < n_buys { TradeSide::Buy } else { TradeSide::Sell };
            let ct = ClassifiedTrade {
                trade: Trade { price: 100.0, quantity: qty, timestamp_ns: i as u64 },
                side,
            };
            vsi.update(&ct);
        }
        if let Some(v) = vsi.value() {
            prop_assert!((-1.0..=1.0).contains(&v),
                "VSI {v} out of [-1, 1]");
        }
    }

    #[test]
    fn prop_microprice_between_bid_and_ask(
        bid in 1.0f64..100_000.0,
        spread in 0.01f64..10.0,
        bid_qty in 0.001f64..1_000_000.0,
        ask_qty in 0.001f64..1_000_000.0,
    ) {
        let ask = bid + spread;
        let book = two_level_book(bid, bid_qty, ask, ask_qty);
        let mut mp = MicropriceCalculator::new();
        mp.update(&book);
        if let Some(microprice) = mp.microprice() {
            prop_assert!(microprice >= bid,
                "microprice {microprice} below bid {bid}");
            prop_assert!(microprice <= ask,
                "microprice {microprice} above ask {ask}");
        }
    }

    #[test]
    fn prop_microprice_deviation_in_range(
        bid in 1.0f64..100_000.0,
        spread in 0.01f64..10.0,
        bid_qty in 0.001f64..1_000_000.0,
        ask_qty in 0.001f64..1_000_000.0,
    ) {
        let ask = bid + spread;
        let book = two_level_book(bid, bid_qty, ask, ask_qty);
        let mut mp = MicropriceCalculator::new();
        mp.update(&book);
        if let Some(dev) = mp.deviation() {
            prop_assert!((-1.0..=1.0).contains(&dev),
                "microprice deviation {dev} out of [-1, 1]");
        }
    }

    #[test]
    fn prop_ofi_finite_on_valid_book(
        bid in 1.0f64..100_000.0,
        spread in 0.01f64..10.0,
        bid_qty in 0.001f64..1_000_000.0,
        ask_qty in 0.001f64..1_000_000.0,
    ) {
        let ask = bid + spread;
        let book = two_level_book(bid, bid_qty, ask, ask_qty);
        let mut ofi = Ofi::new(10).unwrap();
        ofi.update(&book);
        ofi.update(&book);
        if let Some(v) = ofi.value() {
            prop_assert!(v.is_finite(), "OFI {v} is not finite on valid book");
        }
    }

    #[test]
    fn prop_ewma_normaliser_zscore_reasonable(
        values in prop::collection::vec(
            (-1_000_000.0f64..1_000_000.0).prop_filter("finite", |v| v.is_finite()),
            55..100usize,
        )
    ) {
        let mut norm = EwmaNormaliser::new(20.0, 50).unwrap();
        for &v in &values {
            if let Some(z) = norm.update_and_normalise(v) {
                prop_assert!(z.is_finite(), "z-score {z} is not finite");
            }
        }
    }

    #[test]
    fn prop_depth_imbalance_symmetry(
        qty in 0.001f64..1_000_000.0,
        bid_price in 1.0f64..100_000.0,
    ) {
        let ask_price = bid_price + 0.01;
        let book_buy_heavy = two_level_book(bid_price, qty * 2.0, ask_price, qty);
        let book_sell_heavy = two_level_book(bid_price, qty, ask_price, qty * 2.0);

        let mut di = DepthImbalance::new(5).unwrap();
        let buy_heavy = di.update(&book_buy_heavy).unwrap_or(0.0);

        let mut di2 = DepthImbalance::new(5).unwrap();
        let sell_heavy = di2.update(&book_sell_heavy).unwrap_or(0.0);

        prop_assert!(
            (buy_heavy + sell_heavy).abs() < 1e-10,
            "depth imbalance not symmetric: {buy_heavy} vs {sell_heavy}"
        );
    }

    #[test]
    fn prop_depth_imbalance_balanced_book_is_zero(
        qty in 0.001f64..1_000_000.0,
        bid_price in 1.0f64..100_000.0,
    ) {
        let ask_price = bid_price + 0.01;
        let book = two_level_book(bid_price, qty, ask_price, qty);
        let mut di = DepthImbalance::new(5).unwrap();
        if let Some(v) = di.update(&book) {
            prop_assert!(v.abs() < 1e-10,
                "balanced book depth imbalance should be 0, got {v}");
        }
    }
}
