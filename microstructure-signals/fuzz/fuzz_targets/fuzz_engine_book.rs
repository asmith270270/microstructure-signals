#![no_main]

use libfuzzer_sys::fuzz_target;
use microstructure_signals::{SignalEngine, SignalEngineConfig};
use microstructure_signals::types::{BookSnapshot, PriceLevel};

fn make_level(price_bits: u64, qty_bits: u64) -> Option<PriceLevel> {
    let price = f64::from_bits(price_bits);
    let quantity = f64::from_bits(qty_bits);
    if price.is_finite() && price > 0.0 && quantity.is_finite() && quantity > 0.0 {
        Some(PriceLevel { price, quantity })
    } else {
        None
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }

    let mut engine = SignalEngine::new(SignalEngineConfig::default()).unwrap();

    let chunks = data.chunks_exact(16);
    let mut bid_levels: Vec<PriceLevel> = Vec::new();
    let mut ask_levels: Vec<PriceLevel> = Vec::new();

    for (i, chunk) in chunks.enumerate() {
        let price_bits = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
        let qty_bits = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
        if let Some(level) = make_level(price_bits, qty_bits) {
            if i % 2 == 0 { bid_levels.push(level); } else { ask_levels.push(level); }
        }
    }

    bid_levels.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
    ask_levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

    if bid_levels.is_empty() || ask_levels.is_empty() {
        return;
    }

    for _ in 0..3 {
        let book = BookSnapshot::new(&bid_levels, &ask_levels, 0);
        let snap = engine.on_book_update(&book);
        let _ = snap.ofi;
        let _ = snap.depth_imbalance;
        let _ = snap.microprice;
        let _ = snap.spread;
    }
});
