use crate::estimator::WindowedExtremum;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverState {
    pub channel_id: String,
    pub down_max: WindowedExtremum,
    pub up_max: WindowedExtremum,
}

impl ObserverState {
    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), StateError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StateError::Io)?;
        }
        let file_name = path
            .file_name()
            .ok_or_else(|| StateError::InvalidPath(path.display().to_string()))?;
        let temp_path = path.with_file_name(format!(".{}.tmp", file_name.to_string_lossy()));
        let bytes = serde_json::to_vec_pretty(self).map_err(StateError::Serialize)?;
        fs::write(&temp_path, bytes).map_err(StateError::Io)?;
        fs::rename(&temp_path, path).map_err(StateError::Io)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Option<Self>, StateError> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(StateError::Serialize),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(StateError::Io(error)),
        }
    }
}

#[derive(Debug)]
pub enum StateError {
    Io(io::Error),
    Serialize(serde_json::Error),
    InvalidPath(String),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "state I/O failed: {error}"),
            Self::Serialize(error) => write!(f, "state serialization failed: {error}"),
            Self::InvalidPath(path) => write!(f, "state path has no file name: {path}"),
        }
    }
}

impl std::error::Error for StateError {}
