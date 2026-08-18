use microstructure_signals::AdverseSelectionSignal;

#[test]
fn test_as_equal_weights_positive_ofi() {
    let mut signal = AdverseSelectionSignal::with_equal_weights(None).unwrap();
    let result = signal.update(&[2.0, 0.0, 0.0]);
    assert!(result.is_some());
    assert!(result.unwrap() > 0.0);
}

#[test]
fn test_as_negative_means_downward_drift() {
    let mut signal = AdverseSelectionSignal::with_equal_weights(None).unwrap();
    let result = signal.update(&[-2.0, -1.0, -1.0]);
    assert!(result.is_some());
    assert!(result.unwrap() < 0.0);
}

#[test]
fn test_as_custom_coefficients() {
    let mut signal = AdverseSelectionSignal::new(vec![1.0, 2.0, 3.0], None).unwrap();
    let result = signal.update(&[1.0, 1.0, 1.0]);
    assert!(result.is_some());
}

#[test]
fn test_as_value_persistence() {
    let mut signal = AdverseSelectionSignal::with_equal_weights(Some(5.0)).unwrap();
    signal.update(&[1.0, 1.0, 1.0]);
    assert!(signal.value().is_some());
}
