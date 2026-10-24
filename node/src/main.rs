//! Прошивка узла Nucleus для ESP32-C3.
//!
//! Роль узла (M3): BLE-**периферия** (телефон/ПК подключаются как централы)
//! + ESP-NOW-магистраль между узлами. Протокол и flooding — движок
//! `nucleus-core`; своих устройств у узла пока нет (GPIO — M4).
//!
//! Стек: esp-hal + esp-radio (BLE-контроллер + ESP-NOW) + esp-rtos (embassy)
//! + trouble-host (BLE-хост на чистом Rust, GATT-профиль Nucleus).

#![no_std]
#![no_main]

use embassy_futures::join::join3;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::ExternalController;

mod ble;
mod engine;
mod espnow;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 98767);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.interrupt0);

    log::info!("Nucleus node (ESP32-C3) starting");

    // BLE-контроллер → trouble-хост
    let bluetooth = peripherals.BT;
    let connector = BleConnector::new(bluetooth, Default::default()).unwrap();
    let controller: ExternalController<_, 20> = ExternalController::new(connector);

    // Три конкурирующих цикла в одной задаче: движок, ESP-NOW, BLE
    join3(engine::run(), espnow::run(), ble::run(controller)).await;
}
