// crates/nucleusd/src/ble_central.rs — BLE-централ: сервер подключается
// к узлу Nucleus (ESP32) как централ — ровно как телефон, к тому же GATT-профилю.

use std::time::Duration;

use anyhow::{anyhow, Context as _};
use async_trait::async_trait;
use btleplug::api::{
    Central as _, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral as BlePeripheral};
use futures::StreamExt as _;
use nucleus_core::{
    CoreError, DeviceExecutor, FloodRouter, NucleusCore, Transport, TransportError,
};
use uuid::Uuid;

/// UUID сервиса Nucleus (реклама узла) — как в nucleusd::ble и node/.
pub const NUCLEUS_UUID_STR: &str = "a6c6a000-f18e-4bfd-a51a-c609e45263d1";
/// UUID характеристики (write + notify).
pub const CHAR_UUID_STR: &str = "d6c6a000-f18e-4bfd-a51a-c609e45263d1";

/// Таймаут поиска узла.
const SCAN_TIMEOUT: Duration = Duration::from_secs(10);

/// Транспорт поверх GATT-соединения с узлом: write → команды, notify ← ответы.
pub struct BleCentralTransport {
    #[allow(dead_code)] // адаптер держит соединение на некоторых платформах
    adapter: Adapter,
    peripheral: BlePeripheral,
    characteristic: Characteristic,
    rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
}

#[async_trait]
impl Transport for BleCentralTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        self.peripheral
            .write(&self.characteristic, data, WriteType::WithoutResponse)
            .await
            .map_err(|_| TransportError::Send)
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        self.rx.recv().await.ok_or(TransportError::Closed)
    }
}

async fn ble_adapter() -> anyhow::Result<Adapter> {
    let manager = Manager::new().await.context("BLE manager")?;
    manager
        .adapters()
        .await
        .context("BLE adapters")?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("BLE-адаптер не найден"))
}

async fn is_nucleus_node(p: &BlePeripheral) -> bool {
    match p.properties().await {
        Ok(Some(props)) => {
            props.local_name.as_deref() == Some("nu")
                || props
                    .services
                    .contains(&Uuid::parse_str(NUCLEUS_UUID_STR).unwrap())
        }
        _ => false,
    }
}

/// Ищет узел Nucleus по рекламе и подключается к его GATT-профилю.
async fn connect(adapter: &Adapter) -> anyhow::Result<BleCentralTransport> {
    adapter
        .start_scan(ScanFilter::default())
        .await
        .context("запуск сканирования")?;
    let deadline = std::time::Instant::now() + SCAN_TIMEOUT;
    let mut node = None;
    while std::time::Instant::now() < deadline {
        for p in adapter.peripherals().await.context("перечисление")? {
            if is_nucleus_node(&p).await {
                node = Some(p);
                break;
            }
        }
        if node.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let peripheral =
        node.ok_or_else(|| anyhow!("узел Nucleus не найден за {} с", SCAN_TIMEOUT.as_secs()))?;

    peripheral.connect().await.context("BLE-соединение")?;
    peripheral
        .discover_services()
        .await
        .context("обход сервисов")?;
    let characteristic = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == Uuid::parse_str(CHAR_UUID_STR).unwrap())
        .ok_or_else(|| anyhow!("характеристика Nucleus не найдена на узле"))?;

    peripheral
        .subscribe(&characteristic)
        .await
        .context("подписка")?;
    let mut notifications = peripheral.notifications().await.context("notify-поток")?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let char_uuid = characteristic.uuid;
    tokio::spawn(async move {
        while let Some(n) = notifications.next().await {
            if n.uuid == char_uuid {
                let _ = tx.send(n.value);
            }
        }
    });

    tracing::info!("BLE-централ: подключён к узлу {}", peripheral.address());
    Ok(BleCentralTransport {
        adapter: adapter.clone(),
        peripheral,
        characteristic,
        rx,
    })
}

/// Основной цикл BLE-централа: поиск узла → движок поверх соединения,
/// при потере — переподключение. Узлы появляются в M3.
pub async fn run<E>(executor: E)
where
    E: DeviceExecutor + Clone + Send + 'static,
{
    loop {
        let adapter = match ble_adapter().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("BLE-централ: {e:#}, повтор через 10 с");
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        match connect(&adapter).await {
            Ok(transport) => {
                let router = FloodRouter::new(std::num::NonZeroUsize::new(1024).unwrap(), 8);
                let mut core: NucleusCore<BleCentralTransport, E> =
                    NucleusCore::new(executor.clone(), router, transport);
                loop {
                    match core.run_once().await {
                        Ok(()) => {}
                        Err(CoreError::Transport(TransportError::Closed)) => break,
                        Err(e) => {
                            tracing::warn!("BLE-централ, движок: {e}");
                            break;
                        }
                    }
                }
                tracing::warn!("BLE-централ: соединение с узлом потеряно, переподключение");
            }
            Err(e) => {
                tracing::debug!("BLE-централ: {e:#}");
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    }
}
