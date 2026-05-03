mod models;

use anyhow::Result;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{extract::State, routing::get, Json, Router};
use bluer::{
    adv::Advertisement,
    gatt::{
        local::{
            characteristic_control, service_control, Application, Characteristic,
            CharacteristicControlEvent, CharacteristicNotify, CharacteristicNotifyMethod,
            CharacteristicWrite, CharacteristicWriteMethod, Service,
        },
        CharacteristicReader, CharacteristicWriter,
    },
};
use futures::{future, pin_mut, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    time::{interval, sleep},
};
use uuid::Uuid;

pub const NUCLEUS_UUID: &str = "a6c6a000-f18e-4bfd-a51a-c609e45263d1";
pub const COMMAND_UUID: &str = "b6c6a000-f18e-4bfd-a51a-c609e45263d1";
pub const SERVICE_UUID: &str = "c6c6a000-f18e-4bfd-a51a-c609e45263d1";
pub const CHAR_UUID: &str = "d6c6a000-f18e-4bfd-a51a-c609e45263d1";

const STATE_FILE: &str = "state.json";

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    dotenvy::dotenv();

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    let advertisement = Advertisement {
        service_uuids: vec![Uuid::from_str(NUCLEUS_UUID).unwrap()]
            .into_iter()
            .collect(),
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
        uuid: Uuid::from_str(CHAR_UUID).unwrap(),
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
        uuid: Uuid::from_str(SERVICE_UUID).unwrap(),
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

    println!("Service handle is 0x{:x}", service_control.handle()?);
    println!("Characteristic handle is 0x{:x}", char_control.handle()?);

    println!("Nucleus BLE node is up. Listening for commands...");

    // Шаг 4: основной цикл обработки событий
    println!("Service ready. Press enter to quit.");

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    let mut value: Vec<u8> = vec![0x10, 0x01, 0x01, 0x10];
    let mut read_buf = Vec::new();
    let mut reader_opt: Option<CharacteristicReader> = None;
    let mut writer_opt: Option<CharacteristicWriter> = None;
    let mut interval = interval(Duration::from_secs(1));

    pin_mut!(char_control);
    loop {
        tokio::select! {
            _ = lines.next_line() => break,
            evt = char_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        println!("Accepting write event with MTU {} from {}", req.mtu(), req.device_address());
                        read_buf = vec![0; req.mtu()];
                        reader_opt = Some(req.accept()?);
                    },
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {} from {}", notifier.mtu(), notifier.device_address());
                        writer_opt = Some(notifier);
                    },
                    None => break,
                }
            }
            _ = interval.tick() => {
                println!("Decrementing each element by one");
                for v in &mut *value {
                    *v = v.saturating_sub(1);
                }
                println!("Value is {:x?}", &value);
                if let Some(writer) = writer_opt.as_mut() {
                    println!("Notifying with value {:x?}", &value);
                    if let Err(err) = writer.write(&value).await {
                        println!("Notification stream error: {}", &err);
                        writer_opt = None;
                    }
                }
            }
            read_res = async {
                match &mut reader_opt {
                    Some(reader) => reader.read(&mut read_buf).await,
                    None => future::pending().await,
                }
            } => {
                match read_res {
                    Ok(0) => {
                        println!("Write stream ended");
                        reader_opt = None;
                    }
                    Ok(n) => {
                        value = read_buf[0..n].to_vec();
                        println!("Write request with {} bytes: {:x?}", n, &value);
                    }
                    Err(err) => {
                        println!("Write stream error: {}", &err);
                        reader_opt = None;
                    }
                }
            }
        }
    }

    println!("Removing service and advertisement");
    drop(app_handle);
    drop(adv_handle);
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
