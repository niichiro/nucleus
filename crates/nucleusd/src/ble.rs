// crates/nucleusd/src/ble.rs

use std::{str::FromStr, sync::Arc, time::Duration};

use async_trait::async_trait;
use bluer::{
    adv::Advertisement,
    gatt::{
        local::{
            characteristic_control, service_control, Application, Characteristic,
            CharacteristicControlEvent, CharacteristicNotify, CharacteristicNotifyMethod,
            CharacteristicWrite, CharacteristicWriteMethod, Service,
        },
        CharacteristicWriter,
    },
    Adapter,
};
use futures::{pin_mut, StreamExt};
use nucleus_core::{Transport, TransportError};
use tokio::{
    io::AsyncReadExt,
    sync::{mpsc, Mutex},
};
use uuid::Uuid;

/// UUID сервиса Nucleus (реклама и обнаружение).
pub const NUCLEUS_UUID: &str = "a6c6a000-f18e-4bfd-a51a-c609e45263d1";
/// UUID GATT-сервиса.
pub const SERVICE_UUID: &str = "c6c6a000-f18e-4bfd-a51a-c609e45263d1";
/// UUID характеристики: write (команды) + notify (Ack/Status).
pub const CHAR_UUID: &str = "d6c6a000-f18e-4bfd-a51a-c609e45263d1";

/// Транспорт поверх GATT-периферии.
///
/// Вход — write-характеристика, выход — notify (та же характеристика).
/// Исходящий кадр обязан влезать в MTU (~180 байт полезной нагрузки с запасом).
pub struct BleTransport {
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    notify: Arc<Mutex<Option<CharacteristicWriter>>>,
}

#[async_trait]
impl Transport for BleTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut guard = self.notify.lock().await;
        match guard.as_mut() {
            Some(writer) => writer.send(data).await.map_err(|_| TransportError::Send)?,
            // Нет подписчика на notify — отвечать некому, кадр выбрасываем
            None => return Ok(()),
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        self.rx.recv().await.ok_or(TransportError::Closed)
    }
}

/// Поднимает BLE-периферию: реклама «nu» + GATT-сервис Nucleus.
///
/// Возвращает транспорт к движку и JoinHandle задачи обслуживания GATT.
pub async fn start(
    adapter: &Adapter,
) -> bluer::Result<(BleTransport, tokio::task::JoinHandle<()>)> {
    let advertisement = Advertisement {
        service_uuids: vec![Uuid::from_str(NUCLEUS_UUID).unwrap()]
            .into_iter()
            .collect(),
        discoverable: Some(true),
        local_name: Some("nu".to_string()),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(advertisement).await?;

    let (service_control, service_handle) = service_control();
    let (char_control, char_handle) = characteristic_control();

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
    let app_handle = adapter.serve_gatt_application(app).await?;

    tracing::info!(
        "BLE GATT up: service handle 0x{:x}, characteristic handle 0x{:x}",
        service_control.handle()?,
        char_control.handle()?
    );

    let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let notify = Arc::new(Mutex::new(None::<CharacteristicWriter>));
    let writer_slot = Arc::clone(&notify);

    let task = tokio::spawn(async move {
        pin_mut!(char_control);
        loop {
            match char_control.next().await {
                Some(CharacteristicControlEvent::Write(req)) => {
                    let mtu = req.mtu();
                    tracing::debug!("BLE write session: MTU {mtu} from {}", req.device_address());
                    match req.accept() {
                        Ok(reader) => {
                            // Сессия чтения: может нести несколько записей подряд
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let mut reader = reader;
                                let mut buf = vec![0u8; mtu];
                                loop {
                                    match reader.read(&mut buf).await {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            let _ = tx.send(buf[..n].to_vec());
                                        }
                                        Err(e) => {
                                            tracing::debug!("BLE read session ended: {e}");
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => tracing::warn!("BLE accept write: {e}"),
                    }
                }
                Some(CharacteristicControlEvent::Notify(writer)) => {
                    tracing::debug!(
                        "BLE notify subscriber: MTU {} from {}",
                        writer.mtu(),
                        writer.device_address()
                    );
                    *writer_slot.lock().await = Some(writer);
                }
                None => {
                    tracing::info!("BLE characteristic control stream closed");
                    break;
                }
            }
        }
        drop(adv_handle);
        drop(app_handle);
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    Ok((BleTransport { rx, notify }, task))
}
