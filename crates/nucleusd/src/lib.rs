//! Демон Nucleus для Orange Pi: BLE-периферия, устройства, HTTP/MQTT, mDNS.
//!
//! Сетевые роли: этот узел всегда BLE-**периферия** (телефоны и ESP32 — централы).

pub mod ble;
pub mod config;
pub mod devices;
pub mod http;
pub mod mdns;
pub mod mqtt;
pub mod state;
