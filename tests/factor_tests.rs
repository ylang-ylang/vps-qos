use vps_bandwidth_observer::collector::RawCounters;
use vps_bandwidth_observer::factors;

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

#[test]
fn registry_factors_are_uniform_boolean_reports_with_current_speed() {
    let registry = factors::all_factors();
    let reports = factors::observe_all(&registry, &counters(1, 2, 3), &counters(2, 2, 4), 42_000.0);
    assert_eq!(reports.len(), 3);
    assert!(reports.iter().all(|report| report.value_bps == 42_000.0));
    assert_eq!(reports[0].name, "retransmission");
    assert!(reports[0].triggered);
    assert_eq!(reports[1].name, "timeout");
    assert!(!reports[1].triggered);
    assert_eq!(reports[2].name, "zero_window");
    assert!(reports[2].triggered);
}
