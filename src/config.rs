use crate::kalman::KalmanConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Runtime settings. Every operational parameter is represented in
/// `config/default.json` and checked against this implementation by a test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub window_seconds: f64,
    pub sample_interval_seconds: f64,
    pub state_path: PathBuf,
    pub channel_id: String,
    pub interface: String,
    pub proc_root: PathBuf,
    pub nominal_ceiling_bps: f64,
    pub congestion_threshold: f64,
    pub kalman: KalmanConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            window_seconds: 60.0,
            sample_interval_seconds: 2.0,
            state_path: PathBuf::from("state/window.json"),
            channel_id: "default".to_owned(),
            interface: "eth0".to_owned(),
            proc_root: PathBuf::from("/proc"),
            nominal_ceiling_bps: 200_000_000.0,
            congestion_threshold: 0.7,
            kalman: KalmanConfig::default(),
        }
    }
}

impl RuntimeConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(ConfigError::Read)?;
        let config: Self = serde_json::from_str(&contents).map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.window_seconds.is_finite() || self.window_seconds <= 0.0 {
            return Err(ConfigError::Invalid(
                "window_seconds must be finite and greater than zero".to_owned(),
            ));
        }
        if !self.sample_interval_seconds.is_finite() || self.sample_interval_seconds <= 0.0 {
            return Err(ConfigError::Invalid(
                "sample_interval_seconds must be finite and greater than zero".to_owned(),
            ));
        }
        if self.sample_interval_seconds > self.window_seconds {
            return Err(ConfigError::Invalid(
                "sample_interval_seconds must not exceed window_seconds".to_owned(),
            ));
        }
        if self.channel_id.is_empty() || self.interface.is_empty() {
            return Err(ConfigError::Invalid(
                "channel_id and interface must not be empty".to_owned(),
            ));
        }
        if !self.nominal_ceiling_bps.is_finite() || self.nominal_ceiling_bps <= 0.0 {
            return Err(ConfigError::Invalid(
                "nominal_ceiling_bps must be finite and greater than zero".to_owned(),
            ));
        }
        if !self.congestion_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.congestion_threshold)
        {
            return Err(ConfigError::Invalid(
                "congestion_threshold must be in [0, 1]".to_owned(),
            ));
        }
        self.kalman
            .validate()
            .map_err(|error| ConfigError::Invalid(error.to_string()))?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Read(io::Error),
    Parse(serde_json::Error),
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "cannot read configuration: {error}"),
            Self::Parse(error) => write!(f, "cannot parse configuration: {error}"),
            Self::Invalid(error) => write!(f, "invalid configuration: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}
