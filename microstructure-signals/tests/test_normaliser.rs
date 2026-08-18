use microstructure_signals::{ConfigError, EwmaNormaliser};

#[test]
fn test_normaliser_returns_none_during_warmup() {
    let mut norm = EwmaNormaliser::new(10.0, 5).unwrap();
    for _ in 0..4 {
        assert!(norm.update_and_normalise(10.0).is_none());
    }
}

#[test]
fn test_normaliser_constant_input_zero_zscore() {
    let mut norm = EwmaNormaliser::new(10.0, 3).unwrap();
    for _ in 0..10 {
        norm.update(10.0);
    }
    norm.update(11.0);
    for _ in 0..10 {
        norm.update(10.0);
    }

    let z = norm.normalise(norm.mean());
    assert!(
        z.is_some(),
        "normaliser must be ready after 21 mixed updates"
    );
    assert!(z.unwrap().abs() < 0.1, "z-score of mean must be near zero");
}

#[test]
fn test_normaliser_above_mean_positive_zscore() {
    let mut norm = EwmaNormaliser::new(10.0, 3).unwrap();
    for i in 0..20 {
        norm.update(i as f64);
    }
    let mean = norm.mean();
    let z = norm.normalise(mean + norm.std_dev() * 2.0);
    assert!(z.is_some());
    assert!(z.unwrap() > 0.0);
}

#[test]
fn test_normaliser_below_mean_negative_zscore() {
    let mut norm = EwmaNormaliser::new(10.0, 3).unwrap();
    for i in 0..20 {
        norm.update(i as f64);
    }
    let mean = norm.mean();
    let z = norm.normalise(mean - norm.std_dev() * 2.0);
    assert!(z.is_some());
    assert!(z.unwrap() < 0.0);
}

#[test]
fn test_normaliser_half_life_decay() {
    let mut norm = EwmaNormaliser::new(2.0, 1).unwrap();
    norm.update(100.0);
    norm.update(0.0);
    norm.update(0.0);
    norm.update(0.0);

    let mean = norm.mean();
    assert!(mean < 50.0);
    assert!(mean > 0.0);
}

#[test]
fn test_normaliser_near_zero_variance_returns_none() {
    let mut norm = EwmaNormaliser::new(10.0, 3).unwrap();
    for _ in 0..10 {
        norm.update(10.0);
    }
    assert!(norm.normalise(10.0).is_none());
}

#[test]
fn test_normaliser_is_ready() {
    let mut norm = EwmaNormaliser::new(10.0, 5).unwrap();
    assert!(!norm.is_ready());
    for i in 0..10 {
        norm.update(i as f64);
    }
    assert!(norm.is_ready());
}

#[test]
fn test_new_seeded_nan_half_life_returns_err() {
    assert!(matches!(
        EwmaNormaliser::new_seeded(f64::NAN, 10, 0.0, 1.0).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_new_seeded_negative_half_life_returns_err() {
    assert!(matches!(
        EwmaNormaliser::new_seeded(-1.0, 10, 0.0, 1.0).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_new_seeded_zero_half_life_returns_err() {
    assert!(matches!(
        EwmaNormaliser::new_seeded(0.0, 10, 0.0, 1.0).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_new_seeded_infinity_half_life_returns_err() {
    assert!(matches!(
        EwmaNormaliser::new_seeded(f64::INFINITY, 10, 0.0, 1.0).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_new_seeded_valid_args_is_warmup_complete() {
    use approx::assert_relative_eq;
    let norm = EwmaNormaliser::new_seeded(100.0, 50, 10.0, 4.0).unwrap();
    assert!(
        norm.is_warmup_complete(),
        "seeded normaliser must immediately be count-ready"
    );
    assert_relative_eq!(norm.mean(), 10.0, epsilon = 1e-12);
    assert_relative_eq!(norm.variance(), 4.0, epsilon = 1e-12);
}

#[test]
fn test_new_seeded_negative_variance_returns_err() {
    assert!(matches!(
        EwmaNormaliser::new_seeded(100.0, 10, 0.0, -1.0).unwrap_err(),
        ConfigError::VarianceInvalid(_)
    ));
}

#[test]
#[should_panic(expected = "value must be finite")]
fn test_normaliser_update_nan_panics() {
    let mut norm = EwmaNormaliser::new(10.0, 5).unwrap();
    norm.update(f64::NAN);
}

#[test]
#[should_panic(expected = "value must be finite")]
fn test_normaliser_update_infinity_panics() {
    let mut norm = EwmaNormaliser::new(10.0, 5).unwrap();
    norm.update(f64::INFINITY);
}

#[test]
#[should_panic(expected = "value must be finite")]
fn test_normaliser_update_neg_infinity_panics() {
    let mut norm = EwmaNormaliser::new(10.0, 5).unwrap();
    norm.update(f64::NEG_INFINITY);
}

#[test]
#[should_panic(expected = "value must be finite")]
fn test_normaliser_update_and_normalise_nan_panics() {
    let mut norm = EwmaNormaliser::new(10.0, 5).unwrap();
    norm.update_and_normalise(f64::NAN);
}

#[test]
fn test_normaliser_update_finite_values_ok() {
    let mut norm = EwmaNormaliser::new(10.0, 3).unwrap();
    norm.update(0.0);
    norm.update(-1.0);
    norm.update(1e10);
    assert!(norm.is_warmup_complete());
}
