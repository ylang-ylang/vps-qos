use crate::kalman::KalmanConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CONFIG_COMMENT: &str = "VPS bandwidth observer configuration. nominal_ceiling_bps is REQUIRED and must match your VPS's advertised bandwidth. All other fields have sensible defaults and are optional overrides.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(rename = "_comment", default = "default_comment")]
    pub comment: String,
    pub required: RequiredConfig,
    #[serde(default)]
    pub windowed_max_filter: WindowedMaxFilterConfig,
    #[serde(default)]
    pub congestion_detection: CongestionDetectionConfig,
    #[serde(default)]
    pub factors: FactorsConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredConfig {
    pub nominal_ceiling_bps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowedMaxFilterConfig {
    pub window_seconds: f64,
    pub sample_interval_seconds: f64,
    pub state_path: PathBuf,
    pub channel_id: String,
    pub interface: String,
    pub proc_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CongestionDetectionConfig {
    pub congestion_threshold: f64,
    pub kalman: KalmanConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FactorsConfig {
    pub retransmission: DeltaFactorConfig,
    pub timeout: DeltaFactorConfig,
    pub zero_window: DeltaFactorConfig,
    pub rate_stability: RateStabilityConfig,
    pub rate_slope_zero: RateSlopeZeroConfig,
    pub rtt_inflation: RttInflationConfig,
    pub rtt_jitter: RttJitterConfig,
    pub cwnd_shrink: CwndShrinkConfig,
    pub conn_up_throughput_flat: ConnUpThroughputFlatConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeltaFactorConfig {
    pub enabled: bool,
    pub min_delta: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateStabilityConfig {
    pub enabled: bool,
    pub window_ticks: usize,
    pub stability_frac: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateSlopeZeroConfig {
    pub enabled: bool,
    pub window_ticks: usize,
    pub slope_threshold: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RttInflationConfig {
    pub enabled: bool,
    pub fping_targets: Vec<String>,
    pub baseline_samples: usize,
    pub inflation_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RttJitterConfig {
    pub enabled: bool,
    pub jitter_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CwndShrinkConfig {
    pub enabled: bool,
    pub shrink_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConnUpThroughputFlatConfig {
    pub enabled: bool,
    pub conn_growth_ratio: f64,
    pub haproxy_socket: PathBuf,
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
            factors: FactorsConfig::default(),
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
impl Default for FactorsConfig {
    fn default() -> Self {
        Self {
            retransmission: DeltaFactorConfig {
                enabled: true,
                min_delta: 1,
            },
            timeout: DeltaFactorConfig {
                enabled: true,
                min_delta: 1,
            },
            zero_window: DeltaFactorConfig {
                enabled: true,
                min_delta: 1,
            },
            rate_stability: RateStabilityConfig::default(),
            rate_slope_zero: RateSlopeZeroConfig::default(),
            rtt_inflation: RttInflationConfig::default(),
            rtt_jitter: RttJitterConfig::default(),
            cwnd_shrink: CwndShrinkConfig::default(),
            conn_up_throughput_flat: ConnUpThroughputFlatConfig::default(),
        }
    }
}
impl Default for DeltaFactorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_delta: 1,
        }
    }
}
impl Default for RateStabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            window_ticks: 10,
            stability_frac: 0.05,
        }
    }
}
impl Default for RateSlopeZeroConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            window_ticks: 10,
            slope_threshold: 0.01,
        }
    }
}
impl Default for RttInflationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fping_targets: vec!["8.8.8.8".to_owned()],
            baseline_samples: 20,
            inflation_ratio: 1.5,
        }
    }
}
impl Default for RttJitterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jitter_ratio: 0.25,
        }
    }
}
impl Default for CwndShrinkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            shrink_ratio: 0.9,
        }
    }
}
impl Default for ConnUpThroughputFlatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            conn_growth_ratio: 1.1,
            haproxy_socket: PathBuf::from("/var/run/haproxy/admin.sock"),
        }
    }
}

impl FactorsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        for (name, value) in [
            ("retransmission.min_delta", self.retransmission.min_delta),
            ("timeout.min_delta", self.timeout.min_delta),
            ("zero_window.min_delta", self.zero_window.min_delta),
        ] {
            if value == 0 {
                return Err(ConfigError::Invalid(format!(
                    "{name} must be greater than zero"
                )));
            }
        }
        if self.rate_stability.window_ticks < 2 || self.rate_slope_zero.window_ticks < 2 {
            return Err(ConfigError::Invalid(
                "rate factor window_ticks must be at least 2".to_owned(),
            ));
        }
        validate_non_negative(
            "rate_stability.stability_frac",
            self.rate_stability.stability_frac,
        )?;
        validate_non_negative(
            "rate_slope_zero.slope_threshold",
            self.rate_slope_zero.slope_threshold,
        )?;
        if self.rtt_inflation.baseline_samples == 0 {
            return Err(ConfigError::Invalid(
                "rtt_inflation.baseline_samples must be greater than zero".to_owned(),
            ));
        }
        if self.rtt_inflation.enabled && self.rtt_inflation.fping_targets.is_empty() {
            return Err(ConfigError::Invalid(
                "rtt_inflation.fping_targets must not be empty when enabled".to_owned(),
            ));
        }
        validate_positive(
            "rtt_inflation.inflation_ratio",
            self.rtt_inflation.inflation_ratio,
        )?;
        validate_non_negative("rtt_jitter.jitter_ratio", self.rtt_jitter.jitter_ratio)?;
        if !self.cwnd_shrink.shrink_ratio.is_finite()
            || !(0.0..1.0).contains(&self.cwnd_shrink.shrink_ratio)
        {
            return Err(ConfigError::Invalid(
                "cwnd_shrink.shrink_ratio must be in (0, 1)".to_owned(),
            ));
        }
        validate_positive(
            "conn_up_throughput_flat.conn_growth_ratio",
            self.conn_up_throughput_flat.conn_growth_ratio,
        )?;
        if self.conn_up_throughput_flat.enabled
            && self
                .conn_up_throughput_flat
                .haproxy_socket
                .as_os_str()
                .is_empty()
        {
            return Err(ConfigError::Invalid(
                "conn_up_throughput_flat.haproxy_socket must not be empty when enabled".to_owned(),
            ));
        }
        Ok(())
    }
}
fn validate_positive(name: &str, value: f64) -> Result<(), ConfigError> {
    if !value.is_finite() || value <= 0.0 {
        Err(ConfigError::Invalid(format!(
            "{name} must be finite and positive"
        )))
    } else {
        Ok(())
    }
}
fn validate_non_negative(name: &str, value: f64) -> Result<(), ConfigError> {
    if !value.is_finite() || value < 0.0 {
        Err(ConfigError::Invalid(format!(
            "{name} must be finite and non-negative"
        )))
    } else {
        Ok(())
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
        validate_positive("window_seconds", window.window_seconds)?;
        validate_positive("sample_interval_seconds", window.sample_interval_seconds)?;
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
        validate_positive("nominal_ceiling_bps", self.required.nominal_ceiling_bps)?;
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
        self.factors.validate()
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
