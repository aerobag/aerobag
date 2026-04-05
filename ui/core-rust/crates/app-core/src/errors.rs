use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppErrorKind {
    InvalidCatalog,
    InvalidManifest,
    InvalidFlightPlan,
    UnknownAirport,
    UnknownChart,
    UnsupportedOperation,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{kind:?}: {message}")]
pub struct AppError {
    pub kind: AppErrorKind,
    pub message: String,
}
