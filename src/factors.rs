use crate::collector::RawCounters;
use crate::kalman::FactorObservation;

/// Stateless delta factors over consecutive cumulative kernel snapshots.
/// Every factor is emitted in the same representation and has no priority or
/// special handling in the fusion layer. Call once per traffic direction with
/// that direction's current rate.
pub fn observe(
    previous: &RawCounters,
    current: &RawCounters,
    value_bps: f64,
) -> Vec<FactorObservation> {
    vec![
        FactorObservation::new(
            "retransmission",
            increased(
                current.retransmitted_segments,
                previous.retransmitted_segments,
            ),
            value_bps,
        ),
        FactorObservation::new(
            "timeout",
            increased(current.tcp_timeouts, previous.tcp_timeouts),
            value_bps,
        ),
        FactorObservation::new(
            "zero_window",
            zero_window_increased(previous, current),
            value_bps,
        ),
    ]
}

fn increased(current: u64, previous: u64) -> bool {
    current.saturating_sub(previous) > 0
}

fn zero_window_increased(previous: &RawCounters, current: &RawCounters) -> bool {
    increased(
        current.to_zero_window_advertisements,
        previous.to_zero_window_advertisements,
    ) || increased(
        current.from_zero_window_advertisements,
        previous.from_zero_window_advertisements,
    )
}
