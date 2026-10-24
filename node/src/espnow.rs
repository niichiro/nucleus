//! ESP-NOW: магистраль узел↔узел (broadcast-flooding).
//!
//! TODO(build): каркас — реальный приём/отправка включаются при первой
//! сборке с платой (API esp-radio 1.0-beta уточняется по компилятору):
//! создание EspNow поверх контроллера радио, broadcast-пир FF:FF:FF:FF:FF:FF,
//! цикл приёма → engine::INBOX, отправка из broadcast().

use embassy_time::{Duration, Timer};

/// Широковещательный MAC для лавинной рассылки.
const BROADCAST: [u8; 6] = [0xFF; 6];

/// Цикл ESP-NOW (каркас).
pub async fn run() {
    log::info!("ESP-NOW: skeleton, broadcast peer {BROADCAST:02X?}");
    // TODO(build): EspNow::new(...) + цикл приёма в engine::INBOX
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Лавинная отправка кадра соседям.
pub fn broadcast(_data: &[u8]) {
    // TODO(build): esp_now_send(broadcast)
}
