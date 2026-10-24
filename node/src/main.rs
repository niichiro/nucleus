//! Прошивка узла Nucleus для ESP32-C3.
//!
//! Роль узла (M3): BLE-**периферия** (телефон/ПК подключаются как централы)
//! + ESP-NOW-магистраль между узлами. Весь протокол — движок `nucleus-core`,
//! чужие команды ретранслируются (flooding), локальных устройств пока нет
//! (GPIO-исполнители — M4).
//!
//! Каркас: первая сборка против реальной платы потребует точной подгонки
//! API esp32-nimble/esp-now (помечено `TODO(build)`).

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

use async_trait::async_trait;
use esp_idf_svc::esp_idf_sys as sys;
use nucleus_core::{
    Command, DeviceExecutor, DeviceId, DeviceState, ExecuteError, FloodRouter, NucleusCore,
    Transport, TransportError,
};

// UUID GATT-профиля Nucleus — как в nucleusd::ble и nucleus-ctl
// (a6c6a000-f18e-4bfd-a51a-c609e45263d1 / d6c6a000-...-63d1).
// TODO(build): тип UUID зависит от API esp32-nimble (uuid128 из [u8; 16]).
const NUCLEUS_SERVICE_BYTES: [u8; 16] = [
    0xa6, 0xc6, 0xa0, 0x00, 0xf1, 0x8e, 0x4b, 0xfd, 0xa5, 0x1a, 0xc6, 0x09, 0xe4, 0x52, 0x63,
    0xd1,
];
const NUCLEUS_CHAR_BYTES: [u8; 16] = [
    0xd6, 0xc6, 0xa0, 0x00, 0xf1, 0x8e, 0x4b, 0xfd, 0xa5, 0x1a, 0xc6, 0x09, 0xe4, 0x52, 0x63,
    0xd1,
];

/// Входящие кадры: BLE-write + ESP-NOW-recv → движок.
static INBOX: OnceLock<Sender<Vec<u8>>> = OnceLock::new();

/// BLE-характеристика для исходящих кадров (notify), если есть подписчик.
// TODO(build): точный тип — Arc<Mutex<BLECharacteristic>> в терминах esp32-nimble
static NOTIFY_CHAR: OnceLock<Mutex<Option<esp32_nimble::BLECharacteristic>>> = OnceLock::new();

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Nucleus node starting (ESP32-C3)");

    let (tx, rx) = channel::<Vec<u8>>();
    INBOX.set(tx).expect("inbox once");

    espnow::init();
    ble::start();

    // Движок: транспорт = BLE-notify + ESP-NOW-broadcast, приём из INBOX.
    let executor = ForwardExecutor;
    let router = FloodRouter::new(core::num::NonZeroUsize::new(64).unwrap(), 8);
    let mut core = NucleusCore::new(executor, router, NodeTransport { rx });

    loop {
        // Однозадачный цикл: блокирующее ожидание кадра внутри block_on.
        match esp_idf_svc::hal::task::block_on(core.run_once()) {
            Ok(()) => {}
            Err(e) => log::warn!("engine: {e}"),
        }
    }
}

/// Транспорт узла: отправка — всем соседям (BLE notify + ESP-NOW broadcast),
/// приём — из общего INBOX (BLE-write и ESP-NOW-recv сходятся туда).
struct NodeTransport {
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
}

#[async_trait]
impl Transport for NodeTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        // BLE: уведомить подключённые централы
        // TODO(build): notify через характеристику esp32-nimble
        let _ = NOTIFY_CHAR.get();
        // ESP-NOW: лавинная рассылка соседям-узлам
        espnow::broadcast(data);
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        // Прошивка однозадачная: блокирующее ожидание оправдано
        self.rx.recv().map_err(|_| TransportError::Closed)
    }
}

/// Исполнитель без локальных устройств: любая команда «не наша» →
/// движок ретранслирует её по mesh.
#[derive(Clone)]
struct ForwardExecutor;

#[async_trait]
impl DeviceExecutor for ForwardExecutor {
    async fn execute(&mut self, command: &Command) -> Result<DeviceState, ExecuteError> {
        log::debug!("forwarding command for {}", command.device_id);
        Err(ExecuteError::DeviceNotFound)
    }

    async fn states(&self, _device_ids: Option<&[DeviceId]>) -> Vec<(DeviceId, DeviceState)> {
        Vec::new()
    }
}

/// BLE-периферия: тот же GATT-профиль, что принимает nucleusd (роль Orange Pi).
mod ble {
    use super::*;

    pub fn start() {
        // TODO(build): точная последовательность esp32-nimble (esp-idf flavor):
        //   BLEDevice::take() → get_server() → create_service(NUCLEUS_SERVICE)
        //   → characteristic(NUCLEUS_CHAR, READ|WRITE|WRITE_NR|NOTIFY)
        //   → on_write: INBOX.send(arg.recv_data())
        //   → сохранить характеристику в NOTIFY_CHAR
        //   → реклама с сервисом NUCLEUS_SERVICE, имя «nu»
        //   → запуск host-задачи nimble
        log::info!("BLE GATT peripheral: skeleton, включается при первой сборке с платой");
    }
}

/// ESP-NOW: магистраль узел↔узел (broadcast, шифрование — M4).
mod espnow {
    use super::*;

    /// Широковещательный MAC для лавинной рассылки.
    const BROADCAST: [u8; 6] = [0xFF; 6];

    extern "C" fn on_recv(info: *const sys::esp_now_recv_info_t, data: *const u8, len: i32) {
        // TODO(build): сигнатура колбэка IDF 5.x — проверить при первой сборке
        unsafe {
            let bytes = core::slice::from_raw_parts(data, len as usize).to_vec();
            if let Some(tx) = INBOX.get() {
                let _ = tx.send(bytes);
            }
        }
    }

    pub fn init() {
        unsafe {
            // Wi-Fi STA — среда для ESP-NOW
            let mut cfg = sys::wifi_init_config_t::default();
            sys::esp_wifi_init(&mut cfg);
            sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA);
            sys::esp_wifi_start();

            sys::esp_now_init();
            sys::esp_now_register_recv_cb(Some(on_recv));

            // Широковещательный пир для flooding
            let mut peer: sys::esp_now_peer_info_t = core::mem::zeroed();
            peer.peer_addr = BROADCAST;
            peer.channel = 0;
            peer.ifidx = sys::wifi_interface_t_WIFI_IF_STA;
            sys::esp_now_add_peer(&mut peer);
        }
        log::info!("ESP-NOW initialized");
    }

    pub fn broadcast(data: &[u8]) {
        unsafe {
            sys::esp_now_send(
                BROADCAST.as_ptr(),
                data.as_ptr(),
                data.len() as i32,
            );
        }
    }
}
