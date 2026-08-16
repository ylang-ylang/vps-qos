use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

const BITS_PER_BYTE: f64 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RawCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub retransmitted_segments: u64,
    pub tcp_timeouts: u64,
    pub to_zero_window_advertisements: u64,
    pub from_zero_window_advertisements: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RateSample {
    pub timestamp: f64,
    pub down_bps: f64,
    pub up_bps: f64,
    pub counters: RawCounters,
}

#[derive(Debug, Clone)]
pub struct ProcCollector {
    proc_root: PathBuf,
    interface: String,
    previous: Option<(f64, RawCounters)>,
}

impl ProcCollector {
    pub fn new(proc_root: impl Into<PathBuf>, interface: impl Into<String>) -> Self {
        Self {
            proc_root: proc_root.into(),
            interface: interface.into(),
            previous: None,
        }
    }

    /// Reads cumulative kernel counters without applying policy or thresholds.
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

        Ok(RawCounters {
            rx_bytes,
            tx_bytes,
            retransmitted_segments: field(&snmp, "RetransSegs", "Tcp")?,
            tcp_timeouts: field(&netstat, "TCPTimeouts", "TcpExt")?,
            to_zero_window_advertisements: field(&netstat, "TCPToZeroWindowAdv", "TcpExt")?,
            from_zero_window_advertisements: field(&netstat, "TCPFromZeroWindowAdv", "TcpExt")?,
        })
    }

    pub fn previous_counters(&self) -> Option<&RawCounters> {
        self.previous.as_ref().map(|(_, counters)| counters)
    }

    /// Produces rates only after two snapshots. Counter resets yield a zero
    /// delta rather than wrapping to an enormous synthetic sample.
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
            Some(RateSample {
                timestamp,
                down_bps: counter_delta(current.rx_bytes, previous.rx_bytes) as f64 * BITS_PER_BYTE
                    / elapsed,
                up_bps: counter_delta(current.tx_bytes, previous.tx_bytes) as f64 * BITS_PER_BYTE
                    / elapsed,
                counters: current.clone(),
            })
        } else {
            None
        };
        self.previous = Some((timestamp, current));
        Ok(result)
    }
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
        let header_fields = headers.split_whitespace().skip(1);
        let value_fields: Vec<&str> = values.split_whitespace().skip(1).collect();
        let headers: Vec<&str> = header_fields.collect();
        if headers.len() != value_fields.len() {
            return Err(CollectorError::Malformed(format!(
                "protocol {protocol} header/value count differs"
            )));
        }
        return Ok(headers
            .into_iter()
            .zip(value_fields)
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
