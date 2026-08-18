#![no_main]

use libfuzzer_sys::fuzz_target;
use microstructure_signals::{SignalEngine, SignalEngineConfig};
use microstructure_signals::types::{BookSnapshot, PriceLevel, Trade};

fuzz_target!(|data: &[u8]| {
    if data.len() < 24 {
        return;
    }

    let config = SignalEngineConfig::with_vsi_bucket_volume(1000.0).unwrap();
    let mut engine = SignalEngine::new(config).unwrap();

    let bid_price = f64::from_bits(u64::from_le_bytes(data[0..8].try_into().unwrap()));
    let ask_price = f64::from_bits(u64::from_le_bytes(data[8..16].try_into().unwrap()));
    let trade_price = f64::from_bits(u64::from_le_bytes(data[16..24].try_into().unwrap()));

    if !bid_price.is_finite() || bid_price <= 0.0
        || !ask_price.is_finite() || ask_price <= 0.0
        || !trade_price.is_finite() || trade_price <= 0.0
        || bid_price >= ask_price
    {
        return;
    }

    let qty_bits = if data.len() >= 32 {
        u64::from_le_bytes(data[24..32].try_into().unwrap())
    } else {
        1f64.to_bits()
    };
    let quantity = f64::from_bits(qty_bits);
    if !quantity.is_finite() || quantity <= 0.0 {
        return;
    }

    let book = BookSnapshot::new(
        &[PriceLevel { price: bid_price, quantity: 10.0 }],
        &[PriceLevel { price: ask_price, quantity: 10.0 }],
        0,
    );
    engine.on_book_update(&book);

    let trade = Trade { price: trade_price, quantity, timestamp_ns: 0 };
    let snap = engine.on_trade(&trade, &book);
    let _ = snap.vsi;
    let _ = snap.effective_spread;
});
