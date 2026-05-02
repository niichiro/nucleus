use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

// Мок-данные
#[derive(Debug, Clone, Serialize)]
struct Device {
    id: String,
    name: String,
    kind: String,
    state: DeviceState,
}
// 2. Реализуем IntoResponse, чтобы Axum знал, как превратить ошибку в HTTP-ответ
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::LockPoisoned => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::DeviceNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
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

#[derive(Debug, Clone, Serialize)]
pub enum DeviceState {
    Lamp(LampState),
}

#[derive(Debug, Clone, Serialize)]
pub struct LampState {
    pub on: bool,
    pub brightness: u16,
}

// API-данные
#[derive(Debug, Clone, Deserialize)]
struct DeviceCommand {
    on: bool,
}

type Devices = HashMap<String, Device>;

async fn list_devices(
    State(state): State<Arc<Mutex<Devices>>>,
) -> Result<Json<Vec<Device>>, AppError> {
    let db = state.lock().map_err(|_| AppError::LockPoisoned)?;
    Ok(Json(db.values().cloned().collect()))
}

async fn handle_device_command(
    State(state): State<Arc<Mutex<Devices>>>,
    Path(id): Path<String>,
    Json(command): Json<DeviceCommand>,
) -> Result<StatusCode, AppError> {
    let mut db = state.lock().map_err(|_| AppError::LockPoisoned)?;
    let device = db.get_mut(&id).ok_or(AppError::DeviceNotFound(id))?;
    match &mut device.state {
        DeviceState::Lamp(lamp_state) => lamp_state.on = command.on,
    }
    Ok(StatusCode::NO_CONTENT)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Инициализация логов
    tracing_subscriber::fmt::init();

    // Мок-устройство
    let mut devices = HashMap::new();
    devices.insert(
        "lamp:living".into(),
        Device {
            id: "lamp:living".into(),
            name: "Лампа в гостиной".into(),
            kind: "light".into(),
            state: DeviceState::Lamp(LampState {
                on: false,
                brightness: 100,
            }),
        },
    );
    let state = Arc::new(Mutex::new(devices));

    let devices_router = Router::new()
        .route("/devices", get(list_devices))
        .route("/devices/{id}/set", post(handle_device_command));

    // HTTP-роуты
    let app = Router::new().nest("/api", devices_router).with_state(state);

    // Запускаем сервер
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!(
        "Nucleusd API server listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;

    Ok(())
}
