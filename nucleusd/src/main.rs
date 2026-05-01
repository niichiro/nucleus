use axum::{routing::get, Router, Json, extract::State};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

// Мок-данные
#[derive(Debug, Clone, Serialize)]
struct Device {
    id: String,
    name: String,
    kind: String,
    state: serde_json::Value,
}

type Devices = HashMap<String, Device>;

async fn list_devices(State(state): State<Arc<Mutex<Devices>>>) -> Json<Vec<Device>> {
    let db = state.lock().unwrap();
    Json(db.values().cloned().collect())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Инициализация логов
    tracing_subscriber::fmt::init();

    // Мок-устройство
    let mut devices = HashMap::new();
    devices.insert("lamp:living".into(), Device {
        id: "lamp:living".into(),
        name: "Лампа в гостиной".into(),
        kind: "light".into(),
        state: serde_json::json!({ "on": false, "brightness": 100 }),
    });
    let state = Arc::new(Mutex::new(devices));

    // HTTP-роуты
    let app = Router::new()
        .route("/api/devices", get(list_devices))
        .with_state(state);

    // Запускаем сервер
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Nucleusd API server listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
