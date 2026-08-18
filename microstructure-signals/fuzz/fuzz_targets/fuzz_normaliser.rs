#![no_main]

use libfuzzer_sys::fuzz_target;
use microstructure_signals::EwmaNormaliser;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }

    let half_life_bits = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let half_life = f64::from_bits(half_life_bits);
    if !half_life.is_finite() || half_life <= 0.0 || half_life > 1e9 {
        return;
    }

    let mut normaliser = EwmaNormaliser::new(half_life, 10).unwrap();

    for chunk in data[8..].chunks_exact(8) {
        let bits = u64::from_le_bytes(chunk.try_into().unwrap());
        let value = f64::from_bits(bits);
        if value.is_finite() {
            let _ = normaliser.update_and_normalise(value);
        }
    }

    let _ = normaliser.mean();
    let _ = normaliser.variance();
    let _ = normaliser.is_ready();
});
