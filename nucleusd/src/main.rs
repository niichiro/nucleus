mod models;

use anyhow::Result;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{extract::State, routing::get, Json, Router};
use bluer::adv::Advertisement;
use bluer::gatt::local::{
    characteristic_control, service_control, Application, Characteristic,
    CharacteristicControlEvent, CharacteristicNotify, CharacteristicNotifyMethod,
    CharacteristicWrite, CharacteristicWriteMethod, Service,
};
use futures::pin_mut;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::fs;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

pub const NUCLEUS_UUID: [u8; 16] = uuid!("d6c6a000-f18e-4bfd-a51a-c609e45263d1").into_bytes();
pub const COMMAND_UUID: [u8; 16] = uuid!("a6c6a000-f18e-4bfd-a51a-c609e45263d1").into_bytes();
pub const SERVICE_UUID: [u8; 16] = uuid!("c6c6a000-f18e-4bfd-a51a-c609e45263d1").into_bytes();
pub const CHAR_UUID: [u8; 16] = uuid!("b6c6a000-f18e-4bfd-a51a-c609e45263d1").into_bytes();

const STATE_FILE: &str = "state.json";

type Devices = HashMap<String, Device>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    let advertisement = Advertisement {
        service_uuids: vec![Uuid::from_bytes(NUCLEUS_UUID)].into_iter().collect(),
        discoverable: Some(true),
        local_name: Some("nucleus-node".to_string()),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(advertisement).await?;

    // Шаг 1: создаём контрольные точки для сервиса и характеристики
    let (service_control, service_handle) = service_control();
    let (char_control, char_handle) = characteristic_control();

    // Шаг 2: собираем GATT-приложение
    let characteristic = Characteristic {
        uuid: Uuid::from_bytes(CHAR_UUID),
        write: Some(CharacteristicWrite {
            write: true,
            write_without_response: true,
            method: CharacteristicWriteMethod::Io,
            ..Default::default()
        }),
        notify: Some(CharacteristicNotify {
            notify: true,
            method: CharacteristicNotifyMethod::Io,
            ..Default::default()
        }),
        control_handle: char_handle,
        ..Default::default()
    };
    let service = Service {
        uuid: Uuid::from_bytes(SERVICE_UUID),
        primary: true,
        characteristics: vec![characteristic],
        control_handle: service_handle,
        ..Default::default()
    };
    let app = Application {
        services: vec![service],
        ..Default::default()
    };

    // Шаг 3: регистрируем приложение и запускаем рекламу
    let app_handle = adapter.serve_gatt_application(app).await?;

    println!("Nucleus BLE node is up. Listening for commands...");

    // Шаг 4: основной цикл обработки событий
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    let mut value: Vec<u8> = vec![0x10, 0x01, 0x01, 0x10];
    let mut read_buf = Vec::new();
    let mut reader_opt: Option<CharacteristicReader> = None;
    let mut writer_opt: Option<CharacteristicWriter> = None;
    let mut interval = interval(Duration::from_secs(1));
    let mut reader_opt: Option<Box<dyn AsyncReadExt + Unpin>> = None;
    let mut read_buf = vec![0u8; 512];

    pin_mut!(char_control);
    loop {
        tokio::select! {
            evt = char_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        println!("Write from {}", req.device_address());
                        read_buf.resize(req.mtu(), 0);
                        reader_opt = Some(req.accept()?);
                    }
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Notify subscription from {}", notifier.device_address());
                        // Можно сохранить notifier для отправки уведомлений клиенту
                    }
                    None => break,
                    _ => {}
                }
            }
            // Чтение данных от клиента
            _ = async {
                if let Some(ref mut reader) = reader_opt {
                    match reader.read(&mut read_buf).await {
                        Ok(n) if n > 0 => {
                            println!("Received command: {:?}", &read_buf[..n]);
                            // Здесь обрабатываем команду умного дома
                        }
                        _ => { reader_opt = None; }
                    }
                }
                // fallback
                std::future::pending().await
            } => {}
        }
    }

    println!("Removing service and advertisement");
    drop(app_handle);
    drop(adv_handle);
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
