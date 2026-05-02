mod models;

use anyhow::Result;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{extract::State, routing::get, Json, Router};
use ble_node::NucleusApplication;
use bluer::{adv::Advertisement, Adapter, AdapterEvent, Session};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::fs;
use uuid::Uuid;

use crate::models::NUCLEUS_UUID;

const STATE_FILE: &str = "state.json";

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
    tracing_subscriber::fmt::init();

    // Инициализируем хранилище состояний
    let devices: DeviceMap = Arc::new(Mutex::new(HashMap::new()));
    // Добавим тестовую лампу
    {
        let mut db = devices.lock().unwrap();
        db.insert(
            "light.living".to_string(),
            DeviceState {
                on: false,
                brightness: 100,
            },
        );
    }

    // 1. Инициализация адаптера
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    // 2. Запуск GATT-сервера для Nucleus сервиса
    let mut app = bluer::gatt::local::Application::new();

    // Допустим, у нас есть характеристика с UUID-ом COMMAND_UUID
    let (cmd_control, cmd_handle) = bluer::gatt::local::characteristic_control();

    let command_uuid = uuid!("d6c6a000-f18e-4bfd-a51a-c609e45263d1");

    app.add_service(bluer::gatt::local::Service {
        uuid: Uuid::from_bytes(NUCLEUS_UUID),
        primary: true,
        characteristics: vec![bluer::gatt::local::Characteristic {
            uuid: command_uuid,
            write: Some(bluer::gatt::local::CharacteristicWrite {
                write: true,
                write_without_response: true,
                method: bluer::gatt::local::CharacteristicWriteMethod::Io,
                ..Default::default()
            }),
            control_handle: cmd_handle,
            ..Default::default()
        }],
        ..Default::default()
    });

    let _app_handle = adapter.serve_gatt_application(app).await?;

    // 3. Запуск рекламы (advertising)
    let mut manufacturer_data = BTreeMap::new();
    // Добавляем свой ID производителя для идентификации Nucleus устройств
    manufacturer_data.insert(0xFFFF, vec![]);
    let advertisement = bluer::adv::Advertisement {
        service_uuids: vec![Uuid::from_bytes(NUCLEUS_UUID)].into_iter().collect(),
        manufacturer_data,
        discoverable: Some(true),
        ..Default::default()
    };
    let _adv_handle = adapter.advertise(advertisement).await?;

    println!("Nucleus узел запущен и вещает...");

    // 4. Основной цикл: потоковая обработка команд
    loop {
        // Принимаем входящее соединение от центрального устройства
        let stream = cmd_control.accept().await?;
        tokio::spawn(async move {
            let mut buf = Vec::new();
            // Читаем данные, отправленные клиентом
            stream.read_to_end(&mut buf).await.unwrap();
            println!("Получена команда: {:?}", buf);
            // ... Здесь будет логика обработки команд умного дома ...
        });
    }
}
