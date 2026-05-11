use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct Device {
    id: String,
    name: String,
    kind: DeviceKind,
    state: DeviceState,
}

#[derive(Debug, Clone, Serialize)]
pub enum DeviceKind {
    Lamp,
    Switch,
}

impl From<&str> for DeviceKind {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "lamp" => DeviceKind::Lamp,
            "switch" => DeviceKind::Switch,
            _ => panic!("Unknown device kind: {}", value),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceState {
    Lamp(LampState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LampState {
    pub on: bool,
    pub brightness: u8,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal server error")]
    LockPoisoned,

    #[error("Device {0} not found")]
    DeviceNotFound(String),

    #[error("Invalid data: {0}")]
    ValidationError(String),
}

// API-данные
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceCommand {
    on: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Command {
    Lamp(LampCommand),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LampCommand {
    on: bool,
    brightness: u8,
}
