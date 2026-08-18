use approx::assert_relative_eq;
use microstructure_signals::{CompositeToxicity, ConfigError};

#[test]
fn test_composite_equal_weights() {
    let mut comp = CompositeToxicity::new(vec![1.0, 1.0, 1.0], None).unwrap();
    let result = comp.update(&[1.0, 0.0, -1.0]);
    assert_relative_eq!(result.unwrap(), 0.0, epsilon = 1e-10);
}

#[test]
fn test_composite_weighted() {
    let mut comp = CompositeToxicity::new(vec![2.0, 1.0, 1.0], None).unwrap();
    let result = comp.update(&[1.0, 0.0, 0.0]);
    assert_relative_eq!(result.unwrap(), 0.5, epsilon = 1e-10);
}

#[test]
fn test_composite_all_none_returns_none() {
    let mut comp = CompositeToxicity::new(vec![1.0, 1.0, 1.0], None).unwrap();
    let result = comp.update(&[f64::NAN, f64::NAN, f64::NAN]);
    assert!(result.is_none());
}

#[test]
fn test_composite_partial_none_rescales() {
    let mut comp = CompositeToxicity::new(vec![1.0, 1.0, 1.0], None).unwrap();
    let result = comp.update(&[1.0, f64::NAN, f64::NAN]);
    assert!(result.is_some());
}

#[test]
fn test_composite_smoothing() {
    let mut comp = CompositeToxicity::new(vec![1.0], Some(2.0)).unwrap();

    comp.update(&[0.0]);
    let first = comp.value().unwrap();

    comp.update(&[10.0]);
    let second = comp.value().unwrap();

    assert!(second > first);
    assert!(second < 10.0);
}

#[test]
fn test_composite_value_persistence() {
    let mut comp = CompositeToxicity::new(vec![1.0, 1.0], Some(5.0)).unwrap();
    comp.update(&[1.0, 1.0]);
    let val = comp.value();
    assert!(val.is_some());
}

#[test]
fn test_composite_value_none_after_nan_update() {
    let mut comp = CompositeToxicity::new(vec![1.0, 1.0], Some(5.0)).unwrap();

    let r1 = comp.update(&[1.0, 1.0]);
    assert!(r1.is_some(), "valid update should return Some");
    assert!(
        comp.value().is_some(),
        "value() should be Some after valid update"
    );

    let r2 = comp.update(&[f64::NAN, f64::NAN]);
    assert!(r2.is_none(), "all-NaN update should return None");
    assert!(
        comp.value().is_none(),
        "value() must return None after all-NaN update, not the decaying accumulator"
    );

    let r3 = comp.update(&[2.0, 2.0]);
    assert!(
        r3.is_some(),
        "valid update after NaN period should return Some"
    );
    assert!(
        comp.value().is_some(),
        "value() should be Some after resuming valid input"
    );
}

#[test]
fn test_composite_value_none_after_nan_no_smoothing() {
    let mut comp = CompositeToxicity::new(vec![1.0], None).unwrap();
    comp.update(&[3.0]);
    assert!(comp.value().is_some());

    comp.update(&[f64::NAN]);
    assert!(
        comp.value().is_none(),
        "value() must be None after all-NaN update (no smoothing)"
    );
}

#[test]
fn test_composite_smoothing_zero_half_life_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![1.0], Some(0.0)).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_composite_smoothing_negative_half_life_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![1.0], Some(-1.0)).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_composite_smoothing_nan_half_life_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![1.0], Some(f64::NAN)).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_composite_smoothing_infinity_half_life_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![1.0], Some(f64::INFINITY)).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_composite_with_equal_weights_smoothing_zero_returns_err() {
    assert!(matches!(
        CompositeToxicity::with_equal_weights(Some(0.0)).unwrap_err(),
        ConfigError::HalfLifeInvalid(_)
    ));
}

#[test]
fn test_composite_smoothing_valid_half_life_ok() {
    let mut comp = CompositeToxicity::new(vec![1.0, 1.0], Some(10.0)).unwrap();
    let v = comp.update(&[1.0, 1.0]);
    assert!(v.is_some());
}

#[test]
fn test_composite_extra_z_scores_beyond_weights_are_skipped() {
    use approx::assert_relative_eq;

    let mut comp1 = CompositeToxicity::new(vec![1.0], None).unwrap();
    let mut comp2 = CompositeToxicity::new(vec![1.0], None).unwrap();

    let r1 = comp1.update(&[2.0, 5.0, 10.0]);
    let r2 = comp2.update(&[2.0]);

    assert!(r1.is_some() && r2.is_some());
    assert_relative_eq!(r1.unwrap(), r2.unwrap(), epsilon = 1e-12);
}

#[test]
fn test_composite_all_z_scores_beyond_weights_produces_none() {
    let mut comp = CompositeToxicity::new(vec![], None).unwrap();
    let result = comp.update(&[1.0, 2.0]);
    assert!(result.is_none());
}

#[test]
fn test_composite_nan_weight_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![f64::NAN], None).unwrap_err(),
        ConfigError::WeightNotFinite { index: 0, .. }
    ));
}

#[test]
fn test_composite_nan_weight_at_index_1_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![1.0, f64::NAN], None).unwrap_err(),
        ConfigError::WeightNotFinite { index: 1, .. }
    ));
}

#[test]
fn test_composite_inf_weight_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![f64::INFINITY], None).unwrap_err(),
        ConfigError::WeightNotFinite { index: 0, .. }
    ));
}

#[test]
fn test_composite_neg_inf_weight_returns_err() {
    assert!(matches!(
        CompositeToxicity::new(vec![f64::NEG_INFINITY], None).unwrap_err(),
        ConfigError::WeightNotFinite { index: 0, .. }
    ));
}

#[test]
fn test_composite_negative_finite_weight_allowed() {
    let mut comp = CompositeToxicity::new(vec![-1.0, 1.0], None).unwrap();
    let result = comp.update(&[1.0, 1.0]);
    assert_relative_eq!(result.unwrap(), 0.0, epsilon = 1e-12);
}

#[test]
fn test_composite_nan_weight_would_have_bypassed_guard() {
    let mut comp = CompositeToxicity::new(vec![1.0], None).unwrap();
    let result = comp.update(&[2.0]);
    assert!(result.is_some());
    assert!(result.unwrap().is_finite());
}
