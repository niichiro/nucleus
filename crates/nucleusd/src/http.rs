// crates/nucleusd/src/http.rs

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::devices::{AnyDevice, DeviceManager, SetBody, SetError};

/// DTO устройства для HTTP API.
#[derive(Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source: &'static str,
    pub state: nucleus_core::DeviceState,
}

pub fn router(manager: DeviceManager) -> Router {
    Router::new()
        .route("/api/devices", get(list_devices))
        .route("/api/devices/{id}", get(get_device))
        .route("/api/devices/{id}/set", post(set_device))
        .with_state(manager)
}

/// Запускает HTTP API на всех интерфейсах.
pub async fn serve(manager: DeviceManager, port: u16) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("HTTP API listening on :{port}");
    axum::serve(listener, router(manager)).await
}

async fn list_devices(State(manager): State<DeviceManager>) -> Json<Vec<DeviceDto>> {
    Json(manager.list().into_iter().map(device_dto).collect())
}

async fn get_device(State(manager): State<DeviceManager>, Path(id): Path<String>) -> Response {
    match manager.list().into_iter().find(|d| d.id == id) {
        Some(device) => Json(device_dto(device)).into_response(),
        None => not_found(&id),
    }
}

async fn set_device(
    State(manager): State<DeviceManager>,
    Path(id): Path<String>,
    body: Result<Json<SetBody>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid body: {e}")).into_response(),
    };
    let Some(action) = body.to_action() else {
        return (
            StatusCode::BAD_REQUEST,
            "no action in body: expected on / level / value",
        )
            .into_response();
    };

    match manager.set(&id, &action).await {
        Ok(state) => {
            let AnyDevice {
                id: device_id,
                name,
                kind,
                source,
                ..
            } = manager
                .list()
                .into_iter()
                .find(|d| d.id == id)
                .expect("device existed during set");
            Json(DeviceDto {
                id: device_id,
                name,
                kind,
                source,
                state,
            })
            .into_response()
        }
        Err(SetError::NotFound(_)) => not_found(&id),
        Err(SetError::NotSupported(msg)) => (StatusCode::BAD_REQUEST, msg).into_response(),
        Err(SetError::Mqtt(msg)) => {
            (StatusCode::BAD_GATEWAY, format!("mqtt: {msg}")).into_response()
        }
    }
}

fn device_dto(device: AnyDevice) -> DeviceDto {
    DeviceDto {
        id: device.id,
        name: device.name,
        kind: device.kind,
        source: device.source,
        state: device.state,
    }
}

fn not_found(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        format!("device {id} not found on this node"),
    )
        .into_response()
}
