use crate::collector::RawCounters;
use crate::kalman::FactorObservation;

/// Uniform interface implemented by every congestion signal. Factors are
/// stateless, independently evaluated, and receive no priority or weight.
pub trait Factor {
    fn name(&self) -> &'static str;

    fn observe(
        &self,
        previous: &RawCounters,
        current: &RawCounters,
        value_bps: f64,
    ) -> FactorObservation;
}

/// Constructs the production factor registry. Add a new factor implementation
/// below, then register one instance here; the collector and fusion code need
/// no changes.
pub fn all_factors() -> Vec<Box<dyn Factor>> {
    vec![
        Box::new(RetransmissionFactor),
        Box::new(TimeoutFactor),
        Box::new(ZeroWindowFactor),
    ]
}

/// Evaluates any supplied registry and preserves its order.
pub fn observe_all(
    factors: &[Box<dyn Factor>],
    previous: &RawCounters,
    current: &RawCounters,
    value_bps: f64,
) -> Vec<FactorObservation> {
    factors
        .iter()
        .map(|factor| factor.observe(previous, current, value_bps))
        .collect()
}

pub struct RetransmissionFactor;

impl Factor for RetransmissionFactor {
    fn name(&self) -> &'static str {
        "retransmission"
    }

    fn observe(
        &self,
        previous: &RawCounters,
        current: &RawCounters,
        value_bps: f64,
    ) -> FactorObservation {
        FactorObservation::new(
            self.name(),
            increased(
                current.retransmitted_segments,
                previous.retransmitted_segments,
            ),
            value_bps,
        )
    }
}

pub struct TimeoutFactor;

impl Factor for TimeoutFactor {
    fn name(&self) -> &'static str {
        "timeout"
    }

    fn observe(
        &self,
        previous: &RawCounters,
        current: &RawCounters,
        value_bps: f64,
    ) -> FactorObservation {
        FactorObservation::new(
            self.name(),
            increased(current.tcp_timeouts, previous.tcp_timeouts),
            value_bps,
        )
    }
}

pub struct ZeroWindowFactor;

impl Factor for ZeroWindowFactor {
    fn name(&self) -> &'static str {
        "zero_window"
    }

    fn observe(
        &self,
        previous: &RawCounters,
        current: &RawCounters,
        value_bps: f64,
    ) -> FactorObservation {
        FactorObservation::new(
            self.name(),
            increased(
                current.to_zero_window_advertisements,
                previous.to_zero_window_advertisements,
            ) || increased(
                current.from_zero_window_advertisements,
                previous.from_zero_window_advertisements,
            ),
            value_bps,
        )
    }
}

fn increased(current: u64, previous: u64) -> bool {
    current.saturating_sub(previous) > 0
}
