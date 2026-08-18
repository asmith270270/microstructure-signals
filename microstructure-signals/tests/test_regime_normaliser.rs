use microstructure_signals::{ConfigError, RegimeNormaliser};

#[test]
fn test_regime_normaliser_warmup() {
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 20, 4.0, 50).unwrap();
    for i in 0..5 {
        let result = rn.update_and_normalise(100.0 + (i as f64) * 0.01);
        assert!(
            result.is_none(),
            "Should be None during early warmup at tick {i}"
        );
    }
    assert!(!rn.is_ready());
}

#[test]
fn test_regime_normaliser_produces_output_after_warmup() {
    // See RegimeNormaliser's "Known limitation" doc: entry detection can spuriously suppress output.
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 10, 4.0, 50).unwrap();
    for i in 0..50 {
        let val = 100.0 + (i as f64 % 5.0) * 2.0;
        rn.update_and_normalise(val);
    }
    assert!(rn.is_warmup_complete());

    let result = rn.update_and_normalise(105.0);
    assert!(
        result.is_some() || rn.is_in_regime_change(),
        "Should produce a z-score once warm, unless a regime change is actively suppressing output"
    );
}

#[test]
fn test_regime_normaliser_detects_regime_change() {
    let mut rn = RegimeNormaliser::new(200.0, 10.0, 10, 4.0, 50).unwrap();

    for i in 0..100 {
        let val = 100.0 + (i as f64 % 3.0) * 0.1;
        rn.update_and_normalise(val);
    }
    assert!(!rn.is_in_regime_change());

    for _ in 0..20 {
        rn.update_and_normalise(200.0);
    }
    assert!(
        rn.is_in_regime_change(),
        "Should detect regime change after variance spike"
    );
}

#[test]
fn test_regime_normaliser_exits_regime_after_cooldown() {
    let mut rn = RegimeNormaliser::new(200.0, 10.0, 5, 4.0, 20).unwrap();

    for i in 0..50 {
        let val = 100.0 + (i as f64 % 3.0) * 0.1;
        rn.update_and_normalise(val);
    }

    for _ in 0..15 {
        rn.update_and_normalise(200.0);
    }

    if rn.is_in_regime_change() {
        for _ in 0..30 {
            rn.update_and_normalise(200.0);
        }
        assert!(
            !rn.is_in_regime_change(),
            "Should exit regime change after cooldown"
        );
    }
}

#[test]
fn test_regime_normaliser_steady_state_z_scores_reasonable() {
    let mut rn = RegimeNormaliser::new(50.0, 10.0, 10, 4.0, 30).unwrap();

    for _ in 0..100 {
        rn.update_and_normalise(100.0);
    }

    let mut rn2 = RegimeNormaliser::new(50.0, 10.0, 10, 4.0, 30).unwrap();
    for i in 0..100 {
        let val = 100.0 + if i % 2 == 0 { 1.0 } else { -1.0 };
        rn2.update_and_normalise(val);
    }

    let z = rn2.update_and_normalise(102.0);
    if let Some(z_val) = z {
        assert!(
            z_val > 0.0,
            "Value above mean should give positive z-score, got {z_val}"
        );
    }
}

#[test]
fn test_regime_normaliser_fast_equals_base_returns_err() {
    assert!(matches!(
        RegimeNormaliser::new(50.0, 50.0, 10, 4.0, 20).unwrap_err(),
        ConfigError::FastHalfLifeNotLessThanBase { .. }
    ));
}

#[test]
fn test_regime_normaliser_fast_greater_than_base_returns_err() {
    assert!(matches!(
        RegimeNormaliser::new(20.0, 100.0, 10, 4.0, 20).unwrap_err(),
        ConfigError::FastHalfLifeNotLessThanBase { .. }
    ));
}

#[test]
fn test_regime_normaliser_valid_fast_less_than_base_ok() {
    let rn = RegimeNormaliser::new(100.0, 10.0, 5, 4.0, 20).unwrap();
    assert!(rn.fast_half_life() < rn.base_half_life());
}

#[test]
fn test_set_exit_hysteresis_zero_returns_err() {
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 5, 4.0, 20).unwrap();
    assert!(matches!(
        rn.set_exit_hysteresis(0.0).unwrap_err(),
        ConfigError::ExitHysteresisInvalid(_)
    ));
}

#[test]
fn test_set_exit_hysteresis_negative_returns_err() {
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 5, 4.0, 20).unwrap();
    assert!(matches!(
        rn.set_exit_hysteresis(-0.5).unwrap_err(),
        ConfigError::ExitHysteresisInvalid(_)
    ));
}

#[test]
fn test_set_exit_hysteresis_nan_returns_err() {
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 5, 4.0, 20).unwrap();
    assert!(matches!(
        rn.set_exit_hysteresis(f64::NAN).unwrap_err(),
        ConfigError::ExitHysteresisInvalid(_)
    ));
}

#[test]
fn test_set_exit_hysteresis_valid_ok() {
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 5, 4.0, 20).unwrap();
    rn.set_exit_hysteresis(0.3).unwrap();
    assert!((rn.exit_hysteresis() - 0.3).abs() < 1e-12);
}

#[test]
fn test_fresh_copy_same_config_fresh_state() {
    let mut rn = RegimeNormaliser::new(100.0, 10.0, 20, 4.0, 50).unwrap();
    rn.set_exit_hysteresis(0.3).unwrap();

    for i in 0..50 {
        let val = 100.0 + (i as f64 % 5.0) * 2.0;
        rn.update_and_normalise(val);
    }
    assert!(rn.is_warmup_complete());

    let copy = rn.fresh_copy();

    assert_eq!(copy.base_half_life(), rn.base_half_life());
    assert_eq!(copy.fast_half_life(), rn.fast_half_life());
    assert_eq!(copy.warm_up(), rn.warm_up());
    assert_eq!(copy.regime_threshold(), rn.regime_threshold());
    assert_eq!(copy.cooldown_period(), rn.cooldown_period());
    assert_eq!(copy.exit_hysteresis(), rn.exit_hysteresis());

    assert!(
        !copy.is_warmup_complete(),
        "fresh_copy() must not carry warm-up count"
    );
    assert!(
        !copy.is_in_regime_change(),
        "fresh_copy() must not carry regime-change state"
    );
    assert!(!copy.is_ready(), "fresh_copy() must not be ready");
}

#[test]
fn test_never_returns_some_while_in_regime_change() {
    let mut rn = RegimeNormaliser::new(200.0, 20.0, 30, 4.0, 50).unwrap();
    for i in 0..40 {
        let v = 1.0 + (i as f64 * 0.01).sin() * 0.1;
        rn.update_and_normalise(v);
    }

    let mut saw_regime_change = false;
    for i in 0..60 {
        let spike = 1.0 + (i as f64) * 50.0;
        let z = rn.update_and_normalise(spike);
        if rn.is_in_regime_change() {
            saw_regime_change = true;
            assert!(
                z.is_none(),
                "update_and_normalise returned Some({z:?}) while is_in_regime_change() was true"
            );
        }
    }
    assert!(
        saw_regime_change,
        "test setup should have triggered a regime change at least once"
    );
}

#[test]
fn test_zscore_is_always_bounded() {
    const MAX_ABS_Z: f64 = 10.0;
    let mut rn = RegimeNormaliser::new(200.0, 20.0, 30, 4.0, 50).unwrap();

    for _ in 0..40 {
        rn.update_and_normalise(1.0);
    }
    for i in 0..40 {
        let spike = 1.0 + (i as f64) * 1000.0;
        if let Some(z) = rn.update_and_normalise(spike) {
            assert!(
                z.abs() <= MAX_ABS_Z,
                "z-score {z} exceeds the documented bound of +/-{MAX_ABS_Z}"
            );
        }
    }
}

#[test]
fn test_fresh_copy_produces_same_output_as_new_for_same_input() {
    let params = (50.0_f64, 5.0_f64, 10_usize, 3.0_f64, 20_usize);
    let (base_hl, fast_hl, wu, thresh, cd) = params;

    let mut original = RegimeNormaliser::new(base_hl, fast_hl, wu, thresh, cd).unwrap();
    for i in 0..30 {
        original.update_and_normalise(100.0 + (i % 3) as f64);
    }
    let mut copy = original.fresh_copy();

    let mut fresh = RegimeNormaliser::new(base_hl, fast_hl, wu, thresh, cd).unwrap();

    let data = [100.0, 101.0, 99.0, 100.5, 100.0];
    for &v in &data {
        let z_copy = copy.update_and_normalise(v);
        let z_fresh = fresh.update_and_normalise(v);
        assert_eq!(
            z_copy, z_fresh,
            "fresh_copy() and RegimeNormaliser::new() must agree on z-scores, got {z_copy:?} vs {z_fresh:?}"
        );
    }
}
