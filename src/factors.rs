use crate::collector::{AuxiliaryMeasurements, RawCounters};
use crate::config::{
    ConnUpThroughputFlatConfig, CwndShrinkConfig, FactorsConfig, RateSlopeZeroConfig,
    RateStabilityConfig, RttInflationConfig, RttJitterConfig,
};
use crate::kalman::FactorObservation;
use std::collections::VecDeque;

pub struct FactorInput<'a> {
    pub previous: &'a RawCounters,
    pub current: &'a RawCounters,
    pub value_bps: f64,
    pub rate_history_bps: &'a [f64],
    pub auxiliary: &'a AuxiliaryMeasurements,
}

/// Uniform, independently evaluated congestion signal. Stateful factors own
/// only their private baseline; the fusion layer still assigns no priorities.
pub trait Factor {
    fn name(&self) -> &'static str;
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation;
}

pub fn all_factors(config: &FactorsConfig) -> Vec<Box<dyn Factor>> {
    let mut factors: Vec<Box<dyn Factor>> = Vec::new();
    if config.retransmission.enabled {
        factors.push(Box::new(RetransmissionFactor {
            min_delta: config.retransmission.min_delta,
        }));
    }
    if config.timeout.enabled {
        factors.push(Box::new(TimeoutFactor {
            min_delta: config.timeout.min_delta,
        }));
    }
    if config.zero_window.enabled {
        factors.push(Box::new(ZeroWindowFactor {
            min_delta: config.zero_window.min_delta,
        }));
    }
    if config.rate_stability.enabled {
        factors.push(Box::new(RateStabilityFactor::new(
            config.rate_stability.clone(),
        )));
    }
    if config.rate_slope_zero.enabled {
        factors.push(Box::new(RateSlopeZeroFactor::new(
            config.rate_slope_zero.clone(),
        )));
    }
    if config.rtt_inflation.enabled {
        factors.push(Box::new(RttInflationFactor::new(
            config.rtt_inflation.clone(),
        )));
    }
    if config.rtt_jitter.enabled {
        factors.push(Box::new(RttJitterFactor::new(config.rtt_jitter.clone())));
    }
    if config.cwnd_shrink.enabled {
        factors.push(Box::new(CwndShrinkFactor::new(config.cwnd_shrink.clone())));
    }
    if config.conn_up_throughput_flat.enabled {
        factors.push(Box::new(ConnUpThroughputFlatFactor::new(
            config.conn_up_throughput_flat.clone(),
            config.rate_slope_zero.clone(),
        )));
    }
    factors
}

pub fn observe_all(
    factors: &mut [Box<dyn Factor>],
    input: &FactorInput<'_>,
) -> Vec<FactorObservation> {
    factors
        .iter_mut()
        .map(|factor| factor.observe(input))
        .collect()
}

pub struct RetransmissionFactor {
    min_delta: u64,
}
impl Factor for RetransmissionFactor {
    fn name(&self) -> &'static str {
        "retransmission"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        report(
            self.name(),
            delta(
                input.current.retransmitted_segments,
                input.previous.retransmitted_segments,
            ) >= self.min_delta,
            input.value_bps,
        )
    }
}
pub struct TimeoutFactor {
    min_delta: u64,
}
impl Factor for TimeoutFactor {
    fn name(&self) -> &'static str {
        "timeout"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        report(
            self.name(),
            delta(input.current.tcp_timeouts, input.previous.tcp_timeouts) >= self.min_delta,
            input.value_bps,
        )
    }
}
pub struct ZeroWindowFactor {
    min_delta: u64,
}
impl Factor for ZeroWindowFactor {
    fn name(&self) -> &'static str {
        "zero_window"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        let total = delta(
            input.current.to_zero_window_advertisements,
            input.previous.to_zero_window_advertisements,
        )
        .saturating_add(delta(
            input.current.from_zero_window_advertisements,
            input.previous.from_zero_window_advertisements,
        ));
        report(self.name(), total >= self.min_delta, input.value_bps)
    }
}

pub struct RateStabilityFactor {
    config: RateStabilityConfig,
}
impl RateStabilityFactor {
    pub fn new(config: RateStabilityConfig) -> Self {
        Self { config }
    }
}
impl Factor for RateStabilityFactor {
    fn name(&self) -> &'static str {
        "rate_stability"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        let values = tail(input.rate_history_bps, self.config.window_ticks);
        let triggered = values
            .map(|values| coefficient_of_variation(values) <= self.config.stability_frac)
            .unwrap_or(false);
        report(self.name(), triggered, input.value_bps)
    }
}

pub struct RateSlopeZeroFactor {
    config: RateSlopeZeroConfig,
}
impl RateSlopeZeroFactor {
    pub fn new(config: RateSlopeZeroConfig) -> Self {
        Self { config }
    }
}
impl Factor for RateSlopeZeroFactor {
    fn name(&self) -> &'static str {
        "rate_slope_zero"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        let values = tail(input.rate_history_bps, self.config.window_ticks);
        let triggered = values
            .map(|values| normalized_slope(values).abs() <= self.config.slope_threshold)
            .unwrap_or(false);
        report(self.name(), triggered, input.value_bps)
    }
}

pub struct RttInflationFactor {
    config: RttInflationConfig,
    baseline: VecDeque<f64>,
}
impl RttInflationFactor {
    pub fn new(config: RttInflationConfig) -> Self {
        Self {
            config,
            baseline: VecDeque::new(),
        }
    }
}
impl Factor for RttInflationFactor {
    fn name(&self) -> &'static str {
        "rtt_inflation"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        let Some(samples) = input.auxiliary.rtt_ms.as_deref() else {
            return report(self.name(), false, input.value_bps);
        };
        let Some(current) = mean(samples) else {
            return report(self.name(), false, input.value_bps);
        };
        let triggered = if self.baseline.len() >= self.config.baseline_samples {
            mean(self.baseline.make_contiguous()).is_some_and(|baseline| {
                baseline > 0.0 && current > baseline * self.config.inflation_ratio
            })
        } else {
            false
        };
        self.baseline.push_back(current);
        while self.baseline.len() > self.config.baseline_samples {
            self.baseline.pop_front();
        }
        report(self.name(), triggered, input.value_bps)
    }
}

pub struct RttJitterFactor {
    config: RttJitterConfig,
}
impl RttJitterFactor {
    pub fn new(config: RttJitterConfig) -> Self {
        Self { config }
    }
}
impl Factor for RttJitterFactor {
    fn name(&self) -> &'static str {
        "rtt_jitter"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        let triggered = input
            .auxiliary
            .rtt_ms
            .as_deref()
            .is_some_and(|values| coefficient_of_variation(values) > self.config.jitter_ratio);
        report(self.name(), triggered, input.value_bps)
    }
}

pub struct CwndShrinkFactor {
    config: CwndShrinkConfig,
    previous_cwnd: Option<f64>,
}
impl CwndShrinkFactor {
    pub fn new(config: CwndShrinkConfig) -> Self {
        Self {
            config,
            previous_cwnd: None,
        }
    }
}
impl Factor for CwndShrinkFactor {
    fn name(&self) -> &'static str {
        "cwnd_shrink"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        let current = input.auxiliary.average_cwnd;
        let triggered = current
            .zip(self.previous_cwnd)
            .is_some_and(|(current, previous)| {
                previous > 0.0 && current < previous * self.config.shrink_ratio
            });
        if current.is_some() {
            self.previous_cwnd = current;
        }
        report(self.name(), triggered, input.value_bps)
    }
}

pub struct ConnUpThroughputFlatFactor {
    config: ConnUpThroughputFlatConfig,
    rate_config: RateSlopeZeroConfig,
    connection_history: VecDeque<u64>,
}
impl ConnUpThroughputFlatFactor {
    pub fn new(config: ConnUpThroughputFlatConfig, rate_config: RateSlopeZeroConfig) -> Self {
        Self {
            config,
            rate_config,
            connection_history: VecDeque::new(),
        }
    }
}
impl Factor for ConnUpThroughputFlatFactor {
    fn name(&self) -> &'static str {
        "conn_up_throughput_flat"
    }
    fn observe(&mut self, input: &FactorInput<'_>) -> FactorObservation {
        if input.current.tcp_connections == 0 {
            self.connection_history.clear();
            return report(self.name(), false, input.value_bps);
        }
        self.connection_history
            .push_back(input.current.tcp_connections);
        while self.connection_history.len() > self.rate_config.window_ticks {
            self.connection_history.pop_front();
        }

        let connections_growing = (self.connection_history.len() >= self.rate_config.window_ticks)
            .then(|| {
                let first = *self.connection_history.front()?;
                let last = *self.connection_history.back()?;
                Some(first > 0 && last as f64 >= first as f64 * self.config.conn_growth_ratio)
            })
            .flatten()
            .unwrap_or(false);
        let rate_flat = tail(input.rate_history_bps, self.rate_config.window_ticks)
            .map(|values| normalized_slope(values).abs() <= self.rate_config.slope_threshold)
            .unwrap_or(false);
        report(
            self.name(),
            connections_growing && rate_flat,
            input.value_bps,
        )
    }
}

fn report(name: &str, triggered: bool, value_bps: f64) -> FactorObservation {
    FactorObservation::new(name, triggered, value_bps)
}
fn delta(current: u64, previous: u64) -> u64 {
    current.saturating_sub(previous)
}
fn tail(values: &[f64], count: usize) -> Option<&[f64]> {
    (values.len() >= count).then(|| &values[values.len() - count..])
}
fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}
fn coefficient_of_variation(values: &[f64]) -> f64 {
    let Some(average) = mean(values) else {
        return f64::INFINITY;
    };
    if average <= 0.0 {
        return f64::INFINITY;
    }
    let variance = values
        .iter()
        .map(|value| (value - average).powi(2))
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt() / average
}
fn normalized_slope(values: &[f64]) -> f64 {
    let Some(average) = mean(values) else {
        return f64::INFINITY;
    };
    if average <= 0.0 {
        return f64::INFINITY;
    }
    let x_mean = (values.len() - 1) as f64 / 2.0;
    let numerator = values
        .iter()
        .enumerate()
        .map(|(index, value)| (index as f64 - x_mean) * (value - average))
        .sum::<f64>();
    let denominator = values
        .iter()
        .enumerate()
        .map(|(index, _)| (index as f64 - x_mean).powi(2))
        .sum::<f64>();
    numerator / denominator / average
}
