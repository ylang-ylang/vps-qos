use crate::kalman::KalmanConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CONFIG_COMMENT: &str = "VPS bandwidth observer configuration. nominal_ceiling_bps is REQUIRED and must match your VPS's advertised bandwidth. All other fields have sensible defaults and are optional overrides.";

/// Runtime settings grouped by operator responsibility. `required` identifies
/// deployment-specific input; the other groups support defaulted overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(rename = "_comment", default = "default_comment")]
    pub comment: String,
    pub required: RequiredConfig,
    #[serde(default)]
    pub windowed_max_filter: WindowedMaxFilterConfig,
    #[serde(default)]
    pub congestion_detection: CongestionDetectionConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequiredConfig {
    pub nominal_ceiling_bps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowedMaxFilterConfig {
    pub window_seconds: f64,
    pub sample_interval_seconds: f64,
    pub state_path: PathBuf,
    pub channel_id: String,
    pub interface: String,
    pub proc_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CongestionDetectionConfig {
    pub congestion_threshold: f64,
    pub kalman: KalmanConfig,
}

fn default_comment() -> String {
    CONFIG_COMMENT.to_owned()
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            comment: default_comment(),
            required: RequiredConfig::default(),
            windowed_max_filter: WindowedMaxFilterConfig::default(),
            congestion_detection: CongestionDetectionConfig::default(),
        }
    }
}

impl Default for RequiredConfig {
    fn default() -> Self {
        Self {
            nominal_ceiling_bps: 200_000_000.0,
        }
    }
}

impl Default for WindowedMaxFilterConfig {
    fn default() -> Self {
        Self {
            window_seconds: 60.0,
            sample_interval_seconds: 2.0,
            state_path: PathBuf::from("state/window.json"),
            channel_id: "default".to_owned(),
            interface: "eth0".to_owned(),
            proc_root: PathBuf::from("/proc"),
        }
    }
}

impl Default for CongestionDetectionConfig {
    fn default() -> Self {
        Self {
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
        let window = &self.windowed_max_filter;
        let congestion = &self.congestion_detection;
        if !window.window_seconds.is_finite() || window.window_seconds <= 0.0 {
            return Err(ConfigError::Invalid(
                "window_seconds must be finite and greater than zero".to_owned(),
            ));
        }
        if !window.sample_interval_seconds.is_finite() || window.sample_interval_seconds <= 0.0 {
            return Err(ConfigError::Invalid(
                "sample_interval_seconds must be finite and greater than zero".to_owned(),
            ));
        }
        if window.sample_interval_seconds > window.window_seconds {
            return Err(ConfigError::Invalid(
                "sample_interval_seconds must not exceed window_seconds".to_owned(),
            ));
        }
        if window.channel_id.is_empty() || window.interface.is_empty() {
            return Err(ConfigError::Invalid(
                "channel_id and interface must not be empty".to_owned(),
            ));
        }
        if !self.required.nominal_ceiling_bps.is_finite()
            || self.required.nominal_ceiling_bps <= 0.0
        {
            return Err(ConfigError::Invalid(
                "nominal_ceiling_bps must be finite and greater than zero".to_owned(),
            ));
        }
        if !congestion.congestion_threshold.is_finite()
            || !(0.0..=1.0).contains(&congestion.congestion_threshold)
        {
            return Err(ConfigError::Invalid(
                "congestion_threshold must be in [0, 1]".to_owned(),
            ));
        }
        congestion
            .kalman
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
