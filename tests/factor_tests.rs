use vps_bandwidth_observer::collector::{AuxiliaryMeasurements, RawCounters};
use vps_bandwidth_observer::config::{
    ConnUpThroughputFlatConfig, CwndShrinkConfig, FactorsConfig, RateSlopeZeroConfig,
    RateStabilityConfig, RttInflationConfig, RttJitterConfig,
};
use vps_bandwidth_observer::factors::{
    self, ConnUpThroughputFlatFactor, CwndShrinkFactor, Factor, FactorInput, RateSlopeZeroFactor,
    RateStabilityFactor, RttInflationFactor, RttJitterFactor,
};

fn counters(retrans: u64, timeout: u64, zero_window: u64) -> RawCounters {
    RawCounters {
        rx_bytes: 0,
        tx_bytes: 0,
        retransmitted_segments: retrans,
        tcp_timeouts: timeout,
        to_zero_window_advertisements: zero_window,
        from_zero_window_advertisements: 0,
    }
}
fn observe(
    factor: &mut dyn Factor,
    history: &[f64],
    auxiliary: &AuxiliaryMeasurements,
    value: f64,
) -> bool {
    factor
        .observe(&FactorInput {
            previous: &counters(1, 2, 3),
            current: &counters(2, 2, 4),
            value_bps: value,
            rate_history_bps: history,
            auxiliary,
        })
        .triggered
}

#[test]
fn registry_enables_only_configured_factors() {
    let mut registry = factors::all_factors(&FactorsConfig::default());
    let input = FactorInput {
        previous: &counters(1, 2, 3),
        current: &counters(2, 2, 4),
        value_bps: 42_000.0,
        rate_history_bps: &[],
        auxiliary: &AuxiliaryMeasurements::default(),
    };
    let reports = factors::observe_all(&mut registry, &input);
    assert_eq!(reports.len(), 3);
    assert_eq!(
        reports.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["retransmission", "timeout", "zero_window"]
    );
    assert_eq!(
        reports.iter().map(|r| r.triggered).collect::<Vec<_>>(),
        [true, false, true]
    );
}

#[test]
fn rate_stability_uses_configured_window_and_fraction() {
    let mut factor = RateStabilityFactor::new(RateStabilityConfig {
        enabled: true,
        window_ticks: 3,
        stability_frac: 0.02,
    });
    assert!(observe(
        &mut factor,
        &[99.0, 100.0, 101.0],
        &AuxiliaryMeasurements::default(),
        100.0
    ));
    assert!(!observe(
        &mut factor,
        &[50.0, 100.0, 150.0],
        &AuxiliaryMeasurements::default(),
        150.0
    ));
}

#[test]
fn rate_slope_zero_uses_normalized_regression_slope() {
    let mut factor = RateSlopeZeroFactor::new(RateSlopeZeroConfig {
        enabled: true,
        window_ticks: 4,
        slope_threshold: 0.01,
    });
    assert!(observe(
        &mut factor,
        &[100.0, 100.2, 99.9, 100.1],
        &AuxiliaryMeasurements::default(),
        100.1
    ));
    assert!(!observe(
        &mut factor,
        &[100.0, 120.0, 140.0, 160.0],
        &AuxiliaryMeasurements::default(),
        160.0
    ));
}

#[test]
fn rtt_inflation_waits_for_baseline_then_triggers() {
    let mut factor = RttInflationFactor::new(RttInflationConfig {
        enabled: true,
        fping_targets: vec!["test".into()],
        baseline_samples: 2,
        inflation_ratio: 1.5,
    });
    for samples in [vec![10.0, 10.0], vec![10.0, 10.0]] {
        assert!(!observe(
            &mut factor,
            &[],
            &AuxiliaryMeasurements {
                rtt_ms: Some(samples),
                ..Default::default()
            },
            50.0
        ));
    }
    assert!(observe(
        &mut factor,
        &[],
        &AuxiliaryMeasurements {
            rtt_ms: Some(vec![20.0, 20.0]),
            ..Default::default()
        },
        50.0
    ));
}

#[test]
fn rtt_jitter_compares_deviation_to_mean() {
    let mut factor = RttJitterFactor::new(RttJitterConfig {
        enabled: true,
        jitter_ratio: 0.25,
    });
    assert!(observe(
        &mut factor,
        &[],
        &AuxiliaryMeasurements {
            rtt_ms: Some(vec![5.0, 10.0, 20.0]),
            ..Default::default()
        },
        50.0
    ));
}

#[test]
fn cwnd_shrink_compares_consecutive_averages() {
    let mut factor = CwndShrinkFactor::new(CwndShrinkConfig {
        enabled: true,
        shrink_ratio: 0.9,
    });
    assert!(!observe(
        &mut factor,
        &[],
        &AuxiliaryMeasurements {
            average_cwnd: Some(100.0),
            ..Default::default()
        },
        50.0
    ));
    assert!(observe(
        &mut factor,
        &[],
        &AuxiliaryMeasurements {
            average_cwnd: Some(80.0),
            ..Default::default()
        },
        50.0
    ));
}

#[test]
fn growing_connections_with_flat_rate_triggers() {
    let mut factor = ConnUpThroughputFlatFactor::new(ConnUpThroughputFlatConfig {
        enabled: true,
        conn_growth_ratio: 1.1,
        haproxy_socket: "/unused".into(),
    });
    assert!(!observe(
        &mut factor,
        &[],
        &AuxiliaryMeasurements {
            haproxy_conn_cur: Some(100),
            ..Default::default()
        },
        1_000.0
    ));
    assert!(observe(
        &mut factor,
        &[],
        &AuxiliaryMeasurements {
            haproxy_conn_cur: Some(120),
            ..Default::default()
        },
        1_000.0
    ));
}
