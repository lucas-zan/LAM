use super::error::{AppError, Result};
use std::path::PathBuf;

pub fn resolve_home_root() -> Result<PathBuf> {
    if let Ok(value) = std::env::var("LAM_HOME") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }

    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|err| AppError::new("HOME_NOT_FOUND", err.to_string()))
}
