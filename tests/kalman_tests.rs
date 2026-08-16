use vps_bandwidth_observer::kalman::{CongestionKalman, FactorObservation, KalmanConfig};

fn config() -> KalmanConfig {
    KalmanConfig {
        process_noise_per_second: 0.01,
        mean_reversion_per_second: 0.01,
        initial_measurement_noise: 0.1,
        measurement_noise_learning_rate: 0.2,
        minimum_measurement_noise: 0.001,
        maximum_measurement_noise: 2.0,
        initial_state: 0.0,
        initial_covariance: 1.0,
    }
}

#[test]
fn simultaneous_factors_are_independent_same_timestamp_updates() {
    let reports = [
        FactorObservation::new("retransmission", true, 20.0),
        FactorObservation::new("timeout", true, 20.0),
    ];
    let mut together = CongestionKalman::new(config()).unwrap();
    let combined = together.process(10.0, 100.0, &reports).unwrap();

    let mut sequential = CongestionKalman::new(config()).unwrap();
    sequential.process(10.0, 100.0, &reports[..1]).unwrap();
    let expected = sequential.process(10.0, 100.0, &reports[1..]).unwrap();

    assert!((combined - expected).abs() < 1e-12);
    assert!(combined > 0.7);
}

#[test]
fn false_factor_is_absence_of_observation() {
    let mut no_reversion = config();
    no_reversion.mean_reversion_per_second = 0.0;
    let mut filter = CongestionKalman::new(no_reversion).unwrap();
    let before = filter.congestion();
    let after = filter
        .process(1.0, 100.0, &[FactorObservation::new("timeout", false, 1.0)])
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(filter.measurement_noise("timeout"), None);
}

#[test]
fn congestion_naturally_recovers_without_triggered_factors() {
    let mut filter = CongestionKalman::new(config()).unwrap();
    filter
        .process(0.0, 100.0, &[FactorObservation::new("timeout", true, 0.0)])
        .unwrap();
    let congested = filter.congestion();
    let recovered = filter.process(50.0, 100.0, &[]).unwrap();
    assert!(recovered < congested);
}

#[test]
fn persistently_inconsistent_factor_is_adaptively_downweighted() {
    let mut filter = CongestionKalman::new(config()).unwrap();
    let good = FactorObservation::new("retransmission", true, 20.0);
    for timestamp in 0..12 {
        filter
            .process(timestamp as f64, 100.0, std::slice::from_ref(&good))
            .unwrap();
    }

    // This deliberately nonsensical report stands in for a trigger such as
    // "even minute": it asserts the opposite congestion state independently
    // of the established evidence. It is test data only, not a production
    // factor implementation.
    let irrational = FactorObservation::new("irrelevant_clock_signal", true, 100.0);
    let initial_noise = config().initial_measurement_noise;
    filter
        .process(12.0, 100.0, std::slice::from_ref(&irrational))
        .unwrap();
    let first_noise = filter.measurement_noise("irrelevant_clock_signal").unwrap();
    for timestamp in 13..30 {
        filter
            .process(timestamp as f64, 100.0, &[good.clone(), irrational.clone()])
            .unwrap();
    }
    let learned_noise = filter.measurement_noise("irrelevant_clock_signal").unwrap();
    let good_noise = filter.measurement_noise("retransmission").unwrap();

    assert!(first_noise > initial_noise);
    assert!(learned_noise > good_noise);
}

#[test]
fn factor_can_recover_after_a_bad_period() {
    let mut filter = CongestionKalman::new(config()).unwrap();
    for timestamp in 0..10 {
        filter
            .process(
                timestamp as f64,
                100.0,
                &[FactorObservation::new("variable", true, 0.0)],
            )
            .unwrap();
    }
    for timestamp in 10..20 {
        filter
            .process(
                timestamp as f64,
                100.0,
                &[FactorObservation::new("anchor", true, 0.0)],
            )
            .unwrap();
        filter
            .process(
                timestamp as f64,
                100.0,
                &[FactorObservation::new("variable", true, 100.0)],
            )
            .unwrap();
    }
    let noisy = filter.measurement_noise("variable").unwrap();
    for timestamp in 20..80 {
        filter
            .process(
                timestamp as f64,
                100.0,
                &[
                    FactorObservation::new("anchor", true, 0.0),
                    FactorObservation::new("variable", true, 0.0),
                ],
            )
            .unwrap();
    }
    assert!(filter.measurement_noise("variable").unwrap() < noisy);
}
