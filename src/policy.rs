/// Selects the externally consumed ceiling from the operator-provided nominal
/// ceiling and the passively observed recent maximum.
pub fn select_ceiling_bps(
    congestion: f64,
    congestion_threshold: f64,
    nominal_ceiling_bps: f64,
    window_max_bps: f64,
) -> f64 {
    if congestion > congestion_threshold {
        window_max_bps
    } else {
        nominal_ceiling_bps
    }
}
