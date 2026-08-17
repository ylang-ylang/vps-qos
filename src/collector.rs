use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

const BITS_PER_BYTE: f64 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RawCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub retransmitted_segments: u64,
    pub tcp_timeouts: u64,
    pub to_zero_window_advertisements: u64,
    pub from_zero_window_advertisements: u64,
    pub tcp_connections: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuxiliaryMeasurements {
    pub rtt_ms: Option<Vec<f64>>,
    pub average_cwnd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RateSample {
    pub timestamp: f64,
    pub down_bps: f64,
    pub up_bps: f64,
    pub down_history_bps: Vec<f64>,
    pub up_history_bps: Vec<f64>,
    pub counters: RawCounters,
}

#[derive(Debug, Clone)]
pub struct ProcCollector {
    proc_root: PathBuf,
    interface: String,
    previous: Option<(f64, RawCounters)>,
    history_limit: usize,
    down_history_bps: VecDeque<f64>,
    up_history_bps: VecDeque<f64>,
}

impl ProcCollector {
    pub fn new(proc_root: impl Into<PathBuf>, interface: impl Into<String>) -> Self {
        Self::with_history_limit(proc_root, interface, 2)
    }

    pub fn with_history_limit(
        proc_root: impl Into<PathBuf>,
        interface: impl Into<String>,
        history_limit: usize,
    ) -> Self {
        Self {
            proc_root: proc_root.into(),
            interface: interface.into(),
            previous: None,
            history_limit: history_limit.max(2),
            down_history_bps: VecDeque::new(),
            up_history_bps: VecDeque::new(),
        }
    }

    pub fn read_counters(&self) -> Result<RawCounters, CollectorError> {
        let (rx_bytes, tx_bytes) = parse_net_dev(
            &fs::read_to_string(self.proc_root.join("net/dev")).map_err(CollectorError::Read)?,
            &self.interface,
        )?;
        let snmp = parse_protocol_table(
            &fs::read_to_string(self.proc_root.join("net/snmp")).map_err(CollectorError::Read)?,
            "Tcp",
        )?;
        let netstat = parse_protocol_table(
            &fs::read_to_string(self.proc_root.join("net/netstat"))
                .map_err(CollectorError::Read)?,
            "TcpExt",
        )?;
        let tcp_connections = fs::read_to_string(self.proc_root.join("net/sockstat"))
            .ok()
            .and_then(|contents| parse_tcp_connections(&contents))
            .unwrap_or(0);
        Ok(RawCounters {
            rx_bytes,
            tx_bytes,
            retransmitted_segments: field(&snmp, "RetransSegs", "Tcp")?,
            tcp_timeouts: field(&netstat, "TCPTimeouts", "TcpExt")?,
            to_zero_window_advertisements: field(&netstat, "TCPToZeroWindowAdv", "TcpExt")?,
            from_zero_window_advertisements: field(&netstat, "TCPFromZeroWindowAdv", "TcpExt")?,
            tcp_connections,
        })
    }

    pub fn previous_counters(&self) -> Option<&RawCounters> {
        self.previous.as_ref().map(|(_, counters)| counters)
    }

    pub fn sample(&mut self, timestamp: f64) -> Result<Option<RateSample>, CollectorError> {
        if !timestamp.is_finite() {
            return Err(CollectorError::InvalidTimestamp);
        }
        let current = self.read_counters()?;
        let result = if let Some((previous_timestamp, previous)) = &self.previous {
            let elapsed = timestamp - previous_timestamp;
            if elapsed <= 0.0 {
                return Err(CollectorError::NonIncreasingTimestamp);
            }
            let down_bps =
                counter_delta(current.rx_bytes, previous.rx_bytes) as f64 * BITS_PER_BYTE / elapsed;
            let up_bps =
                counter_delta(current.tx_bytes, previous.tx_bytes) as f64 * BITS_PER_BYTE / elapsed;
            push_bounded(&mut self.down_history_bps, down_bps, self.history_limit);
            push_bounded(&mut self.up_history_bps, up_bps, self.history_limit);
            Some(RateSample {
                timestamp,
                down_bps,
                up_bps,
                down_history_bps: self.down_history_bps.iter().copied().collect(),
                up_history_bps: self.up_history_bps.iter().copied().collect(),
                counters: current.clone(),
            })
        } else {
            None
        };
        self.previous = Some((timestamp, current));
        Ok(result)
    }
}

fn push_bounded(history: &mut VecDeque<f64>, value: f64, limit: usize) {
    history.push_back(value);
    while history.len() > limit {
        history.pop_front();
    }
}

/// Best-effort RTT collection. A missing/non-zero `fping` produces `None`.
pub fn collect_fping(targets: &[String]) -> Option<Vec<f64>> {
    if targets.is_empty() {
        return None;
    }
    let output = Command::new("fping")
        .args(["-C", "3", "-q"])
        .args(targets)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stderr);
    let samples = parse_fping_output(&text);
    (!samples.is_empty()).then_some(samples)
}

/// Best-effort average congestion-window collection from `ss -ti`.
pub fn collect_average_cwnd() -> Option<f64> {
    let output = Command::new("ss").args(["-tiH"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_average_cwnd(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_fping_output(output: &str) -> Vec<f64> {
    output
        .lines()
        .filter_map(|line| line.split_once(':').map(|(_, values)| values))
        .flat_map(str::split_whitespace)
        .filter_map(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect()
}

pub fn parse_average_cwnd(output: &str) -> Option<f64> {
    let values: Vec<f64> = output
        .split_whitespace()
        .filter_map(|token| token.strip_prefix("cwnd:")?.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// Parses the system-wide number of TCP sockets currently in use from
/// `/proc/net/sockstat`. Unknown or malformed input is intentionally `None` so
/// the proc collector can treat this optional counter as best-effort.
pub fn parse_tcp_connections(contents: &str) -> Option<u64> {
    let fields: Vec<&str> = contents
        .lines()
        .find(|line| line.starts_with("TCP:"))?
        .split_whitespace()
        .collect();
    fields
        .windows(2)
        .find(|pair| pair[0] == "inuse")?
        .get(1)?
        .parse()
        .ok()
}

fn counter_delta(current: u64, previous: u64) -> u64 {
    current.saturating_sub(previous)
}
fn parse_net_dev(contents: &str, interface: &str) -> Result<(u64, u64), CollectorError> {
    for line in contents.lines() {
        let Some((name, values)) = line.split_once(':') else {
            continue;
        };
        if name.trim() != interface {
            continue;
        }
        let fields: Vec<&str> = values.split_whitespace().collect();
        if fields.len() < 16 {
            return Err(CollectorError::Malformed(format!(
                "interface {interface} has fewer than 16 counters"
            )));
        }
        return Ok((
            parse_counter(fields[0], interface)?,
            parse_counter(fields[8], interface)?,
        ));
    }
    Err(CollectorError::Missing(format!("interface {interface}")))
}
fn parse_protocol_table(
    contents: &str,
    protocol: &str,
) -> Result<HashMap<String, String>, CollectorError> {
    let prefix = format!("{protocol}:");
    let mut lines = contents.lines();
    while let Some(headers) = lines.next() {
        if !headers.starts_with(&prefix) {
            continue;
        }
        let values = lines.next().ok_or_else(|| {
            CollectorError::Malformed(format!("protocol {protocol} is missing its value row"))
        })?;
        if !values.starts_with(&prefix) {
            return Err(CollectorError::Malformed(format!(
                "protocol {protocol} value row has the wrong prefix"
            )));
        }
        let headers: Vec<&str> = headers.split_whitespace().skip(1).collect();
        let values: Vec<&str> = values.split_whitespace().skip(1).collect();
        if headers.len() != values.len() {
            return Err(CollectorError::Malformed(format!(
                "protocol {protocol} header/value count differs"
            )));
        }
        return Ok(headers
            .into_iter()
            .zip(values)
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect());
    }
    Err(CollectorError::Missing(format!("protocol {protocol}")))
}
fn field(
    fields: &HashMap<String, String>,
    name: &str,
    protocol: &str,
) -> Result<u64, CollectorError> {
    fields
        .get(name)
        .ok_or_else(|| CollectorError::Missing(format!("{protocol}.{name}")))
        .and_then(|value| parse_counter(value, protocol))
}
fn parse_counter(value: &str, context: &str) -> Result<u64, CollectorError> {
    value
        .parse()
        .map_err(|_| CollectorError::Malformed(format!("invalid counter {value:?} in {context}")))
}

#[derive(Debug)]
pub enum CollectorError {
    Read(io::Error),
    Missing(String),
    Malformed(String),
    InvalidTimestamp,
    NonIncreasingTimestamp,
}
impl std::fmt::Display for CollectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "cannot read proc data: {error}"),
            Self::Missing(item) => write!(f, "missing proc item: {item}"),
            Self::Malformed(item) => write!(f, "malformed proc data: {item}"),
            Self::InvalidTimestamp => write!(f, "timestamp must be finite"),
            Self::NonIncreasingTimestamp => write!(f, "timestamp must increase between samples"),
        }
    }
}
impl std::error::Error for CollectorError {}
