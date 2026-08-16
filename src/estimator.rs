use serde::{Deserialize, Serialize};

/// The number of equal sub-windows in the constant-space filter. This is an
/// algorithmic invariant of the three-slot win-minmax design, not a tuning
/// parameter.
pub const SUBWINDOW_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Extremum {
    Max,
    Min,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    bucket: i64,
    value: f64,
    timestamp: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowedExtremum {
    window_seconds: f64,
    kind: Extremum,
    slots: [Option<Slot>; SUBWINDOW_COUNT],
    last_timestamp: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EstimatorError {
    InvalidWindow,
    InvalidSample,
    TimestampMovedBackward { previous: f64, received: f64 },
}

impl std::fmt::Display for EstimatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWindow => write!(f, "window length must be finite and positive"),
            Self::InvalidSample => write!(f, "sample value and timestamp must be finite"),
            Self::TimestampMovedBackward { previous, received } => {
                write!(f, "timestamp moved backward from {previous} to {received}")
            }
        }
    }
}

impl std::error::Error for EstimatorError {}

impl WindowedExtremum {
    pub fn max(window_seconds: f64) -> Result<Self, EstimatorError> {
        Self::new(window_seconds, Extremum::Max)
    }

    pub fn min(window_seconds: f64) -> Result<Self, EstimatorError> {
        Self::new(window_seconds, Extremum::Min)
    }

    pub fn new(window_seconds: f64, kind: Extremum) -> Result<Self, EstimatorError> {
        if !window_seconds.is_finite() || window_seconds <= 0.0 {
            return Err(EstimatorError::InvalidWindow);
        }
        Ok(Self {
            window_seconds,
            kind,
            slots: [None; SUBWINDOW_COUNT],
            last_timestamp: None,
        })
    }

    /// Records a sample and returns the current window extremum.
    ///
    /// The first sample is merely the only observation currently available;
    /// later larger (max mode) or smaller (min mode) observations replace it
    /// immediately. There is no seeded watermark and no sample down-weighting.
    pub fn feed(&mut self, value: f64, timestamp: f64) -> Result<f64, EstimatorError> {
        if !value.is_finite() || !timestamp.is_finite() {
            return Err(EstimatorError::InvalidSample);
        }
        if let Some(previous) = self.last_timestamp
            && timestamp < previous
        {
            return Err(EstimatorError::TimestampMovedBackward {
                previous,
                received: timestamp,
            });
        }

        self.expire(timestamp);
        let bucket = self.bucket(timestamp);
        let index = bucket.rem_euclid(SUBWINDOW_COUNT as i64) as usize;
        match self.slots[index] {
            Some(mut slot) if slot.bucket == bucket => {
                if self.prefer(value, slot.value) {
                    slot.value = value;
                    slot.timestamp = timestamp;
                    self.slots[index] = Some(slot);
                }
            }
            _ => {
                self.slots[index] = Some(Slot {
                    bucket,
                    value,
                    timestamp,
                });
            }
        }
        self.last_timestamp = Some(timestamp);
        Ok(self.extremum().expect("the newly inserted slot exists"))
    }

    /// Advances expiration without introducing an observation.
    pub fn advance(&mut self, timestamp: f64) -> Result<Option<f64>, EstimatorError> {
        if !timestamp.is_finite() {
            return Err(EstimatorError::InvalidSample);
        }
        if let Some(previous) = self.last_timestamp
            && timestamp < previous
        {
            return Err(EstimatorError::TimestampMovedBackward {
                previous,
                received: timestamp,
            });
        }
        self.expire(timestamp);
        self.last_timestamp = Some(timestamp);
        Ok(self.extremum())
    }

    pub fn estimate(&self) -> Option<f64> {
        self.extremum()
    }

    pub fn window_seconds(&self) -> f64 {
        self.window_seconds
    }

    pub fn kind(&self) -> Extremum {
        self.kind
    }

    fn subwindow_seconds(&self) -> f64 {
        self.window_seconds / SUBWINDOW_COUNT as f64
    }

    fn bucket(&self, timestamp: f64) -> i64 {
        (timestamp / self.subwindow_seconds()).floor() as i64
    }

    fn expire(&mut self, timestamp: f64) {
        let current_bucket = self.bucket(timestamp);
        for slot in &mut self.slots {
            if slot.is_some_and(|entry| current_bucket - entry.bucket >= SUBWINDOW_COUNT as i64) {
                *slot = None;
            }
        }
    }

    fn prefer(&self, candidate: f64, incumbent: f64) -> bool {
        match self.kind {
            Extremum::Max => candidate > incumbent,
            Extremum::Min => candidate < incumbent,
        }
    }

    fn extremum(&self) -> Option<f64> {
        self.slots
            .iter()
            .flatten()
            .map(|slot| slot.value)
            .reduce(|left, right| {
                if self.prefer(right, left) {
                    right
                } else {
                    left
                }
            })
    }
}
