use vps_bandwidth_observer::policy::select_ceiling_bps;

#[test]
fn no_congestion_selects_operator_nominal_ceiling() {
    assert_eq!(select_ceiling_bps(0.4, 0.7, 200.0, 80.0), 200.0);
}

#[test]
fn congestion_selects_current_window_maximum() {
    assert_eq!(select_ceiling_bps(0.8, 0.7, 200.0, 80.0), 80.0);
}

#[test]
fn threshold_is_strict_as_specified() {
    assert_eq!(select_ceiling_bps(0.7, 0.7, 200.0, 80.0), 200.0);
}
