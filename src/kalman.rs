use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a scalar random-walk Kalman filter whose state is the
/// probability-like congestion degree in the closed interval `[0, 1]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KalmanConfig {
    pub process_noise_per_second: f64,
    pub mean_reversion_per_second: f64,
    pub initial_measurement_noise: f64,
    pub measurement_noise_learning_rate: f64,
    pub minimum_measurement_noise: f64,
    pub maximum_measurement_noise: f64,
    pub initial_state: f64,
    pub initial_covariance: f64,
}

impl Default for KalmanConfig {
    fn default() -> Self {
        Self {
            process_noise_per_second: 0.005,
            mean_reversion_per_second: 0.01,
            initial_measurement_noise: 0.1,
            measurement_noise_learning_rate: 0.1,
            minimum_measurement_noise: 0.001,
            maximum_measurement_noise: 1.0,
            initial_state: 0.0,
            initial_covariance: 1.0,
        }
    }
}

impl KalmanConfig {
    pub fn validate(&self) -> Result<(), KalmanError> {
        if !self.process_noise_per_second.is_finite() || self.process_noise_per_second < 0.0 {
            return Err(KalmanError::InvalidConfig(
                "process_noise_per_second must be finite and non-negative",
            ));
        }
        if !self.mean_reversion_per_second.is_finite() || self.mean_reversion_per_second < 0.0 {
            return Err(KalmanError::InvalidConfig(
                "mean_reversion_per_second must be finite and non-negative",
            ));
        }
        if !self.initial_measurement_noise.is_finite()
            || !self.minimum_measurement_noise.is_finite()
            || !self.maximum_measurement_noise.is_finite()
            || self.minimum_measurement_noise <= 0.0
            || self.minimum_measurement_noise > self.initial_measurement_noise
            || self.initial_measurement_noise > self.maximum_measurement_noise
        {
            return Err(KalmanError::InvalidConfig(
                "measurement noise bounds must be finite, positive, and contain the initial value",
            ));
        }
        if !self.measurement_noise_learning_rate.is_finite()
            || self.measurement_noise_learning_rate <= 0.0
            || self.measurement_noise_learning_rate > 1.0
        {
            return Err(KalmanError::InvalidConfig(
                "measurement_noise_learning_rate must be in (0, 1]",
            ));
        }
        if !self.initial_state.is_finite() || !(0.0..=1.0).contains(&self.initial_state) {
            return Err(KalmanError::InvalidConfig(
                "initial_state must be in [0, 1]",
            ));
        }
        if !self.initial_covariance.is_finite() || self.initial_covariance <= 0.0 {
            return Err(KalmanError::InvalidConfig(
                "initial_covariance must be finite and positive",
            ));
        }
        Ok(())
    }
}

/// A uniform factor report. A false report is an absence of an observation,
/// not a measurement of zero congestion.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorObservation {
    pub name: String,
    pub triggered: bool,
    pub value_bps: f64,
}

impl FactorObservation {
    pub fn new(name: impl Into<String>, triggered: bool, value_bps: f64) -> Self {
        Self {
            name: name.into(),
            triggered,
            value_bps,
        }
    }
}

/// Scalar Kalman fusion with one adaptively learned measurement variance per
/// factor identity. Every new factor starts with exactly the same variance.
#[derive(Debug, Clone, PartialEq)]
pub struct CongestionKalman {
    config: KalmanConfig,
    state: f64,
    covariance: f64,
    last_timestamp: Option<f64>,
    measurement_noise: HashMap<String, f64>,
}

impl CongestionKalman {
    pub fn new(config: KalmanConfig) -> Result<Self, KalmanError> {
        config.validate()?;
        Ok(Self {
            state: config.initial_state,
            covariance: config.initial_covariance,
            config,
            last_timestamp: None,
            measurement_noise: HashMap::new(),
        })
    }

    /// Predicts once for a sampling tick, then applies all triggered reports as
    /// independent same-timestamp Gaussian observations. Sequential scalar
    /// updates are equivalent to multiplying their independent likelihoods.
    pub fn process(
        &mut self,
        timestamp: f64,
        nominal_ceiling_bps: f64,
        reports: &[FactorObservation],
    ) -> Result<f64, KalmanError> {
        if !timestamp.is_finite() || !nominal_ceiling_bps.is_finite() || nominal_ceiling_bps <= 0.0
        {
            return Err(KalmanError::InvalidInput);
        }
        self.predict(timestamp)?;
        for report in reports.iter().filter(|report| report.triggered) {
            self.update(report, nominal_ceiling_bps)?;
        }
        Ok(self.state)
    }

    pub fn congestion(&self) -> f64 {
        self.state
    }

    pub fn covariance(&self) -> f64 {
        self.covariance
    }

    pub fn measurement_noise(&self, factor_name: &str) -> Option<f64> {
        self.measurement_noise.get(factor_name).copied()
    }

    fn predict(&mut self, timestamp: f64) -> Result<(), KalmanError> {
        if let Some(previous) = self.last_timestamp {
            let elapsed = timestamp - previous;
            if elapsed < 0.0 {
                return Err(KalmanError::TimestampMovedBackward {
                    previous,
                    received: timestamp,
                });
            }
            self.state *= (1.0 - self.config.mean_reversion_per_second * elapsed).max(0.0);
            self.covariance += self.config.process_noise_per_second * elapsed;
        }
        self.last_timestamp = Some(timestamp);
        Ok(())
    }

    fn update(
        &mut self,
        report: &FactorObservation,
        nominal_ceiling_bps: f64,
    ) -> Result<(), KalmanError> {
        if report.name.is_empty() || !report.value_bps.is_finite() || report.value_bps < 0.0 {
            return Err(KalmanError::InvalidInput);
        }

        // A triggered factor records the speed at the event. The speed deficit
        // turns that common unit into the filter's dimensionless state space.
        let observation = (1.0 - report.value_bps / nominal_ceiling_bps).clamp(0.0, 1.0);
        let innovation = observation - self.state;
        let noise = self
            .measurement_noise
            .entry(report.name.clone())
            .or_insert(self.config.initial_measurement_noise);
        let alpha = self.config.measurement_noise_learning_rate;
        *noise = ((1.0 - alpha) * *noise + alpha * innovation * innovation).clamp(
            self.config.minimum_measurement_noise,
            self.config.maximum_measurement_noise,
        );

        let gain = self.covariance / (self.covariance + *noise);
        self.state = (self.state + gain * innovation).clamp(0.0, 1.0);
        self.covariance *= 1.0 - gain;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KalmanError {
    InvalidConfig(&'static str),
    InvalidInput,
    TimestampMovedBackward { previous: f64, received: f64 },
}

impl std::fmt::Display for KalmanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid Kalman configuration: {message}")
            }
            Self::InvalidInput => write!(formatter, "Kalman inputs must be finite and valid"),
            Self::TimestampMovedBackward { previous, received } => write!(
                formatter,
                "Kalman timestamp moved backward from {previous} to {received}"
            ),
        }
    }
}

impl std::error::Error for KalmanError {}
